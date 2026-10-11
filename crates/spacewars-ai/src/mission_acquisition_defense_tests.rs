use super::*;
use crate::mission_policy::{MissionBot, MissionPolicy};
use engine_common::Scenario;
use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, pilot::LandingSiteQuery};
use std::time::Duration;

fn fixture(enabled: bool) -> (MaterialMissionPilot, MissionObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.mission_observation(0, None);
    o.match_rules = true;
    o.sun = None;
    o.boundary.center = Vec2::ZERO;
    o.boundary.radius = 5000.0;
    for (i, planet) in o.planets.iter_mut().enumerate() {
        planet.motion.position = Vec2::new(i as f32 * 2000.0, 0.0);
        planet.motion.velocity = Vec2::ZERO;
        planet.motion.spin = 0.0;
        planet.radius = 60.0;
        planet.claim.as_mut().unwrap().owner = None;
    }
    let c = &mut o.local.combat;
    c.recovery.flight.flight.enabled = true;
    let p = &mut c.recovery.flight.pilot;
    p.tick = 100;
    p.planet = o.planets[0].clone();
    p.ship.position = Vec2::new(0.0, 160.0);
    p.ship.velocity = Vec2::ZERO;
    p.ship.angle = 0.0;
    p.ship.spin = 0.0;
    p.gravity = Vec2::ZERO;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    p.landing.altitude = 100.0;
    p.sites.clear();
    p.site_query = LandingSiteQuery::Deferred { next_tick: 120 };
    c.laser_available = true;
    c.cannon_ready = true;
    c.weapons.last_hit_taken_tick = Some(100);
    c.weapons.last_hit_source = Some("laser");
    let t = c.target.as_mut().unwrap();
    t.motion.position = p.ship.position + Vec2::Y * 120.0;
    t.motion.velocity = Vec2::ZERO;
    t.ship_form = Some(ShipForm::Ship);
    t.health = 100.0;
    t.health_fraction = 1.0;
    t.visible = true;
    t.ground_occluded = false;
    o.opponent.as_mut().unwrap().motion = t.motion;
    o.local.landing_objective = None;
    o.local.objective_evidence = None;
    let mut bot = MaterialMissionPilot::with_policy(
        BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        },
        Default::default(),
        MissionPolicy::ValuePlanner,
    );
    bot.configure_acquisition_defense(enabled);
    bot.telemetry.target = Some(0);
    bot.capture = Some(bot.new_capture_task(&o));
    (bot, o)
}

fn last(bot: &MaterialMissionPilot) -> &AcquisitionDefenseAttempt {
    bot.telemetry
        .acquisition_defense
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap()
}

#[test]
fn handoff_binds_native_current_capture_and_preserves_disabled_behavior() {
    let (mut on, o) = fixture(true);
    let (mut off, _) = fixture(false);
    let unchanged = o.clone();
    let native = off.intent(&o);
    let action = on.intent(&o);
    assert_eq!(o, unchanged);
    assert_eq!(last(&on).native_actions, native.encode(PlayerId::PLAYER_1));
    assert_eq!(
        &last(&on).capture,
        off.capture.as_ref().unwrap().telemetry()
    );
    assert!(on.acquisition_defending() && on.capture.is_none());
    assert_eq!(last(&on).started_tick, 100);
    assert_eq!(last(&on).deadline_tick, 820);
    assert_eq!(last(&on).controlled_ticks, 1);
    assert!(on.telemetry.pursuit.is_none());
    assert!(!action.flight.controls.interact_held);
    assert_eq!(on.deferred, vec![(0, 1900)]);
    assert!(off.telemetry.acquisition_defense.is_none());
    assert!(
        serde_json::to_value(off.telemetry)
            .unwrap()
            .get("acquisition_defense")
            .is_none()
    );
}

#[test]
fn stale_nonweapon_hidden_invalid_or_surface_observations_cannot_arm() {
    for change in 0..15 {
        let (mut bot, mut o) = fixture(true);
        let c = &mut o.local.combat;
        let p = &mut c.recovery.flight.pilot;
        match change {
            0 => c.weapons.last_hit_taken_tick = Some(99),
            1 => c.weapons.last_hit_source = Some("ground"),
            2 => c.target.as_mut().unwrap().visible = false,
            3 => c.target.as_mut().unwrap().ground_occluded = true,
            4 => c.target.as_mut().unwrap().ship_form = Some(ShipForm::EscapePod),
            5 => c.target.as_mut().unwrap().motion.position = p.ship.position + Vec2::X * 300.0,
            6 => p.controls_armed = false,
            7 => p.queries_ready = false,
            8 => c.recovery.flight.flight.enabled = false,
            9 => p.location = PilotLocation::OnFoot,
            10 => p.landing.supported_feet = 1,
            11 => p.landing.phase = LandingPhase::Landed,
            12 => o.match_rules = false,
            13 => c.target = None,
            _ => p.ship_available = false,
        }
        bot.intent(&o);
        assert_eq!(
            bot.telemetry.acquisition_defense.unwrap().attempts,
            0,
            "change {change}"
        );
    }
}

