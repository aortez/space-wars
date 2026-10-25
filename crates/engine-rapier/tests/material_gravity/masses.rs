//! Public mass contracts, saved-state continuation and atomic refusal.
use super::*;

fn step(world: &mut PhysicsWorld, dt: f32, expected_version: u32) -> GravityStep {
    let policy = GravitySettling::BoundedNumerical;
    let mut copy = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let trace = world.step_material_flat_gravity(dt, policy).unwrap();
    let replay = copy.step_material_flat_gravity(dt, policy).unwrap();
    assert_eq!(trace.records, replay.records);
    assert_eq!(trace.numerical, replay.numerical);
    assert_eq!(trace.durations, replay.durations);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        copy.snapshot_bytes().unwrap()
    );
    assert_eq!(version(world), expected_version);
    assert_eq!(
        trace.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    trace
}

#[test]
fn native_mass_api_pair_support_and_settlement_survive_restore() {
    for mass in [5.0, 13.0, 31.0, 80.0] {
        for speed in [0.0, 0.01] {
            let (mut world, tokens) = scene(Scene {
                masses: [mass, 20.0],
                speed,
                parts: [4, 2],
                copies: 2,
                reverse: true,
                ..Default::default()
            });
            let initial = capacity(&world, tokens);
            let trace = step(&mut world, 0.01, 8);
            assert_eq!(
                trace
                    .records
                    .iter()
                    .filter(|r| matches!(r.record, GravityRecord::Settlement(_)))
                    .count(),
                usize::from(speed > 0.0)
            );
            let held = step(&mut world, 0.01, 8);
            let [entry] = held.records.as_slice() else {
                panic!("one hold")
            };
            let GravityRecord::Support(s) = &entry.record else {
                panic!("support")
            };
            assert_eq!(s.force, f64::from(world.body_mass(body(0)).unwrap()) * 10.0);
            assert_eq!(s.gravity_work, 0.0);
            assert_eq!(s.kick_energy, s.projection_energy);
            assert_eq!(capacity(&world, tokens), initial);
        }
    }
}

#[test]
fn native_mass_api_stack_owners_weights_and_terminal_invalidation_survive_restore() {
    for masses in [
        [5.0, 20.0, 20.0],
        [20.0, 5.0, 20.0],
        [13.0, 31.0, 20.0],
        [80.0, 20.0, 20.0],
    ] {
        for speed in [0.0, 0.01] {
            let (mut world, tokens) = scene_with_roles(Scene {
                masses,
                speed,
                parts: [4, 2, 4],
                copies: 2,
                reverse: true,
                ..Default::default()
            });
            let capacity_before = capacity(&world, tokens);
            let first = step(&mut world, 0.01, 9);
            let owners = first
                .records
                .iter()
                .find_map(|r| match &r.record {
                    GravityRecord::StackSupport(s) => Some(s.episodes),
                    _ => None,
                })
                .unwrap();
            assert_ne!(owners[0], owners[1]);
            let actual = [body(0), body(1)].map(|b| f64::from(world.body_mass(b).unwrap()));
            let next = step(&mut world, 0.01, 9);
            let [entry] = next.records.as_slice() else {
                panic!("one hold")
            };
            let GravityRecord::StackSupport(s) = &entry.record else {
                panic!("stack support")
            };
            assert_eq!(s.episodes, owners);
            assert_eq!(s.forces, [actual[0] * 10.0, (actual[0] + actual[1]) * 10.0]);
            assert_eq!(s.gravity_work, 0.0);
            assert_eq!(s.kick_energy, s.projection_energy);
            assert_eq!(capacity(&world, tokens), capacity_before);
            let before = world.snapshot_bytes().unwrap();
            assert_eq!(
                world
                    .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                    .unwrap()
                    .numerical,
                next.numerical
            );
            assert_eq!(world.snapshot_bytes().unwrap(), before);
            world.set_velocity(body(1), Vec2::X, 0.0, true);
            world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
            let mut restored =
                PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
            assert_eq!(version(&restored), 9);
            assert_eq!(
                restored
                    .step_material_flat_gravity(0.0, GravitySettling::Refuse)
                    .unwrap()
                    .numerical,
                next.numerical
            );
            refused(
                &mut restored,
                0.01,
                GravitySettling::BoundedNumerical,
                GravityRejection::StaleContinuation,
            );
        }
    }
}

#[test]
fn native_mass_api_outside_bounds_overload_and_late_failure_are_atomic() {
    for mass in [4.0, 81.0] {
        let (mut world, _) = scene(Scene {
            masses: [mass, 20.0],
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
    for masses in [[40.0, 5.0, 20.0], [5.0, 40.0, 20.0]] {
        let (mut world, _) = scene_with_roles(Scene {
            masses,
            ..Default::default()
        });
        refused(
            &mut world,
            0.01,
            GravitySettling::BoundedNumerical,
            GravityRejection::UnsupportedScene,
        );
    }
    let (mut overload, _) = scene_with_roles(Scene {
        masses: [80.0, 20.0, 20.0],
        speed: 0.0,
        gravity: 100.0,
        ..Default::default()
    });
    refused(
        &mut overload,
        0.01,
        GravitySettling::BoundedNumerical,
        GravityRejection::UnsafeSupport,
    );
    // Prior successful output must survive a later touching approach below the
    // configured response threshold, even after an enrolled v9 restore.
    let (mut world, _) = scene_with_roles(Scene {
        masses: [13.0, 31.0, 20.0],
        gap: 0.02,
        speed: 0.5,
        capture: CaptureConfig {
            min_approach_speed: 1.0,
            ..Default::default()
        },
        ..Default::default()
    });
    step(&mut world, 0.001, 9);
    refused(
        &mut world,
        0.1,
        GravitySettling::BoundedNumerical,
        GravityRejection::UnaccountedApproach,
    );
}
