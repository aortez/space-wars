use super::*;
use crate::world::loading::response::gravity::tests::masses::set_masses;

// Independently spell the original v7 tail, including enum tags and field order.
fn frozen_v7(c: &Continuation) -> Vec<u8> {
    let h = &c.history;
    let mut bytes = bincode::serialize(&(
        h.revision,
        match h.settling {
            GravitySettling::Refuse => 0_u32,
            GravitySettling::BoundedNumerical => 1,
        },
        h.reference,
        h.scales,
        h.errors,
        2_u32,
    ))
    .unwrap();
    let Status::Stack(s) = &c.status else {
        unreachable!()
    };
    bincode::serialize_into(&mut bytes, &(s.corrections.impulse, s.corrections.work)).unwrap();
    match &s.status {
        StackStatus::Invalidated(reason) => {
            bincode::serialize_into(
                &mut bytes,
                &(
                    1_u32,
                    match reason {
                        Invalidation::ExternalEdit => 0_u32,
                        Invalidation::OtherStep => 1,
                    },
                ),
            )
            .unwrap();
        }
        StackStatus::Active(a) => {
            bincode::serialize_into(&mut bytes, &0_u32).unwrap();
            let plates = a.plates.each_ref().map(|p| {
                let m = &p.material;
                let e = &p.endpoint;
                (
                    p.body,
                    p.source,
                    p.slices,
                    p.copies,
                    p.colliders,
                    (m.parameters, m.brittle, m.volume, m.thickness, m.depth),
                    (
                        e.position,
                        e.velocity,
                        e.angle,
                        e.angular_velocity,
                        e.mass,
                        e.local_com,
                        e.inertia,
                    ),
                    p.additional_iterations,
                )
            });
            let incoming = a.incoming.as_ref().map(|e| {
                (
                    e.sources,
                    e.id,
                    e.energy,
                    e.gap_upper,
                    e.turned,
                    e.held,
                    e.settled_work,
                )
            });
            bincode::serialize_into(
                &mut bytes,
                &(
                    plates,
                    a.capture,
                    a.integration,
                    (a.support.sources, a.support.id),
                    incoming,
                ),
            )
            .unwrap();
        }
    }
    bytes
}

#[test]
fn native_mass_legacy_v7_tail_stays_byte_exact_in_every_lifecycle() {
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for episode in 0..4 {
            if episode == 3 && policy == GravitySettling::Refuse {
                continue;
            }
            for invalidation in [
                None,
                Some(Invalidation::ExternalEdit),
                Some(Invalidation::OtherStep),
            ] {
                let mut world = enrolled([11, 2, 7], policy, episode);
                if let Some(reason) = invalidation {
                    world.loading.as_mut().unwrap().invalidate_gravity(reason);
                }
                let c = world.loading.as_ref().unwrap().gravity.as_ref().unwrap();
                let frozen = frozen_v7(c);
                assert_eq!(bincode::serialize(c).unwrap(), frozen);
                let bytes = world.snapshot_bytes().unwrap();
                assert_eq!(&bytes[..4], &7_u32.to_le_bytes());
                assert!(bytes.ends_with(&frozen));
                assert_eq!(roundtrip(&world).snapshot_bytes().unwrap(), bytes);
            }
        }
    }
}

#[test]
fn native_mass_stack_restore_keeps_original_bounds_and_refuses_corruption() {
    for masses in [[20.0, 5.0], [13.0, 31.0], [80.0, 20.0]] {
        let mut world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
        set_masses(&mut world, &[11, 2], &masses);
        world
            .step_material_flat_gravity(0.001, GravitySettling::BoundedNumerical)
            .unwrap();
        assert_eq!(history(&world).revision, 2);
        for invalidated in [false, true] {
            let mut initial = world.clone();
            // Nonzero retained totals ensure invalidation cannot erase them.
            state_mut(&mut initial).corrections = Corrections {
                impulse: 1.0e-7,
                work: 1.0e-12,
            };
            if invalidated {
                initial
                    .loading
                    .as_mut()
                    .unwrap()
                    .invalidate_gravity(Invalidation::OtherStep);
            }
            let bytes = initial.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &9_u32.to_le_bytes());
            let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            assert_eq!(state(&restored), state(&initial));
            for case in 0..7 {
                let mut changed = initial.clone();
                match case {
                    0 => {
                        changed
                            .loading
                            .as_mut()
                            .unwrap()
                            .gravity
                            .as_mut()
                            .unwrap()
                            .history
                            .revision = 5
                    }
                    1 => {
                        changed
                            .loading
                            .as_mut()
                            .unwrap()
                            .gravity
                            .as_mut()
                            .unwrap()
                            .history
                            .errors[0] = 1.0
                    }
                    2 => state_mut(&mut changed).corrections.impulse = 1.0,
                    3 => state_mut(&mut changed).corrections.work = f64::NAN,
                    4 => state_mut(&mut changed).corrections.work = -1.0,
                    5 => state_mut(&mut changed).corrections.work = 1.0,
                    _ => state_mut(&mut changed).corrections.work = 0.0,
                }
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err()
                );
            }
        }
        for case in 0..6 {
            let mut changed = world.clone();
            let a = active(&mut changed);
            match case {
                0 => a.plates[0].endpoint.mass = 81.0,
                1 => a.plates[1].endpoint.mass = 4.0,
                2 => a.plates[1].endpoint.mass = f32::INFINITY,
                3 => {
                    a.plates[0].endpoint.mass = 5.0;
                    a.plates[1].endpoint.mass = 80.0;
                }
                4 => a.support.id = EpisodeId(999),
                _ => {
                    changed
                        .loading
                        .as_mut()
                        .unwrap()
                        .gravity
                        .as_mut()
                        .unwrap()
                        .history
                        .reference[2] = 0.0
                }
            }
            assert!(PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_err());
        }
    }
}

