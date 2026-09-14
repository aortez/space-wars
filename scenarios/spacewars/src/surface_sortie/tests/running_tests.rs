use super::*;

fn runner(radius: f32, gravity: f32, spin: f32) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 42);
    state.outposts.clear();
    let spec = SurfaceSortieState::spec();
    let planet = &mut state.world.planets[0];
    planet.radius = radius;
    planet.mass = gravity / (60.0 * GRAVITY)
        * (radius * BODY_BOUNDS_RADIUS_SCALE + spec.half_height()).powi(2);
    planet.wrapper_omega = spin;
    state.world.ships[0].position += Vec2::new(300.0, 300.0);
    // Reconcile the changed radius before replacing its collider with material.
    idle(&mut state, 1);
    state.world.terrain.legacy_services = false;
    state.world.terrain.surface = engine_terrain::TerrainSurface::Interpolated;
    state.world.enable_planet_terrain(0).unwrap();
    idle(&mut state, 1);
    let planet = state.world.planets[0];
    let up = Vec2::Y;
    let hit = state
        .world
        .physics
        .material_ground_ray(0, planet.position + up * (radius + 20.0), -up, 40.0)
        .unwrap();
    let position = hit.point + up * (spec.half_height() + 0.02);
    let body = SpacelingAssembly::insert(
        &mut state.world.physics.world,
        pilot_physics_id(PlayerId::PLAYER_1),
        position,
        rotation_for_direction(up),
        spec,
    )
    .unwrap();
    state.world.physics.world.set_velocity(
        body.body(),
        planet_surface_velocity(&planet, position),
        spin,
        true,
    );
    state.pilots[0].body = Some(body);
    state.pilots[0].controls_armed = true;
    // The pilot runs around the whole planet; no returning ship may obstruct it.
    state.world.ships[0].dead = true;
    // Suppress the normal breakup, which would release a pod and debris.
    state.world.ships[0].fragmented = true;
    idle(&mut state, 120);
    assert!(state.world.debris.is_empty());
    assert!(
        state
            .world
            .physics
            .world
            .motion(state.world.physics.ship_body(0))
            .is_none()
    );
    let start = state.spaceling_snapshot(0).unwrap();
    assert!(start.grounded() && !start.needs_get_up(), "{start:?}");
    state
}

fn run_for_three_minutes(radius: f32, gravity: f32, spin: f32, direction: f32) {
    let mut state = runner(radius, gravity, spin);
    let mut distance = 0.0;
    let mut max_tilt = 0.0_f32;
    for frame in 0..10_800 {
        let before = state.spaceling_snapshot(0).unwrap();
        let planet = state.world.planets[0];
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: direction,
                ..Default::default()
            },
        );
        let after = state.spaceling_snapshot(0).unwrap();
        let tilt = Vec2::Y
            .rotate_radians(after.motion.angle)
            .dot(after.up)
            .clamp(-1.0, 1.0)
            .acos();
        max_tilt = max_tilt.max(tilt);
        assert!(
            !after.needs_get_up(),
            "radius={radius} gravity={gravity} spin={spin} direction={direction} frame={frame}: {after:?}"
        );
        // Measure real travel relative to the planet, independent of the
        // controller's contact-point speed (which also includes body rotation).
        let travel = after.motion.position
            - before.motion.position
            - planet_surface_velocity(&planet, before.motion.position) / 60.0;
        distance += travel.dot(Vec2::new(before.up.y, -before.up.x)) * direction;
    }
    assert!(
        distance > 180.0 * SurfaceSortieState::spec().walk_speed * 0.85,
        "radius={radius} gravity={gravity} spin={spin} direction={direction} distance={distance} max_tilt={} final={:?}",
        max_tilt.to_degrees(),
        state.spaceling_snapshot(0)
    );
    assert_eq!(state.spaceling_snapshot(0).unwrap().jumps, 0);
    eprintln!(
        "running radius={radius} gravity={gravity} spin={spin} direction={direction}: distance={distance:.1} max_tilt={:.1}",
        max_tilt.to_degrees()
    );
}

#[test]
fn full_speed_running_stays_upright_across_round_planet_sizes() {
    for radius in [15.0, 30.0, 60.0, 100.0, 150.0] {
        for direction in [-1.0, 1.0] {
            run_for_three_minutes(radius, 18.0, 0.0, direction);
        }
    }
}

#[test]
fn full_speed_running_handles_rotating_ground_and_varied_gravity() {
    for radius in [15.0, 60.0, 150.0] {
        for gravity in [9.0, 36.0] {
            for spin in [-0.04, 0.04] {
                for direction in [-1.0, 1.0] {
                    run_for_three_minutes(radius, gravity, spin, direction);
                }
            }
        }
    }
}

#[test]
fn very_weak_gravity_allows_a_runner_to_leave_the_curved_surface() {
    let mut state = runner(15.0, 0.5, 0.0);
    let mut max_height = 0.0_f32;
    let mut airborne = 0;
    for _ in 0..300 {
        tick(
            &mut state,
            SurfaceSortieAction {
                horizontal: 1.0,
                ..Default::default()
            },
        );
        let pilot = state.spaceling_snapshot(0).unwrap();
        max_height = max_height.max(
            pilot
                .motion
                .position
                .distance_to(state.world.planets[0].position)
                - 15.0 * BODY_BOUNDS_RADIUS_SCALE
                - SurfaceSortieState::spec().half_height(),
        );
        airborne += usize::from(!pilot.grounded());
        assert_eq!(pilot.jumps, 0);
    }
    assert!(max_height > 2.0, "height={max_height} airborne={airborne}");
    assert!(airborne > 120);
}
