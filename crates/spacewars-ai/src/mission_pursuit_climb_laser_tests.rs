use super::*;
use crate::mission_policy::{MissionBot, MissionPolicy};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{LandingPhase, SurfaceSortieScenario};
use std::time::Duration;

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.match_rules = true;
    o.sun = None;
    o.boundary.radius = 5000.0;
    for (i, planet) in o.planets.iter_mut().enumerate() {
        planet.motion.position = Vec2::new(i as f32 * 2000.0, 0.0);
        planet.motion.velocity = Vec2::ZERO;
        planet.motion.spin = 0.0;
        planet.radius = 60.0;
        planet.claim.as_mut().unwrap().owner = None;
    }
    o.planets[0].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 100;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(0.0, 110.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.ship_health = 100.0;
    p.gravity = Vec2::ZERO;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.altitude = 50.0;
    p.landing.supported_feet = 0;
    p.landing.phase = LandingPhase::Flying;
    o.local.combat.recovery.flight.flight.enabled = true;
    o.local.combat.laser_available = true;
    o.local.combat.cannon_ready = true;
    o.local.combat.weapons.last_hit_taken_tick = None;
    aim_ahead(&mut o);
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        CombatBreakSettings {
            interval_seconds: 1,
            duration_seconds: 4,
        },
        MissionPolicy::ValuePlanner,
    );
    bot.configure_pursuit_climb_laser(enabled);
    (bot, o)
}

fn aim_ahead(o: &mut MissionObservationV1) {
    let p = &o.local.combat.recovery.flight.pilot;
    let target = o.local.combat.target.as_mut().unwrap();
    target.motion.position = p.ship.position + Vec2::new(0.0, 100.0);
    target.motion.velocity = p.ship.velocity;
    target.ship_form = Some(ShipForm::Ship);
    target.health = 100.0;
    target.health_fraction = 1.0;
    target.visible = true;
    target.ground_occluded = false;
    o.opponent.as_mut().unwrap().motion = target.motion;
}

fn last(bot: &MaterialMissionPilot) -> &PursuitClimbLaserCheck {
    bot.telemetry
        .pursuit_climb_laser
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap()
}

#[test]
fn both_climb_entries_add_only_laser_and_preserve_the_consumed_observation() {
    for source in [
        PursuitClimbLaserSource::Mission,
        PursuitClimbLaserSource::Combat,
    ] {
        let (mut enabled, mut o) = fixture(true);
        let (mut disabled, _) = fixture(false);
        if source == PursuitClimbLaserSource::Combat {
            let p = &mut o.local.combat.recovery.flight.pilot;
            // Outside the mission's 78-unit limit, inside combat's 86.67-unit limit.
            p.ship.position.y = 142.0;
            p.ship.velocity.y = -20.0;
            aim_ahead(&mut o);
        }
        let before = o.clone();
        let base = disabled.intent(&o);
        let changed = enabled.intent(&o);
        assert!(changed.weapons.laser && !changed.weapons.cannon);
        assert!(!base.weapons.laser && !base.weapons.cannon);
        assert_eq!(changed.flight, base.flight);
        assert_eq!(last(&enabled).source, source);
        assert_eq!(
            last(&enabled).native_actions,
            base.encode(PlayerId::PLAYER_1)
        );
        let (a, b) = (enabled.sensor_request(), disabled.sensor_request());
        assert_eq!(a.site, b.site);
        assert_eq!(a.last_survey, b.last_survey);
        assert_eq!(a.objective_planning, b.objective_planning);
        assert_eq!(a.vehicle_flight, b.vehicle_flight);
        assert_eq!(a.destination_cover, b.destination_cover);
        assert_eq!(enabled.telemetry.pursuit, disabled.telemetry.pursuit);
        assert_eq!(enabled.patrol.telemetry(), disabled.patrol.telemetry());
        assert_eq!(o, before);
    }
}

#[test]
fn explicit_break_stays_weapon_free_including_when_mission_climb_takes_over() {
    let (mut enabled, mut o) = fixture(true);
    let (mut disabled, _) = fixture(false);
    o.local.combat.recovery.flight.pilot.ship.position.y = 300.0;
    aim_ahead(&mut o);
    let mut until = None;
    for tick in 100..200 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        assert_eq!(enabled.intent(&o), disabled.intent(&o));
        until = enabled.patrol.telemetry().breaks.active_until_tick;
        if until.is_some() {
            break;
        }
    }
    let until = until.expect("short scheduled break starts");
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.ship.position.y = 110.0;
    aim_ahead(&mut o);
    assert_eq!(enabled.intent(&o), disabled.intent(&o));
    assert_eq!(
        last(&enabled).decision,
        PursuitClimbLaserDecision::ScheduledBreak
    );
    assert_eq!(enabled.patrol.telemetry(), disabled.patrol.telemetry());
    o.local.combat.recovery.flight.pilot.tick = until;
    let changed = enabled.intent(&o);
    let base = disabled.intent(&o);
    assert!(changed.weapons.laser && !base.weapons.laser);
    assert_eq!(changed.flight, base.flight);
    assert_eq!(enabled.patrol.telemetry(), disabled.patrol.telemetry());
}

