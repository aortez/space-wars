use super::*;
use flight_policy_observation::{FlightPolicyContext, OBJECT_CAPACITY, SELF_FEATURES};

fn feature(observation: &flight_policy_observation::FlightPolicyObservation, name: &str) -> f32 {
    observation.inputs.self_goal[SELF_FEATURES.iter().position(|n| *n == name).unwrap()]
}

fn context(env: &FlightEnvironment) -> FlightPolicyContext {
    FlightPolicyContext {
        target_position: env.frame.target_position,
        target_velocity: env.frame.target_velocity,
        arrival: env.trace.task.arrival,
        elapsed_ticks: env.progress.elapsed_ticks,
        dwell_ticks: env.progress.dwell_ticks,
        deadline_ticks: env.trace.task.deadline_ticks,
        previous_action: env.previous_action,
    }
}

fn insert_box(env: &mut FlightEnvironment, id: u64, position: Vec2) -> PhysicsBodyId {
    let body = PhysicsBodyId::new(PhysicsId::new(id), BodyRole::PRIMARY);
    assert!(env.state.world.physics.world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
            1.0,
            2.0
        )]
    ));
    body
}

#[test]
fn policy_schema_masks_no_clipping_and_runtime_adapter_parity() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    env.step(FlightAction {
        steering: 0.5,
        thrust: 1.0,
        brake: 1.0,
    })
    .unwrap();
    let before = env.state.world.physics.world.snapshot_bytes().unwrap();
    let encoded = env.policy_observation().unwrap();
    assert_eq!(
        encoded,
        env.state
            .flight_policy_observation(0, &env.sensor, context(&env))
            .unwrap()
    );
    assert_eq!(
        before,
        env.state.world.physics.world.snapshot_bytes().unwrap()
    );
    assert_eq!(encoded.inputs.self_goal.len(), 35);
    assert_eq!(encoded.inputs.cells.len(), 1472);
    assert_eq!(encoded.inputs.rays.len(), 128);
    assert_eq!(encoded.inputs.objects.len(), OBJECT_CAPACITY);
    assert_eq!(encoded.inputs.values().count(), 4009);
    assert!(encoded.inputs.values().all(f32::is_finite));
    assert_eq!(feature(&encoded, "previous_thrust"), 1.0);
    assert_eq!(feature(&encoded, "previous_brake"), 1.0);
    assert_eq!(feature(&encoded, "solar_exposure"), 0.0);
    assert_eq!(feature(&encoded, "solar_available"), 0.0);
    assert!(encoded.inputs.rays.iter().any(|r| r[0] == 0.0));
    for ray in &encoded.inputs.rays {
        if ray[0] == 0.0 {
            assert_eq!(*ray, [0.0; 4]);
        }
    }
    for (row, id) in encoded.inputs.objects.iter().zip(&encoded.object_sources) {
        assert_eq!(row[0] == 1.0, id.is_some());
        if id.is_none() {
            assert_eq!(*row, [0.0; 16]);
        }
    }
    let mut request = context(&env);
    request.target_position =
        env.frame.center_of_mass + Vec2::new(1200.0, 0.0).rotate_radians(env.frame.angle);
    request.target_velocity =
        env.frame.velocity + Vec2::new(280.0, 0.0).rotate_radians(env.frame.angle);
    let large = env
        .state
        .flight_policy_observation(0, &env.sensor, request)
        .unwrap();
    assert!((feature(&large, "goal_com_x") - 4.0).abs() < 0.0001);
    assert!((feature(&large, "target_minus_com_velocity_x") - 4.0).abs() < 0.0001);
    request.target_velocity.x = f32::NAN;
    assert!(
        env.state
            .flight_policy_observation(0, &env.sensor, request)
            .is_err()
    );
    assert!(
        env.state
            .flight_policy_observation(
                0,
                &PolarSensor::new(PolarConfig::default()).unwrap(),
                context(&env)
            )
            .is_err()
    );
}

