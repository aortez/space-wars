//! Small firing-position adjustments layered on the ordinary range policy.
use super::*;

const CHECK_TICKS: u64 = 60;
const MOVE_DISTANCE: f32 = 2.5;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FootingTarget {
    pub x: f32,
    pub predicted_pitch_degrees: f32,
}

#[derive(Default, Clone, Serialize)]
pub(super) struct FootingDriver {
    target: Option<FootingTarget>,
    goal: Option<TravelGoal>,
    next_check: u64,
    started: u32,
    completed: u32,
    origin_x: f32,
    origin_pitch: f32,
    brake_since: Option<u64>,
    brake_attempted: bool,
    hold_until_shot: Option<u64>,
    rejected_at: Option<f32>,
}

impl Duel {
    /// A terrain-profile estimate, not a promise of future support. Native
    /// contacts and the current route are checked again while travelling.
    pub(crate) fn firing_footing_target(
        &self,
        physics: &PhysicsWorld,
        player: usize,
    ) -> Option<FootingTarget> {
        if !self.fighting()
            || self.config.course.is_some()
            || !self.has_track_support(physics, player)
        {
            return None;
        }
        let drive = &self.driving[player];
        if !matches!(drive.diagnostics.mode, "hold range" | "blocked ground")
            || drive.diagnostics.track_speed.abs() > 0.2
        {
            return None;
        }
        let motion = self.motion(physics, self.tanks[player].body).unwrap();
        let pitch = motion.angle.to_degrees().abs();
        if pitch < 20.0 || motion.linear_velocity.length() > 0.6 {
            return None;
        }
        let enemy_x = self
            .motion(physics, self.tanks[1 - player].body)
            .unwrap()
            .position
            .x;
        let toward = if enemy_x >= motion.position.x {
            1.0
        } else {
            -1.0
        };
        let mut best: Option<FootingTarget> = None;
        for direction in [toward, -toward] {
            let x = motion.position.x + direction * MOVE_DISTANCE;
            if x.abs() > self.placement.drive_limit()
                || (drive.diagnostics.mode == "hold range"
                    && !(18.0..=32.0).contains(&(enemy_x - x).abs()))
            {
                continue;
            }
            let ground = self.check_drive_direction(physics, player, direction);
            if ground.reason.is_some() {
                continue;
            }
            // The destination is the center of a three-unit footprint inside
            // the existing four-unit lookahead. Check its interior too, so a
            // dip between equal-height endpoints does not look like flat dirt.
            let left = ground.probes[2]?.point?;
            let right = ground.probes[8]?.point?;
            let grade = (right[1] - left[1]) / (right[0] - left[0]);
            let predicted_pitch = grade.abs().atan().to_degrees();
            let rough = ground.probes[2..].iter().flatten().any(|probe| {
                let p = probe.point.unwrap();
                (p[1] - (left[1] + grade * (p[0] - left[0]))).abs() > 0.35
            });
            if rough || predicted_pitch > 18.0 || predicted_pitch + 8.0 >= pitch {
                continue;
            }
            if best.is_none_or(|previous| predicted_pitch < previous.predicted_pitch_degrees) {
                best = Some(FootingTarget {
                    x,
                    predicted_pitch_degrees: predicted_pitch,
                });
            }
        }
        best
    }

    pub(super) fn seek_firing_footing(
        &mut self,
        physics: &mut PhysicsWorld,
        player: usize,
        range_axis: f32,
    ) -> f32 {
        let mut driver = self.driving[player].footing.take().unwrap_or_default();
        let axis = driver.decide(self, physics, player, range_axis);
        self.driving[player].footing = Some(driver);
        axis
    }

    fn better_firing_footing(
        &self,
        physics: &PhysicsWorld,
        player: usize,
        original_pitch: f32,
    ) -> bool {
        let pitch = self
            .motion(physics, self.tanks[player].body)
            .unwrap()
            .angle
            .to_degrees()
            .abs();
        if pitch > 18.0 || pitch + 8.0 > original_pitch || !self.has_track_support(physics, player)
        {
            return false;
        }
        let context = self.support_context(physics, player);
        self.running_gear[player].as_ref().is_none_or(|gear| {
            gear.wheel_colliders().iter().all(|c| {
                physics
                    .surface_contacts(*c)
                    .any(|contact| context.accepts(contact))
            })
        })
    }
}

