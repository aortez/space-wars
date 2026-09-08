use super::*;

#[test]
fn orbital_support_handles_opposite_and_faster_orbit_and_spin() {
    for (orbit, spin) in [(0.11_f32, 0.08_f32), (-0.11, -0.08), (-0.065, 0.015)] {
        let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, 7);
        let planet = &mut state.world.planets[0];
        planet.orbit_omega = orbit;
        planet.wrapper_omega = spin;
        state.world.sun.as_mut().unwrap().mass =
            orbit.powi(2) * planet.orbit_radius.powi(3) / (60.0 * GRAVITY);
        state.world.ships[0].velocity =
            planet_surface_velocity(planet, state.world.ships[0].position + SHIP_PIVOT);
        state.world.ships[0].omega = spin;
        idle(&mut state, 120);
        assert!(
            state.vehicle_settled(0),
            "{orbit}/{spin}: {:?}",
            state.observation(0)
        );
        disembark(&mut state);
        let mut supported = 0;
        for _ in 0..3600 {
            idle(&mut state, 1);
            supported += usize::from(state.spaceling_snapshot(0).unwrap().grounded());
        }
        assert!(
            supported > 3500,
            "{orbit}/{spin}: {:?}",
            state.observation(0)
        );
        assert_eq!(state.pilots[0].motion_metrics.ship_damage, 0.0);
        assert_eq!(state.pilots[0].motion_metrics.knockdowns, 0);
        assert!(
            state.pilots[0].motion_metrics.max_idle_drift < 1.0,
            "{orbit}/{spin}: {:?}",
            state.pilots[0].motion_metrics
        );
        eprintln!(
            "rates orbit={orbit} spin={spin}: {}",
            serde_json::to_string(&state.pilots[0].motion_metrics).unwrap()
        );
    }
}

#[test]
fn single_pilot_fixture_does_not_simulate_the_unused_ship_slot() {
    for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
        let mut state = SurfaceSortieScenario::init(preset, 7);
        let inactive = state.world.physics.ship_body(1);
        assert_eq!(state.world.physics.world.motion(inactive), None);
        // Even an overlapping legacy slot cannot create contacts or reappear
        // through reconciliation. Only the pilot's vehicle enters the field.
        state.world.ships[1].position = state.world.ships[0].position;
        idle(&mut state, 120);
        assert_eq!(state.world.physics.world.motion(inactive), None);
        assert!(state.vehicle_settled(0));
        assert_eq!(state.pilots[0].motion_metrics.ship_damage, 0.0);
        assert!(
            !state
                .world
                .gravity_participants
                .iter()
                .any(|participant| { participant.id == tagged_gravity_id(GRAVITY_SHIP_TAG, 1) })
        );
    }
}

