use super::*;
use compatibility::{GeneratedSurfaceCase, ProbeKind, ProbeOutcome};

#[test]
fn surface_v1_initial_matrix_passes_without_changing_probe_criteria() {
    let mut cases = 0;
    for seed in 0..4 {
        for planet in 0..GeneratedSurfaceCase::planet_count(seed) {
            for bearing in 0..4 {
                let case = GeneratedSurfaceCase::new(seed, planet, bearing)
                    .with_profile(GeneratedSurfaceProfile::SurfaceV1);
                let report = case.run().unwrap();
                for probe in report.probes {
                    assert_eq!(probe.outcome, ProbeOutcome::Passed, "{case:?}: {probe:?}");
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 72);
}

#[test]
fn surface_v1_leaves_room_outside_outermost_planets_but_retains_approach_limits() {
    // This planet originally has only 2.5 units before the wall: the ship
    // doesn't fit on its outward side. Profile clearance fixes the fixture.
    let outer =
        GeneratedSurfaceCase::new(13, 4, 0).with_profile(GeneratedSurfaceProfile::SurfaceV1);
    for result in outer.run().unwrap().probes {
        assert_eq!(result.outcome, ProbeOutcome::Passed, "{result:?}");
    }
    // A broader matrix still exposes an imperfect passive approach. Retain it
    // as reproducible evidence, not a claim of guaranteed automatic landing.
    let report = GeneratedSurfaceCase::new(30, 4, 2)
        .with_profile(GeneratedSurfaceProfile::SurfaceV1)
        .run()
        .unwrap();
    assert_eq!(report.probes[0].outcome, ProbeOutcome::Failed);
    assert!(report.probes[0].final_landing.angle_degrees > 90.0);
    assert!(
        report.probes[1..]
            .iter()
            .all(|probe| probe.outcome == ProbeOutcome::Passed)
    );
}

#[test]
fn profile_reports_replay_and_old_case_identifiers_default_to_raw() {
    let old: GeneratedSurfaceCase =
        serde_json::from_str(r#"{"seed":0,"planet":2,"bearing":1}"#).unwrap();
    assert_eq!(old, GeneratedSurfaceCase::new(0, 2, 1));
    let case = old.with_profile(GeneratedSurfaceProfile::SurfaceV1);
    let report = case.run().unwrap();
    assert_eq!(report.version, 2);
    assert_eq!(report, case.run().unwrap());
    assert_eq!(
        serde_json::from_str::<GeneratedSurfaceCase>(&serde_json::to_string(&case).unwrap())
            .unwrap(),
        case
    );
}

#[test]
fn surface_v1_supports_the_existing_outpost_round_trip_on_generated_planets() {
    for (seed, planet, bearing) in [(0, 0, 0), (0, 3, 2), (1, 2, 1), (2, 5, 3)] {
        let case = GeneratedSurfaceCase::new(seed, planet, bearing)
            .with_profile(GeneratedSurfaceProfile::SurfaceV1);
        eprintln!("round trip {case:?}");
        super::outpost_tests::round_trip_state(case.init().unwrap());
    }
}

#[test]
fn surface_v1_jump_and_coasting_use_the_shared_field_without_transport() {
    for (seed, planet, bearing) in [(0, 0, 0), (0, 3, 2), (1, 2, 1), (2, 5, 3)] {
        let case = GeneratedSurfaceCase::new(seed, planet, bearing)
            .with_profile(GeneratedSurfaceProfile::SurfaceV1);
        super::motion_tests::free_flight_without_transport(|| case.init().unwrap());
    }
}

#[test]
fn surface_v1_extended_idle_support() {
    for (seed, planet, bearing) in [(0, 0, 0), (0, 3, 2), (1, 2, 1), (2, 5, 3)] {
        let case = GeneratedSurfaceCase::new(seed, planet, bearing)
            .with_profile(GeneratedSurfaceProfile::SurfaceV1);
        let mut state = case.init().unwrap();
        idle(&mut state, 120);
        assert!(state.vehicle_settled(), "{case:?}");
        disembark(&mut state);
        let bodies = state.world.physics.world.body_count();
        state.motion_metrics = SurfaceMotionMetrics::default();
        state.idle_anchor = None;
        let mut unsupported_streak = 0;
        let mut longest_gap = 0;
        let mut max_hatch_distance = 0.0_f32;
        for _ in 0..12000 {
            idle(&mut state, 1);
            let snapshot = state.spaceling_snapshot().unwrap();
            unsupported_streak = if snapshot.grounded() {
                0
            } else {
                unsupported_streak + 1
            };
            longest_gap = longest_gap.max(unsupported_streak);
            max_hatch_distance = max_hatch_distance.max(
                snapshot
                    .motion
                    .position
                    .distance_to(state.access_position()),
            );
        }
        let metrics = state.motion_metrics;
        eprintln!(
            "extended {case:?}: {metrics:?}, longest_gap={longest_gap}, max_hatch={max_hatch_distance}, hatch={}",
            state
                .spaceling_snapshot()
                .unwrap()
                .motion
                .position
                .distance_to(state.access_position())
        );
        assert_eq!(state.world.physics.world.body_count(), bodies);
        assert_eq!(metrics.knockdowns, 0);
        assert_eq!(metrics.ship_damage, 0.0);
        // Unlike the 10-second probe, this crosses substantial orbital/spin
        // motion. Bound sustained separation, not idealized magnetic contact.
        assert!(metrics.supported_ticks >= 11880, "{case:?}: {metrics:?}");
        assert!(
            longest_gap <= 30,
            "{case:?}: {longest_gap} ticks unsupported"
        );
        assert!(
            max_hatch_distance < 5.0,
            "{case:?}: hatch {max_hatch_distance}"
        );
        assert_eq!(metrics.ship_support_losses, 0);
    }
}

#[test]
fn surface_v1_changes_only_the_explicit_world_profile_not_actors_or_geometry() {
    for seed in 0..4 {
        let case = GeneratedSurfaceCase::new(seed, 0, 0);
        let raw = case.init().unwrap();
        let tuned = case
            .with_profile(GeneratedSurfaceProfile::SurfaceV1)
            .init()
            .unwrap();
        let mut expected_config = raw.world.config.clone();
        expected_config.universe_radius += profiles::FLIGHT_MARGIN;
        assert_eq!(expected_config, tuned.world.config);
        assert_eq!(
            raw.world.physics.world.body_count(),
            tuned.world.physics.world.body_count()
        );
        assert_eq!(
            raw.world.physics.world.collider_count(),
            tuned.world.physics.world.collider_count()
        );
        let sun = tuned.world.sun.unwrap();
        assert_eq!(
            sun.position,
            raw.world.sun.unwrap().position + Vec2::splat(profiles::FLIGHT_MARGIN as f32)
        );
        assert_eq!(sun.radius, raw.world.sun.unwrap().radius);
        for (original, planet) in raw.world.planets.iter().zip(&tuned.world.planets) {
            assert_eq!(original.radius, planet.radius);
            assert_eq!(
                original.position + Vec2::splat(profiles::FLIGHT_MARGIN as f32),
                planet.position
            );
            assert_eq!(original.orbit_radius, planet.orbit_radius);
            assert_eq!(original.orbit_angle, planet.orbit_angle);
            assert_eq!(original.wrapper_angle, planet.wrapper_angle);
            let standing_radius =
                planet.radius * BODY_BOUNDS_RADIUS_SCALE + SurfaceSortieState::spec().half_height();
            let gravity = 60.0 * GRAVITY * planet.mass / standing_radius.powi(2);
            assert!((gravity - profiles::SURFACE_GRAVITY).abs() < 1.0e-4);
            assert!(planet.wrapper_omega.abs() <= profiles::MAX_SPIN);
            assert!(
                planet.wrapper_omega.powi(2) * standing_radius
                    <= gravity * profiles::SPIN_GRAVITY_FRACTION
            );
            assert_eq!(
                planet.wrapper_omega.signum(),
                original.wrapper_omega.signum()
            );
            assert_eq!(planet.orbit_omega.signum(), original.orbit_omega.signum());
            let acceleration = planet.orbit_omega.powi(2) * planet.orbit_radius;
            let central = 60.0 * GRAVITY * sun.mass / planet.orbit_radius.powi(2);
            assert!((acceleration - central).abs() < 1.0e-5);
        }
        let environment = compatibility::SurfaceEnvironment::read(&tuned);
        assert_eq!(environment.ship_thrust_acceleration, 45.0);
        assert_eq!(environment.spaceling_jump_speed, 8.0);
        assert!(tuned.world.planets.iter().all(|planet| planet.mass > 0.0));
    }
}

fn controlled(radius: f32, gravity: f32, spin: f32) -> SurfaceSortieState {
    let mut world = SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 7).world;
    let planet = &mut world.planets[0];
    planet.radius = radius;
    planet.mass = gravity / (60.0 * GRAVITY)
        * (radius * BODY_BOUNDS_RADIUS_SCALE + SurfaceSortieState::spec().half_height()).powi(2);
    planet.wrapper_omega = spin;
    SurfaceSortieScenario::on_surface(
        world,
        SurfaceMotionPreset::Stationary,
        0,
        Vec2::Y,
        -20.4 / radius,
    )
}

#[test]
fn surface_controller_envelope() {
    // One-factor probes, not a promise that every combination is safe. Keep
    // this visible alongside the generated matrix; blocked stages stay blocked.
    for (radius, gravity, spin) in [
        (15.0, 18.0, 0.015),
        (60.0, 18.0, 0.015),
        (150.0, 18.0, 0.015),
        (60.0, 6.0, 0.015),
        (60.0, 36.0, 0.015),
        (60.0, 90.0, 0.015),
        (60.0, 180.0, 0.015),
        (60.0, 700.0, 0.015),
        (60.0, 18.0, 0.08),
        (60.0, 18.0, 0.25),
        (60.0, 18.0, 0.5),
        (60.0, 18.0, -0.5),
    ] {
        let results = ProbeKind::ALL
            .map(|kind| compatibility::run_probe(controlled(radius, gravity, spin), kind));
        eprintln!(
            "radius={radius} gravity={gravity} spin={spin}: {}",
            results
                .iter()
                .map(|r| format!(
                    "{:?}={:?}({:.2})",
                    r.kind,
                    r.outcome,
                    r.distance.unwrap_or(r.metrics.ship_damage)
                ))
                .collect::<Vec<_>>()
                .join(" ")
        );
        for result in &results {
            if (gravity == 18.0 && spin <= 0.08 && spin > 0.0) || gravity == 36.0 {
                assert_eq!(result.outcome, ProbeOutcome::Passed, "{result:?}");
            }
            assert!(result.distance.is_none_or(f32::is_finite));
            assert!(result.metrics.supported_ticks <= result.ticks);
            assert!(serde_json::to_vec(result).is_ok());
        }
        if gravity >= 90.0 {
            assert_eq!(
                results[4].outcome,
                ProbeOutcome::Failed,
                "insufficient thrust: {:?}",
                results[4]
            );
            assert!(results[4].distance.unwrap() < 1.0);
        }
    }
}

#[test]
fn compatibility_probes_pass_on_the_known_good_stationary_and_orbital_labs() {
    for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
        for kind in ProbeKind::ALL {
            let result = compatibility::run_probe(SurfaceSortieScenario::init(preset, 7), kind);
            assert_eq!(
                result.outcome,
                ProbeOutcome::Passed,
                "{preset:?} {result:?}"
            );
        }
    }
}

#[test]
fn generated_fixture_preserves_all_world_sources_and_the_selected_planet_identity() {
    let case = GeneratedSurfaceCase::new(0, 2, 3);
    let mut state = case.init().unwrap();
    let (sun, planets) = build_world(&SpacewarsConfig::default(), case.seed);
    assert_eq!(state.world.planets, planets);
    assert_eq!(state.world.sun, sun);
    assert_eq!(GeneratedSurfaceCase::planet_count(case.seed), planets.len());
    assert_eq!(state.pilot.planet, case.planet);
    assert_eq!(state.outposts[0].planet, case.planet);
    assert_eq!(state.observation().generated_case, Some(case));
    assert_eq!(state.world.physics.world.body_count(), planets.len() + 3);
    let bodies = state.world.physics.world.body_count();
    idle(&mut state, 1);
    for (index, mut expected) in planets.into_iter().enumerate() {
        expected.update_orbit(
            sun.unwrap().position,
            Duration::from_secs_f64(1.0 / 60.0).as_secs_f32(),
        );
        let actual = state.world.planets[index];
        assert_eq!(actual.position, expected.position);
        assert!(
            (actual.wrapper_angle - expected.wrapper_angle.rem_euclid(std::f32::consts::TAU)).abs()
                < 1.0e-6
        );
        assert_eq!(actual.mass, expected.mass);
        assert_eq!(actual.orbit_omega, expected.orbit_omega);
    }
    assert_eq!(state.world.physics.world.body_count(), bodies);
    assert_eq!(
        state.planet_motion().position,
        state.world.planets[case.planet].position
    );
    assert!(!state.world.physics.ship_is_constrained(0));
    assert!(
        state
            .world
            .gravity_participants
            .iter()
            .any(|p| p.id == tagged_gravity_id(GRAVITY_BODY_TAG, 0))
    );
}

#[test]
fn generated_reports_reproduce_the_existing_gravity_and_takeoff_mismatch() {
    let report = GeneratedSurfaceCase::new(0, 0, 0).run().unwrap();
    let e = &report.environment;
    assert!(e.own_surface_gravity > 690.0 && e.own_surface_gravity < 725.0);
    assert!(e.frame_acceleration_mismatch > 150.0);
    assert!(e.effective_inward_acceleration > e.ship_thrust_acceleration * 10.0);
    let takeoff = report
        .probes
        .iter()
        .find(|p| p.kind == ProbeKind::Takeoff)
        .unwrap();
    assert_eq!(takeoff.outcome, ProbeOutcome::Failed);
    assert_eq!(takeoff.ticks, 180, "failure must be measured, not skipped");
    assert!(takeoff.distance.unwrap() < 1.0);
    assert_eq!(takeoff.final_landing.supported_feet, 2);
    let landing = &report.probes[0];
    assert_eq!(landing.outcome, ProbeOutcome::Failed);
    assert!(
        landing.metrics.ship_damage > 0.0,
        "touchdown is not necessarily a safe landing"
    );
}

#[test]
fn blocked_on_foot_probes_are_not_counted_as_physics_success_or_attempts() {
    let result = compatibility::run_probe(
        GeneratedSurfaceCase::new(0, 0, 0).init().unwrap(),
        ProbeKind::Jump,
    );
    assert_eq!(result.outcome, ProbeOutcome::Blocked);
    assert_eq!(result.reason, "balanced_support_prerequisite");
    assert_eq!(result.ticks, 0);
    assert_eq!(result.distance, None);
    assert_eq!(result.metrics.jumps, 0);
    assert!(result.setup_metrics.on_foot_ticks > 0);
    assert!(result.setup_metrics.knockdowns > 0);
    assert!(!result.final_pilot.unwrap().balanced);
}

#[test]
fn generated_field_report_matches_the_shared_solver_and_support_is_planet_specific() {
    assert_eq!(
        GRAVITY_SOFTENING, 0.0,
        "update diagnostic field formula if the kernel changes"
    );
    let mut state = GeneratedSurfaceCase::new(0, 2, 0).init().unwrap();
    let environment = compatibility::SurfaceEnvironment::read(&state);
    idle(&mut state, 1);
    assert!(
        (state.pilot.ship_gravity_delta * 60.0 - environment.gravity_at_ship_spawn).length() < 0.01
    );
    for _ in 0..600 {
        if state.vehicle_settled() {
            break;
        }
        idle(&mut state, 1);
    }
    assert!(state.vehicle_settled());
    let planet = state.world.planets[2];
    let up = (state.world.ships[0].position + SHIP_PIVOT - planet.position).normalized();
    assert_eq!(state.world.physics.landing_feet_supported(0, 2, up), 2);
    assert_eq!(state.world.physics.landing_feet_supported(0, 0, up), 0);
}

#[test]
fn generated_reports_replay_and_validate_case_identifiers() {
    let case = GeneratedSurfaceCase::new(0, 2, 1);
    assert_eq!(case.run().unwrap(), case.run().unwrap());
    let state = case.init().unwrap();
    let bytes = SurfaceSortieScenario::observe(&state).payload;
    assert_eq!(bytes, SurfaceSortieScenario::observe(&state).payload);
    assert_eq!(
        bytes,
        SurfaceSortieScenario::observe(&case.init().unwrap()).payload
    );
    assert!(GeneratedSurfaceCase::new(0, usize::MAX, 0).init().is_err());
    assert!(GeneratedSurfaceCase::new(0, 0, 4).init().is_err());
}
