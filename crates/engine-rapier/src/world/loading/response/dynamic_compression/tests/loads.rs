use super::*;

#[test]
fn unchanged_falling_histories_keep_frozen_v17_bytes_and_replay() {
    for original in falling::samples() {
        for reason in [
            None,
            Some(gravity::Invalidation::ExternalEdit),
            Some(gravity::Invalidation::OtherStep),
        ] {
            let mut w = original.clone();
            if let Some(reason) = reason {
                w.loading.as_mut().unwrap().invalidate_gravity(reason);
            }
            let c = state(&mut w);
            let mut frozen = bincode::serialize(&3_u32).unwrap();
            frozen.extend(model::frozen_v15_history(&c.history));
            match &c.status {
                Status::Falling(a) => {
                    bincode::serialize_into(&mut frozen, &3_u32).unwrap();
                    frozen.extend(compression::scene::frozen_v14_scene(&a.scene.plates));
                    bincode::serialize_into(
                        &mut frozen,
                        &(
                            a.scene.mass,
                            a.scene.acceleration,
                            a.owner,
                            a.pending_support,
                        ),
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
                _ => panic!("v17 lifecycle changed"),
            }
            assert_eq!(bincode::serialize(&c).unwrap(), frozen);
            bincode::serialize_into(&mut frozen, &c.history.incoming.as_ref().unwrap().velocity)
                .unwrap();
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &17_u32.to_le_bytes());
            assert!(bytes.ends_with(&frozen));
            if reason.is_none() {
                w.set_material_dynamic_compression_acceleration(50.0)
                    .unwrap();
                assert_eq!(w.snapshot_bytes().unwrap(), bytes);
            }
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

fn change_refused(w: &mut PhysicsWorld, acceleration: f32, rejection: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let batches = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(acceleration),
        Err(rejection)
    );
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), batches);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn load_commands_are_bounded_atomic_and_cannot_revive_history() {
    change_refused(&mut world(), 50.0, R::UnsupportedScene);
    let mut w = enrolled();
    let legacy = w.snapshot_bytes().unwrap();
    w.set_material_dynamic_compression_acceleration(400.0)
        .unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), legacy);
    for a in [f32::NAN, f32::INFINITY, -1.0, 600.01, 1e7] {
        change_refused(&mut w, a, R::InvalidCommand);
    }
    let before = w.material_dynamic_compression_summary();
    let impulse = w.material_dynamic_compression_gravity_impulse();
    let episodes = bincode::serialize(&w.loading.as_ref().unwrap().episodes).unwrap();
    for i in 0..MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
        w.set_material_dynamic_compression_acceleration(if i % 2 == 0 { 0.0 } else { 600.0 })
            .unwrap();
        assert_eq!(w.material_dynamic_compression_summary(), before);
        assert_eq!(w.material_dynamic_compression_gravity_impulse(), impulse);
        assert_eq!(
            bincode::serialize(&w.loading.as_ref().unwrap().episodes).unwrap(),
            episodes
        );
        w = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
        w.step_material_dynamic_compression(0.0).unwrap();
    }
    change_refused(&mut w, 50.0, R::Limit);
    w.set_material_dynamic_compression_acceleration(600.0)
        .unwrap();
    w.set_gravity(Vec2::X * 599.0);
    w.set_gravity(Vec2::X * 600.0);
    change_refused(&mut w, 600.0, R::StaleContinuation);
    w.remove_entity(body(11).entity);
    w.remove_entity(body(2).entity);
    let bytes = w.snapshot_bytes().unwrap();
    let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.material_dynamic_compression_summary(), before);
    assert_eq!(
        restored.material_dynamic_compression_gravity_impulse(),
        impulse
    );
}

#[test]
fn load_change_retains_pending_impact_support_across_restore() {
    let mut w = world();
    w.set_gravity(Vec2::X * 50.0);
    w.set_velocity(body(11), Vec2::X, 0.0, true);
    let step = w.step_material_dynamic_compression(1.0 / 8192.0).unwrap();
    assert_eq!(step.events.len(), 1);
    assert_eq!(step.events[0].event.transition, EpisodeTransition::Impact);
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let next = w.step_material_dynamic_compression(1.0 / 8192.0).unwrap();
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].event.transition, EpisodeTransition::Support);
    assert_eq!(next.events[0].event.id, step.events[0].event.id);
    assert_eq!(
        restored
            .step_material_dynamic_compression(1.0 / 8192.0)
            .unwrap(),
        next
    );
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}