#[test]
fn repeated_hits_clone_and_duplicate_ticks_cannot_restart_the_clock() {
    let (mut bot, mut o) = fixture(true);
    let mut copy = bot.clone();
    assert_eq!(bot.intent(&o), copy.intent(&o));
    assert_eq!(bot.telemetry, copy.telemetry);
    let action = bot.intent(&o);
    let snapshot = bot.telemetry.clone();
    assert_eq!(bot.intent(&o), action);
    assert_eq!(bot.telemetry, snapshot);
    for tick in [101, 300, 819] {
        o.local.combat.recovery.flight.pilot.tick = tick;
        o.local.combat.weapons.last_hit_taken_tick = Some(tick);
        bot.intent(&o);
        assert!(bot.acquisition_defending());
        assert_eq!(last(&bot).deadline_tick, 820);
        assert_eq!(
            bot.telemetry.acquisition_defense.as_ref().unwrap().attempts,
            1
        );
    }
    o.local.combat.recovery.flight.pilot.tick = 820;
    bot.intent(&o);
    assert!(!bot.acquisition_defending());
    assert_eq!(last(&bot).reason, Some("defense deadline"));
    bot.reset(bot.context);
    assert_eq!(
        bot.telemetry.acquisition_defense,
        Some(AcquisitionDefense::default())
    );
}

#[test]
fn disabled_queries_and_controls_still_consume_deadline() {
    for disarmed in [false, true] {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 101;
        if disarmed {
            p.controls_armed = false;
        } else {
            p.queries_ready = false;
        }
        assert_eq!(bot.intent(&o), CombatIntent::default());
        assert!(bot.acquisition_defending());
        o.local.combat.recovery.flight.pilot.tick = 820;
        bot.intent(&o);
        assert_eq!(last(&bot).finished_tick, Some(820));
        assert_eq!(last(&bot).controlled_ticks, 1);
    }
}

#[test]
fn solar_safety_preempts_flight_without_renewing_defense() {
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    o.sun = Some(
        scenario_spacewars::surface_sortie::mission::MissionObstacle {
            position: Vec2::ZERO,
            radius: 150.0,
        },
    );
    for tick in [101, 819, 820] {
        o.local.combat.recovery.flight.pilot.tick = tick;
        let action = bot.intent(&o);
        assert_eq!(bot.telemetry.goal, MissionGoal::AvoidSun);
        assert!(!action.weapons.laser && !action.weapons.cannon);
        assert_eq!(last(&bot).controlled_ticks, 1);
    }
    assert_eq!(last(&bot).finished_tick, Some(820));
    assert_eq!(last(&bot).reason, Some("defense deadline"));
}

#[test]
fn separation_requires_sustained_positive_evidence_and_boundary_room() {
    for missing in [false, true] {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        if missing {
            o.local.combat.target = None;
        } else {
            o.local.combat.target.as_mut().unwrap().ground_occluded = true;
        }
        for tick in 101..161 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            bot.intent(&o);
            assert!(bot.acquisition_defending());
        }
        o.local.combat.recovery.flight.pilot.tick = 161;
        bot.intent(&o);
        assert_eq!(bot.acquisition_defending(), missing);
        if !missing {
            assert_eq!(last(&bot).reason, Some("separation established"));
        }
    }
    let (mut bot, mut o) = fixture(true);
    bot.intent(&o);
    o.local.combat.recovery.flight.pilot.tick = 101;
    o.boundary.radius = 180.0;
    o.local.combat.target.as_mut().unwrap().ground_occluded = true;
    let action = bot.intent(&o);
    assert!(last(&bot).boundary.active && action.flight.controls.brake_held);
    assert!(!action.flight.wings.closed);
    assert!(last(&bot).clear_since.is_none());
}

