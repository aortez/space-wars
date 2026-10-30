//! Bounded near-miss recovery, shared by both automatic movement profiles.
//! Select one actual edge; the shared actuator still owns all clearance/motion.
use super::*;
use engine_common::{ClockDuckAutoLedgeOutcome as Outcome, ClockDuckAutoLedgeState};

pub(super) const HANG_TICKS: u32 = 18;
pub(super) const ATTEMPT_TICKS: u32 = 180;
pub(super) const COOLDOWN_TICKS: u32 = 60;
pub(super) const MAX_ATTEMPTS: u32 = 3;

#[derive(Default)]
pub(in crate::events::duck) struct Automatic {
    pub stats: ClockDuckAutoLedgeState,
    caught: bool,
    climbs_before: u32,
}

impl DuckEvent {
    pub(in crate::events::duck) fn stop_automatic_ledge(&mut self) {
        let auto = &mut self.ledge.automatic;
        if auto.stats.active {
            auto.stats.active = false;
            auto.stats.target_body = None;
            auto.stats.outcome = Some(Outcome::TakenOver);
            auto.caught = false;
        }
    }

    pub(in crate::events::duck) fn decide_automatic_ledge(&mut self) {
        if self.player.is_some() {
            return;
        }
        let auto = &mut self.ledge.automatic.stats;
        auto.candidates_checked = 0;
        auto.cooldown_ticks = auto.cooldown_ticks.saturating_sub(1);
        if auto.active {
            auto.attempt_ticks += 1;
            let stop = if self.water_report.submerged_fraction >= 0.15 {
                Some(Outcome::Wet)
            } else if self.tick >= DUCK_TICKS - RESET_TICKS - 1 {
                Some(Outcome::Deadline)
            } else if self.ledge.automatic.stats.attempt_ticks >= ATTEMPT_TICKS {
                Some(Outcome::TimedOut)
            } else if self.ledge.stats.reason == Some(Reason::ClimbBlocked) {
                Some(Outcome::Blocked)
            } else {
                None
            };
            if let Some(outcome) = stop {
                self.ledge.automatic.stats.outcome = Some(outcome);
                self.set_ledge_command(LedgeCommand::Release);
            } else if self.ledge.stats.phase == Phase::Hanging {
                self.ledge.automatic.stats.hang_ticks += 1;
                if self.ledge.automatic.stats.hang_ticks >= HANG_TICKS {
                    self.set_ledge_command(LedgeCommand::Climb);
                }
            }
            return;
        }
        if self.ledge_attached()
            || self.ledge.detour.active()
            || self.grounded()
            || !self.controller.ledge_ready()
            || self.water_report.submerged_fraction >= 0.15
            || self.ledge.stats.cooldown_ticks > 0
            || self.ledge.automatic.stats.cooldown_ticks > 0
            || self.ledge.automatic.stats.attempts >= MAX_ATTEMPTS
            || self.tick + u64::from(ATTEMPT_TICKS) >= DUCK_TICKS - RESET_TICKS
        {
            return;
        }
        let Some(edge) = self.automatic_ledge_target() else {
            return;
        };
        self.start_automatic_ledge(edge);
    }

    pub(super) fn start_automatic_ledge(&mut self, edge: Edge) {
        self.set_ledge_command(LedgeCommand::Catch);
        // A failed clearance test must not silently catch an unrelated edge.
        self.ledge.catch_target = Some(edge);
        let auto = &mut self.ledge.automatic;
        auto.stats.active = true;
        auto.stats.target_body = Some(edge.body.entity.value());
        auto.stats.attempts += 1;
        auto.stats.attempt_ticks = 1;
        auto.stats.hang_ticks = 0;
        auto.stats.outcome = None;
        auto.caught = false;
        auto.climbs_before = self.ledge.stats.climbs;
    }

    fn automatic_ledge_target(&mut self) -> Option<Edge> {
        let world = self.world.as_ref().unwrap();
        let motion = world.motion(DUCK_BODY).unwrap();
        let caps = self.controller.capabilities().unwrap();
        let gravity = 8.0 * caps.height / caps.flight.powi(2);
        let apex = motion.position.y + motion.linear_velocity.y.max(0.0).powi(2) / (2.0 * gravity);
        let direction = if self.controller.navigator.flight_tick.is_some() {
            // A near miss belongs to the committed flight, even when the exit
            // opens behind it. Finish reaching safe support before rerouting.
            self.controller.direction
        } else if self.exit_visible() {
            1.0
        } else {
            self.controller.facing()
        };
        let target = if self.controller.navigator.flight_tick.is_some() {
            self.controller.navigator.plan.map(|p| p.target)
        } else {
            self.controller.recovery.stats.target_surface
        };
        let mut best = None;
        let mut distance = f32::INFINITY;
        for edge in edges(self).into_iter().flatten() {
            self.ledge.automatic.stats.candidates_checked += 1;
            if edge.outward * self.direction * direction >= 0.0
                || target.is_some_and(|index| {
                    edge.body.entity.value()
                        != crate::arena::ids::canonical::COURSE.start + index as u64
                })
                || !edge.in_reach(self)
            {
                continue;
            }
            let anchor = edge.anchor(world).unwrap();
            // A jump still high enough to clear the top keeps its ordinary
            // landing plan. Only an edge-only near miss requests an attachment.
            if apex >= anchor.y + self.radius * 1.05 {
                continue;
            }
            let candidate_distance = (anchor - motion.position).length_squared();
            if candidate_distance < distance {
                distance = candidate_distance;
                best = Some(edge);
            }
        }
        best
    }

    /// Called around the solve so plans end on a real attachment and climb
    /// successes are counted only after the actuator confirms support contact.
    pub(in crate::events::duck) fn observe_automatic_ledge(&mut self) {
        if self.player.is_some() || !self.ledge.automatic.stats.active {
            return;
        }
        if self.ledge_attached() {
            if !self.ledge.automatic.caught {
                self.controller.interrupt_for_ledge();
                self.ledge.automatic.caught = true;
            }
            return;
        }
        let auto = &mut self.ledge.automatic;
        if self.ledge.stats.climbs > auto.climbs_before {
            auto.stats.completed += 1;
            auto.stats.outcome = Some(Outcome::Climbed);
            self.controller.behavior = engine_common::ClockDuckBehavior::Landing;
        } else {
            auto.stats.failed += 1;
            auto.stats.outcome.get_or_insert(if auto.caught {
                Outcome::Interrupted
            } else {
                Outcome::Missed
            });
        }
        auto.stats.active = false;
        auto.stats.target_body = None;
        auto.stats.cooldown_ticks = COOLDOWN_TICKS;
        auto.caught = false;
    }
}