#[test]
fn polar_surface_motion_and_com_origin_frames_match_physics() {
    let mut task = FlightTask::default();
    task.start.planet_spin = 0.015;
    task.start.spin = 1.0;
    task.start.heading_offset = 0.7;
    let env = FlightEnvironment::new(task).unwrap();
    let encoded = env.policy_observation().unwrap();
    let raw = env.observation().unwrap();
    let flight = &raw.surroundings.flight;
    let local = |v: Vec2| v.rotate_radians(-flight.ship.angle);
    let origin_velocity = flight.ship.velocity;
    assert!((env.frame.velocity - origin_velocity).length() > 0.5);
    let expected_com = local(env.frame.velocity) / 70.0;
    let expected_origin = local(origin_velocity) / 70.0;
    assert!((feature(&encoded, "com_world_velocity_x") - expected_com.x).abs() < 0.000001);
    assert!((feature(&encoded, "origin_world_velocity_x") - expected_origin.x).abs() < 0.000001);
    let world = &env.state.world.physics.world;
    let mut hit_rotating_planet = false;
    for (ray, values) in raw
        .surroundings
        .geometry
        .rays
        .iter()
        .zip(&encoded.inputs.rays)
    {
        if let Some(id) = ray.collider {
            let body = world.collider_body(id).unwrap();
            let point = flight.ship.position
                + Vec2::from_radians(flight.ship.angle + ray.bearing) * ray.distance.unwrap();
            let expected =
                local(world.velocity_at_point(body, point).unwrap() - origin_velocity) / 70.0;
            assert!((Vec2::new(values[2], values[3]) - expected).length() < 0.000001);
            hit_rotating_planet |= body == env.state.world.physics.planet_body(0);
        }
    }
    assert!(hit_rotating_planet);
    let point = raw.surroundings.geometry.nearest_point.unwrap();
    let body = world
        .collider_body(encoded.nearest_source.unwrap())
        .unwrap();
    let expected = local(world.velocity_at_point(body, point).unwrap() - origin_velocity) / 70.0;
    assert!(
        (Vec2::new(encoded.inputs.nearest[4], encoded.inputs.nearest[5]) - expected).length()
            < 0.000001
    );
    assert_eq!(
        encoded
            .inputs
            .objects
            .iter()
            .filter(|r| r[15] == 1.0)
            .count(),
        1
    );
    let index = encoded
        .object_sources
        .iter()
        .position(|id| *id == Some(env.state.world.physics.planet_body(0)))
        .unwrap();
    assert_eq!(encoded.inputs.objects[index][1], 1.0);
    assert!((encoded.inputs.objects[index][11] - 0.015 / 1.8).abs() < 0.000001);
}

#[test]
fn current_shapes_additions_and_capacity_are_not_silently_dropped() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let initial = env.policy_observation().unwrap();
    let position = env.frame.origin + Vec2::new(20.0, 0.0);
    let body = insert_box(&mut env, 9_200_001, position);
    env.step(FlightAction::default()).unwrap();
    let added = env.policy_observation().unwrap();
    assert_eq!(
        added
            .object_sources
            .iter()
            .filter(|id| id.is_some())
            .count(),
        initial
            .object_sources
            .iter()
            .filter(|id| id.is_some())
            .count()
            + 1
    );
    assert!(added.object_sources.contains(&Some(body)));
    assert_ne!(initial.inputs.cells, added.inputs.cells);
    let planet = env.state.world.physics.planet_body(0);
    let mut extension = ColliderSpec::ball(
        ColliderId::new(planet.entity, ColliderRole::new(990), 0),
        2.0,
    );
    extension.local_position = Vec2::new(150.0, 0.0);
    assert!(
        env.state
            .world
            .physics
            .world
            .insert_collider(planet, &extension)
    );
    env.step(FlightAction::default()).unwrap();
    let extended = env.policy_observation().unwrap();
    let p = extended
        .object_sources
        .iter()
        .position(|id| *id == Some(planet))
        .unwrap();
    assert_eq!(extended.object_sources, added.object_sources);
    assert!(extended.inputs.objects[p][12] > added.inputs.objects[p][12]);
    assert!((extended.inputs.objects[p][12] - 152.0 / 500.0).abs() < 0.00001);
    env.state.world.physics.world.remove_entity(body.entity);
    env.step(FlightAction::default()).unwrap();
    assert!(
        !env.policy_observation()
            .unwrap()
            .object_sources
            .contains(&Some(body))
    );
    let count = env
        .policy_observation()
        .unwrap()
        .object_sources
        .iter()
        .filter(|id| id.is_some())
        .count();
    for i in 0..(OBJECT_CAPACITY - count) {
        insert_box(
            &mut env,
            9_300_000 + i as u64,
            Vec2::new(2000.0 + i as f32 * 5.0, 2000.0),
        );
    }
    env.step(FlightAction::default()).unwrap();
    assert_eq!(
        env.policy_observation()
            .unwrap()
            .object_sources
            .iter()
            .filter(|id| id.is_some())
            .count(),
        OBJECT_CAPACITY
    );
    insert_box(&mut env, 9_400_000, Vec2::new(2500.0, 2500.0));
    env.step(FlightAction::default()).unwrap();
    assert_eq!(
        env.policy_observation().unwrap_err(),
        "flight object capacity exceeded"
    );
}

