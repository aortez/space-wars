use super::*;
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{
    PlanetFlagObservation, SurfaceSortieScenario, TransferResult,
    live_planning::{
        ActualLanding, ActualLocalAttemptFailure, ObjectiveWorkEvidence, ObjectiveWorkState,
    },
    mission::MissionObstacle,
};
use std::time::Duration;

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1, ActualRouteAbort) {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.match_rules = true;
    o.sun = None;
    o.boundary.center = Vec2::ZERO;
    o.boundary.radius = 5000.0;
    for (i, planet) in o.planets.iter_mut().enumerate() {
        planet.motion.position = Vec2::new(i as f32 * 1000.0, 0.0);
        planet.motion.velocity = Vec2::ZERO;
        planet.motion.angle = 0.0;
        planet.motion.spin = 0.0;
        planet.radius = 60.0;
        planet.claim.as_mut().unwrap().owner = None;
    }
    let c = &mut o.local.combat;
    c.recovery.flight.flight.enabled = true;
    let p = &mut c.recovery.flight.pilot;
    p.tick = 200;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(0.0, 65.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.gravity = -Vec2::Y * 18.0;
    p.ship_health = 32.0;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Landed;
    p.landing.supported_feet = 2;
    p.hatch = Some(Vec2::new(1.0, 60.0));
    p.boarding_hatches = [p.hatch, None];
    p.transfer = TransferResult::Ready;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: Vec2::new(20.0, 60.0),
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    o.planets[0] = p.planet.clone();
    let objective = LandingObjective::read(p).unwrap();
    let attempt = ActualLocalAttemptFailure {
        actor: p.owner,
        reason: "arrival_window",
        pose: ActualLanding {
            vehicle: p.ship.position,
            angle: p.ship.angle,
            exit: p.hatch.unwrap(),
            boarding_hatches: p.boarding_hatches,
        },
    };
    let abort = ActualRouteAbort {
        tick: 200,
        generation: 29,
        request_tick: 180,
        measurement_tick: 180,
        objective,
        attempt,
    };
    let target = c.target.as_mut().unwrap();
    target.motion.position = p.ship.position + Vec2::X * 120.0;
    target.motion.velocity = Vec2::ZERO;
    target.ship_form = Some(ShipForm::Ship);
    target.health = 100.0;
    target.health_fraction = 1.0;
    target.visible = true;
    target.ground_occluded = false;
    o.opponent.as_mut().unwrap().motion = target.motion;
    c.weapons.last_hit_taken_tick = Some(200);
    o.local.objective_work = Some(ObjectiveWorkState::Pending);
    o.local.landing_objective = None;
    o.local.objective_evidence = Some(ObjectiveWorkEvidence {
        tick: 200,
        objective,
        source_objective: Some(objective),
        generation: Some(29),
        request_tick: Some(180),
        measurement_tick: Some(180),
        invalidated_by: None,
        submission_deferred_by: None,
        publication: None,
        exhausted_walk: None,
        unsupported_walk: None,
        covered_handoff: None,
        actual_local_failure: Some(attempt),
    });
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
        crate::mission_policy::MissionPolicy::ValuePlanner,
    );
    bot.configure_actual_route_recovery(true);
    bot.configure_powered_capture(true);
    bot.configure_capture_escape(enabled);
    (bot, o, abort)
}

fn armed() -> (MaterialMissionPilot, MissionObservationV1) {
    let (mut bot, mut o, abort) = fixture(true);
    o.local.combat.recovery.flight.pilot.tick += 1;
    bot.start_capture_escape(&o, abort);
    assert!(bot.capture_escaping());
    o.local.combat.recovery.flight.pilot.tick += 1;
    (bot, o)
}