#[test]
fn translated_extension_preserves_frozen_v9_stack_tail() {
    for offset in [0.0, 3.25] {
        for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
            for dt in [0.001, 0.01] {
                let mut w = stack_world([11, 2, 7], true, [4, 2, 4], 2);
                set_masses(&mut w, &[11, 2], &[13.0, 31.0]);
                for id in [11, 2, 7] {
                    let p = w.motion(body(id)).unwrap().position;
                    w.set_pose(body(id), p + Vec2::new(0.0, offset), 0.0, true);
                }
                w.step_material_flat_gravity(dt, policy).unwrap();
                for reason in [
                    None,
                    Some(Invalidation::ExternalEdit),
                    Some(Invalidation::OtherStep),
                ] {
                    let mut saved = w.clone();
                    if let Some(reason) = reason {
                        saved.loading.as_mut().unwrap().invalidate_gravity(reason);
                    }
                    let s = saved.loading.as_ref().unwrap().gravity.as_ref().unwrap();
                    let frozen = frozen_v7(s);
                    assert_eq!(bincode::serialize(s).unwrap(), frozen);
                    let bytes = saved.snapshot_bytes().unwrap();
                    assert_eq!(
                        &bytes[..4],
                        &(if offset == 0.0 { 9_u32 } else { 11 }).to_le_bytes()
                    );
                    assert!(bytes.ends_with(&frozen));
                    assert_eq!(
                        PhysicsWorld::from_snapshot_bytes(&bytes)
                            .unwrap()
                            .snapshot_bytes()
                            .unwrap(),
                        bytes
                    );
                }
            }
        }
    }
}

#[test]
fn translated_stack_restore_rejects_changed_frame_owners_and_retained_corrections() {
    let mut world = stack_world([11, 2, 7], true, [4, 2, 4], 2);
    set_masses(&mut world, &[11, 2], &[13.0, 31.0]);
    for (role, id) in [11, 2, 7].into_iter().enumerate() {
        world.set_pose(body(id), Vec2::new(role as f32, 3.25), 0.0, true);
    }
    world.set_velocity(body(11), Vec2::X * 0.01, 0.0, true);
    world
        .step_material_flat_gravity(0.01, GravitySettling::BoundedNumerical)
        .unwrap();
    for case in 0..10 {
        let mut w = world.clone();
        match case {
            0 => active(&mut w).plates[1].endpoint.position[0] += 0.001,
            1 => active(&mut w).plates[2].endpoint.position[0] += 0.001,
            2 => active(&mut w).plates[0].endpoint.position[1] = 0.0,
            3 => active(&mut w).plates[1].endpoint.position[1] = 0.0,
            4 => active(&mut w).plates[2].endpoint.position[1] = 17.0,
            5 => active(&mut w).incoming.as_mut().unwrap().gap_upper = -1.0,
            6 => active(&mut w).support.id = EpisodeId(999),
            7 => active(&mut w).incoming.as_mut().unwrap().settled_work = 1.0,
            8 => state_mut(&mut w).corrections.impulse = 1.0,
            _ => state_mut(&mut w).corrections.work = 1.0,
        }
        assert!(
            PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err(),
            "case {case}"
        );
    }
    state_mut(&mut world).corrections = Corrections {
        impulse: 1.0e-8,
        work: 1.0e-14,
    };
    let before = state(&world).corrections.clone();
    for id in [11, 2, 7] {
        world.remove_entity(body(id).entity);
    }
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(state(&restored).corrections, before);
    assert_eq!(&bytes[..4], &11_u32.to_le_bytes());
    state_mut(&mut restored).corrections.work = 1.0;
    assert!(PhysicsWorld::from_snapshot_bytes(&restored.snapshot_bytes().unwrap()).is_err());
}