#[test]
fn recovery_and_new_ground_contact_preempt_defense_without_landing_permission() {
    for ground in [false, true] {
        let (mut bot, mut o) = fixture(true);
        bot.intent(&o);
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.tick = 101;
        if ground {
            p.landing.supported_feet = 1;
        } else {
            p.ship_form = ShipForm::EscapePod;
        }
        bot.intent(&o);
        assert!(!bot.acquisition_defending());
        assert_eq!(
            last(&bot).reason,
            Some(if ground {
                "surface task takes priority"
            } else {
                "recovery required"
            })
        );
    }
}

#[test]
fn weapon_requests_are_exactly_the_existing_combat_controller() {
    let (mut bot, o) = fixture(true);
    let mut combat = bot.patrol.clone();
    let expected = combat.intent(&o.local.combat);
    let action = bot.intent(&o);
    assert_eq!(action.weapons, expected.weapons);
    assert_eq!(bot.telemetry.combat.as_ref(), Some(combat.telemetry()));
    // Feeding the flight half of combat would turn toward the attacker instead
    // of following the separately selected escape axis.
    assert_ne!(action.flight, expected.flight);
}

#[test]
fn defensive_flight_keeps_scheduled_combat_breaks_weapons_free() {
    let (mut bot, mut o) = fixture(true);
    bot.patrol = RulePilotV4::with_combat_breaks(
        bot.context,
        CombatBreakSettings {
            interval_seconds: 1,
            duration_seconds: 2,
        },
    );
    let mut breaking = 0;
    for tick in 100..500 {
        o.local.combat.recovery.flight.pilot.tick = tick;
        let action = bot.intent(&o);
        assert!(bot.acquisition_defending());
        if bot.patrol.telemetry().breaks.active_until_tick.is_some() {
            breaking += 1;
            assert!(!action.weapons.laser && !action.weapons.cannon);
        }
    }
    assert!(breaking >= 120);
}

#[test]
fn enabled_configuration_is_v13_only_and_cannot_change_mid_episode() {
    for policy in [MissionPolicy::Legacy, MissionPolicy::JetpackPlanner] {
        let result = std::panic::catch_unwind(|| {
            MissionBot::new(
                policy,
                BrainReset {
                    actor: PlayerId::PLAYER_1,
                    episode_seed: 42,
                },
                Default::default(),
            )
            .with_acquisition_defense(true)
        });
        assert!(result.is_err());
    }
    let (mut bot, o) = fixture(true);
    bot.intent(&o);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || bot.configure_acquisition_defense(false)
        ))
        .is_err()
    );
}

fn without_defense_options(bot: &MaterialMissionPilot) -> serde_json::Value {
    let mut value = serde_json::to_value(&bot.telemetry).unwrap();
    value.as_object_mut().unwrap().remove("acquisition_defense");
    value
        .as_object_mut()
        .unwrap()
        .remove("acquisition_clearance");
    value
}

#[test]
fn negative_forecast_preserves_native_capture_actions_and_coordinator_state() {
    let (mut gated, mut o) = fixture(true);
    gated.configure_acquisition_clearance(true);
    o.boundary.radius = 170.0;
    let (mut native, _) = fixture(false);
    let observation = o.clone();
    assert_eq!(gated.intent(&o), native.intent(&o));
    assert_eq!(o, observation);
    assert_eq!(
        without_defense_options(&gated),
        without_defense_options(&native)
    );
    assert_eq!(gated.deferred, native.deferred);
    assert_eq!(gated.next_pursuit_tick, native.next_pursuit_tick);
    assert_eq!(
        gated.capture.as_ref().unwrap().telemetry(),
        native.capture.as_ref().unwrap().telemetry()
    );
    assert_eq!(
        gated.telemetry.acquisition_defense,
        Some(AcquisitionDefense::default())
    );
    let gate = gated.telemetry.acquisition_clearance.as_ref().unwrap();
    assert_eq!((gate.checks, gate.rejected), (1, 1));
    let check = gate.last.as_ref().unwrap();
    assert_eq!(check.tick, 100);
    assert_eq!(check.decision, "negative_clearance");
    assert!(check.estimated_clearance < 0.0);
    assert_eq!(&check.capture, native.capture.as_ref().unwrap().telemetry());
    assert_eq!(
        check.native_actions,
        native.intent(&o).encode(PlayerId::PLAYER_1)
    );
}