#[test]
fn native_capture_failure_arms_only_the_enabled_mission_on_the_following_tick() {
    for enabled in [false, true] {
        let (mut bot, mut o, _) = fixture(enabled);
        bot.telemetry.target = Some(0);
        bot.capture = Some(bot.new_capture_task(&o));
        assert_eq!(bot.intent(&o), CombatIntent::default());
        assert_eq!(
            bot.telemetry.capture.as_ref().unwrap().failed_tick,
            Some(200)
        );
        o.local.combat.recovery.flight.pilot.tick += 1;
        assert_eq!(bot.intent(&o), CombatIntent::default());
        assert!(bot.capture.is_none());
        assert_eq!(bot.capture_escaping(), enabled);
        assert!(bot.deferred.contains(&(0, 201 + 30 * 60)));
        o.local.combat.recovery.flight.pilot.tick += 1;
        let action = bot.intent(&o);
        assert_eq!(
            bot.telemetry.goal,
            if enabled {
                MissionGoal::Disengage
            } else {
                MissionGoal::Hunt
            }
        );
        if enabled {
            assert!(bot.telemetry.pursuit.is_none());
            assert!(action.flight.controls.primary_held && !action.flight.controls.interact_held);
            assert!(!action.flight.wings.closed);
            let a = bot.telemetry.capture_escape.as_ref().unwrap().last.unwrap();
            assert_eq!(a.deadline_tick, 200 + 12 * 60);
            assert_eq!(a.guidance, "lift");
        }
    }
}

#[test]
fn stale_unrelated_and_unthreatened_aborts_cannot_arm() {
    for change in 0..11 {
        let (mut bot, mut o, mut abort) = fixture(true);
        o.local.combat.recovery.flight.pilot.tick += 1;
        match change {
            0 => abort.tick -= 1,
            1 => abort.attempt.actor = PlayerId::PLAYER_2,
            2 => abort.attempt.pose.exit.x += 0.01,
            3 => o.local.combat.recovery.flight.pilot.planet.revision += 1,
            4 => o.local.combat.recovery.flight.pilot.controls_armed = false,
            5 => o.local.combat.recovery.flight.pilot.location = PilotLocation::OnFoot,
            6 => o.local.combat.target = None,
            7 => o.local.combat.target.as_mut().unwrap().ground_occluded = true,
            8 => o.local.combat.target.as_mut().unwrap().visible = false,
            9 => o.local.combat.target.as_mut().unwrap().ship_form = Some(ShipForm::EscapePod),
            _ => o.local.combat.target.as_mut().unwrap().motion.position = Vec2::new(1000.0, 65.0),
        }
        bot.start_capture_escape(&o, abort);
        assert!(!bot.capture_escaping(), "change {change}");
        assert_eq!(bot.telemetry.capture_escape.unwrap().attempts, 0);
    }
}

#[test]
fn incoming_fire_does_not_extend_deadline_and_clone_reset_remain_bounded() {
    let (mut bot, mut o) = armed();
    let mut copy = bot.clone();
    let first = bot.intent(&o);
    assert_eq!(first, copy.intent(&o));
    assert_eq!(bot.telemetry, copy.telemetry);
    let before = bot.telemetry.clone();
    assert_eq!(bot.intent(&o), first);
    assert_eq!(bot.telemetry, before);
    for tick in [300, 700, 919] {
        o.local.combat.recovery.flight.pilot.tick = tick;
        o.local.combat.weapons.last_hit_taken_tick = Some(tick);
        bot.intent(&o);
        assert!(bot.capture_escaping() && bot.telemetry.pursuit.is_none());
        assert_eq!(
            bot.telemetry
                .capture_escape
                .as_ref()
                .unwrap()
                .last
                .unwrap()
                .deadline_tick,
            920
        );
    }
    o.local.combat.recovery.flight.pilot.tick = 920;
    bot.intent(&o);
    assert!(!bot.capture_escaping());
    assert_eq!(
        bot.telemetry
            .capture_escape
            .as_ref()
            .unwrap()
            .last
            .unwrap()
            .reason,
        Some("escape deadline")
    );
    assert!(bot.telemetry.pursuit.is_some());
    bot.reset(bot.context);
    assert_eq!(bot.telemetry.capture_escape, Some(CaptureEscape::default()));
    assert!(bot.actual_route_recovery);
}

