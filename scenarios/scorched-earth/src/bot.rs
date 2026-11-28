//! Observed-shot calibration and bounded combat decisions, above the driver.
use super::*;

mod aim;
pub(crate) mod comparison;
mod dodge;
mod plot;
mod position;
mod priority;
use aim::{Aim, Geometry};
pub(crate) use plot::Plot;
#[cfg(test)]
mod tests;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuPolicy {
    #[default]
    Sweep,
    Calibrated,
    Reactive,
    /// Traces a shell for each elevation through the world before every shot,
    /// so it can fire through an opening, and moves after hits as `Reactive`
    /// does.
    Plotted,
}

impl CpuPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sweep => "Sweep",
            Self::Calibrated => "Learning",
            Self::Reactive => "Learning + move",
            Self::Plotted => "Plotting + move",
        }
    }
    /// Whether a CPU on this policy drives in a fight after it is hit.
    pub fn moves(self) -> bool {
        matches!(self, Self::Reactive | Self::Plotted)
    }
}

impl ScorchedConfig {
    /// The name of `player`'s CPU, as the duel's CPU button shows it.
    pub fn cpu_label(&self, player: usize) -> &'static str {
        match self.cpu_policy[player] {
            CpuPolicy::Reactive if self.cpu_dodging[player] => "Learning + dodge",
            CpuPolicy::Plotted if self.cpu_dodging[player] => "Plotting + dodge",
            policy => policy.label(),
        }
    }

    /// Whether `player`'s CPU dodges after a hit: it must move, and have
    /// dodging on.
    pub(crate) fn dodges(&self, player: usize) -> bool {
        self.cpu_dodging[player] && self.cpu_policy[player].moves()
    }

    /// Moves both tanks to the next CPU on the duel's CPU button.
    pub(crate) fn next_cpu(&mut self) {
        const CYCLE: [(CpuPolicy, bool); 6] = [
            (CpuPolicy::Sweep, false),
            (CpuPolicy::Calibrated, false),
            (CpuPolicy::Reactive, false),
            (CpuPolicy::Reactive, true),
            (CpuPolicy::Plotted, false),
            (CpuPolicy::Plotted, true),
        ];
        let index = CYCLE
            .iter()
            .position(|&cpu| cpu == (self.cpu_policy[0], self.dodges(0)))
            .expect("every CPU is on the button");
        let (policy, dodging) = CYCLE[(index + 1) % CYCLE.len()];
        self.cpu_policy = [policy; 2];
        self.cpu_dodging = [dodging; 2];
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
    /// The plotted CPU's plot for its next shot, kept after firing until the
    /// next one begins.
    #[serde(skip_serializing_if = "Option::is_none")]
    plot: Option<Plot>,
    /// A dodging CPU's dodges, from its first hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    dodging: Option<dodge::Dodging>,
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

    fn finish_relocation(
        &mut self,
        state: &Duel,
        physics: &PhysicsWorld,
        player: usize,
        reason: &'static str,
    ) {
        let relocation = self.relocation.take().unwrap();
        self.last_relocation = Some(RelocationOutcome {
            started: relocation.started,
            ended: state.tick,
            origin: relocation.origin,
            destination_x: relocation.destination_x,
            position: Geometry::read(state, physics, player).shooter,
            reason,
        });
        self.relocate_after = state.tick + 600;
        self.hold_until = state.tick + 180;
        self.mode = "settle after hit";
        self.relocations_finished += 1;
    }

    // Call after refreshing aim against the current poses. This is also the
    // checkpoint boundary used by the lab; it does not reset learned history.
    fn wants_relocation(&self, state: &Duel, player: usize) -> bool {
        let policy = state.config.cpu_policy[player];
        // A plotted tank is confident while its last plot found an arc that
        // reaches the target.
        let confident = if policy == CpuPolicy::Plotted {
            self.plotted_hit()
        } else {
            self.aim.bracket.confident
        };
        policy.moves()
            && self.relocation.is_none()
            && self
                .shot_priority
                .as_ref()
                .is_none_or(|p| p.ended.is_some())
            && self.hit_serial != self.considered_hit
            && !confident
            && !self.require_shot
            && state.tick >= self.relocate_after
            && self
                .last_hit
                .is_some_and(|(tick, _)| state.tick.saturating_sub(tick) <= 180)
    }

    fn consider_relocation(
        &mut self,
        state: &Duel,
        physics: &PhysicsWorld,
        player: usize,
        x: Option<f32>,
    ) {
        self.considered_hit = self.hit_serial;
        if let Some(x) = x {
            self.relocation = Some(Relocation {
                goal: driving::TravelGoal::new(x, 2.5),
                deadline: state.tick + 240,
                started: state.tick,
                origin: Geometry::read(state, physics, player).shooter,
                destination_x: x,
            });
            self.relocations += 1;
            self.require_shot = true;
            self.aim.bracket.confident = false;
            self.plot = None;
            // Gate firing before the drive ramp changes measured velocity.
            self.mode = "relocating after hit";
        } else {
            self.relocation_refusals += 1;
            self.relocate_after = state.tick + 180;
        }
    }

    fn react_to_hit(&mut self, state: &Duel, physics: &PhysicsWorld, player: usize) {
        let destination = state.escape_destination(physics, player, self.aim.bracket.elevation);
        if destination.is_some() && self.try_shot_priority(state, physics, player) {
            self.considered_hit = self.hit_serial;
        } else {
            self.consider_relocation(state, physics, player, destination);
        }
    }

    fn drive(&mut self, state: &Duel, physics: &PhysicsWorld, player: usize) -> Option<f32> {
        self.aim.refresh(Geometry::read(state, physics, player));
        // A dodging CPU answers a hit with a dodge instead of a move.
        if state.config.dodges(player) {
            if let Some(axis) = self.dodge(state, physics, player) {
                return Some(axis);
            }
        } else {
            if let Some(move_goal) = &mut self.relocation {
                let decision = move_goal.goal.decide_in(state, physics, player);
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
                    self.finish_relocation(state, physics, player, reason);
                    return Some(0.0);
                }
                self.mode = "relocating after hit";
                return Some(decision.axis);
            }
            if self.protect_correction(state, physics, player) {
                return Some(0.0);
            }
            if self.wants_relocation(state, player) {
                self.react_to_hit(state, physics, player);
                if self.protect_correction(state, physics, player) {
                    return Some(0.0);
                }
                if self.relocation.is_some() {
                    return self.drive(state, physics, player);
                }
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
        self.mode = if state.config.cpu_policy[player] == CpuPolicy::Plotted {
            self.plot.as_ref().map_or("waiting to plot", Plot::mode)
        } else if self.aim.bracket.confident {
            "aim acquired"
        } else {
            "ranging"
        };
        None
    }
}

impl Duel {
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

    pub(super) fn bot_drive(&mut self, physics: &mut PhysicsWorld, player: usize) -> Option<f32> {
        if !self.calibrated_cpu(player) {
            return None;
        }
        let mut bot = std::mem::take(&mut self.bots[player]);
        let axis = bot.drive(self, physics, player);
        if axis.is_some() {
            self.driving[player].diagnostics.mode = bot.mode;
        }
        self.bots[player] = bot;
        axis
    }

    fn escape_destination(
        &self,
        physics: &PhysicsWorld,
        player: usize,
        elevation: f32,
    ) -> Option<f32> {
        self.relocation_sites(physics, player, elevation)
            .into_iter()
            .filter(|site| site.rejection.is_none())
            .min_by(|a, b| a.score.unwrap().total_cmp(&b.score.unwrap()))
            .map(|site| site.x)
    }

    pub(super) fn bot_allows_fire(&self, player: usize) -> bool {
        !self.calibrated_cpu(player)
            || (self.bots[player].pending.is_none()
                && (!self.config.cpu_driving
                    || (self.bots[player].relocation.is_none() && !self.bots[player].dodging()))
                && (self.config.cpu_policy[player] != CpuPolicy::Plotted
                    || self.bots[player].plotted_arc().is_some()))
    }

    pub(super) fn aim_calibrated(&mut self, physics: &mut PhysicsWorld, player: usize) {
        if self.config.cpu_policy[player] == CpuPolicy::Plotted {
            self.aim_plotted(physics, player);
            return;
        }
        let geometry = Geometry::read(self, physics, player);
        let aim = &mut self.bots[player].aim;
        aim.refresh(geometry);
        self.tanks[player].facing = (geometry.target.x - geometry.shooter.x).signum();
        self.tanks[player].elevation = aim.bracket.elevation;
        self.tanks[player].speed = aim.bracket.power;
    }

    pub(super) fn record_launch(
        &mut self,
        physics: &mut PhysicsWorld,
        player: usize,
        origin: Vec2,
        velocity: Vec2,
        blocked_barrel: bool,
    ) -> Flight {
        let geometry = Geometry::read(self, physics, player);
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
            if let Some(plot) = &mut bot.plot {
                plot.fired = true;
            }
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
            // A plotted tank plots each shot afresh and learns nothing from one.
            if self.config.cpu_policy[shell.owner] == CpuPolicy::Plotted {
                bot.mode = "waiting to plot";
            } else {
                bot.aim.learn(&result);
                bot.mode = if bot.aim.bracket.confident {
                    "aim acquired"
                } else {
                    "ranging"
                };
            }
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

impl ScorchedState {
    #[cfg(test)]
    pub(super) fn bot_drive(&mut self, player: usize) -> Option<f32> {
        self.duel.bot_drive(&mut self.physics, player)
    }
    fn escape_destination(&self, player: usize, elevation: f32) -> Option<f32> {
        self.duel
            .escape_destination(&self.physics, player, elevation)
    }
    #[cfg(test)]
    pub(super) fn aim_calibrated(&mut self, player: usize) {
        self.duel.aim_calibrated(&mut self.physics, player)
    }
}
