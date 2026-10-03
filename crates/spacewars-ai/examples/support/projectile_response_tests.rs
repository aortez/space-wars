use super::*;
use engine_core::Vec2;

#[test]
fn native_transfers_require_explicit_scope_and_retain_physical_priorities() {
    use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
    use spacewars_ai::{BrainReset, mission_pilot::MaterialMissionPilot};
    let state = SurfaceSortieScenario::init_material_combat(42);
    let mut o = state.mission_observation(1, None);
    o.match_rules = true;
    o.local.combat.recovery.flight.flight.enabled = true;
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.queries_ready = true;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    let mut m = MaterialMissionPilot::new(
        BrainReset {
            actor: p.owner,
            episode_seed: 42,
        },
        Default::default(),
    )
    .telemetry()
    .clone();
    m.goal = MissionGoal::Transfer;
    m.goal_since = 10;
    m.target = Some(0);
    assert_eq!(response_key(Scope::Escape, &o, &m), None);
    assert_eq!(
        response_key(Scope::Transfer, &o, &m),
        Some(ResponseKey::Native {
            goal_since: 10,
            vehicle: VehicleId(1),
            destination: 0,
        })
    );
    for goal in [
        MissionGoal::Launch,
        MissionGoal::AvoidSun,
        MissionGoal::Capture,
        MissionGoal::Recover,
    ] {
        m.goal = goal;
        assert_eq!(response_key(Scope::Transfer, &o, &m), None);
    }
    m.goal = MissionGoal::Transfer;
    o.local.combat.recovery.flight.flight.enabled = false;
    assert_eq!(response_key(Scope::Transfer, &o, &m), None);
    o.local.combat.recovery.flight.flight.enabled = true;
    o.local.combat.recovery.flight.pilot.landing.supported_feet = 1;
    assert_eq!(response_key(Scope::Transfer, &o, &m), None);
    assert_eq!(response_scope("escape"), Scope::Escape);
    assert_eq!(response_scope("transfer"), Scope::Transfer);
}

#[test]
fn a_new_native_transfer_episode_cancels_without_rearming() {
    let original = ResponseKey::Native {
        goal_since: 10,
        vehicle: VehicleId(1),
        destination: 0,
    };
    let next = ResponseKey::Native {
        goal_since: 21,
        vehicle: VehicleId(1),
        destination: 0,
    };
    let mut p = pulse(Mode::Left);
    let mut intent = CombatIntent::default();
    assert!(p.step(20, Some(original), Some(threat()), &mut intent));
    assert!(!p.step(21, Some(next), Some(threat()), &mut intent));
    assert!(!p.step(22, Some(original), Some(threat()), &mut intent));
    assert_eq!(p.attempt.unwrap().applied_ticks, 1);
}

#[test]
fn escape_identity_keeps_its_existing_serialization() {
    assert_eq!(
        serde_json::to_value(ResponseKey::Escape(key())).unwrap(),
        serde_json::to_value(key()).unwrap()
    );
}

#[test]
fn response_seat_can_target_p2_without_changing_the_reporting_seat() {
    assert_eq!(response_seat("reporting", 0), 0);
    assert_eq!(response_seat("reporting", 1), 1);
    assert_eq!(response_seat("1", 0), 1);
    assert_eq!(response_seat("0", 1), 0);
}

#[test]
#[should_panic(expected = "--probe-projectile-response-seat")]
fn response_seat_rejects_an_invalid_actor() {
    response_seat("2", 0);
}

fn key() -> TransferKey {
    TransferKey {
        started_tick: 100,
        deadline_tick: 3700,
        selected_tick: 100,
        vehicle: VehicleId(0),
        destination: 2,
    }
}
fn threat() -> Threat {
    Threat {
        id: DebrisId::from_value(100001).unwrap(),
        spawn_tick: 90,
        entry_seconds: 0.9,
    }
}
fn pulse(mode: Mode) -> Pulse {
    Pulse {
        mode,
        last_tick: None,
        attempt: None,
    }
}

#[test]
fn circle_screen_matches_direct_approach_and_rejects_misses() {
    let toward = Vec2::new(-50.0, 0.0);
    assert!((circle_entry(Vec2::new(100.0, 0.0), toward, 10.0).unwrap() - 1.8).abs() < 1.0e-12);
    for (p, v) in [
        (Vec2::new(100.0, 30.0), toward),
        (Vec2::new(100.0, 0.0), -toward),
        (Vec2::new(100.0, 0.0), Vec2::new(0.0, 100.0)),
        (Vec2::new(300.0, 0.0), toward),
        (Vec2::new(100.0, 0.0), Vec2::ZERO),
    ] {
        assert_eq!(circle_entry(p, v, 10.0), None);
    }
    assert_eq!(
        circle_entry(Vec2::new(5.0, 0.0), Vec2::ZERO, 10.0),
        Some(0.0)
    );
}