#[test]
fn separation_needs_source_clearance_sustained_cover_and_boundary_room() {
    let (bot, source) = armed();
    for blocked in 0..4 {
        let mut bot = bot.clone();
        let mut o = source.clone();
        o.local.combat.target.as_mut().unwrap().ground_occluded = true;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.ship.position = Vec2::new(0.0, 200.0);
        p.landing.phase = LandingPhase::Flying;
        p.landing.supported_feet = 0;
        match blocked {
            0 => {}
            1 => p.landing.supported_feet = 1,
            2 => o.local.combat.target = None,
            _ => o.boundary.radius = 230.0,
        }
        for tick in 202..=261 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            bot.intent(&o);
            assert!(bot.capture_escaping());
        }
        o.local.combat.recovery.flight.pilot.tick = 262;
        bot.intent(&o);
        assert_eq!(!bot.capture_escaping(), blocked == 0);
        if blocked == 0 {
            assert_eq!(bot.telemetry.capture_escape.as_ref().unwrap().separated, 1);
            assert_eq!(bot.next_pursuit_tick, 920);
        }
    }
}

#[test]
fn solar_recovery_and_unavailable_controls_retain_priority_without_extending_time() {
    for change in 0..5 {
        let (mut bot, mut o) = armed();
        match change {
            0 => {
                o.sun = Some(MissionObstacle {
                    position: Vec2::ZERO,
                    radius: 50.0,
                })
            }
            1 => o.local.combat.recovery.flight.pilot.ship_available = false,
            2 => o.local.combat.recovery.flight.pilot.controls_armed = false,
            3 => o.local.combat.recovery.flight.pilot.queries_ready = false,
            _ => o.local.combat.recovery.flight.flight.enabled = false,
        }
        let action = bot.intent(&o);
        let a = bot.telemetry.capture_escape.as_ref().unwrap().last.unwrap();
        assert_eq!(a.deadline_tick, 920);
        assert_eq!(a.controlled_ticks, 0);
        if change == 0 {
            assert_eq!(bot.telemetry.goal, MissionGoal::AvoidSun);
        }
        if change == 1 {
            assert_eq!(a.reason, Some("recovery required"));
        }
        if change >= 2 {
            assert_eq!(action, CombatIntent::default());
        }
        if change != 1 {
            o.local.combat.recovery.flight.pilot.tick = 920;
            bot.intent(&o);
            assert!(!bot.capture_escaping());
            assert_eq!(
                bot.telemetry
                    .capture_escape
                    .as_ref()
                    .unwrap()
                    .last
                    .unwrap()
                    .reason,
                Some("escape deadline")
            );
        }
    }
}

#[test]
fn boundary_braking_overrides_lift_and_opens_wings() {
    for feet in [0, 2] {
        let (mut bot, mut o) = armed();
        o.boundary.radius = 300.0;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.ship.position = Vec2::new(0.0, 200.0);
        p.ship.velocity = Vec2::Y * 100.0;
        p.landing.supported_feet = feet;
        let action = bot.intent(&o);
        let a = bot.telemetry.capture_escape.as_ref().unwrap().last.unwrap();
        assert_eq!(a.guidance, "boundary");
        assert!(a.boundary.active && action.flight.controls.brake_held);
        assert!(!action.flight.wings.closed && !action.flight.controls.interact_held);
        assert_eq!(a.clear_since, None);
    }
}

#[test]
fn defensive_fire_keeps_readiness_visibility_and_aim_checks_without_changing_flight() {
    let (bot, mut source) = armed();
    let c = &mut source.local.combat;
    let p = &mut c.recovery.flight.pilot;
    p.ship.position = Vec2::new(0.0, 300.0);
    p.gravity = Vec2::ZERO;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    let target = c.target.as_mut().unwrap();
    target.motion.position = p.ship.position + Vec2::Y * 100.0;
    c.laser_available = true;
    c.cannon_ready = true;
    let mut armed = bot.clone();
    let fire = armed.intent(&source);
    assert!(fire.weapons.laser && fire.weapons.cannon);
    assert!(!fire.flight.controls.interact_held);
    for change in 0..3 {
        let mut branch = bot.clone();
        let mut o = source.clone();
        match change {
            0 => {
                o.local.combat.laser_available = false;
                o.local.combat.cannon_ready = false;
            }
            1 => o.local.combat.target.as_mut().unwrap().visible = false,
            _ => o.local.combat.target.as_mut().unwrap().motion.position += Vec2::X * 100.0,
        }
        let action = branch.intent(&o);
        assert_eq!(action.weapons, Default::default());
        assert_eq!(action.flight, fire.flight);
    }
}
