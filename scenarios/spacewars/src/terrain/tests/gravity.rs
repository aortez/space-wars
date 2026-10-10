use super::*;

#[test]
fn material_gravity_integrates_once_and_clears_when_the_source_stops() {
    let mut state = SpacewarsScenario::init_terrain_fixture(42);
    state
        .enable_loose_terrain(LooseTerrainConfig::default())
        .unwrap();
    let dt = 1.0 / 60.0;
    let body = state.physics.ship_body(0);
    state
        .physics
        .world
        .set_velocity(body, Vec2::ZERO, 0.0, true);
    let before = state.physics.world.motion(body).unwrap();
    let planet = state.planets[0];
    let offset = planet.position - state.ships[0].position;
    // The craft is outside the bounded spherical source, where its acceleration
    // is the ordinary inverse-square law. Keep the sampled location fixed here
    // to detect accidental force accumulation across successive solves.
    assert!(offset.length() > planet.radius + 20.0);
    let delta = offset.normalized() * (GRAVITY * planet.mass / offset.length_squared());
    for tick in 1..=2 {
        let previous = state.physics.world.motion(body).unwrap();
        apply_world_gravity(&mut state);
        assert_eq!(state.physics.world.motion(body), Some(previous));
        state.physics.step(dt);
        let after = state.physics.world.motion(body).unwrap();
        assert!(after.linear_velocity.distance_to(delta * tick as f32) < 1e-4);
        assert!(after.position.distance_to(before.position) > 0.0);
    }

    // A zero-output tick must discard the previous tick's force, including
    // when no gravity output passes the early-out in the dispatcher.
    state.planets[0].mass = 0.0;
    let previous = state.physics.world.motion(body).unwrap().linear_velocity;
    apply_world_gravity(&mut state);
    state.physics.step(dt);
    assert!(
        state
            .physics
            .world
            .motion(body)
            .unwrap()
            .linear_velocity
            .distance_to(previous)
            < 1e-5
    );
}