#[test]
fn motion_metrics_preserve_damage_through_repair_and_reset_only_current_idle_drift() {
    let mut state = parked();
    idle(&mut state, 120);
    let maximum = state.pilots[0].motion_metrics.max_idle_drift;
    assert!(state.pilots[0].motion_metrics.idle_drift > 0.0);
    assert!(state.pilots[0].idle_anchor.is_some());
    tick(
        &mut state,
        SurfaceSortieAction {
            brake_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert_eq!(state.pilots[0].motion_metrics.idle_drift, 0.0);
    assert_eq!(state.pilots[0].idle_anchor, None);
    assert_eq!(state.pilots[0].motion_metrics.max_idle_drift, maximum);
    idle(&mut state, 1);
    assert!(state.pilots[0].idle_anchor.is_some());
    assert_eq!(state.pilots[0].motion_metrics.idle_drift, 0.0);

    // Isolate the counter's before/after contract from impact tuning. Recording
    // happens before the repair service; even immediate healing cannot mask it.
    let before = motion::StepSample::read(&state, 0);
    state.world.ships[0].life -= 10.0;
    state.record_motion_step(0, before, SurfaceSortieAction::default());
    assert_eq!(state.pilots[0].motion_metrics.ship_damage, 10.0);
    state.outposts[0].owner = Some(state.pilots[0].owner);
    let health = state.world.ships[0].life;
    idle(&mut state, 120);
    assert!(state.world.ships[0].life > health);
    assert_eq!(state.pilots[0].motion_metrics.ship_damage, 10.0);

    let before = motion::StepSample::read(&state, 0);
    let remaining = state.world.ships[0].life;
    state.world.ships[0].change_to_escape_pod();
    state.record_motion_step(0, before, SurfaceSortieAction::default());
    assert_eq!(state.pilots[0].motion_metrics.ship_damage, 10.0 + remaining);
}

#[test]
fn first_completed_surface_frame_has_the_presets_origin_velocity() {
    for preset in [
        SurfaceMotionPreset::Stationary,
        SurfaceMotionPreset::Translating,
        SurfaceMotionPreset::Orbit,
    ] {
        let mut state = SurfaceSortieScenario::init(preset, 7);
        let origin = state.world.planets[0].position;
        idle(&mut state, 1);
        let motion = state.motion_observation(0);
        let expected = (state.world.planets[0].position - origin) * 60.0;
        assert!(
            (motion.planet_velocity - expected).length() < 0.01,
            "{preset:?}: {motion:?}"
        );
    }
}

#[test]
fn orbital_approaches_land_from_eight_bearings_without_damage() {
    for bearing in 0..8 {
        let mut state = approach_in(
            SurfaceMotionPreset::Orbit,
            bearing as f32 * std::f32::consts::TAU / 8.0,
            18.0,
            5.0_f32.to_radians(),
            5.0,
            3.0,
        );
        for _ in 0..900 {
            idle(&mut state, 1);
            if state.vehicle_settled(0) {
                break;
            }
        }
        assert!(
            state.vehicle_settled(0),
            "bearing={bearing} {:?}",
            state.observation(0)
        );
        assert_eq!(state.pilots[0].motion_metrics.ship_damage, 0.0);
        disembark(&mut state);
        interact(&mut state);
        assert_eq!(state.pilots[0].last_transfer, TransferResult::Boarded);
    }
}

#[test]
fn orbiting_jump_and_ship_takeoff_inherit_motion_then_fly_independently() {
    free_flight_without_transport(|| SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, 7));
}

pub(super) fn free_flight_without_transport(init: impl Fn() -> SurfaceSortieState) {
    let mut state = init();
    idle(&mut state, 120);
    disembark(&mut state);
    let snapshot = state.spaceling_snapshot(0).unwrap();
    let velocity = snapshot.motion.linear_velocity;
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert_eq!(state.pilots[0].motion_metrics.jumps, 1);
    assert_eq!(
        state.pilots[0].motion_metrics.pilot_support_losses, 0,
        "a jump isn't unexpected support loss"
    );
    let after = state.spaceling_snapshot(0).unwrap();
    let right = Vec2::new(snapshot.up.y, -snapshot.up.x);
    assert!(
        (after.motion.linear_velocity - velocity).dot(right).abs() < 0.2,
        "inherit tangential momentum"
    );
    assert!(after.motion.linear_velocity.dot(snapshot.up) > velocity.dot(snapshot.up) + 4.0);
    // After separation, neutral air control applies no linear change beyond
    // the shared gravity field. Moving terrain must not carry a jumping actor.
    idle(&mut state, 2);
    for _ in 0..12 {
        let before = state.spaceling_snapshot(0).unwrap();
        assert!(!before.grounded());
        idle(&mut state, 1);
        let after = state.spaceling_snapshot(0).unwrap();
        let expected = before.motion.linear_velocity + state.pilots[0].gravity / 60.0;
        assert!(
            (after.motion.linear_velocity - expected).length() < 1.0e-4,
            "no air transport: {after:?}"
        );
    }
    idle(&mut state, 180);
    assert!(state.spaceling_snapshot(0).unwrap().grounded());

    // A fresh, reproducible fixture exercises takeoff through player controls.
    let mut state = init();
    idle(&mut state, 120);
    let before = state.world.ships[0].velocity;
    tick(
        &mut state,
        SurfaceSortieAction {
            primary_held: true,
            ..SurfaceSortieAction::default()
        },
    );
    assert!(
        (state.world.ships[0].velocity - before).length() < 1.0,
        "no hidden launch impulse"
    );
    assert_eq!(state.pilots[0].motion_metrics.departures, 1);
    assert!(!state.world.physics.ship_is_constrained(0));
    for _ in 0..100 {
        tick(
            &mut state,
            SurfaceSortieAction {
                primary_held: true,
                ..SurfaceSortieAction::default()
            },
        );
    }
    assert!(state.pilots[0].landing.altitude > 25.0);
    for _ in 0..10 {
        let velocity = state.world.ships[0].velocity;
        idle(&mut state, 1);
        let expected = velocity + state.pilots[0].ship_gravity_delta;
        assert!(
            (state.world.ships[0].velocity - expected).length() < 1.0e-4,
            "ship coasts independently outside assist range"
        );
    }
}

#[test]
fn excessive_support_acceleration_causes_separation_and_stops_capture_and_repair() {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, 7);
    idle(&mut state, 120);
    disembark(&mut state);
    super::outpost_tests::walk_to(&mut state, super::outpost_tests::terminal_approach);
    idle(&mut state, 40);
    assert!(
        state.outposts[0]
            .observation(&state.world.planets[0], 0)
            .capture_progress
            > 0.0
    );
    // Deliberately exceed both controllers' acceleration budgets. Do not add
    // magnetic attachment to make this unsafe scripted trajectory "pass".
    state.world.planets[0].orbit_omega = 0.8;
    idle(&mut state, 30);
    assert!(!state.spaceling_snapshot(0).unwrap().grounded());
    assert!(!state.vehicle_settled(0));
    assert_eq!(
        state.outposts[0]
            .observation(&state.world.planets[0], 0)
            .capture_progress,
        0.0
    );
    assert!(state.pilots[0].motion_metrics.pilot_support_losses > 0);
    state.outposts[0].owner = Some(state.pilots[0].owner);
    let healed = state.outposts[0]
        .observation(&state.world.planets[0], 0)
        .repaired_health;
    idle(&mut state, 30);
    assert_ne!(
        state.outposts[0]
            .observation(&state.world.planets[0], 0)
            .repair_status,
        RepairStatus::Repairing
    );
    assert_eq!(
        state.outposts[0]
            .observation(&state.world.planets[0], 0)
            .repaired_health,
        healed
    );
}

