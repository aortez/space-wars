//! Fixed-size save integrity, live common motion, and atomic refusal boundaries.
use super::super::tests::{body, world};
use super::*;

const DT: f32 = 1.0 / 8192.0;
fn pair(speed: f32) -> PhysicsWorld {
    let mut w = world();
    w.set_gravity(Vec2::ZERO);
    w.set_body_kind(body(2), BodyKind::Dynamic, true);
    w.refresh_mass_properties(body(2));
    w.set_pose(body(11), Vec2::new(-0.5, 0.0), 0.0, true);
    w.set_pose(body(2), Vec2::new(0.5, 0.0), 0.0, true);
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w
}
fn continuation(w: &mut PhysicsWorld) -> &mut Continuation {
    w.loading
        .as_mut()
        .unwrap()
        .dynamic_compression
        .as_mut()
        .unwrap()
}
fn refused(w: &mut PhysicsWorld, dt: f32, error: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_dynamic_compression(dt), Err(error));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn moving_saves_validate_both_trajectories_even_after_removal() {
    let mut original = pair(4.0);
    original
        .step_material_dynamic_compression(1.0 / 128.0)
        .unwrap();
    let corruptions: &[fn(&mut MovingHistory)] = &[
        |m| m.masses[1] += 1.0,
        |m| m.initial.positions[0] += 0.01,
        |m| m.initial.velocities[1] += 0.5,
        |m| m.current.positions[0] += 0.01,
        |m| m.current.velocities[0] += 0.5,
        |m| m.integrated_velocity[0] += 0.01,
        |m| m.errors[0] += 1.0,
        |m| m.errors[1] += 1.0,
        |m| m.errors[2] += 1.0,
        |m| m.errors[3] += 0.01,
        |m| m.errors[4] += 0.01,
        |m| m.absolute_errors[0] = -1.0,
        |m| m.absolute_errors[1] = 1.0,
        |m| m.absolute_errors[2] = 1.0,
        |m| m.absolute_errors[3] = 0.01,
        |m| m.absolute_errors[4] = f64::NAN,
    ];
    for terminal in [false, true] {
        for corrupt in corruptions {
            let mut w = original.clone();
            if terminal {
                w.remove_entity(body(11).entity);
                w.remove_entity(body(2).entity);
            }
            corrupt(continuation(&mut w).history.moving.as_mut().unwrap());
            assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
        }
    }
    // A common velocity edit preserves relative speed but is still a stale scene.
    let mut w = original.clone();
    for id in [body(11), body(2)] {
        let h = w.body_handle(id).unwrap();
        let velocity = w.raw.bodies[h].linvel() + Vector::X;
        w.raw.bodies[h].set_linvel(velocity, true);
    }
    assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
    refused(&mut w, DT, R::StaleContinuation);
}

#[test]
fn moving_required_extension_and_pending_support_replay_without_downgrade() {
    let mut w = pair(4.0);
    let first = w.step_material_dynamic_compression(DT).unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].event.transition, EpisodeTransition::Impact);
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..4], &19_u32.to_le_bytes());
    for version in 0_u32..=18 {
        let mut bad = bytes.clone();
        bad[..4].copy_from_slice(&version.to_le_bytes());
        assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
    }
    let extension = bincode::serialized_size(continuation(&mut w).history.moving.as_ref().unwrap())
        .unwrap() as usize;
    for missing in [1, extension / 2, extension] {
        assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - missing]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let step = w.step_material_dynamic_compression(DT).unwrap();
    assert_eq!(step.events.len(), 1);
    assert_eq!(step.events[0].event.transition, EpisodeTransition::Support);
    assert_eq!(step.events[0].event.id, first.events[0].event.id);
    assert_eq!(
        restored.step_material_dynamic_compression(DT).unwrap(),
        step
    );
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );

    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(1.0),
        Err(R::InvalidCommand)
    );
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    w.step_material_dynamic_compression(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    let totals = w.material_moving_support_summary();
    w.set_gravity(Vec2::X);
    w.set_gravity(Vec2::ZERO);
    refused(&mut w, DT, R::StaleContinuation);
    w.remove_entity(body(11).entity);
    w.remove_entity(body(2).entity);
    let mut terminal = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(terminal.material_moving_support_summary(), totals);
    refused(&mut terminal, DT, R::StaleContinuation);
}

#[test]
fn moving_failure_and_each_body_numerical_allowance_refuse_atomically() {
    let mut w = pair(4.0);
    w.loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .values_mut()
        .next()
        .unwrap()
        .remaining = 1.0;
    w.step_material_dynamic_compression(DT).unwrap();
    refused(&mut w, 1.0 / 30.0, R::FailedRegion);

    let mut w = pair(0.5);
    w.set_velocity(body(11), Vec2::X * 0.6, 0.0, true);
    w.set_velocity(body(2), Vec2::X * 0.1, 0.0, true);
    for tick in 0..4096 {
        let before = w.snapshot_bytes().unwrap();
        match w.step_material_dynamic_compression(DT) {
            Ok(_) => assert!(tick < 4095, "expected a finite numerical allowance"),
            Err(error) => {
                assert_eq!(error, R::Numerical);
                assert_eq!(w.snapshot_bytes().unwrap(), before);
                let totals = w.material_moving_support_summary();
                let m = totals.as_ref().unwrap();
                assert!(
                    m.absolute_errors[3..]
                        .iter()
                        .zip(&m.scales[3..])
                        .any(|(error, scale)| *error > 0.99e-5 * scale)
                );
                w.step_material_dynamic_compression(0.0).unwrap();
                assert_eq!(w.material_moving_support_summary(), totals);
                let mut restored =
                    PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
                refused(&mut restored, DT, R::Numerical);
                break;
            }
        }
    }
}

#[test]
fn moving_enrollment_refuses_external_force_and_non_normal_motion() {
    let edits: &[fn(&mut PhysicsWorld)] = &[
        |w| {
            w.set_gravity(Vec2::X);
        },
        |w| {
            w.set_velocity(body(2), Vec2::Y, 0.0, true);
        },
        |w| {
            w.set_velocity(body(2), Vec2::ZERO, 1.0, true);
        },
        |w| {
            w.set_body_kind(body(2), BodyKind::KinematicVelocity, true);
        },
    ];
    for edit in edits {
        let mut w = pair(4.0);
        edit(&mut w);
        refused(&mut w, DT, R::UnsupportedScene);
    }
}
