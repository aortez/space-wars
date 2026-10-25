//! Translated continuations preserve world witnesses and local budgets.
use super::*;
use masses::set_masses;

fn shifted(stack: bool, mass: bool, offset: Vec2, gap: f32, speed: f32) -> PhysicsWorld {
    let mut w = if stack {
        stack_world([11, 2, 7], true, [4, 2, 4], 2)
    } else {
        world()
    };
    if mass {
        set_masses(
            &mut w,
            &[11, 2],
            if stack { &[13.0, 31.0] } else { &[13.0] },
        );
    }
    let ids = if stack { &[11, 2, 7][..] } else { &[11, 2][..] };
    for (role, &id) in ids.iter().enumerate() {
        let x = if role == 0 { -gap } else { role as f32 };
        w.set_pose(body(id), offset + Vec2::X * x, 0.0, true);
    }
    w.set_velocity(body(11), Vec2::X * speed, 0.0, true);
    w
}

#[test]
fn translated_saved_histories_replay_every_phase_and_keep_terminal_ledgers() {
    for stack in [false, true] {
        for mass in [false, true] {
            for offset in [Vec2::new(0.0, -16.0), Vec2::new(1.0 / 8192.0, 16.0)] {
                for (gap, speed, dt) in [
                    (0.02, 4.0, 0.001),
                    (0.0, 4.0, 0.001),
                    (0.0, 0.0, 0.01),
                    (0.0, 0.01, 0.01),
                ] {
                    let mut w = shifted(stack, mass, offset, gap, speed);
                    let policy = GravitySettling::BoundedNumerical;
                    let first = w.step_material_flat_gravity(dt, policy).unwrap();
                    let bytes = w.snapshot_bytes().unwrap();
                    let version: u32 = if stack { 11 } else { 10 };
                    assert_eq!(&bytes[..4], &version.to_le_bytes());
                    for wrong in 1_u32..=12 {
                        if wrong == version {
                            continue;
                        }
                        let mut invalid = bytes.clone();
                        invalid[..4].copy_from_slice(&wrong.to_le_bytes());
                        assert!(PhysicsWorld::from_snapshot_bytes(&invalid).is_err());
                    }
                    for cut in [1, 4, 8, 24, 64] {
                        assert!(
                            PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err()
                        );
                    }
                    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                    assert_eq!(replay.snapshot_bytes().unwrap(), bytes);
                    let next = w.step_material_flat_gravity(0.001, policy).unwrap();
                    let again = replay.step_material_flat_gravity(0.001, policy).unwrap();
                    assert_eq!(next.records, again.records);
                    assert_eq!(next.numerical, again.numerical);
                    assert_eq!(
                        w.snapshot_bytes().unwrap(),
                        replay.snapshot_bytes().unwrap()
                    );
                    assert_eq!(
                        first.numerical.unwrap().scales,
                        next.numerical.as_ref().unwrap().scales
                    );
                    let h = history(&w).clone();
                    assert_eq!(h.revision, 3);
                    let bytes = w.snapshot_bytes().unwrap();
                    assert_eq!(
                        w.step_material_flat_gravity(0.0, GravitySettling::Refuse)
                            .unwrap()
                            .numerical,
                        next.numerical
                    );
                    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
                    for reason in [Invalidation::ExternalEdit, Invalidation::OtherStep] {
                        let mut invalid = w.clone();
                        invalid.loading.as_mut().unwrap().invalidate_gravity(reason);
                        for id in [11, 2, 7] {
                            invalid.remove_entity(body(id).entity);
                        }
                        let s = invalid.loading.as_ref().unwrap().gravity.as_ref().unwrap();
                        assert_eq!(s.invalidation(), Some(reason));
                        assert_eq!(s.history, h);
                        let bytes = invalid.snapshot_bytes().unwrap();
                        assert_eq!(&bytes[..4], &version.to_le_bytes());
                        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
                        assert_eq!(
                            restored
                                .step_material_flat_gravity(0.0, policy)
                                .unwrap()
                                .numerical,
                            next.numerical
                        );
                        assert_eq!(
                            restored
                                .step_material_flat_gravity(0.001, policy)
                                .unwrap_err(),
                            GravityRejection::StaleContinuation
                        );
                        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
                    }
                }
            }
        }
    }
}

