//! One early entrance detour, before calibration. Commands use the ordinary
//! actuator; actual attachment/contact alone can hold the door open.
use super::*;
use engine_common::{
    ClockDuckFlapDetourOutcome as Outcome, ClockDuckFlapDetourPhase as DetourPhase,
    ClockDuckFlapDetourState,
};

mod direct;
mod plan;
use plan::Return;

// Includes approach, shared hang/climb, a readable perch and the return flight.
pub(super) const BUDGET_TICKS: u32 = 240;
pub(super) const PERCH_TICKS: u32 = 36;

#[derive(Default)]
pub(in crate::events::duck) struct Detour {
    pub stats: ClockDuckFlapDetourState,
    // Independent of visit phase changes when the exit opens.
    pub(super) age: u64,
    edge: Option<Edge>,
    landing: Option<Return>,
    launched: bool,
    return_airborne: bool,
    direct: Option<direct::Plan>,
}

impl Detour {
    pub fn active(&self) -> bool {
        !matches!(
            self.stats.phase,
            DetourPhase::Pending | DetourPhase::Finished
        )
    }
}

pub(super) fn selected(seed: u64) -> bool {
    // Independent stream: selecting a detour never rerolls the course/profile.
    StdRng::seed_from_u64(seed ^ 0x464c_4150_5649_5349).random_ratio(1, 5)
}

impl DuckEvent {
    pub(crate) fn select_direct_flap_landing(&mut self, enabled: bool) {
        assert_eq!(self.tick, 0);
        self.ledge.detour.stats.direct_landing = enabled;
    }

    /// Older timed collision/calibration fixtures retain their original route.
    /// Detour fixtures separately exercise the new choice and interruptions.
    #[cfg(test)]
    pub(crate) fn skip_flap_detour_for_fixture(&mut self) {
        assert_eq!(self.tick, 0);
        self.ledge.detour.stats.phase = DetourPhase::Finished;
        self.ledge.detour.stats.outcome = Some(Outcome::NotSelected);
    }

    pub(in crate::events::duck) fn stop_flap_detour(&mut self) {
        if self.ledge.detour.active() {
            // Takeover retains the real attachment, contact and velocity.
            self.ledge.detour.stats.phase = DetourPhase::Finished;
            self.ledge.detour.stats.outcome = Some(Outcome::TakenOver);
            self.ledge.detour.stats.clearance_queries = 0;
        }
    }

    fn finish_flap_detour(&mut self, outcome: Outcome) {
        self.ledge.detour.stats.phase = DetourPhase::Finished;
        self.ledge.detour.stats.outcome = Some(outcome);
        if self.ledge_attached() {
            self.set_ledge_command(LedgeCommand::Release);
        }
        self.controller.interrupt_for_ledge();
        self.controller.direction = 1.0;
    }

