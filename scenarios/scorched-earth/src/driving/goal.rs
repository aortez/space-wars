//! Travel toward a retained destination using ordinary drive input;
//! the native support gate, acceleration ramp and motor limits remain in charge.
use super::*;
use crate::driving_lab::SupportCheck;

const MAX_SPEED: f32 = 2.5;
const ARRIVAL_DISTANCE: f32 = 0.20;
const RETRY_TICKS: u64 = 180;
const RETRY_BUDGET: u8 = 2;
const RETREAT_DISTANCE: f32 = 4.0;
const RETREAT_TICKS: u64 = 360;

#[derive(Debug, Clone, Serialize)]
struct Retreat {
    destination_x: Option<f32>,
    direction: f32,
    deadline: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TravelGoal {
    destination_x: f32,
    cruise_speed: f32,
    best_distance: Option<f32>,
    progress_window: Option<(u64, f32)>,
    stalled_windows: u8,
    retries: u8,
    retry_at: u64,
    exhausted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    retreat: Option<Retreat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt_best_distance: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalDecision {
    pub destination_x: f32,
    pub distance: f32,
    /// Desired and actual speed along the sampled route, toward the goal.
    pub desired_speed: f32,
    pub actual_speed: f32,
    pub axis: f32,
    pub mode: &'static str,
    pub retries: u8,
}

impl TravelGoal {
    /// Intentional combat stops do not count as failed progress. Retain the
    /// target, best progress, cooldown and retries; retreat deadlines still
    /// expire in world time instead of extending an attempt indefinitely.
    pub(crate) fn suspend_progress(&mut self) {
        self.progress_window = None;
        self.stalled_windows = 0;
    }

    pub fn new(destination_x: f32, cruise_speed: f32) -> Self {
        assert!(destination_x.is_finite() && destination_x.abs() <= 44.0);
        assert!(cruise_speed.is_finite() && cruise_speed > 0.0 && cruise_speed <= MAX_SPEED);
        Self {
            destination_x,
            cruise_speed,
            best_distance: None,
            progress_window: None,
            stalled_windows: 0,
            retries: 0,
            retry_at: 0,
            exhausted: false,
            retreat: None,
            attempt_best_distance: None,
        }
    }

    /// Inspect current terrain and motion without advancing or resetting the
    /// scenario. Keep this controller across ticks, including terrain changes.
    pub fn decide(&mut self, state: &ScorchedState, player: usize) -> GoalDecision {
        self.decide_with_support(state, player, SupportCheck::Current)
    }

    /// Same goal history and speed policy with an explicitly selected support
    /// check. Used to continue a retained failure without resetting its goal.
    pub fn decide_with_support(
        &mut self,
        state: &ScorchedState,
        player: usize,
        support: SupportCheck,
    ) -> GoalDecision {
        self.decide_policy(state, player, support, true)
    }

    /// Retained speed/retry policy for exact prefixes and controlled comparisons.
    pub fn decide_reference(
        &mut self,
        state: &ScorchedState,
        player: usize,
        support: SupportCheck,
    ) -> GoalDecision {
        self.decide_policy(state, player, support, false)
    }

    fn decide_policy(
        &mut self,
        state: &ScorchedState,
        player: usize,
        support: SupportCheck,
        candidate: bool,
    ) -> GoalDecision {
        assert!(player < 2);
        let motion = state.physics.motion(state.tanks[player].body).unwrap();
        let offset = self.destination_x - motion.position.x;
        let direction = if offset < 0.0 { -1.0 } else { 1.0 };
        let distance = offset.abs();
        let best = self
            .best_distance
            .map_or(distance, |previous| previous.min(distance));
        self.best_distance = Some(best);
        let mut decision = GoalDecision {
            destination_x: self.destination_x,
            distance,
            desired_speed: 0.0,
            actual_speed: motion.linear_velocity.x * direction,
            axis: 0.0,
            mode: "travel",
            retries: self.retries,
        };
        if self.exhausted {
            decision.mode = "stalled";
            return decision;
        }
        if distance <= ARRIVAL_DISTANCE {
            if candidate && !support.supported(state, player) {
                decision.mode = "no support";
                return decision;
            }
            decision.mode = if motion.linear_velocity.length() < 0.15 {
                "arrived"
            } else {
                "braking"
            };
            self.progress_window = None;
            self.stalled_windows = 0;
            return decision;
        }
        if state.tick < self.retry_at {
            decision.mode = "cooldown";
            return decision;
        }
        if candidate
            && self
                .retreat
                .as_ref()
                .is_some_and(|r| state.tick >= r.deadline)
        {
            self.retreat = None;
            decision.mode = "retreat timeout";
            return decision;
        }
        if !support.supported(state, player) {
            decision.mode = "no support";
            return decision;
        }
        if candidate && let Some(retreat) = &mut self.retreat {
            if state
                .check_drive_direction(player, retreat.direction)
                .reason
                .is_some()
            {
                self.retreat = None;
                decision.mode = "retreat blocked";
                return decision;
            }
            // Gravity may move the tank during the cooldown. Measure the
            // run-up from its actual resting position, not the earlier stall.
            let target = *retreat
                .destination_x
                .get_or_insert(motion.position.x + retreat.direction * RETREAT_DISTANCE);
            let remaining = (target - motion.position.x) * retreat.direction;
            if remaining <= ARRIVAL_DISTANCE {
                decision.mode = "retreat brake";
                if motion.linear_velocity.length() < 0.15 {
                    self.retreat = None;
                    self.progress_window = None;
                    // A paid retry needs time to cover its deliberate run-up.
                    // Retain the overall best and retry budget; measure forward
                    // progress monotonically within this attempt as well.
                    self.attempt_best_distance = Some(distance);
                }
            } else {
                decision.mode = "retreat";
                decision.actual_speed = motion.linear_velocity.x * retreat.direction;
                decision.desired_speed = self.cruise_speed.min(1.5).min(remaining * 1.5);
                let request = (decision.desired_speed
                    + 0.75 * (decision.desired_speed - decision.actual_speed))
                    .clamp(0.0, self.cruise_speed.min(1.5));
                decision.axis = retreat.direction * request / MAX_SPEED;
            }
            return decision;
        }
        let ground = state.check_drive_direction(player, direction);
        if ground.reason.is_some() {
            decision.mode = "blocked ground";
            self.progress_window = None;
            self.stalled_windows = 0;
            return decision;
        }

        let progress = if candidate {
            let attempt = self.attempt_best_distance.get_or_insert(best);
            *attempt = attempt.min(distance);
            *attempt
        } else {
            best
        };

        if let Some((tick, previous_distance)) = self.progress_window {
            if state.tick >= tick + u64::from(FIXED_HZ) {
                // Progress toward the destination, not absolute motion: rocking
                // or rolling backward must not be mistaken for a useful trip.
                let minimum_progress = (self.cruise_speed * 0.25).min(0.10);
                self.stalled_windows = if previous_distance - progress < minimum_progress {
                    self.stalled_windows + 1
                } else {
                    0
                };
                self.progress_window = Some((state.tick, progress));
                if self.stalled_windows >= 3 {
                    self.stalled_windows = 0;
                    self.progress_window = None;
                    if self.retries < RETRY_BUDGET {
                        self.retries += 1;
                        self.retry_at = state.tick + RETRY_TICKS;
                        if candidate
                            && state
                                .check_drive_direction(player, -direction)
                                .reason
                                .is_none()
                        {
                            self.retreat = Some(Retreat {
                                destination_x: None,
                                direction: -direction,
                                deadline: self.retry_at + RETREAT_TICKS,
                            });
                        }
                        decision.mode = "cooldown";
                    } else {
                        self.exhausted = true;
                        decision.mode = "stalled";
                    }
                    decision.retries = self.retries;
                    return decision;
                }
            }
        } else {
            self.progress_window = Some((state.tick, progress));
        }

        let points: Vec<_> = ground
            .probes
            .iter()
            .flatten()
            .filter_map(|p| p.point)
            .collect();
        let first = points.first().unwrap();
        let last = points.last().unwrap();
        let grade = (last[1] - first[1]) / (last[0] - first[0]);
        let uneven = points.windows(2).any(|p| (p[1][1] - p[0][1]).abs() > 0.08);
        // Keep momentum until the chassis clears the crest as well as the
        // lookahead. A falling profile ahead does not mean the rear wheel has
        // finished climbing the current step.
        let climbing = candidate
            && (grade * direction > 0.08
                || motion.angle.sin() * direction > 5.0_f32.to_radians().sin());
        let terrain_speed = if uneven && !climbing {
            self.cruise_speed.min(1.5)
        } else {
            self.cruise_speed
        };
        // A proportional approach reduces speed smoothly before the goal.
        decision.desired_speed = terrain_speed.min(distance * 1.5);
        decision.actual_speed = (motion.linear_velocity.x + grade * motion.linear_velocity.y)
            * direction
            / (1.0 + grade * grade).sqrt();
        let error = decision.desired_speed - decision.actual_speed;
        // Wheel velocity already has native motor feedback. This outer loop
        // adjusts its request using chassis progress, with no accumulated error
        // that could wind up while a wheel is stuck or loses contact.
        let request = (decision.desired_speed + 0.75 * error).clamp(0.0, MAX_SPEED);
        decision.axis = direction * request / MAX_SPEED;
        decision.mode = if distance * 1.5 < terrain_speed {
            "approach goal"
        } else if climbing {
            "climb"
        } else if uneven {
            "rough ground"
        } else {
            "cruise"
        };
        decision
    }
}

#[cfg(test)]
mod tests;