#[test]
fn positive_forecast_preserves_original_escape_and_deadline() {
    let (mut gated, o) = fixture(true);
    gated.configure_acquisition_clearance(true);
    let (mut original, _) = fixture(true);
    assert_eq!(gated.intent(&o), original.intent(&o));
    assert_eq!(last(&gated), last(&original));
    assert_eq!(last(&gated).deadline_tick, 820);
    assert_eq!(
        without_defense_options(&gated),
        without_defense_options(&original)
    );
    assert_eq!(gated.deferred, original.deferred);
    let check = gated
        .telemetry
        .acquisition_clearance
        .as_ref()
        .unwrap()
        .last
        .as_ref()
        .unwrap();
    assert_eq!(check.decision, "admitted");
    assert!(check.estimated_clearance >= 0.0);
    assert_eq!(check.estimated_clearance, last(&gated).estimated_clearance);
    assert_eq!(check.capture, last(&gated).capture);
}

#[test]
fn rejection_does_not_prevent_a_later_safe_proposal_or_start_its_clock_early() {
    let (mut bot, mut o) = fixture(true);
    bot.configure_acquisition_clearance(true);
    o.boundary.radius = 170.0;
    bot.intent(&o);
    assert!(!bot.acquisition_defending());
    o.boundary.radius = 5000.0;
    o.local.combat.recovery.flight.pilot.tick = 101;
    o.local.combat.weapons.last_hit_taken_tick = Some(101);
    bot.intent(&o);
    assert!(bot.acquisition_defending());
    assert_eq!(last(&bot).started_tick, 101);
    assert_eq!(last(&bot).deadline_tick, 821);
    assert_eq!(bot.deferred, vec![(0, 1901)]);
    let gate = bot.telemetry.acquisition_clearance.as_ref().unwrap();
    assert_eq!((gate.checks, gate.rejected), (2, 1));
}

#[test]
fn duplicate_ticks_clones_and_reset_keep_the_clearance_receipt_bounded() {
    let (mut bot, mut o) = fixture(true);
    bot.configure_acquisition_clearance(true);
    o.boundary.radius = 170.0;
    let mut cloned = bot.clone();
    assert_eq!(bot.intent(&o), cloned.intent(&o));
    assert_eq!(bot.telemetry, cloned.telemetry);
    let first = bot.telemetry.clone();
    bot.intent(&o);
    assert_eq!(bot.telemetry, first);
    o.local.combat.recovery.flight.pilot.tick = 101;
    o.local.combat.weapons.last_hit_taken_tick = Some(101);
    bot.intent(&o);
    let gate = bot.telemetry.acquisition_clearance.as_ref().unwrap();
    assert_eq!((gate.checks, gate.rejected), (2, 2));
    assert_eq!(gate.last.as_ref().unwrap().tick, 101);
    bot.reset(bot.context);
    assert_eq!(
        bot.telemetry.acquisition_clearance,
        Some(AcquisitionClearance::default())
    );
    assert_eq!(
        bot.telemetry.acquisition_defense,
        Some(AcquisitionDefense::default())
    );
}

#[test]
fn clearance_is_only_checked_for_a_current_eligible_handoff() {
    for change in 0..4 {
        let (mut bot, mut o) = fixture(true);
        bot.configure_acquisition_clearance(true);
        match change {
            0 => o.local.combat.weapons.last_hit_taken_tick = Some(99),
            1 => o.local.combat.target.as_mut().unwrap().visible = false,
            2 => o.local.combat.recovery.flight.pilot.landing.supported_feet = 1,
            _ => o.local.combat.recovery.flight.pilot.controls_armed = false,
        }
        bot.intent(&o);
        assert_eq!(
            bot.telemetry.acquisition_clearance,
            Some(AcquisitionClearance::default())
        );
    }
}

#[test]
fn zero_clearance_is_admissible_but_negative_and_nonfinite_are_not() {
    for value in [0.0, -0.0, 20.0] {
        assert_eq!(clearance_decision(value), "admitted");
    }
    for value in [-f32::EPSILON, -16.47] {
        assert_eq!(clearance_decision(value), "negative_clearance");
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(clearance_decision(value), "nonfinite_forecast");
    }
}

#[test]
fn clearance_requires_defense_and_configuration_before_the_first_intent() {
    let (mut disabled, _) = fixture(false);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            disabled.configure_acquisition_clearance(true);
        }))
        .is_err()
    );
    let (mut bot, o) = fixture(true);
    assert!(
        serde_json::to_value(&bot.telemetry)
            .unwrap()
            .get("acquisition_clearance")
            .is_none()
    );
    bot.configure_acquisition_clearance(true);
    bot.configure_acquisition_defense(false);
    assert!(bot.telemetry.acquisition_clearance.is_none());
    bot.configure_acquisition_defense(true);
    bot.configure_acquisition_clearance(true);
    bot.intent(&o);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bot.configure_acquisition_clearance(false);
        }))
        .is_err()
    );
}
