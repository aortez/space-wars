//! Hostile snapshots, private mutations and bounded transaction failures.
use super::*;
use engine_material::{ARMOR, WOOD};

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, 0)
}
fn world() -> PhysicsWorld {
    let mut w = PhysicsWorld::new(PhysicsWorldConfig::default());
    w.enable_loading_capture(CaptureConfig::default()).unwrap();
    w.enable_material_response().unwrap();
    // Deliberately reverse geometric and canonical source order.
    for (role, n) in [11, 2].into_iter().enumerate() {
        let mut shape = ColliderSpec::cuboid(collider(n), 0.5, 1.0);
        shape.friction = 0.0;
        w.insert_body(
            body(n),
            BodySpec {
                kind: if role == 0 {
                    BodyKind::KinematicVelocity
                } else {
                    BodyKind::Fixed
                },
                position: Vec2::new(role as f32, 0.0),
                can_sleep: false,
                lock_rotation: true,
                ..Default::default()
            },
            &[shape],
        );
        let token = w
            .create_loading_region(
                RegionId {
                    entity: body(n).entity,
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        w.bind_loading_collider(
            collider(n),
            ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
        if role == 1 {
            w.register_loading_material(
                token,
                Region::new(WOOD, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        }
    }
    w
}
fn enrolled() -> PhysicsWorld {
    let mut w = world();
    w.step_material_compression(1.0, 0.0625).unwrap();
    w
}
fn state(w: &mut PhysicsWorld) -> &mut Continuation {
    w.loading.as_mut().unwrap().compression.as_mut().unwrap()
}
fn rollback(w: &mut PhysicsWorld, dt: f32, v: f32, reason: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_compression(dt, v), Err(reason));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn compression_snapshot_validates_every_physical_and_numerical_total() {
    let w = enrolled();
    let bad: &[fn(&mut Continuation)] = &[
        |c| c.revision += 1,
        |c| c.history.summary.indentation = 0.3,
        |c| c.history.summary.plastic_indentation = -0.001,
        |c| c.history.summary.elastic_energy += 1.0,
        |c| c.history.summary.driver_work += 1.0,
        |c| c.history.summary.damage_work += 1.0,
        |c| c.history.summary.viscous_loss = -1.0,
        |c| c.history.summary.charge_error += 1.0,
        |c| c.history.summary.absolute_charge_error = 1.0,
        |c| c.history.summary.absolute_motion_error = 0.0001,
        |c| c.history.summary.elapsed = f64::NAN,
        |c| c.history.summary.elapsed = 3601.0,
        |c| c.history.summary.steps = 0,
        |c| c.history.summary.steps = 1_000_001,
    ];
    for terminal in [false, true] {
        for mutate in bad {
            let mut wrong = w.clone();
            if terminal {
                wrong.remove_entity(body(11).entity);
                wrong.remove_entity(body(2).entity);
            }
            mutate(state(&mut wrong));
            assert!(PhysicsWorld::from_snapshot_bytes(&wrong.snapshot_bytes().unwrap()).is_err());
        }
    }
}

#[test]
fn compression_snapshot_witness_checks_live_state_and_owner() {
    let w = enrolled();
    let bad: &[fn(&mut PhysicsWorld)] = &[
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.owner = None;
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.owner = Some(EpisodeId(999));
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.x += 0.001;
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.normal = [0.0, 1.0];
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.sources.swap(0, 1);
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.material.depth = 0.5;
            }
        },
        |w| {
            w.loading.as_mut().unwrap().episodes.clear();
        },
        |w| {
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
                .remaining -= 1.0;
        },
        |w| {
            let h = w.body_handle(body(11)).unwrap();
            w.raw.bodies[h].set_translation(Vector::new(0.1, 0.0), true);
        },
        |w| {
            let h = w.collider_handle(collider(2)).unwrap();
            w.raw.colliders[h].set_friction(0.1);
        },
        |w| w.raw.integration_parameters.num_solver_iterations = 8,
    ];
    for mutate in bad {
        let mut wrong = w.clone();
        mutate(&mut wrong);
        assert!(PhysicsWorld::from_snapshot_bytes(&wrong.snapshot_bytes().unwrap()).is_err());
        rollback(&mut wrong, 0.01, 0.0, R::StaleContinuation);
    }
}

#[test]
fn compression_versions_cannot_be_relabelled_truncated_or_reenrolled() {
    for phase in 0..4 {
        let mut w = enrolled();
        if phase >= 1 {
            w.step_material_compression(0.5, -0.125).unwrap();
        }
        if phase >= 2 {
            w.step_material_compression(0.25, 0.125).unwrap();
        }
        if phase == 3 {
            w.step(0.01);
        }
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &14_u32.to_le_bytes());
        let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        for version in 0_u32..=15 {
            if version == 14 {
                continue;
            }
            let mut wrong = bytes.clone();
            wrong[..4].copy_from_slice(&version.to_le_bytes());
            assert!(
                PhysicsWorld::from_snapshot_bytes(&wrong).is_err(),
                "version {version}"
            );
        }
        for cut in [1, 4, 8, 64] {
            assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(PhysicsWorld::from_snapshot_bytes(&extra).is_err());
        assert_eq!(
            w.enable_loading_capture(CaptureConfig::default()),
            Err(LoadingError::AlreadyEnabled)
        );
        assert_eq!(
            w.step_material_flat_gravity(0.01, gravity::GravitySettling::Refuse)
                .unwrap_err(),
            gravity::GravityRejection::StaleContinuation
        );
    }
    let mut old = world().snapshot_bytes().unwrap();
    old[..4].copy_from_slice(&14_u32.to_le_bytes());
    assert!(PhysicsWorld::from_snapshot_bytes(&old).is_err());
}

#[test]
fn compression_caps_and_unrepresentable_charges_roll_back() {
    let mut w = world();
    w.loading.as_mut().unwrap().config.max_records_per_step = 1;
    rollback(&mut w, 0.01, 0.1, R::Limit);
    w.loading.as_mut().unwrap().config.max_records_per_step = 3;
    let first = w.step_material_compression(1.0, 0.0625).unwrap();
    assert_eq!(first.events.len(), 1);
    let event = first.events[0].event;
    assert!(event.sources[0] < event.sources[1]);
    assert_eq!(first.load.unwrap().episode, Some(event.id));
    // Rounded f32 force cannot express the tiny overload, even though the
    // requested motion and the material's f64 capacity charge are nonzero.
    rollback(&mut w, 1.0, 1e-7, R::Numerical);
    let remaining = &mut w
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .values_mut()
        .next()
        .unwrap()
        .remaining;
    *remaining = 0.0;
    rollback(&mut w, 1.0, 0.01, R::StaleContinuation);

    // A very large remaining capacity cannot retain a tiny elastic/plastic
    // increment. The per-charge guard must refuse rather than trust the ledger.
    let mut w = world();
    let token = *w
        .loading
        .as_ref()
        .unwrap()
        .response
        .as_ref()
        .unwrap()
        .materials
        .keys()
        .next()
        .unwrap();
    w.loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .remove(&token);
    w.register_loading_material(
        token,
        Region::new(ARMOR, 1e12).unwrap(),
        Section::new(1.0, 1.0),
    )
    .unwrap();
    rollback(&mut w, 1.0, 0.010001, R::Numerical);
}

#[test]
fn compression_rejects_unsupported_enrollment_without_consuming_time() {
    let changes: &[fn(&mut PhysicsWorld)] = &[
        |w| {
            w.set_gravity(Vec2::X);
        },
        |w| {
            w.set_velocity(body(11), Vec2::X, 0.0, true);
        },
        |w| {
            w.set_pose(body(11), Vec2::new(-0.001, 0.0), 0.0, true);
        },
        |w| {
            w.set_pose(body(2), Vec2::new(1.0, 0.01), 0.0, true);
        },
        |w| {
            w.set_pose(body(2), Vec2::X, 0.1, true);
        },
        |w| {
            let h = w.collider_handle(collider(2)).unwrap();
            w.raw.colliders[h].set_sensor(true);
        },
        |w| {
            let h = w.body_handle(body(11)).unwrap();
            w.raw.bodies[h].set_body_type(RigidBodyType::Dynamic, true);
        },
        |w| {
            let t = *w
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials
                .keys()
                .next()
                .unwrap();
            w.loading
                .as_mut()
                .unwrap()
                .response
                .as_mut()
                .unwrap()
                .materials
                .remove(&t);
            w.register_loading_material(
                t,
                Region::new(WOOD, 20.0).unwrap(),
                Section::new(1.0, 2.0),
            )
            .unwrap();
        },
    ];
    for change in changes {
        let mut w = world();
        change(&mut w);
        rollback(&mut w, 0.01, 0.05, R::UnsupportedScene);
    }
}

#[test]
fn compression_depth_and_thickness_scale_the_physical_curve() {
    let mut w = world();
    let material = w
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .values_mut()
        .next()
        .unwrap();
    material.depth = 0.5;
    material.thickness = 2.0;
    let load = w
        .step_material_compression(1.0, 0.03125)
        .unwrap()
        .load
        .unwrap();
    // A=4, k=1.6e6, Y=12000, ey=.0075, eta=6000.
    assert!((load.elastic_after - 45.0).abs() < 1e-10);
    assert!((load.damage_work - 285.0).abs() < 1e-4);
    assert!((load.viscous_loss - 4.453125).abs() < 1e-10);
    assert!((load.force - 12187.5).abs() < 1e-10);
    let again = w
        .step_material_compression(0.5, 0.03125)
        .unwrap()
        .load
        .unwrap();
    let pure = sustained(
        &Region::new(WOOD, 20.0).unwrap(),
        SustainedLoad {
            force: 12187.5,
            width: 2.0,
            section: Section::new(2.0, 0.5),
            duration: 0.5,
            measured_plastic_displacement: None,
        },
    )
    .unwrap();
    // Under constant plastic speed, the chosen viscosity reproduces the slow
    // law's force-controlled fallback rate, with actual motion/work accounted.
    assert!((again.damage_work - f64::from(pure.consumed[0])).abs() < 1e-5);
    assert!((again.impulse - 12187.5 * 0.5).abs() < 1e-9);
}

#[test]
fn dynamic_extension_preserves_frozen_v14_history_and_scene_bytes() {
    for phase in 0..3 {
        for reason in [
            None,
            Some(gravity::Invalidation::ExternalEdit),
            Some(gravity::Invalidation::OtherStep),
        ] {
            let mut w = world();
            w.step_material_compression(if phase == 0 { 0.01 } else { 1.0 }, 0.0625)
                .unwrap();
            if phase == 2 {
                w.step_material_compression(1.0, -0.0625).unwrap();
            }
            if let Some(reason) = reason {
                w.loading.as_mut().unwrap().invalidate_gravity(reason);
            }
            let c = w.loading.as_ref().unwrap().compression.as_ref().unwrap();
            let mut frozen = bincode::serialize(&c.revision).unwrap();
            frozen.extend(model::frozen_v14_history(&c.history));
            match &c.status {
                Status::Active(a) => {
                    bincode::serialize_into(&mut frozen, &0_u32).unwrap();
                    frozen.extend(scene::frozen_v14_scene(&a.scene));
                    bincode::serialize_into(&mut frozen, &a.owner).unwrap();
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
            }
            assert_eq!(bincode::serialize(c).unwrap(), frozen);
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &14_u32.to_le_bytes());
            assert!(bytes.ends_with(&frozen));
            assert_eq!(
                PhysicsWorld::from_snapshot_bytes(&bytes)
                    .unwrap()
                    .snapshot_bytes()
                    .unwrap(),
                bytes
            );
            let mut wrong = bytes.clone();
            wrong[..4].copy_from_slice(&15_u32.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&wrong).is_err());
            if reason.is_none() {
                let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
                let command = if phase == 2 { 0.01 } else { 0.0 };
                assert_eq!(
                    w.step_material_compression(0.01, command).unwrap(),
                    restored.step_material_compression(0.01, command).unwrap()
                );
                assert_eq!(
                    w.snapshot_bytes().unwrap(),
                    restored.snapshot_bytes().unwrap()
                );
            }
        }
    }
}