#[test]
fn translated_snapshot_validates_frame_and_relative_reference_without_f32_truncation() {
    let mut w = shifted(false, true, Vec2::new(0.125, -3.25), 0.0, 4.0);
    w.set_pose(body(11), Vec2::new(-0.000001, -3.25), 0.0, true);
    w.step_material_flat_gravity(0.001, GravitySettling::BoundedNumerical)
        .unwrap();
    let x = history(&w).reference[0];
    assert_ne!(f64::from(x as f32), x);
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&bytes)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        bytes
    );
    for case in 0..12 {
        let mut changed = w.clone();
        let s = changed.loading.as_mut().unwrap().gravity.as_mut().unwrap();
        match case {
            0 => s.history.revision = 2,
            1 => s.history.revision = 4,
            2 => s.history.reference[0] = -1.0e300,
            3 => s.history.errors[0] = s.history.scales[0],
            4 => s.history.scales[2] *= 2.0,
            _ => {
                let Status::Active(a) = &mut s.status else {
                    unreachable!()
                };
                match case {
                    5 => a.plates[1].endpoint.position[0] = 18.0,
                    6 => a.plates[1].endpoint.position[1] = 17.0,
                    7 => a.plates[0].endpoint.position[1] = 0.0,
                    8 => a.plates[0].endpoint.position[0] = 0.126,
                    9 => a.plates[1].endpoint.position[0] = f32::NAN,
                    10 => a.plates[0].endpoint.mass = 20.0,
                    _ => a.plates[1].endpoint.velocity[0] = 0.01,
                }
            }
        }
        assert!(
            PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err(),
            "case {case}"
        );
    }
    // Raw edits exercise stale detection independently of public invalidation.
    let mut stale = w.clone();
    for (_, b) in stale.raw.bodies.iter_mut() {
        b.set_translation(b.translation() + Vector::Y, true);
    }
    let bytes = stale.snapshot_bytes().unwrap();
    assert_eq!(
        stale
            .step_material_flat_gravity(0.001, GravitySettling::BoundedNumerical)
            .unwrap_err(),
        GravityRejection::StaleContinuation
    );
    assert_eq!(stale.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn translated_frame_bounds_and_stationary_alignment_refuse_before_enrollment() {
    let beyond = f32::from_bits(16.0_f32.to_bits() + 1);
    for stack in [false, true] {
        for offset in [
            Vec2::new(17.0, 0.0),
            Vec2::new(-17.0, 0.0),
            Vec2::new(0.0, beyond),
            Vec2::new(0.0, -beyond),
        ] {
            let mut w = shifted(stack, true, offset, 0.0, 0.0);
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(
                w.step_material_flat_gravity(0.01, GravitySettling::Refuse)
                    .unwrap_err(),
                GravityRejection::UnsupportedScene
            );
            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
            assert!(w.loading.as_ref().unwrap().gravity.is_none());
        }
        let mut w = shifted(stack, true, Vec2::new(0.125, 3.25), 0.0, 0.0);
        let h = w.body_handle(body(11)).unwrap();
        let p = w.raw.bodies[h].translation();
        w.raw.bodies[h].set_translation(p + Vector::Y * 0.001, true);
        assert_eq!(
            w.step_material_flat_gravity(0.01, GravitySettling::Refuse)
                .unwrap_err(),
            GravityRejection::UnsupportedScene
        );
    }
    for x in [-16.0_f32, 16.0] {
        assert!(Frame::from_base([x + 1.0, 16.0], 1).is_some());
        assert!(Frame::from_base([x + 2.0, -16.0], 2).is_some());
    }
}
