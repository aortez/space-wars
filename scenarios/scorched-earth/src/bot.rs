//! Observed-shot calibration and bounded combat decisions, above the driver.
use super::*;

mod aim;
pub(crate) mod comparison;
mod position;
mod priority;
use aim::{Aim, Geometry};
#[cfg(test)]
mod tests;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuPolicy {
    #[default]
    Sweep,
    Calibrated,
    Reactive,
}

impl CpuPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sweep => "Sweep",
            Self::Calibrated => "Learning",
            Self::Reactive => "Learning + move",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Sweep => Self::Calibrated,
            Self::Calibrated => Self::Reactive,
            Self::Reactive => Self::Sweep,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Launch {
    pub id: u64,
    pub tick: u64,
    pub owner: usize,
    pub origin: Vec2,
    pub velocity: Vec2,
    pub elevation: f32,
    pub power: f32,
    pub shooter: Vec2,
    pub pitch: f32,
    pub target: Vec2,
    pub blocked_barrel: bool,
    pub calibrated: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct Flight {
    launch: Launch,
    closest: f32,
    crossing_height: Option<f32>,
    target_moved: bool,
}

impl Flight {
    pub fn observe(&mut self, from: Vec2, to: Vec2, target: Vec2) {
        self.target_moved |= target.distance_to(self.launch.target) > 1.5;
        self.closest = self
            .closest
            .min(from.distance_to(target))
            .min(to.distance_to(target));
        let dx = to.x - from.x;
        if dx.abs() > 0.0001 {
            let t = (self.launch.target.x - from.x) / dx;
            if (0.0..=1.0).contains(&t) {
                self.crossing_height = Some(from.y + (to.y - from.y) * t - self.launch.target.y);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ShotObservation {
    pub launch: Launch,
    pub tick: u64,
    pub end: Vec2,
    pub termination: &'static str,
    pub assessment: &'static str,
    pub closest: f32,
    pub crossing_height: Option<f32>,
    pub target_moved: bool,
    pub damage: [f32; 2],
}

#[derive(Default, Clone, Serialize)]
pub(super) struct Bot {
    aim: Aim,
    pending: Option<u64>,
    pub mode: &'static str,
    pub outcomes: u64,
    pub useful_shots: u64,
    pub last_shot: Option<ShotObservation>,
    last_hit: Option<(u64, f32)>,
    hit_serial: u64,
    considered_hit: u64,
    relocation: Option<Relocation>,
    relocate_after: u64,
    require_shot: bool,
    hold_until: u64,
    pub relocations: u64,
    pub relocations_finished: u64,
    pub relocation_refusals: u64,
    last_relocation: Option<RelocationOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shot_priority: Option<priority::ShotPriority>,
}

#[derive(Clone, Serialize)]
struct Relocation {
    goal: driving::TravelGoal,
    deadline: u64,
    started: u64,
    origin: Vec2,
    destination_x: f32,
}

#[derive(Clone, Serialize)]
struct RelocationOutcome {
    started: u64,
    ended: u64,
    origin: Vec2,
    destination_x: f32,
    position: Vec2,
    reason: &'static str,
}

impl Bot {
    pub fn hit(&mut self, tick: u64, damage: f32) {
        self.last_hit = Some((tick, damage));
        self.hit_serial += 1;
    }

    fn finish_relocation(&mut self, state: &ScorchedState, player: usize, reason: &'static str) {
        let relocation = self.relocation.take().unwrap();
        self.last_relocation = Some(RelocationOutcome {
            started: relocation.started,
            ended: state.tick,
            origin: relocation.origin,
            destination_x: relocation.destination_x,
            position: Geometry::read(state, player).shooter,
            reason,
        });
        self.relocate_after = state.tick + 600;
        self.hold_until = state.tick + 180;
        self.mode = "settle after hit";
        self.relocations_finished += 1;
    }

    // Call after refreshing aim against the current poses. This is also the
    // checkpoint boundary used by the lab; it does not reset learned history.
    fn wants_relocation(&self, state: &ScorchedState, player: usize) -> bool {
        state.config.cpu_policy[player] == CpuPolicy::Reactive
            && self.relocation.is_none()
            && self
                .shot_priority
                .as_ref()
                .is_none_or(|p| p.ended.is_some())
            && self.hit_serial != self.considered_hit
            && !self.aim.confident
            && !self.require_shot
            && state.tick >= self.relocate_after
            && self
                .last_hit
                .is_some_and(|(tick, _)| state.tick.saturating_sub(tick) <= 180)
    }

    fn consider_relocation(&mut self, state: &ScorchedState, player: usize, x: Option<f32>) {
        self.considered_hit = self.hit_serial;
        if let Some(x) = x {
            self.relocation = Some(Relocation {
                goal: driving::TravelGoal::new(x, 2.5),
                deadline: state.tick + 240,
                started: state.tick,
                origin: Geometry::read(state, player).shooter,
                destination_x: x,
            });
            self.relocations += 1;
            self.require_shot = true;
            self.aim.confident = false;
            // Gate firing before the drive ramp changes measured velocity.
            self.mode = "relocating after hit";
        } else {
            self.relocation_refusals += 1;
            self.relocate_after = state.tick + 180;
        }
    }

    fn react_to_hit(&mut self, state: &ScorchedState, player: usize) {
        let destination = state.escape_destination(player, self.aim.elevation);
        if destination.is_some() && self.try_shot_priority(state, player) {
            self.considered_hit = self.hit_serial;
        } else {
            self.consider_relocation(state, player, destination);
        }
    }

    fn drive(&mut self, state: &ScorchedState, player: usize) -> Option<f32> {
        self.aim.refresh(Geometry::read(state, player));
        if let Some(move_goal) = &mut self.relocation {
            let decision = move_goal.goal.decide(state, player);
            if state.tick >= move_goal.deadline
                || matches!(
                    decision.mode,
                    "arrived" | "stalled" | "blocked ground" | "no support"
                )
            {
                let reason = if state.tick >= move_goal.deadline {
                    "deadline"
                } else {
                    decision.mode
                };
                self.finish_relocation(state, player, reason);
                return Some(0.0);
            }
            self.mode = "relocating after hit";
            return Some(decision.axis);
        }
        if self.protect_correction(state, player) {
            return Some(0.0);
        }
        if self.wants_relocation(state, player) {
            self.react_to_hit(state, player);
            if self.protect_correction(state, player) {
                return Some(0.0);
            }
            if self.relocation.is_some() {
                return self.drive(state, player);
            }
        }
        if state.tick < self.hold_until || self.pending.is_some() {
            self.mode = if self.pending.is_some() {
                "watching shot"
            } else {
                "settle after hit"
            };
            return Some(0.0);
        }
        self.mode = if self.aim.confident {
            "aim acquired"
        } else {
            "ranging"
        };
        None
    }
}

impl ScorchedState {
    pub(super) fn calibrated_cpu(&self, player: usize) -> bool {
        self.config.cpu_policy[player] != CpuPolicy::Sweep
            && self.config.course.is_none()
            && !self.config.roof
            && !self.config.hillside
            && !self.config.undercut
            && self.config.drop_height.is_none()
            && self.config.bombardment_seconds == 0
    }

    pub fn shot_observations(&self) -> &[ShotObservation] {
        &self.shot_observations
    }

    pub fn cpu_diagnostics(&self) -> serde_json::Value {
        serde_json::to_value(&self.bots).expect("finite bot state")
    }

    pub(super) fn bot_drive(&mut self, player: usize) -> Option<f32> {
        if !self.calibrated_cpu(player) {
            return None;
        }
        let mut bot = std::mem::take(&mut self.bots[player]);
        let axis = bot.drive(self, player);
        if axis.is_some() {
            self.driving[player].diagnostics.mode = bot.mode;
        }
        self.bots[player] = bot;
        axis
    }

    fn escape_destination(&self, player: usize, elevation: f32) -> Option<f32> {
        self.relocation_sites(player, elevation)
            .into_iter()
            .filter(|site| site.rejection.is_none())
            .min_by(|a, b| a.score.unwrap().total_cmp(&b.score.unwrap()))
            .map(|site| site.x)
    }

    pub(super) fn bot_allows_fire(&self, player: usize) -> bool {
        !self.calibrated_cpu(player)
            || (self.bots[player].pending.is_none()
                && (!self.config.cpu_driving || self.bots[player].relocation.is_none()))
    }

    pub(super) fn aim_calibrated(&mut self, player: usize) {
        let geometry = Geometry::read(self, player);
        let aim = &mut self.bots[player].aim;
        aim.refresh(geometry);
        self.tanks[player].facing = (geometry.target.x - geometry.shooter.x).signum();
        self.tanks[player].elevation = aim.elevation;
        self.tanks[player].speed = aim.power;
    }

    pub(super) fn record_launch(
        &mut self,
        player: usize,
        origin: Vec2,
        velocity: Vec2,
        blocked_barrel: bool,
    ) -> Flight {
        let geometry = Geometry::read(self, player);
        let calibrated =
            self.calibrated_cpu(player) && (self.config.demo || player != self.selected);
        let launch = Launch {
            id: self.shots + 1,
            tick: self.tick,
            owner: player,
            origin,
            velocity,
            elevation: self.tanks[player].elevation,
            power: self.tanks[player].speed,
            shooter: geometry.shooter,
            pitch: geometry.pitch,
            target: geometry.target,
            blocked_barrel,
            calibrated,
        };
        if calibrated {
            let bot = &mut self.bots[player];
            bot.pending = Some(launch.id);
            bot.require_shot = false;
            bot.hold_until = 0;
            bot.mode = "watching shot";
            bot.priority_launched(launch.id, launch.tick);
        }
        Flight {
            launch,
            closest: origin.distance_to(geometry.target),
            crossing_height: None,
            target_moved: false,
        }
    }

    pub(super) fn record_outcome(
        &mut self,
        shell: Shell,
        point: Vec2,
        kind: &'static str,
        health_before: [f32; 2],
    ) {
        let Some(flight) = shell.flight else {
            return;
        };
        let mut result = ShotObservation {
            launch: flight.launch,
            tick: self.tick,
            end: point,
            termination: kind,
            assessment: "unclassified",
            closest: flight.closest,
            crossing_height: flight.crossing_height,
            target_moved: flight.target_moved,
            damage: std::array::from_fn(|i| health_before[i] - self.tanks[i].health),
        };
        result.assessment = aim::assess(&result, shell.velocity);
        let bot = &mut self.bots[shell.owner];
        bot.outcomes += 1;
        bot.useful_shots += u64::from(result.assessment == "useful hit");
        if bot.pending == Some(result.launch.id) {
            bot.pending = None;
        }
        if result.launch.calibrated {
            bot.aim.learn(&result);
            bot.mode = if bot.aim.confident {
                "aim acquired"
            } else {
                "ranging"
            };
        }
        bot.last_shot = Some(result);
        bot.priority_observed(&result);
        // Diagnostics are bounded even when a match cannot reach an outcome.
        if self.shot_observations.len() == 64 {
            self.shot_observations.remove(0);
        }
        self.shot_observations.push(result);
    }
}