impl FootingDriver {
    pub(super) fn suspend(&mut self) {
        if let Some(goal) = &mut self.goal {
            goal.suspend_progress();
        }
    }
    fn finish(&mut self, state: &mut Duel, player: usize) {
        self.goal = None;
        self.target = None;
        self.brake_since = None;
        self.completed += 1;
        self.next_check = state.tick + 2 * CHECK_TICKS;
        self.hold_until_shot = Some(state.tanks[player].shots);
        state.driving[player].next_plan = state.tick;
        state.driving[player].diagnostics.mode = "hold footing";
    }

    fn reject(&mut self, x: f32) {
        self.goal = None;
        self.target = None;
        self.brake_since = None;
        // Do not repeatedly reset a failed goal at the same location. Normal
        // range driving remains available; moving four units earns a new site.
        self.rejected_at = Some(x);
    }

    fn decide(
        &mut self,
        state: &mut Duel,
        physics: &PhysicsWorld,
        player: usize,
        range_axis: f32,
    ) -> f32 {
        let mode = state.driving[player].diagnostics.mode;
        let motion = state.motion(physics, state.tanks[player].body).unwrap();
        if self
            .rejected_at
            .is_some_and(|x| (motion.position.x - x).abs() >= 4.0)
        {
            self.rejected_at = None;
        }
        if matches!(mode, "aim and fire" | "no support" | "cooldown") {
            self.suspend();
            return range_axis;
        }
        if let Some(shots) = self.hold_until_shot {
            if shots == state.tanks[player].shots && mode != "make room" {
                state.driving[player].cpu_direction = 0.0;
                state.driving[player].diagnostics.mode = "hold footing";
                return 0.0;
            }
            self.hold_until_shot = None;
        }
        if self.goal.is_none()
            && self.rejected_at.is_none()
            && state.tick >= self.next_check
            && range_axis == 0.0
        {
            self.next_check = state.tick + CHECK_TICKS;
            if let Some(target) = state.firing_footing_target(physics, player) {
                self.target = Some(target);
                self.goal = Some(TravelGoal::new(target.x, DRIVE_SPEED));
                self.started += 1;
                self.origin_x = motion.position.x;
                self.origin_pitch = motion.angle.to_degrees().abs();
                self.brake_attempted = false;
            }
        }
        let Some(goal) = &mut self.goal else {
            return range_axis;
        };
        let direction = (self.target.unwrap().x - motion.position.x).signum();
        // Spacing takes priority if an opponent approaches the old destination.
        if mode == "make room" && direction * range_axis < 0.0 {
            goal.suspend_progress();
            return range_axis;
        }
        let decision = goal.decide_in(state, physics, player);
        let better = state.better_firing_footing(physics, player, self.origin_pitch);
        state.driving[player].cpu_direction = 0.0;
        if self
            .brake_since
            .is_some_and(|tick| state.tick >= tick + CHECK_TICKS)
        {
            self.brake_since = None;
        }
        // Brake once on a promising patch encountered before the predicted
        // destination. A transient pitch change alone never completes the move.
        if !self.brake_attempted
            && better
            && (motion.position.x - self.origin_x).abs() >= 0.75
            && matches!(
                decision.mode,
                "travel" | "climb" | "rough ground" | "cruise" | "approach goal" | "braking"
            )
        {
            self.brake_since = Some(state.tick);
            self.brake_attempted = true;
        }
        if self.brake_since.is_some() {
            goal.suspend_progress();
            state.driving[player].diagnostics.mode = "settle footing";
            if better
                && motion.linear_velocity.length() < 0.15
                && motion.angular_velocity.abs() < 0.15
            {
                self.finish(state, player);
            }
            return 0.0;
        }
        if decision.mode == "arrived" {
            if better && motion.angular_velocity.abs() < 0.15 {
                self.finish(state, player);
            } else {
                self.reject(motion.position.x);
                state.driving[player].diagnostics.mode = "footing unsuitable";
            }
            return 0.0;
        }
        if decision.mode == "stalled" {
            self.reject(motion.position.x);
            state.driving[player].diagnostics.mode = "footing stalled";
            return 0.0;
        }
        state.driving[player].diagnostics.mode = match decision.mode {
            "blocked ground" => "footing blocked",
            "cooldown" => "footing cooldown",
            "retreat" | "retreat brake" => "footing retry",
            _ => "seek footing",
        };
        decision.axis
    }
}

#[cfg(test)]
mod tests;

impl ScorchedState {
    pub(crate) fn firing_footing_target(&self, player: usize) -> Option<FootingTarget> {
        self.duel.firing_footing_target(&self.physics, player)
    }

    #[cfg(test)]
    fn better_firing_footing(&self, player: usize, original_pitch: f32) -> bool {
        self.duel
            .better_firing_footing(&self.physics, player, original_pitch)
    }
}
