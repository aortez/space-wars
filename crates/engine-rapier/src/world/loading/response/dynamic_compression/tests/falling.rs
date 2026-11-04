//! Incoming motion, phase ownership and v15/v16/v17 compatibility.
use super::*;
fn falling_world(x: f32, v: f32) -> PhysicsWorld {
    let mut w = world();
    w.set_gravity(Vec2::X * 50.0);
    w.set_pose(body(11), Vec2::X * x, 0.0, true);
    w.set_velocity(body(11), Vec2::X * v, 0.0, true);
    w
}
fn samples() -> Vec<PhysicsWorld> {
    let mut w = falling_world(-1.0 / 16.0, 0.5);
    w.step_material_dynamic_compression(0.001).unwrap();
    let mut result = vec![w.clone()];
    let h = 1.0 / 8192.0;
    loop {
        let step = w.step_material_dynamic_compression(h).unwrap();
        if step
            .events
            .iter()
            .any(|e| e.event.transition == EpisodeTransition::Impact)
        {
            break;
        }
    }
    assert!(matches!(&state(&mut w).status,Status::Falling(a) if a.pending_support));
    result.push(w.clone());
    w.step_material_dynamic_compression(h).unwrap();
    result.push(w.clone());
    for _ in 0..12 {
        w.step_material_dynamic_compression(1.0 / 240.0).unwrap();
    }
    assert!(state(&mut w).active().unwrap().1.is_none());
    result.push(w);
    result
}
#[test]
fn falling_preserves_frozen_v16_histories_and_replay() {
    for (weight, x, t) in [
        (1000.0, 1.0 / 128.0, 0.004),
        (1000.0, 1.0 / 128.0, 0.025),
        (1000.0, 1.0 / 128.0, 0.05),
        (4000.0, 0.0, 0.03),
    ] {
        for reason in [
            None,
            Some(gravity::Invalidation::ExternalEdit),
            Some(gravity::Invalidation::OtherStep),
        ] {
            let mut w = world();
            w.set_gravity(Vec2::X * (weight / 20.0));
            w.set_pose(body(11), Vec2::X * x, 0.0, true);
            w.step_material_dynamic_compression(t / 2.0).unwrap();
            w.step_material_dynamic_compression(t / 2.0).unwrap();
            if let Some(reason) = reason {
                w.loading.as_mut().unwrap().invalidate_gravity(reason);
            }
            let c = state(&mut w);
            assert_eq!(c.revision, 2);
            assert!(c.history.incoming.is_none());
            let mut frozen = bincode::serialize(&2_u32).unwrap();
            frozen.extend(model::frozen_v15_history(&c.history));
            match &c.status {
                Status::Rebound(a) => {
                    bincode::serialize_into(&mut frozen, &2_u32).unwrap();
                    frozen.extend(compression::scene::frozen_v14_scene(&a.scene.plates));
                    bincode::serialize_into(
                        &mut frozen,
                        &(a.scene.mass, a.scene.acceleration, a.owner),
                    )
                    .unwrap();
                }
                Status::Invalidated(reason) => {
                    bincode::serialize_into(&mut frozen, &1_u32).unwrap();
                    bincode::serialize_into(
                        &mut frozen,
                        &match reason {
                            gravity::Invalidation::ExternalEdit => 0_u32,
                            gravity::Invalidation::OtherStep => 1,
                        },
                    )
                    .unwrap();
                }
                _ => panic!("v16 lifecycle changed"),
            }
            assert_eq!(bincode::serialize(c).unwrap(), frozen);
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &16_u32.to_le_bytes());
            assert!(bytes.ends_with(&frozen));
            let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            assert_eq!(
                restored.step_material_dynamic_compression(0.001),
                w.step_material_dynamic_compression(0.001)
            );
            assert_eq!(
                restored.snapshot_bytes().unwrap(),
                w.snapshot_bytes().unwrap()
            );
        }
    }
}
#[test]
fn falling_v17_motion_extension_and_modes_cannot_reset_accounting() {
    for original in samples() {
        for terminal in [false, true] {
            let mut w = original.clone();
            if terminal {
                w.remove_entity(body(11).entity);
                w.remove_entity(body(2).entity);
            }
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &17_u32.to_le_bytes());
            assert_eq!(
                PhysicsWorld::from_snapshot_bytes(&bytes)
                    .unwrap()
                    .snapshot_bytes()
                    .unwrap(),
                bytes
            );
            for version in 0_u32..=18 {
                if version == 17 {
                    continue;
                }
                let mut bad = bytes.clone();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&bad).is_err(),
                    "version {version}"
                );
            }
            let tail = bincode::serialize(state(&mut w)).unwrap();
            for (version, tag) in [
                (15_u32, DYNAMIC_COMPRESSION_SNAPSHOT_TAG),
                (16, REBOUND_SNAPSHOT_TAG),
            ] {
                // Rewrite both discriminators and drop the mandatory v17 motion:
                // no legacy reader may reinterpret an incoming-energy history.
                let mut bad = bytes[..bytes.len() - 4].to_vec();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                let at = bad.len() - tail.len() - 8;
                bad[at..at + 8].copy_from_slice(&tag.to_le_bytes());
                assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
            }
            for v in [f32::NAN, -1.0, 65.0, 0.0, 1.0] {
                let mut bad = bytes.clone();
                let end = bad.len();
                bad[end - 4..].copy_from_slice(&v.to_le_bytes());
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&bad).is_err(),
                    "initial v={v}"
                );
            }
            for cut in [1, 4, 8, 64] {
                assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
            }
            let mut bad = bytes.clone();
            bad.push(0);
            assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
            assert_eq!(
                w.step_material_compression(0.001, 0.0),
                Err(R::UnsupportedScene)
            );
            assert_eq!(
                w.step_material_flat_gravity(0.001, gravity::GravitySettling::Refuse)
                    .unwrap_err(),
                gravity::GravityRejection::StaleContinuation
            );
            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
        }
    }
}
#[test]
fn falling_rejects_corrupt_balance_phase_and_live_witnesses() {
    let changes: &[fn(&mut Continuation)] = &[
        |c| c.revision = 4,
        |c| c.history.summary.kinetic_energy += 1.0,
        |c| c.history.summary.gravity_work += 1.0,
        |c| c.history.summary.elastic_energy += 1.0,
        |c| c.history.summary.damage_work += 1.0,
        |c| c.history.summary.reaction_impulse += 1.0,
        |c| c.history.summary.integrated_velocity += 0.01,
        |c| c.history.summary.plastic_indentation += 0.001,
        |c| c.history.summary.scales[0] *= 2.0,
        |c| c.history.summary.absolute_errors[2] = 0.01,
        |c| c.history.summary.solves = 0,
    ];
    for original in samples() {
        for terminal in [false, true] {
            for change in changes {
                let mut w = original.clone();
                if terminal {
                    w.remove_entity(body(11).entity);
                    w.remove_entity(body(2).entity);
                }
                change(state(&mut w));
                assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
            }
        }
        for change in [
            (|a: &mut FallingActive| a.owner = Some(EpisodeId(999))) as fn(&mut FallingActive),
            |a| {
                a.owner = None;
                a.pending_support = true;
            },
            |a| a.scene.mass *= 2.0,
            |a| a.scene.plates.sources.swap(0, 1),
        ] {
            let mut w = original.clone();
            let Status::Falling(a) = &mut state(&mut w).status else {
                panic!()
            };
            change(a);
            assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
            rollback(&mut w, 0.001, R::StaleContinuation);
        }
    }
}
#[test]
fn falling_late_handoff_record_limit_and_identity_exhaustion_are_atomic() {
    let mut w = falling_world(0.0, 1.0);
    let dt = 1.0 / 240.0;
    let count = History::new(&Scene::read(&w).unwrap())
        .unwrap()
        .subdivision(dt)
        .unwrap()
        .0;
    // Entry fits; the same-owner Support observation must still fit before commit.
    w.loading.as_mut().unwrap().config.max_records_per_step = count + 1;
    rollback(&mut w, dt, R::Limit);
    let mut w = falling_world(-1.0 / 64.0, 0.0);
    w.loading.as_mut().unwrap().next_episode = u64::MAX;
    w.step_material_dynamic_compression(0.001).unwrap();
    assert!(w.loading.as_ref().unwrap().episodes.is_empty());
    rollback(&mut w, 1.0 / 30.0, R::Limit);
    let mut w = falling_world(-1.0 / 64.0, 0.0);
    let dt = 1.0 / 8192.0;
    w.loading.as_mut().unwrap().config.max_records_per_step = 1;
    // No phantom enrollment event/owner consumes an output slot during flight.
    assert!(
        w.step_material_dynamic_compression(dt)
            .unwrap()
            .events
            .is_empty()
    );
}