#[test]
fn orbital_motion_metrics_and_rendering_replay_and_restart_deterministically() {
    for preset in [
        SurfaceMotionPreset::Stationary,
        SurfaceMotionPreset::Translating,
        SurfaceMotionPreset::Orbit,
    ] {
        let mut a = SurfaceSortieScenario::init(preset, 123);
        let mut b = SurfaceSortieScenario::init(preset, 123);
        let initial = SurfaceSortieScenario::observe(&a).payload;
        for frame in 0..500 {
            let input = SurfaceSortieAction {
                interact_held: frame == 120,
                horizontal: if (250..300).contains(&frame) {
                    1.0
                } else {
                    0.0
                },
                primary_held: frame == 350,
                ..SurfaceSortieAction::default()
            };
            tick(&mut a, input);
            tick(&mut b, input);
            assert_eq!(a.observation(0), b.observation(0));
        }
        let before = SurfaceSortieScenario::observe(&a).payload;
        for _ in 0..3 {
            SurfaceSortieScenario::render_frame(&a);
            SurfaceSortieScenario::minimap_frame(&a, 0, 9.0 / 16.0);
            SurfaceSortieScenario::step(&mut a, &[], Duration::ZERO);
        }
        assert_eq!(SurfaceSortieScenario::observe(&a).payload, before);
        assert_eq!(
            SurfaceSortieScenario::observe(&SurfaceSortieScenario::init(preset, 123)).payload,
            initial
        );
        assert_eq!(a.observation(0).version, 10);
        assert!(a.pilots[0].motion_metrics.on_foot_ticks > 0);
        assert_eq!(a.pilots[0].motion_metrics.jumps, 1);
        assert_eq!(a.pilots[0].motion_metrics.ship_damage, 0.0);
    }
}

#[test]
fn orbital_frame_has_matched_external_gravity_without_a_follow_force() {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, 7);
    idle(&mut state, 1);
    for _ in 0..120 {
        let observation = state.motion_observation(0);
        assert!(
            (observation.scripted_acceleration - observation.external_gravity_at_center).length()
                < 1.0e-5
        );
        assert!(observation.scripted_acceleration.length() > 0.9);
        idle(&mut state, 1);
    }
    // Ordinary worlds do not necessarily have this agreement. The diagnostic
    // exposes the difference; it does not silently subtract or add a force.
    state.world.sun.as_mut().unwrap().mass *= 10.0;
    let observation = state.motion_observation(0);
    assert!(
        (observation.scripted_acceleration - observation.external_gravity_at_center).length() > 8.0
    );
}

#[test]
fn landing_uses_completed_terrain_pose_while_the_next_pose_is_scheduled() {
    let state = parked();
    let planet = state.world.planets[0];
    let before = LandingTelemetry::measure(&state.world.physics, 0, 0, &planet);
    let mut next = planet;
    next.position += Vec2::new(20.0, -10.0);
    next.wrapper_angle += 0.2;
    // In the canonical step, scenario terrain already describes the next
    // kinematic target when control reads the previous completed contacts.
    let scheduled = LandingTelemetry::measure(&state.world.physics, 0, 0, &next);
    assert_eq!(before, scheduled);
}

