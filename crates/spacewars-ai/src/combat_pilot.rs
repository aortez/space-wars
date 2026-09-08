//! Material combat mission: fly, engage occupied ships, recover and rejoin.
//! Uses the Spacewars combat solution and the same RecoverShipTask as V3.
use crate::{
    BrainReset, RuleShipBrainConfig, combat_solution,
    flight_pilot::FlightIntent,
    recovery_task::{RecoverShipTask, RecoveryTelemetry, TaskStatus},
    shortest_heading_error,
};
use engine_common::{Action, CombatBreakSettings};
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceWingAction,
        combat::{CombatObservationV2, SurfaceWeaponAction},
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
    pub breaks: CombatBreakTelemetry,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CombatBreakTelemetry {
    pub config: CombatBreakSettings,
    pub started: u32,
    pub completed: u32,
    pub interrupted: u32,
    pub active_until_tick: Option<u64>,
    pub last_started_tick: Option<u64>,
    pub last_finished_tick: Option<u64>,
    pub last_reengaged_tick: Option<u64>,
    pub remaining_engagement_ticks: u64,
    pub weapon_free_ticks: u64,
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
    break_random: u64,
    break_side: f32,
    awaiting_break_return: bool,
}
impl RulePilotV4 {
    /// The original policy remains available for regression and Off comparisons.
    pub fn new(context: BrainReset) -> Self {
        Self::with_combat_breaks(
            context,
            CombatBreakSettings {
                interval_seconds: 0,
                ..Default::default()
            },
        )
    }
    pub fn with_combat_breaks(context: BrainReset, config: CombatBreakSettings) -> Self {
        let mut pilot = Self {
            context,
            task: None,
            seen_losses: 0,
            previous_tick: None,
            previous_intent: CombatIntent::default(),
            pwm: 0.0,
            clearing_ground: false,
            awaiting_rejoin: false,
            break_random: context.episode_seed
                ^ (context.actor.index() as u64 + 1).wrapping_mul(0xd1b5_4a32_d192_ed03),
            break_side: 1.0,
            awaiting_break_return: false,
            telemetry: CombatPilotTelemetry {
                policy: RULE_PILOT_V4_POLICY_ID,
                goal: "takeoff",
                target: None,
                engagement_ticks: 0,
                completed_recoveries: 0,
                combat_returns: 0,
                rejoined_tick: None,
                recovery: None,
                breaks: CombatBreakTelemetry {
                    config: config.normalized(),
                    started: 0,
                    completed: 0,
                    interrupted: 0,
                    active_until_tick: None,
                    last_started_tick: None,
                    last_finished_tick: None,
                    last_reengaged_tick: None,
                    remaining_engagement_ticks: 0,
                    weapon_free_ticks: 0,
                },
            },
        };
        pilot.schedule_break();
        pilot
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::with_combat_breaks(context, self.telemetry.breaks.config);
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
    pub fn intent(&mut self, o: &CombatObservationV2) -> CombatIntent {
        let p = &o.recovery.flight.pilot;
        if o.version != 2
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
            self.finish_break(p.tick, true);
            self.awaiting_break_return = false;
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
    fn fly(&mut self, o: &CombatObservationV2) -> CombatIntent {
        let p = &o.recovery.flight.pilot;
        let delta = p.ship.position - p.planet.motion.position;
        let radius = delta.length();
        let up = delta.normalized();
        let velocity = p.ship.velocity - p.planet.velocity_at(p.ship.position);
        let altitude = radius - p.planet.radius;
        let falling = (-velocity.dot(up)).max(0.0);
        self.telemetry.target = o.target.map(|t| t.owner);
        if self
            .telemetry
            .breaks
            .active_until_tick
            .is_some_and(|end| p.tick >= end)
        {
            self.finish_break(p.tick, false);
        }
        // Loss of the opponent ends the exhibition and resumes normal patrol.
        if o.target.is_none() {
            self.finish_break(p.tick, true);
            self.awaiting_break_return = false;
        }
        let breaking = self.telemetry.breaks.active_until_tick.is_some();
        // The conservative routing bound is never a landing/support claim.
        // Reserve braking and turning room before turning inward to fire.
        if altitude < 40.0 + falling * 2.0 + falling * falling / 60.0 {
            self.clearing_ground = true;
        }
        if altitude > 85.0 && falling < 4.0 {
            self.clearing_ground = false;
        }
        if self.clearing_ground {
            self.telemetry.goal = if breaking {
                "flyby / clearing ground"
            } else {
                "climb clear of ground"
            };
            self.telemetry.breaks.weapon_free_ticks += u64::from(breaking);
            return self.guide(o, up * 16.0);
        }
        if breaking {
            return self.flyby(o, up, radius);
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
                if self.telemetry.breaks.config.interval_seconds > 0 {
                    if self.telemetry.breaks.remaining_engagement_ticks == 0 {
                        self.telemetry.breaks.started += 1;
                        self.telemetry.breaks.last_started_tick = Some(p.tick);
                        self.telemetry.breaks.active_until_tick = Some(
                            p.tick + u64::from(self.telemetry.breaks.config.duration_seconds) * 60,
                        );
                        // Turn along the local horizon away from the opponent.
                        self.break_side = if Vec2::new(-up.y, up.x).dot(relative) >= 0.0 {
                            -1.0
                        } else {
                            1.0
                        };
                        return self.flyby(o, up, radius);
                    }
                    self.telemetry.breaks.remaining_engagement_ticks -= 1;
                }
                self.telemetry.goal = "engage ship";
                self.telemetry.engagement_ticks += 1;
                if self.awaiting_break_return {
                    self.telemetry.breaks.last_reengaged_tick = Some(p.tick);
                    self.awaiting_break_return = false;
                }
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
    fn flyby(&mut self, o: &CombatObservationV2, up: Vec2, radius: f32) -> CombatIntent {
        self.telemetry.goal = "flyby / weapons off";
        self.telemetry.breaks.weapon_free_ticks += 1;
        self.guide(
            o,
            Vec2::new(-up.y, up.x) * self.break_side * 25.0
                + up * ((o.recovery.flight.pilot.planet.radius + 110.0 - radius) * 0.6)
                    .clamp(-12.0, 16.0),
        )
    }
    fn finish_break(&mut self, tick: u64, interrupted: bool) {
        if self.telemetry.breaks.active_until_tick.take().is_none() {
            return;
        }
        self.telemetry.breaks.last_finished_tick = Some(tick);
        if interrupted {
            self.telemetry.breaks.interrupted += 1;
        } else {
            self.telemetry.breaks.completed += 1;
        }
        self.awaiting_break_return = !interrupted;
        self.schedule_break();
    }
    fn schedule_break(&mut self) {
        let ticks = u64::from(self.telemetry.breaks.config.interval_seconds) * 60;
        if ticks == 0 {
            return;
        }
        // A private SplitMix64 stream keeps timings reproducible without changing
        // world randomness. Uniform ±25% avoids synchronized combat opponents.
        self.break_random = self.break_random.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut bits = self.break_random;
        bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        bits ^= bits >> 31;
        self.telemetry.breaks.remaining_engagement_ticks = ticks * 3 / 4 + bits % (ticks / 2 + 1);
    }
    fn guide(&mut self, o: &CombatObservationV2, relative_velocity: Vec2) -> CombatIntent {
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
fn turn(o: &CombatObservationV2, direction: Vec2) -> f32 {
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
    fn clear_flight() -> CombatObservationV2 {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], std::time::Duration::from_nanos(16_666_667));
        let mut o = state.combat_observation(1, None);
        let p = &mut o.recovery.flight.pilot;
        p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 120.0);
        p.ship.velocity = p.planet.velocity_at(p.ship.position);
        let target = o.target.as_mut().unwrap();
        target.motion.position = p.ship.position + Vec2::X * 60.0;
        target.motion.velocity = p.ship.velocity;
        target.ground_occluded = false;
        target.visible = true;
        o
    }

    #[test]
    fn combat_breaks_are_timed_disarmed_seeded_and_resettable() {
        let context = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 42,
        };
        let config = CombatBreakSettings {
            interval_seconds: 1,
            duration_seconds: 2,
        };
        let mut brain = RulePilotV4::with_combat_breaks(context, config);
        let mut replay = brain.clone();
        let initial = brain.telemetry().clone();
        let other = RulePilotV4::with_combat_breaks(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                ..context
            },
            config,
        );
        assert_ne!(
            initial.breaks.remaining_engagement_ticks,
            other.telemetry.breaks.remaining_engagement_ticks
        );
        let mut o = clear_flight();
        for tick in 1..=600 {
            o.recovery.flight.pilot.tick = tick;
            let intent = brain.intent(&o);
            assert_eq!(intent, replay.intent(&o));
            assert_eq!(brain.telemetry(), replay.telemetry());
            let telemetry = brain.telemetry().clone();
            assert_eq!(brain.intent(&o), intent);
            assert_eq!(brain.telemetry(), &telemetry);
            if let Some(until) = telemetry.breaks.active_until_tick {
                assert_eq!(until - telemetry.breaks.last_started_tick.unwrap(), 120);
                assert!(tick < until);
                assert_eq!(intent.weapons, SurfaceWeaponAction::default());
                assert_eq!(brain.label(), "flyby / weapons off");
            }
        }
        assert!(brain.telemetry.breaks.completed >= 3);
        assert!(brain.telemetry.breaks.last_reengaged_tick.is_some());
        brain.reset(context);
        assert_eq!(brain.telemetry(), &initial);
    }

    #[test]
    fn breaks_count_combat_only_and_yield_to_ground_avoidance_and_recovery() {
        let context = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 42,
        };
        let config = CombatBreakSettings {
            interval_seconds: 1,
            duration_seconds: 4,
        };
        let mut brain = RulePilotV4::with_combat_breaks(context, config);
        let mut o = clear_flight();
        let target = o.target.take();
        let remaining = brain.telemetry.breaks.remaining_engagement_ticks;
        for tick in 1..=200 {
            o.recovery.flight.pilot.tick = tick;
            brain.intent(&o);
        }
        assert_eq!(brain.telemetry.breaks.remaining_engagement_ticks, remaining);
        o.target = target;
        for tick in 201..=300 {
            o.recovery.flight.pilot.tick = tick;
            brain.intent(&o);
        }
        assert!(brain.telemetry.breaks.active_until_tick.is_some());
        let p = &mut o.recovery.flight.pilot;
        p.tick += 1;
        p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 20.0);
        let intent = brain.intent(&o);
        assert_eq!(intent.weapons, SurfaceWeaponAction::default());
        assert_eq!(brain.label(), "flyby / clearing ground");
        o.recovery.flight.pilot.tick += 1;
        o.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
        assert_eq!(brain.intent(&o).weapons, SurfaceWeaponAction::default());
        assert!(brain.telemetry.recovery.is_some());
        assert_eq!(brain.telemetry.breaks.interrupted, 1);
        assert_eq!(brain.telemetry.breaks.active_until_tick, None);
    }

    #[test]
    fn disabled_breaks_preserve_original_controls_and_validate_configuration() {
        let context = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 42,
        };
        let mut original = RulePilotV4::new(context);
        let mut disabled = RulePilotV4::with_combat_breaks(
            context,
            CombatBreakSettings {
                interval_seconds: 0,
                duration_seconds: 15,
            },
        );
        let mut o = clear_flight();
        for tick in 1..=2000 {
            o.recovery.flight.pilot.tick = tick;
            assert_eq!(original.intent(&o), disabled.intent(&o));
        }
        assert_eq!(disabled.telemetry.breaks.started, 0);
        assert_eq!(
            CombatBreakSettings {
                interval_seconds: u32::MAX,
                duration_seconds: 0
            }
            .normalized(),
            CombatBreakSettings {
                interval_seconds: 120,
                duration_seconds: 1
            }
        );
    }

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