#[test]
fn readiness_visibility_range_alignment_and_invalid_motion_keep_their_gates() {
    for refusal in 0..8 {
        let (mut bot, mut o) = fixture(true);
        match refusal {
            0 => o.local.combat.laser_available = false,
            1 => o.local.combat.target.as_mut().unwrap().visible = false,
            2 => o.local.combat.target.as_mut().unwrap().ground_occluded = true,
            3 => o.local.combat.target.as_mut().unwrap().motion.position.y += 151.0,
            4 => o.local.combat.recovery.flight.pilot.ship.angle = 0.1,
            5 => o.local.combat.target.as_mut().unwrap().motion.velocity.x = f32::NAN,
            6 => o.local.combat.target.as_mut().unwrap().motion.position.x = 1.0e30,
            _ => o.local.combat.target = None,
        }
        // Test the isolated addition with an already-selected climb. Some
        // malformed/hidden targets would be declined by the coordinator first.
        bot.telemetry.goal = MissionGoal::Hunt;
        bot.telemetry.pursuit = Some(MissionPursuit {
            started_tick: 90,
            last_visible_tick: 100,
            reason: "test",
        });
        let result = bot.pursuit_climb_laser_intent(
            &o,
            CombatIntent::default(),
            PursuitClimbLaserSource::Mission,
        );
        assert_eq!(result, CombatIntent::default());
        let expected = match refusal {
            0 => PursuitClimbLaserDecision::Unready,
            1 | 2 => PursuitClimbLaserDecision::Occluded,
            3 | 4 => PursuitClimbLaserDecision::AimOrRange,
            5 | 6 => PursuitClimbLaserDecision::InvalidMotion,
            _ => PursuitClimbLaserDecision::NoTarget,
        };
        assert_eq!(last(&bot).decision, expected);
    }
}

#[test]
fn other_tasks_and_unavailable_actors_do_not_gain_weapons() {
    for exclusion in 0..13 {
        let (mut bot, mut o) = fixture(true);
        bot.telemetry.goal = MissionGoal::Hunt;
        bot.telemetry.pursuit = Some(MissionPursuit {
            started_tick: 90,
            last_visible_tick: 100,
            reason: "test",
        });
        match exclusion {
            0 => bot.telemetry.goal = MissionGoal::Transfer,
            1 => bot.telemetry.goal = MissionGoal::Capture,
            2 => bot.telemetry.pursuit = None,
            3 => bot.capture = Some(bot.new_capture_task(&o)),
            4 => bot.recovery = Some(RecoverShipTask::new(bot.context)),
            5 => bot.escaping_sun = true,
            6 => o.match_rules = false,
            7 => o.local.combat.recovery.flight.pilot.controls_armed = false,
            8 => o.local.combat.recovery.flight.pilot.queries_ready = false,
            9 => o.local.combat.recovery.flight.flight.enabled = false,
            10 => o.local.combat.recovery.flight.pilot.ship_available = false,
            11 => o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod,
            _ => o.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
        }
        let intent = CombatIntent::default();
        assert_eq!(
            bot.pursuit_climb_laser_intent(&o, intent, PursuitClimbLaserSource::Mission),
            intent
        );
        assert_eq!(last(&bot).decision, PursuitClimbLaserDecision::Unavailable);
    }
}

#[test]
fn repeated_ticks_clone_and_reset_keep_configuration_without_reusing_checks() {
    let (mut bot, o) = fixture(true);
    let mut cloned = bot.clone();
    let action = bot.intent(&o);
    assert_eq!(cloned.intent(&o), action);
    let telemetry = bot.telemetry().clone();
    assert_eq!(cloned.telemetry(), &telemetry);
    assert_eq!(bot.intent(&o), action);
    assert_eq!(bot.telemetry(), &telemetry);
    bot.reset(bot.context);
    assert_eq!(
        bot.telemetry.pursuit_climb_laser,
        Some(PursuitClimbLaserTelemetry::default())
    );
    assert_eq!(bot.intent(&o), action);
    assert_eq!(bot.telemetry(), &telemetry);
}

#[test]
fn default_and_non_v13_identities_do_not_silently_acquire_the_option() {
    let context = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    for policy in MissionPolicy::ALL {
        let bot = MissionBot::new(policy, context, Default::default());
        assert!(bot.telemetry().pursuit_climb_laser.is_none());
        assert!(
            serde_json::to_value(bot.telemetry())
                .unwrap()
                .get("pursuit_climb_laser")
                .is_none()
        );
        if policy != MissionPolicy::ValuePlanner {
            assert!(std::panic::catch_unwind(|| bot.with_pursuit_climb_laser(true)).is_err());
        }
    }
}

#[test]
#[should_panic(expected = "configure before the first intent")]
fn configuration_cannot_change_after_controls_have_been_issued() {
    let (mut bot, o) = fixture(false);
    bot.intent(&o);
    bot.configure_pursuit_climb_laser(true);
}