#[test]
fn load_changes_keep_falling_phases_and_incoming_motion() {
    for mut w in falling::samples() {
        let before = w.material_dynamic_compression_summary();
        w.set_material_dynamic_compression_acceleration(100.0)
            .unwrap();
        assert_eq!(w.material_dynamic_compression_summary(), before);
        let bytes = w.snapshot_bytes().unwrap();
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
        for terminal in [false, true] {
            let mut bad = w.clone();
            if terminal {
                bad.remove_entity(body(11).entity);
                bad.remove_entity(body(2).entity);
            }
            let valid = bad.snapshot_bytes().unwrap();
            assert_eq!(
                PhysicsWorld::from_snapshot_bytes(&valid)
                    .unwrap()
                    .snapshot_bytes()
                    .unwrap(),
                valid
            );
            state(&mut bad).history.incoming = None;
            assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
        }
    }
}

#[test]
fn changing_load_saves_validate_source_integrals_and_discriminators() {
    let mut original = enrolled();
    original
        .set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    original.step_material_dynamic_compression(0.001).unwrap();
    original
        .set_material_dynamic_compression_acceleration(50.0)
        .unwrap();
    original.step_material_dynamic_compression(0.001).unwrap();
    let changes: &[fn(&mut Continuation)] = &[
        |c| c.revision = 3,
        |c| c.history.loads.as_mut().unwrap().clear(),
        |c| c.history.loads.as_mut().unwrap()[0].elapsed = f64::NAN,
        |c| c.history.loads.as_mut().unwrap()[0].elapsed = 0.0,
        |c| c.history.loads.as_mut().unwrap()[0].elapsed *= 0.5,
        |c| c.history.loads.as_mut().unwrap()[0].indentation += 0.001,
        |c| c.history.loads.as_mut().unwrap()[0].acceleration = -1.0,
        |c| c.history.loads.as_mut().unwrap()[0].acceleration = 400.0,
        |c| c.history.loads.as_mut().unwrap()[1].elapsed = 1.0,
        |c| c.history.summary.gravity_work += 1.0,
        |c| c.history.summary.reaction_impulse += 1.0,
        |c| c.history.summary.absolute_errors[0] = 1.0,
        |c| c.history.summary.scales[1] *= 2.0,
    ];
    for terminal in [false, true] {
        let mut w = original.clone();
        if terminal {
            w.remove_entity(body(11).entity);
            w.remove_entity(body(2).entity);
        }
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &18_u32.to_le_bytes());
        assert_eq!(
            PhysicsWorld::from_snapshot_bytes(&bytes)
                .unwrap()
                .snapshot_bytes()
                .unwrap(),
            bytes
        );
        for change in changes {
            let mut bad = w.clone();
            change(state(&mut bad));
            assert!(
                bad.snapshot_bytes()
                    .and_then(|b| PhysicsWorld::from_snapshot_bytes(&b))
                    .is_err()
            );
        }
        for cut in [1, 4, 20, 45] {
            assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
        // The extension is Option<incoming>, u32 count, then two 20-byte records.
        let extension = 45;
        for count in [0_u32, 33, u32::MAX] {
            let mut bad = bytes.clone();
            let at = bad.len() - extension + 1;
            bad[at..at + 4].copy_from_slice(&count.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
        }
        let tail = bincode::serialize(state(&mut w)).unwrap();
        for (version, tag) in [
            (15_u32, DYNAMIC_COMPRESSION_SNAPSHOT_TAG),
            (16, REBOUND_SNAPSHOT_TAG),
            (17, FALLING_SUPPORT_SNAPSHOT_TAG),
        ] {
            let mut bad = bytes[..bytes.len() - extension].to_vec();
            bad[..4].copy_from_slice(&version.to_le_bytes());
            let at = bad.len() - tail.len() - 8;
            bad[at..at + 8].copy_from_slice(&tag.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
        }
    }
}