#[test]
fn a_sortie_with_a_sun_advances_planet_spin_only_once_per_tick() {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 7);
    state.world.sun = Some(SunState {
        position: state.world.planets[0].position,
        radius: 1.0,
        mass: 0.0,
        color: Color::YELLOW,
    });
    let before = state.world.planets[0].wrapper_angle;
    let expected = before + state.world.planets[0].wrapper_omega / 60.0;
    idle(&mut state, 1);
    assert!((state.world.planets[0].wrapper_angle - expected).abs() < 1.0e-7);
}

#[test]
fn orbiting_and_translating_sorties_stay_supported_without_actor_transport() {
    for (preset, ticks) in [
        (SurfaceMotionPreset::Translating, 1200),
        (SurfaceMotionPreset::Orbit, 12000),
    ] {
        let mut state = SurfaceSortieScenario::init(preset, 7);
        idle(&mut state, 120);
        assert!(state.vehicle_settled(0), "{:?}", state.observation(0));
        disembark(&mut state);
        let bodies = state.world.physics.world.body_count();
        let colliders = state.world.physics.world.collider_count();
        let health = state.world.ships[0].life;
        let mut supported = 0;
        let mut landed = 0;
        let mut max_hatch_distance = 0.0_f32;
        let started = Instant::now();
        for _ in 0..ticks {
            idle(&mut state, 1);
            let snapshot = state.spaceling_snapshot(0).unwrap();
            supported += usize::from(snapshot.grounded());
            assert_eq!(state.world.physics.world.body_count(), bodies);
            assert_eq!(state.world.physics.world.collider_count(), colliders);
            landed += usize::from(state.vehicle_settled(0));
            max_hatch_distance = max_hatch_distance.max(
                snapshot
                    .motion
                    .position
                    .distance_to(state.access_position(0)),
            );
        }
        eprintln!(
            "{preset:?}: supported={supported}/{ticks} landed={landed}/{ticks} hatch_max={max_hatch_distance:.3} health={} bodies={} ticks/s={:.0}",
            state.world.ships[0].life,
            state.world.physics.world.body_count(),
            ticks as f64 / started.elapsed().as_secs_f64(),
        );
        assert!(supported > ticks * 98 / 100, "{:?}", state.observation(0));
        assert!(landed > ticks * 98 / 100, "{:?}", state.observation(0));
        assert!(max_hatch_distance < BOARDING_RANGE);
        assert_eq!(state.spaceling_snapshot(0).unwrap().knockdowns, 0);
        assert_eq!(state.world.ships[0].life, health);
        assert_eq!(state.world.physics.world.body_count(), bodies);
        assert_eq!(state.pilots[0].motion_metrics.ship_damage, 0.0);
        assert_eq!(state.pilots[0].motion_metrics.pilot_support_losses, 0);
        eprintln!(
            "metrics: {}",
            serde_json::to_string(&state.pilots[0].motion_metrics).unwrap()
        );
        interact(&mut state);
        assert_eq!(state.pilots[0].last_transfer, TransferResult::Boarded);
    }
}

#[test]
fn physical_spin_matches_prescribed_motion_during_landing() {
    let mut state = approach(
        3.0 * std::f32::consts::TAU / 8.0,
        18.0,
        5.0_f32.to_radians(),
        5.0,
        3.0,
    );
    for _ in 0..900 {
        idle(&mut state, 1);
        let surface = state.planet_motion(0);
        assert!(
            (surface.angular_velocity - state.world.planets[0].wrapper_omega).abs() < 0.001,
            "tick {}: prescribed spin {}, completed motion {surface:?}",
            state.world.tick,
            state.world.planets[0].wrapper_omega
        );
    }
}

#[test]
fn frame_velocity_matches_motion_of_the_surface_not_its_offset_center_of_mass() {
    let state = parked();
    let planet = state.world.planets[0];
    let frame = state.planet_motion(0);
    for point in [
        planet.position,
        state.access_position(0),
        state.outposts[0].position(&planet),
    ] {
        let actual = state
            .world
            .physics
            .world
            .velocity_at_point(state.world.physics.planet_body(0), point)
            .unwrap();
        assert!((motion::point_velocity(frame, point) - actual).length() < 1.0e-6);
        // Kinematic velocity is a finite-step difference of f32 poses near
        // (500, 500), not the exact analytic endpoint derivative.
        assert!(
            (motion::point_velocity(frame, point) - planet_surface_velocity(&planet, point))
                .length()
                < 0.01,
            "frame={frame:?} point={point:?}"
        );
    }
}
