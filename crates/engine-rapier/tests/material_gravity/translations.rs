//! Translated scenes through public APIs only, including saved-state contracts.
use super::*;

fn step(world: &mut PhysicsWorld, dt: f32, expected_version: u32) -> GravityStep {
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let policy = GravitySettling::BoundedNumerical;
    let result = world.step_material_flat_gravity(dt, policy).unwrap();
    let again = replay.step_material_flat_gravity(dt, policy).unwrap();
    assert_eq!(result.records, again.records);
    assert_eq!(result.numerical, again.numerical);
    assert_eq!(result.durations, again.durations);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(version(world), expected_version);
    assert_eq!(
        result.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    result
}

fn lifecycle<const N: usize>(masses: [f32; N]) {
    let expected_version = if N == 2 { 10 } else { 11 };
    for offset in [Vec2::new(0.0, -16.0), Vec2::new(1.0 / 8192.0, 16.0)] {
        for speed in [0.0, 0.01, 4.0] {
            let (mut world, tokens) = scene_with_roles(Scene {
                masses,
                offset,
                speed,
                parts: [4; N],
                copies: 2,
                reverse: true,
                ..Default::default()
            });
            let initial = capacity(&world, tokens);
            let first = step(&mut world, 0.01, expected_version);
            for role in 1..N {
                assert_eq!(
                    world.motion(body(role)).unwrap().position,
                    offset + Vec2::X * role as f32
                );
            }
            assert_eq!(world.motion(body(0)).unwrap().position.y, offset.y);
            if speed < 0.1 {
                assert_eq!(capacity(&world, tokens), initial);
            }
            let next = step(&mut world, 0.001, expected_version);
            assert_eq!(
                first.numerical.unwrap().scales,
                next.numerical.as_ref().unwrap().scales
            );
            let bytes = world.snapshot_bytes().unwrap();
            let paused = world
                .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                .unwrap();
            assert!(paused.records.is_empty());
            assert_eq!(paused.numerical, next.numerical);
            assert_eq!(world.snapshot_bytes().unwrap(), bytes);
            refused(
                &mut world,
                0.001,
                GravitySettling::Refuse,
                GravityRejection::PolicyChanged,
            );
            let original = world.motion(body(N - 1)).unwrap().position;
            world.set_pose(body(N - 1), original + Vec2::Y, 0.0, true);
            world.set_pose(body(N - 1), original, 0.0, true);
            let mut restored =
                PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
            assert_eq!(version(&restored), expected_version);
            assert_eq!(
                restored
                    .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                    .unwrap()
                    .numerical,
                next.numerical
            );
            refused(
                &mut restored,
                0.001,
                GravitySettling::BoundedNumerical,
                GravityRejection::StaleContinuation,
            );
        }
    }
}

#[test]
fn translated_api_pair_and_stack_replay_support_settlement_and_invalidation() {
    for masses in [[20.0, 20.0], [5.0, 20.0], [13.0, 20.0], [80.0, 20.0]] {
        lifecycle(masses);
    }
    for masses in [
        [20.0, 20.0, 20.0],
        [5.0, 20.0, 20.0],
        [13.0, 31.0, 20.0],
        [80.0, 20.0, 20.0],
    ] {
        lifecycle(masses);
    }
}

#[test]
fn translated_api_geometry_precision_and_late_arrival_refusals_preserve_outputs() {
    for offset in [Vec2::new(17.0, 0.0), Vec2::new(0.0, -17.0)] {
        let (mut world, _) = scene(Scene {
            offset,
            ..Default::default()
        });
        refused(
            &mut world,
            0.01,
            GravitySettling::BoundedNumerical,
            GravityRejection::UnsupportedScene,
        );
        assert_eq!(version(&world), 3);
    }
    // A tiny rebound is unrepresentable on this world-coordinate grid. It
    // cannot consume damage, a support owner, numerical history or output.
    let (mut world, _) = scene(Scene {
        offset: Vec2::new(8.0, 3.25),
        speed: 0.01,
        ..Default::default()
    });
    refused(
        &mut world,
        0.01,
        GravitySettling::BoundedNumerical,
        GravityRejection::Numerical,
    );
    assert_eq!(version(&world), 3);
    let (mut world, _) = scene_with_roles(Scene {
        masses: [13.0, 31.0, 20.0],
        offset: Vec2::new(0.125, -3.25),
        gap: 0.02,
        speed: 0.5,
        capture: CaptureConfig {
            min_approach_speed: 1.0,
            ..Default::default()
        },
        ..Default::default()
    });
    step(&mut world, 0.001, 11);
    refused(
        &mut world,
        0.1,
        GravitySettling::BoundedNumerical,
        GravityRejection::UnaccountedApproach,
    );
}

#[test]
fn translated_api_legacy_mass_versions_remain_unchanged() {
    for (mass, expected) in [(20.0, 6), (13.0, 8)] {
        let (mut world, _) = scene(Scene {
            masses: [mass, 20.0],
            ..Default::default()
        });
        step(&mut world, 0.001, expected);
        let bytes = world.snapshot_bytes().unwrap();
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        step(&mut restored, 0.001, expected);
    }
    for (masses, expected) in [([20.0, 20.0, 20.0], 7), ([13.0, 31.0, 20.0], 9)] {
        let (mut world, _) = scene_with_roles(Scene {
            masses,
            ..Default::default()
        });
        step(&mut world, 0.001, expected);
        let bytes = world.snapshot_bytes().unwrap();
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        step(&mut restored, 0.001, expected);
    }
}
