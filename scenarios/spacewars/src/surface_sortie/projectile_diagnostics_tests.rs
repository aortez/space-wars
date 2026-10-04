use super::*;
use std::time::Duration;

fn fixture(offsets: &[Vec2]) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    let origin = state.pilot_observation(0, None).ship.position;
    for (i, offset) in offsets.iter().enumerate() {
        state.world.debris.push(DebrisState::new_shell(
            i % 2,
            i as u64,
            origin + *offset,
            Vec2::new(13.0, -7.0),
            0.4,
        ));
    }
    reconcile_physics(&mut state.world, 1.0 / 60.0);
    state
}

#[test]
fn physical_origins_and_point_velocities_match_material_observation() {
    let mut state = fixture(&[Vec2::new(100.0, 30.0)]);
    let body = state.world.physics.ship_body(0);
    state
        .world
        .physics
        .world
        .set_velocity(body, Vec2::new(3.0, 4.0), 1.7, true);
    let expected = state.pilot_observation(0, None).ship;
    let projectile = physics::primary_body(PhysicsId::new(state.world.debris[0].physics_id));
    let motion = physical_motion(&state, projectile).unwrap();
    // The diagnostic must not substitute legacy render origins or cached COM velocity.
    state.world.ships[0].position = Vec2::splat(-2000.0);
    state.world.debris[0].position = Vec2::splat(2000.0);
    state.world.debris[0].velocity = Vec2::splat(2000.0);
    let d = state.projectile_diagnostics(0).unwrap();
    assert_eq!(d.observer, expected);
    let p = d.projectiles[0];
    assert_eq!(p.motion, motion);
    assert_eq!(p.relative_position, motion.position - expected.position);
    assert_eq!(p.relative_velocity, motion.velocity - expected.velocity);
    assert_eq!(p.owner, Some(PlayerId::PLAYER_1));
    assert_eq!(p.spawn_tick, 0);
    assert!(p.collision_radius > 0.0 && d.observer_radius > 0.0);
}

#[test]
fn bounded_nearest_set_and_equal_distance_ties_survive_debris_reordering() {
    let mut state = fixture(&vec![Vec2::new(100.0, 0.0); MAX_PROJECTILES + 3]);
    let a = state.projectile_diagnostics(0).unwrap();
    assert_eq!(a.projectiles.len(), MAX_PROJECTILES);
    assert_eq!(a.shells_in_range, MAX_PROJECTILES + 3);
    assert_eq!(a.unavailable_shells, 0);
    assert!(a.projectiles.windows(2).all(|p| p[0].id < p[1].id));
    let mut ids = state
        .world
        .debris
        .iter()
        .map(|d| d.physics_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(
        a.projectiles.last().unwrap().id.value(),
        ids[MAX_PROJECTILES - 1]
    );
    state.world.debris.reverse();
    assert_eq!(state.projectile_diagnostics(0).unwrap(), a);
}

#[test]
fn range_boundary_filters_dead_nonprojectiles_and_unavailable_physics() {
    let mut state = fixture(&[
        Vec2::new(PROJECTILE_RANGE, 0.0),
        Vec2::new(PROJECTILE_RANGE + 1.0, 0.0),
        Vec2::new(40.0, 0.0),
        Vec2::new(30.0, 0.0),
    ]);
    state.world.debris[2].dead = true;
    state.world.debris[3].kind = DebrisKind::Fragment;
    state
        .world
        .debris
        .push(DebrisState::new_shell(1, 1, Vec2::ZERO, Vec2::ZERO, 0.0));
    let d = state.projectile_diagnostics(0).unwrap();
    assert_eq!(d.debris_scanned, 5);
    assert_eq!(d.shells_in_range, 1);
    assert_eq!(d.unavailable_shells, 1);
    assert_eq!(
        d.projectiles[0].relative_position.length(),
        PROJECTILE_RANGE
    );
    assert!(state.projectile_diagnostics(2).is_none());
    state.world.ships[0].dead = true;
    assert!(state.projectile_diagnostics(0).is_none());
}

#[test]
fn common_translation_and_velocity_preserve_relative_sample() {
    let mut state = fixture(&[Vec2::new(80.0, 30.0)]);
    let before = state.projectile_diagnostics(0).unwrap();
    let ship = state.world.physics.ship_body(0);
    let shell = physics::primary_body(PhysicsId::new(state.world.debris[0].physics_id));
    for body in [ship, shell] {
        let world = &mut state.world.physics.world;
        let m = world.motion(body).unwrap();
        world.set_pose(body, m.position + Vec2::new(40.0, 50.0), m.angle, true);
        world.set_velocity(
            body,
            m.linear_velocity + Vec2::new(7.0, 11.0),
            m.angular_velocity,
            true,
        );
    }
    let after = state.projectile_diagnostics(0).unwrap();
    assert!(
        before.projectiles[0]
            .relative_position
            .distance_to(after.projectiles[0].relative_position)
            < 0.001
    );
    assert!(
        before.projectiles[0]
            .relative_velocity
            .distance_to(after.projectiles[0].relative_velocity)
            < 0.001
    );
}

#[test]
fn repeated_reads_preserve_snapshot_and_clone_continuation() {
    let mut state = fixture(&[Vec2::new(120.0, 30.0)]);
    let mut untouched = state.clone();
    let snapshot = state.world.physics.snapshot_bytes();
    let first = state.projectile_diagnostics(0);
    for _ in 0..3 {
        assert_eq!(state.projectile_diagnostics(0), first);
        assert_eq!(
            state.projectile_diagnostics(1),
            untouched.projectile_diagnostics(1)
        );
    }
    assert_eq!(state.world.physics.snapshot_bytes(), snapshot);
    for _ in 0..10 {
        let dt = Duration::from_nanos(16_666_667);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        SurfaceSortieScenario::step(&mut untouched, &[], dt);
        let _ = state.projectile_diagnostics(0);
    }
    assert_eq!(
        state.world.physics.snapshot_bytes(),
        untouched.world.physics.snapshot_bytes()
    );
    assert_eq!(
        SurfaceSortieScenario::observe(&state).payload,
        SurfaceSortieScenario::observe(&untouched).payload
    );
}
