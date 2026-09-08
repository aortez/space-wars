//! Material combat mission: fly, engage occupied ships, recover and rejoin.
//! Uses the Spacewars combat solution and the same RecoverShipTask as V3.
use crate::{
    BrainReset, RuleShipBrainConfig, combat_solution,
    flight_pilot::FlightIntent,
    recovery_task::{RecoverShipTask, RecoveryTelemetry, TaskStatus},
    shortest_heading_error,
};
use engine_common::Action;
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceWingAction,
        combat::{CombatObservationV1, SurfaceWeaponAction},
        pilot::LandingSiteId,
    },
};
use serde::Serialize;

pub const RULE_PILOT_V4_POLICY_ID: &str = "rule_pilot_v4";
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CombatIntent {
    pub flight: FlightIntent,
    pub weapons: SurfaceWeaponAction,
}
impl CombatIntent {
    pub fn encode(self, owner: PlayerId) -> [Action; 3] {
        let [controls, wings] = self.flight.encode(owner);
        [controls, wings, self.weapons.encode(owner)]
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CombatPilotTelemetry {
    pub policy: &'static str,
    pub goal: &'static str,
    pub target: Option<PlayerId>,
    pub engagement_ticks: u64,
    pub completed_recoveries: u32,
    pub combat_returns: u32,
    pub rejoined_tick: Option<u64>,
    pub recovery: Option<RecoveryTelemetry>,
}
#[derive(Debug, Clone)]
pub struct RulePilotV4 {
    context: BrainReset,
    task: Option<RecoverShipTask>,
    telemetry: CombatPilotTelemetry,
    seen_losses: u64,
    previous_tick: Option<u64>,
    previous_intent: CombatIntent,
    pwm: f32,
    clearing_ground: bool,
    awaiting_rejoin: bool,
}
impl RulePilotV4 {
    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            task: None,
            seen_losses: 0,
            previous_tick: None,
            previous_intent: CombatIntent::default(),
            pwm: 0.0,
            clearing_ground: false,
            awaiting_rejoin: false,
            telemetry: CombatPilotTelemetry {
                policy: RULE_PILOT_V4_POLICY_ID,
                goal: "takeoff",
                target: None,
                engagement_ticks: 0,
                completed_recoveries: 0,
                combat_returns: 0,
                rejoined_tick: None,
                recovery: None,
            },
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context);
    }
    pub fn telemetry(&self) -> &CombatPilotTelemetry {
        &self.telemetry
    }
    pub fn label(&self) -> &'static str {
        self.telemetry.goal
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        // During flight only one material site is sampled; recovery owns its
        // full initial survey and subsequent site revalidation.
        self.task.as_ref().map_or(
            Some(LandingSiteId {
                planet: 0,
                bearing: 0,
            }),
            |t| t.site_request(),
        )
    }
    pub fn intent(&mut self, o: &CombatObservationV1) -> CombatIntent {
        let p = &o.recovery.flight.pilot;
        if o.version != 1
            || o.recovery.version != 1
            || o.recovery.flight.version != 2
            || p.version != 1
            || p.owner != self.context.actor
        {
            return CombatIntent::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_intent;
        }
        let losses = p.recovery.as_ref().map_or(0, |r| r.ships_lost);
        if losses > self.seen_losses
            || self.task.is_none()
                && (!p.ship_available
                    || p.ship_form == ShipForm::EscapePod
                    || p.location == PilotLocation::OnFoot)
        {
            self.task = Some(RecoverShipTask::new(self.context));
            self.awaiting_rejoin = false;
        }
        self.seen_losses = losses;
        let intent = if let Some(task) = &mut self.task {
            let flight = task.step(&o.recovery);
            let t = task.telemetry();
            self.telemetry.goal = t.reason.unwrap_or(t.goal.label());
            self.telemetry.recovery = Some(t.clone());
            if t.status == TaskStatus::Succeeded {
                self.telemetry.completed_recoveries += 1;
                self.awaiting_rejoin = true;
                self.task = None;
            }
            CombatIntent {
                flight,
                ..Default::default()
            }
        } else if !p.controls_armed || !p.queries_ready {
            CombatIntent::default()
        } else {
            self.fly(o)
        };
        self.previous_tick = Some(p.tick);
        self.previous_intent = intent;
        intent
    }
    fn fly(&mut self, o: &CombatObservationV1) -> CombatIntent {
        let p = &o.recovery.flight.pilot;
        let delta = p.ship.position - p.planet.motion.position;
        let radius = delta.length();
        let up = delta.normalized();
        let velocity = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let altitude = radius - p.planet.radius;
        let falling = (-velocity.dot(up)).max(0.0);
        self.telemetry.target = o.target.map(|t| t.owner);
        // The conservative routing bound is never a landing/support claim.
        // Reserve braking and turning room before turning inward to fire.
        if altitude < 40.0 + falling * 2.0 + falling * falling / 60.0 {
            self.clearing_ground = true;
        }
        if altitude > 85.0 && falling < 4.0 {
            self.clearing_ground = false;
        }
        if self.clearing_ground {
            self.telemetry.goal = "climb clear of ground";
            return self.guide(o, up * 16.0);
        }
        if let Some(target) = o.target {
            let relative = target.motion.position - p.ship.position;
            let local = relative.rotate_radians(-p.ship.angle);
            let local_velocity =
                (target.motion.velocity - p.ship.velocity).rotate_radians(-p.ship.angle);
            let config = RuleShipBrainConfig {
                arrival_distance: 55.0,
                fast_pursuit_distance: 180.0,
                laser_range: 250.0,
                cannon_min_range: 20.0,
                cannon_max_range: 220.0,
                ..Default::default()
            };
            let solution = combat_solution(
                local,
                local_velocity,
                p.ship.spin,
                o.laser_available,
                o.cannon_ready,
                &config,
            );
            if !target.ground_occluded && solution.distance < 260.0 {
                self.telemetry.goal = "engage ship";
                self.telemetry.engagement_ticks += 1;
                if self.awaiting_rejoin
                    && target.visible
                    && (solution.intent.laser || solution.intent.cannon)
                {
                    self.telemetry.rejoined_tick = Some(p.tick);
                    self.telemetry.combat_returns += 1;
                    self.awaiting_rejoin = false;
                }
                let aim = solution.aim.rotate_radians(p.ship.angle).normalized();
                let closing =
                    -(target.motion.velocity - p.ship.velocity).dot(relative.normalized());
                let brake = velocity.length() > 30.0 || solution.distance < 70.0 || closing > 35.0;
                return CombatIntent {
                    flight: FlightIntent {
                        controls: SurfaceSortieAction {
                            horizontal: turn(o, aim),
                            primary_held: solution.intent.thrust > 0.0 && !brake,
                            brake_held: brake,
                            interact_held: false,
                        },
                        wings: SurfaceWingAction {
                            closed: solution.intent.wings_closed && !brake && altitude > 90.0,
                        },
                    },
                    weapons: SurfaceWeaponAction {
                        laser: target.visible && solution.intent.laser,
                        cannon: target.visible && solution.intent.cannon,
                    },
                };
            }
            self.telemetry.goal = "route around planet";
            let target_up = (target.motion.position - p.planet.motion.position).normalized();
            let side = if up.x * target_up.y - up.y * target_up.x >= 0.0 {
                1.0
            } else {
                -1.0
            };
            return self.guide(
                o,
                Vec2::new(-up.y, up.x) * side * 25.0
                    + up * ((p.planet.radius + 100.0 - radius) * 0.6).clamp(-16.0, 16.0),
            );
        }
        self.telemetry.goal = "patrol / opponent recovering";
        let side = if self.context.actor.index() == 0 {
            1.0
        } else {
            -1.0
        };
        self.guide(
            o,
            Vec2::new(-up.y, up.x) * side * 12.0
                + up * ((p.planet.radius + 90.0 - radius) * 0.6).clamp(-12.0, 12.0),
        )
    }
    fn guide(&mut self, o: &CombatObservationV1, relative_velocity: Vec2) -> CombatIntent {
        let f = &o.recovery.flight;
        let p = &f.pilot;
        let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let acceleration =
            (relative_velocity - relative) * 1.5 - p.gravity - f.flight.limits.braking(relative);
        let aligned = Vec2::Y
            .rotate_radians(p.ship.angle)
            .dot(acceleration.normalized())
            > 0.97;
        let available = f.flight.limits.thrust_acceleration
            * f.flight.limits.thrust_fraction(f.flight.forward_speed);
        let duty = if aligned && available > 0.01 {
            (acceleration.length() / available).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.pwm = (self.pwm + duty).min(2.0);
        let thrust = self.pwm >= 1.0;
        if thrust {
            self.pwm -= 1.0;
        }
        CombatIntent {
            flight: FlightIntent {
                controls: SurfaceSortieAction {
                    horizontal: turn(o, acceleration.normalized()),
                    primary_held: thrust,
                    brake_held: true,
                    interact_held: false,
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
fn turn(o: &CombatObservationV1, direction: Vec2) -> f32 {
    let f = &o.recovery.flight;
    let p = &f.pilot;
    let error = shortest_heading_error(direction.rotate_radians(-p.ship.angle));
    let desired = -error * 3.0;
    let command = desired - (p.ship.spin - desired) * 0.2;
    let assist = if p.landing.assist_strength > 0.0 {
        p.planet.motion.spin
    } else {
        0.0
    };
    ((assist - command) / f.flight.limits.turn_speed).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
    #[test]
    fn combat_policy_validates_identity_repeats_ticks_and_disarms_during_recovery() {
        let context = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 42,
        };
        let mut brain = RulePilotV4::new(context);
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], std::time::Duration::from_nanos(16_666_667));
        assert_eq!(
            brain.intent(&state.combat_observation(0, brain.site_request())),
            CombatIntent::default()
        );
        let mut o = state.combat_observation(1, brain.site_request());
        let first = brain.intent(&o);
        let telemetry = brain.telemetry().clone();
        assert_eq!(brain.intent(&o), first);
        assert_eq!(brain.telemetry(), &telemetry);
        o.recovery.flight.pilot.tick += 1;
        o.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        let intent = brain.intent(&o);
        assert_eq!(intent.weapons, SurfaceWeaponAction::default());
        assert!(brain.telemetry().recovery.is_some());
        brain.reset(context);
        assert_eq!(brain.telemetry().completed_recoveries, 0);
        assert!(brain.telemetry().recovery.is_none());
    }
}
