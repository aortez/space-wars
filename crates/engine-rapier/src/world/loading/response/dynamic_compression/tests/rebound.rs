//! Compatibility, hostile continuation and boundary ownership checks.
use super::*;

fn released() -> PhysicsWorld {
    let mut w = world();
    w.set_gravity(Vec2::X * 50.0);
    w.set_pose(body(11), Vec2::X / 128.0, 0.0, true);
    w
}
fn phase(t: f32) -> PhysicsWorld {
    let mut w = released();
    w.step_material_dynamic_compression(t / 2.0).unwrap();
    w.step_material_dynamic_compression(t / 2.0).unwrap();
    w
}
#[test]
fn rebound_preserves_frozen_v15_overload_equilibrium_and_invalidated_bytes() {
    for (weight, x, dt) in [
        (8000.0, 0.0, 0.001),
        (8000.0, 0.0, 0.02),
        (1562.5, 1.0 / 256.0, 0.01),
    ] {
        for reason in [
            None,
            Some(gravity::Invalidation::ExternalEdit),
            Some(gravity::Invalidation::OtherStep),
        ] {
            let mut w = world();
            w.set_gravity(Vec2::X * (weight / 20.0));
            w.set_pose(body(11), Vec2::X * x, 0.0, true);
            w.step_material_dynamic_compression(dt).unwrap();
            if let Some(reason) = reason {
                w.loading.as_mut().unwrap().invalidate_gravity(reason);
            }
            let c = state(&mut w);
            assert_eq!(c.revision, 1);
            let mut frozen = bincode::serialize(&1_u32).unwrap();
            frozen.extend(model::frozen_v15_history(&c.history));
            match &c.status {
                Status::Active(a) => {
                    bincode::serialize_into(&mut frozen, &0_u32).unwrap();
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
                _ => panic!("v15 contract changed"),
            }
            assert_eq!(bincode::serialize(c).unwrap(), frozen);
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &15_u32.to_le_bytes());
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
fn rebound_versions_and_modes_cannot_relabel_or_renew_history() {
    for t in [0.004, 0.025, 0.05] {
        for terminal in [false, true] {
            let mut w = phase(t);
            if terminal {
                w.step(0.01);
            }
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &16_u32.to_le_bytes());
            assert_eq!(
                PhysicsWorld::from_snapshot_bytes(&bytes)
                    .unwrap()
                    .snapshot_bytes()
                    .unwrap(),
                bytes
            );
            for version in 0_u32..=17 {
                if version == 16 {
                    continue;
                }
                let mut bad = bytes.clone();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&bad).is_err(),
                    "version {version}"
                );
            }
            // Even replacing both the header and tag cannot pass revision 2 as v15.
            let tail = bincode::serialize(state(&mut w)).unwrap();
            let mut bad = bytes.clone();
            bad[..4].copy_from_slice(&15_u32.to_le_bytes());
            let tag = bad.len() - tail.len() - 8;
            bad[tag..tag + 8].copy_from_slice(&DYNAMIC_COMPRESSION_SNAPSHOT_TAG.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
            for cut in [1, 4, 8, 64] {
                assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
            assert_eq!(
                w.step_material_compression(0.01, 0.0),
                Err(R::UnsupportedScene)
            );
            assert_eq!(
                w.step_material_flat_gravity(0.01, gravity::GravitySettling::Refuse)
                    .unwrap_err(),
                gravity::GravityRejection::StaleContinuation
            );
            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
        }
    }
}
#[test]
fn rebound_corrupt_totals_and_owner_phase_refuse() {
    let changes: &[fn(&mut Continuation)] = &[
        |c| c.revision = 1,
        |c| c.revision = 4,
        |c| c.history.summary.indentation = -0.3,
        |c| c.history.summary.velocity += 1e-12,
        |c| c.history.summary.plastic_indentation = -0.01,
        |c| c.history.summary.kinetic_energy += 1.0,
        |c| c.history.summary.elastic_energy += 1.0,
        |c| c.history.summary.gravity_work += 1.0,
        |c| c.history.summary.damage_work += 1.0,
        |c| c.history.summary.viscous_loss = -1.0,
        |c| c.history.summary.charge_error += 1.0,
        |c| c.history.summary.absolute_charge_error = -1.0,
        |c| c.history.summary.reaction_impulse += 1.0,
        |c| c.history.summary.integrated_velocity += 0.01,
        |c| c.history.summary.errors[0] += 1.0,
        |c| c.history.summary.errors[1] += 1.0,
        |c| c.history.summary.errors[2] += 0.01,
        |c| c.history.summary.absolute_errors[0] = 1.0,
        |c| c.history.summary.absolute_errors[1] = 1.0,
        |c| c.history.summary.absolute_errors[2] = 0.01,
        |c| c.history.summary.scales[0] *= 2.0,
        |c| c.history.summary.scales[1] *= 2.0,
        |c| c.history.summary.scales[2] *= 2.0,
        |c| c.history.summary.solves = 0,
    ];
    for t in [0.004, 0.025, 0.05] {
        let original = phase(t);
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
            (|a: &mut ReboundActive| a.owner = Some(EpisodeId(999))) as fn(&mut ReboundActive),
            |a| a.scene.plates.sources.swap(0, 1),
            |a| a.scene.mass *= 2.0,
            |a| a.scene.acceleration *= 2.0,
        ] {
            let mut w = original.clone();
            let Status::Rebound(a) = &mut state(&mut w).status else {
                panic!()
            };
            change(a);
            assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
            rollback(&mut w, 0.001, R::StaleContinuation);
        }
    }
    // Forge a live owner while detached: neither matching ID nor source pair
    // can replace the required contact-phase witness.
    let mut w = phase(0.025);
    let mut pair;
    if let Status::Rebound(a) = &mut state(&mut w).status {
        assert!(a.scene.plates.x < 0.0 && a.owner.is_none());
        a.owner = Some(EpisodeId(1));
        pair = a.scene.plates.sources;
    } else {
        panic!()
    }
    pair.sort();
    w.loading.as_mut().unwrap().episodes.insert(
        pair,
        Episode {
            id: EpisodeId(1),
            absent_seconds: 0.0,
        },
    );
    assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
    rollback(&mut w, 0.001, R::StaleContinuation);
}
#[test]
fn rebound_late_event_limit_and_identity_exhaustion_roll_back() {
    let mut w = released();
    // Three transitions fit in this mass-5 cycle: entry, exit, re-entry.
    let h = w.collider_handle(collider(11)).unwrap();
    w.raw.colliders[h].set_mass(5.0);
    w.refresh_mass_properties(body(11));
    w.set_gravity(Vec2::X * 200.0);
    let count = History::new(&Scene::read(&w).unwrap())
        .unwrap()
        .subdivision(1.0 / 30.0)
        .unwrap()
        .0;
    w.loading.as_mut().unwrap().config.max_records_per_step = count + 2;
    rollback(&mut w, 1.0 / 30.0, R::Limit);
    let mut w = released();
    w.loading.as_mut().unwrap().next_episode = u64::MAX - 1;
    w.step_material_dynamic_compression(0.025).unwrap();
    assert!(state(&mut w).active().unwrap().1.is_none());
    rollback(&mut w, 0.025, R::Limit);
}

#[test]
fn rebound_external_mutations_cannot_be_undone_to_reactivate_a_flight() {
    let original = phase(0.025);
    let changes: &[fn(&mut PhysicsWorld)] = &[
        |w| {
            let g = Vec2::X * 50.0;
            w.set_gravity(Vec2::ZERO);
            w.set_gravity(g);
        },
        |w| {
            let m = w.motion(body(11)).unwrap();
            w.set_velocity(body(11), Vec2::ZERO, 0.0, true);
            w.set_velocity(body(11), m.linear_velocity, 0.0, true);
        },
        |w| {
            let m = w.motion(body(11)).unwrap();
            w.set_pose(body(11), Vec2::ZERO, 0.0, true);
            w.set_pose(body(11), m.position, 0.0, true);
        },
        |w| {
            w.apply_force(body(11), Vec2::X, true);
            w.clear_forces();
        },
        |w| {
            w.step(0.00001);
        },
    ];
    for change in changes {
        let mut w = original.clone();
        change(&mut w);
        assert!(matches!(state(&mut w).status, Status::Invalidated(_)));
        let mut restored = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
        rollback(&mut restored, 0.001, R::StaleContinuation);
    }
}