    pub(in crate::events::duck) fn decide_flap_detour(
        &mut self,
        world: &PhysicsWorld,
        water: Option<&WaterWorld>,
    ) -> Option<Command> {
        self.ledge.detour.stats.clearance_queries = 0;
        if self.player.is_some() || self.ledge.detour.stats.phase == DetourPhase::Finished {
            return None;
        }
        self.ledge.detour.age += 1;
        if self.ledge.detour.stats.phase == DetourPhase::Pending && !selected(self.seed) {
            self.finish_flap_detour(Outcome::NotSelected);
            return None;
        }
        if self.water_report.submerged_fraction > 0.05 {
            self.finish_flap_detour(Outcome::Wet);
            return None;
        }
        let departing = self.exit_visible();
        if self.tick + u64::from(BUDGET_TICKS) >= DUCK_TICKS - RESET_TICKS
            || (departing
                && (!self.ledge.detour.active()
                    || (self.ledge.detour.stats.phase == DetourPhase::Approaching
                        && !self.ledge.detour.launched)))
        {
            self.finish_flap_detour(Outcome::Deadline);
            return None;
        }
        if self.ledge.detour.active() {
            self.ledge.detour.stats.ticks += 1;
            if self.ledge.detour.stats.ticks >= BUDGET_TICKS {
                self.finish_flap_detour(Outcome::TimedOut);
                return None;
            }
            let landing = self.ledge.detour.landing.unwrap();
            if !landing.valid(self, world) || !landing.dry(water, self.radius) {
                self.finish_flap_detour(Outcome::Blocked);
                return None;
            }
        }
        let still = || Command {
            gait: Gait::Still,
            direction: 1.0,
            jump: false,
        };
        if self.ledge.detour.stats.direct_landing
            && matches!(
                self.ledge.detour.stats.phase,
                DetourPhase::Pending | DetourPhase::Approaching
            )
        {
            return self.decide_direct_flap_approach(world, water);
        }
        match self.ledge.detour.stats.phase {
            DetourPhase::Pending => {
                // Let the ordinary spawn settle. Never start a calibration
                // jump while the selected entrance opportunity is pending.
                if self.ledge.detour.age > 6 {
                    self.finish_flap_detour(Outcome::Expired);
                    return None;
                }
                if !self.grounded(world) {
                    return Some(still());
                }
                let Some((edge, landing)) = self.plan_flap_detour(world, water) else {
                    self.finish_flap_detour(Outcome::Unsafe);
                    return None;
                };
                self.controller.interrupt_for_ledge();
                let detour = &mut self.ledge.detour;
                detour.edge = Some(edge);
                detour.landing = Some(landing);
                detour.stats.phase = DetourPhase::Approaching;
                detour.stats.attempts = 1;
                detour.stats.ticks = 1;
            }
            DetourPhase::Recovering => {
                if self.ledge.automatic.stats.active {
                    return Some(still());
                }
                if self.ledge.automatic.stats.outcome
                    != Some(engine_common::ClockDuckAutoLedgeOutcome::Climbed)
                {
                    self.finish_flap_detour(Outcome::Interrupted);
                    return None;
                }
                self.ledge.detour.stats.phase = DetourPhase::Perching;
                self.controller.direction = 1.0;
            }
            _ => {}
        }
        match self.ledge.detour.stats.phase {
            DetourPhase::Approaching => {
                let edge = self.ledge.detour.edge.unwrap();
                if !edge.valid(self, world) || self.ledge.detour.age > 20 {
                    self.finish_flap_detour(Outcome::Expired);
                    return None;
                }
                if edge.in_reach(self, world) && !self.grounded(world) {
                    self.start_automatic_ledge(edge);
                    self.ledge.detour.stats.phase = DetourPhase::Recovering;
                    return Some(still());
                }
                self.controller.direction = -1.0;
                let jump = !self.ledge.detour.launched && self.grounded(world);
                self.ledge.detour.launched |= jump;
                Some(self.detour_approach(self.position(world).unwrap(), jump))
            }
            DetourPhase::Perching => {
                if !self.standing_on(world, flaps::body(0)) {
                    self.finish_flap_detour(Outcome::Interrupted);
                    return None;
                }
                let landing = self.ledge.detour.landing.unwrap();
                self.ledge.detour.stats.perch_ticks += 1;
                if !departing && self.ledge.detour.stats.perch_ticks < PERCH_TICKS {
                    if let Some(plan) = self.ledge.detour.direct {
                        return Some(self.direct_position_command(world, plan.perch_x, false));
                    }
                    return Some(still());
                }
                let motion = world.motion(DUCK_BODY).unwrap();
                if !self.detour_return_clear(
                    world,
                    motion.position,
                    motion.linear_velocity,
                    landing,
                ) {
                    self.finish_flap_detour(Outcome::Blocked);
                    return None;
                }
                self.ledge.detour.stats.phase = DetourPhase::Returning;
                Some(self.detour_return_command(motion.position, landing, true))
            }
            DetourPhase::Returning => {
                let landing = self.ledge.detour.landing.unwrap();
                self.ledge.detour.return_airborne |= !self.grounded(world);
                let motion = world.motion(DUCK_BODY).unwrap();
                if self.ledge.detour.return_airborne && self.grounded(world) {
                    let success = self.standing_on(world, landing.body)
                        && (motion.position.x - landing.center.x).abs() < self.radius;
                    self.finish_flap_detour(if success {
                        Outcome::Returned
                    } else {
                        Outcome::Interrupted
                    });
                    return None;
                }
                Some(self.detour_return_command(motion.position, landing, false))
            }
            _ => Some(still()),
        }
    }

    fn standing_on(&self, world: &PhysicsWorld, body: BodyId) -> bool {
        world.surface_contacts(DUCK_COLLIDER).any(|c| {
            c.collider.entity == body.entity
                && c.normal.y > 0.95
                && c.separation <= self.radius * 0.05
        })
    }

    fn detour_approach(&self, local: Vec2, jump: bool) -> Command {
        Command {
            gait: if local.x > self.radius * 5.6 {
                Gait::Pace(0.3)
            } else {
                Gait::Still
            },
            direction: -1.0,
            jump,
        }
    }

    fn detour_return_command(&self, position: Vec2, landing: Return, jump: bool) -> Command {
        let distance = (landing.center.x - position.x) * self.direction;
        Command {
            gait: Gait::Pace(
                (distance.abs() * 6.0).min(self.radius * 8.0) / self.movement.run_speed,
            ),
            direction: distance.signum(),
            jump,
        }
    }
}
