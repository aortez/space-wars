//! Dynamic physical-history witnesses, malformed saves and atomic limits.
use super::*;
use engine_material::WOOD;

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, 0)
}
fn world() -> PhysicsWorld {
    let mut w = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::X * 400.0,
        ..Default::default()
    });
    w.enable_loading_capture(CaptureConfig::default()).unwrap();
    w.enable_material_response().unwrap();
    // Deliberately reverse geometric and canonical source order.
    for (role, n) in [11, 2].into_iter().enumerate() {
        let mut shape = ColliderSpec::cuboid(collider(n), 0.5, 1.0);
        shape.friction = 0.0;
        shape.mass_properties = Some(ColliderMassProperties {
            center: Vec2::ZERO,
            mass: 20.0,
            inertia: 10.0,
        });
        w.insert_body(
            body(n),
            BodySpec {
                kind: if role == 0 {
                    BodyKind::Dynamic
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
        w.refresh_mass_properties(body(n));
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
    w.step_material_dynamic_compression(1.0 / 60.0).unwrap();
    w
}
fn state(w: &mut PhysicsWorld) -> &mut Continuation {
    w.loading
        .as_mut()
        .unwrap()
        .dynamic_compression
        .as_mut()
        .unwrap()
}
fn rollback(w: &mut PhysicsWorld, dt: f32, expected: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_dynamic_compression(dt), Err(expected));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}
#[test]
fn dynamic_history_rejects_corrupt_totals_even_after_invalidation() {
    let changes: &[fn(&mut Continuation)] = &[
        |c| c.revision = 2,
        |c| c.history.mass = 0.0,
        |c| c.history.acceleration = 0.0,
        |c| c.history.summary.indentation = 0.3,
        |c| c.history.summary.velocity = -1.0,
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
        |c| c.history.summary.elapsed = f64::NAN,
        |c| c.history.summary.solves = 0,
    ];
    let original = enrolled();
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
}
#[test]
fn dynamic_history_checks_live_mass_gravity_material_geometry_and_owner() {
    let changes: &[fn(&mut PhysicsWorld)] = &[
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.owner = EpisodeId(999);
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.plates.sources.swap(0, 1);
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.mass += 1.0;
            }
        },
        |w| {
            if let Status::Active(a) = &mut state(w).status {
                a.scene.acceleration += 1.0;
            }
        },
        |w| w.raw.gravity.x += 1.0,
        |w| {
            let h = w.body_handle(body(11)).unwrap();
            w.raw.bodies[h].set_linvel(Vector::new(3.0, 0.0), true);
        },
        |w| {
            let h = w.body_handle(body(11)).unwrap();
            w.raw.bodies[h].set_translation(Vector::new(0.05, 0.0), true);
        },
        |w| {
            let h = w.collider_handle(collider(11)).unwrap();
            w.raw.colliders[h].set_mass(30.0);
        },
        |w| {
            let h = w.collider_handle(collider(2)).unwrap();
            w.raw.colliders[h].set_friction(0.1);
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
    ];
    let original = enrolled();
    for change in changes {
        let mut w = original.clone();
        change(&mut w);
        assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
        rollback(&mut w, 0.01, R::StaleContinuation);
    }
}
#[test]
fn dynamic_versions_and_modes_cannot_reset_or_replace_history() {
    for dt in [0.001, 0.02] {
        for terminal in [false, true] {
            let mut w = world();
            w.step_material_dynamic_compression(dt).unwrap();
            if terminal {
                w.step(0.01);
            }
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &15_u32.to_le_bytes());
            assert_eq!(
                PhysicsWorld::from_snapshot_bytes(&bytes)
                    .unwrap()
                    .snapshot_bytes()
                    .unwrap(),
                bytes
            );
            for version in 0_u32..=16 {
                if version == 15 {
                    continue;
                }
                let mut bad = bytes.clone();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                assert!(
                    PhysicsWorld::from_snapshot_bytes(&bad).is_err(),
                    "version {version}"
                );
            }
            for cut in [1, 4, 8, 64] {
                assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
            assert_eq!(
                w.step_material_compression(0.01, 0.05),
                Err(R::UnsupportedScene)
            );
            assert_eq!(
                w.step_material_flat_gravity(0.01, gravity::GravitySettling::Refuse)
                    .unwrap_err(),
                gravity::GravityRejection::StaleContinuation
            );
            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
            assert_eq!(
                w.enable_loading_capture(CaptureConfig::default()),
                Err(LoadingError::AlreadyEnabled)
            );
            assert_eq!(
                w.enable_material_response(),
                Err(LoadingError::AlreadyEnabled)
            );
        }
    }
    let mut old = world().snapshot_bytes().unwrap();
    old[..4].copy_from_slice(&15_u32.to_le_bytes());
    assert!(PhysicsWorld::from_snapshot_bytes(&old).is_err());
}
#[test]
fn dynamic_limits_and_unsupported_enrollment_roll_back() {
    type Change = fn(&mut PhysicsWorld);
    let changes: &[(Change, R)] = &[
        (|w| w.raw.gravity = Vector::ZERO, R::UnsupportedScene),
        (
            |w| w.raw.gravity = Vector::new(200.0, 0.0),
            R::UnsupportedScene,
        ),
        (
            |w| w.raw.gravity = Vector::new(1000.0, 0.0),
            R::UnsupportedScene,
        ),
        (
            |w| {
                w.set_velocity(body(11), Vec2::X, 0.0, true);
            },
            R::UnsupportedScene,
        ),
        (
            |w| {
                w.set_pose(body(11), Vec2::new(-0.01, 0.0), 0.0, true);
            },
            R::UnsupportedScene,
        ),
        (
            |w| {
                let h = w.body_handle(body(11)).unwrap();
                w.raw.bodies[h].set_body_type(RigidBodyType::KinematicVelocityBased, true);
            },
            R::UnsupportedScene,
        ),
        (
            |w| {
                let h = w.collider_handle(collider(2)).unwrap();
                w.raw.colliders[h].set_sensor(true);
            },
            R::UnsupportedScene,
        ),
        (
            |w| w.loading.as_mut().unwrap().config.max_records_per_step = 1,
            R::Limit,
        ),
        (
            |w| w.loading.as_mut().unwrap().next_episode = u64::MAX,
            R::Limit,
        ),
        (
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
                    .parameters[1] = 2e9
            },
            R::Limit,
        ),
        (
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
                    .remaining = 0.0
            },
            R::FailedRegion,
        ),
        // A huge capacity cannot retain the first small plastic charge.
        (
            |w| {
                let r = w
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
                r.volume = 1e12;
                r.remaining = 6e13;
            },
            R::Numerical,
        ),
    ];
    for (change, reason) in changes {
        let mut w = world();
        change(&mut w);
        rollback(&mut w, 1.0 / 30.0, *reason);
    }
}