#[test]
fn observe_mode_leaves_actions_exact_and_still_closes_fixed_window() {
    let mut p = pulse(Mode::Observe);
    let mut intent = CombatIntent::default();
    intent.flight.controls.horizontal = 0.3;
    intent.weapons.cannon = true;
    let original = intent;
    for tick in 200..=230 {
        assert!(!p.step(
            tick,
            Some(ResponseKey::Escape(key())),
            Some(threat()),
            &mut intent
        ));
        assert_eq!(intent, original);
    }
    let a = p.attempt.unwrap();
    assert_eq!(a.applied_ticks, 0);
    assert_eq!(a.finished_tick, Some(230));
    assert_eq!(a.source, ResponseKey::Escape(key()));
}

#[test]
fn pulses_are_thirty_wall_ticks_and_cannot_rearm_from_repeated_threats() {
    for mode in [Mode::Brake, Mode::Left, Mode::Right] {
        let mut p = pulse(mode);
        for tick in 200..=250 {
            let mut intent = CombatIntent::default();
            let original = intent;
            assert_eq!(
                p.step(
                    tick,
                    Some(ResponseKey::Escape(key())),
                    Some(threat()),
                    &mut intent
                ),
                tick < 230
            );
            if tick >= 230 {
                assert_eq!(intent, original);
            }
        }
        let a = p.attempt.unwrap();
        assert_eq!(a.applied_ticks, 30);
        assert_eq!(
            (a.started_tick, a.end_tick, a.finished_tick),
            (200, 230, Some(230))
        );
    }
}

#[test]
fn native_priority_and_identity_changes_cancel_permanently() {
    for changed in [
        None,
        Some(TransferKey {
            vehicle: VehicleId(1),
            ..key()
        }),
        Some(TransferKey {
            selected_tick: 101,
            ..key()
        }),
        Some(TransferKey {
            destination: 1,
            ..key()
        }),
        Some(TransferKey {
            deadline_tick: 3800,
            ..key()
        }),
    ] {
        let mut p = pulse(Mode::Left);
        let mut intent = CombatIntent::default();
        assert!(p.step(
            200,
            Some(ResponseKey::Escape(key())),
            Some(threat()),
            &mut intent
        ));
        let mut original = CombatIntent::default();
        assert!(!p.step(
            201,
            changed.map(ResponseKey::Escape),
            Some(threat()),
            &mut original
        ));
        assert_eq!(original, CombatIntent::default());
        assert!(!p.step(
            202,
            Some(ResponseKey::Escape(key())),
            Some(threat()),
            &mut original
        ));
        assert_eq!(p.attempt.unwrap().applied_ticks, 1);
    }
}

#[test]
fn steering_retains_braking_and_weapons_while_brake_preserves_turn() {
    for mode in [Mode::Brake, Mode::Left, Mode::Right] {
        for brake in [false, true] {
            let mut intent = CombatIntent::default();
            intent.flight.controls.horizontal = 0.25;
            intent.flight.controls.brake_held = brake;
            intent.flight.wings.closed = true;
            intent.weapons.laser = true;
            intent.weapons.cannon = true;
            apply(mode, &mut intent);
            assert!(intent.weapons.laser && intent.weapons.cannon);
            assert!(!intent.flight.wings.closed && !intent.flight.controls.interact_held);
            let c = intent.flight.controls;
            if mode == Mode::Brake {
                assert!(c.brake_held && !c.primary_held);
                assert_eq!(c.horizontal, 0.25);
            } else {
                assert_eq!(c.brake_held, brake);
                assert_eq!(c.primary_held, !brake);
            }
        }
    }
}

#[test]
fn no_eligible_transfer_or_threat_cannot_arm() {
    let mut p = pulse(Mode::Brake);
    let mut intent = CombatIntent::default();
    assert!(!p.step(200, None, Some(threat()), &mut intent));
    assert!(!p.step(201, Some(ResponseKey::Escape(key())), None, &mut intent));
    assert!(p.attempt.is_none());
}

#[test]
#[should_panic(expected = "one forward observation per tick")]
fn duplicate_ticks_cannot_extend_or_double_count_a_pulse() {
    let mut p = pulse(Mode::Brake);
    let mut intent = CombatIntent::default();
    p.step(
        200,
        Some(ResponseKey::Escape(key())),
        Some(threat()),
        &mut intent,
    );
    p.step(
        200,
        Some(ResponseKey::Escape(key())),
        Some(threat()),
        &mut intent,
    );
}