#[test]
fn rotating_translating_entire_scene_preserves_ship_frame_inputs() {
    let mut task = FlightTask::default();
    task.obstacles.push(FlightObstacle {
        offset: Vec2::new(20.0, 10.0),
        half_extents: Vec2::new(2.0, 3.0),
        angle: 0.2,
    });
    let env = FlightEnvironment::new(task).unwrap();
    let original = env.policy_observation().unwrap();
    let mut rotated = env.state.clone();
    let angle = 0.7;
    let offset = Vec2::new(80.0, -40.0);
    let motions: Vec<_> = rotated.world.physics.world.motions().collect();
    for body in motions {
        rotated.world.physics.world.set_pose(
            body.id,
            body.motion.position.rotate_radians(angle) + offset,
            body.motion.angle + angle,
            true,
        );
        rotated.world.physics.world.set_velocity(
            body.id,
            body.motion.linear_velocity.rotate_radians(angle),
            body.motion.angular_velocity,
            true,
        );
    }
    rotated.world.physics.world.step(1.0 / 60.0);
    let mut request = context(&env);
    request.target_position = request.target_position.rotate_radians(angle) + offset;
    request.target_velocity = request.target_velocity.rotate_radians(angle);
    let transformed = rotated
        .flight_policy_observation(0, &env.sensor, request)
        .unwrap();
    assert_eq!(original.object_sources, transformed.object_sources);
    for (i, (a, b)) in original
        .inputs
        .values()
        .zip(transformed.inputs.values())
        .enumerate()
    {
        assert!((a - b).abs() < 0.0002, "feature {i}: {a} != {b}");
    }
}

#[test]
fn replay_preserves_policy_inputs_and_terrain_edits_refresh_them() {
    let mut env = FlightEnvironment::new(FlightTask::default()).unwrap();
    let before = env.policy_observation().unwrap();
    let center = env.state.world.terrain.planets[&0]
        .field
        .local_to_cell(Vec2::new(0.0, 55.0))
        .unwrap();
    env.queue_terrain_edit(TerrainEdit {
        brush: engine_terrain::Brush::Circle { center, radius: 12 },
        mode: engine_terrain::EditMode::Remove,
    })
    .unwrap();
    assert_eq!(before, env.policy_observation().unwrap());
    env.step(FlightAction::default()).unwrap();
    let changed = env.policy_observation().unwrap();
    assert_ne!(changed.inputs.cells, before.inputs.cells);
    let replay = FlightEnvironment::replay(env.trace()).unwrap();
    assert_eq!(changed, replay.policy_observation().unwrap());
}

#[test]
fn global_solar_boundary_tracks_production_hazard_and_availability() {
    let mut state = SurfaceSortieScenario::init_material_match(42);
    state.set_flight_control_mode(0, FlightControlMode::Learned);
    SurfaceSortieScenario::step(&mut state, &[], FLIGHT_DT);
    let sensor = PolarSensor::new(flight_sensor_config()).unwrap();
    let request = FlightPolicyContext {
        target_position: Vec2::ZERO,
        target_velocity: Vec2::ZERO,
        arrival: ArrivalCriteria::default(),
        elapsed_ticks: 0,
        dwell_ticks: 0,
        deadline_ticks: 1800,
        previous_action: FlightAction::default(),
    };
    for active in [true, false] {
        if !active {
            for pilot in &mut state.pilots {
                pilot.combat = None;
            }
        }
        let encoded = state
            .flight_policy_observation(0, &sensor, request)
            .unwrap();
        let sun = encoded.inputs.objects.iter().find(|r| r[2] == 1.0).unwrap();
        let hazard = state.solar_hazard().unwrap();
        let radius = state.world.config.universe_radius as f32;
        assert!((sun[13] * radius - hazard.heat_radius).abs() < 0.0001);
        assert_eq!(sun[14], f32::from(active));
        assert_eq!(feature(&encoded, "solar_available"), f32::from(active));
        assert_eq!(
            feature(&encoded, "solar_exposure"),
            state.solar_exposure(0).map_or(0.0, |s| s.intensity)
        );
    }
}
