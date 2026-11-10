use super::super::tests::{body, world};
use super::*;

const DT: f32 = 1.0 / 8192.0;
fn fixture() -> PhysicsWorld {
    let mut w = world();
    let m = w
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
    m.volume = 2.0;
    m.remaining = 120.0;
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
fn collapsed() -> PhysicsWorld {
    let mut w = fixture();
    for _ in 0..1000 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        if !step.failures.is_empty() {
            return w;
        }
    }
    panic!("support did not collapse");
}
fn refused(w: &mut PhysicsWorld, dt: f32, reason: R) {
    let before = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_collapsing_support(dt), Err(reason));
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn collapse_commits_once_and_flight_preserves_damage_energy_and_replay() {
    let mut w = collapsed();
    let event = w.material_compression_failure().unwrap();
    let start = w.material_dynamic_compression_summary().unwrap();
    assert_eq!(event.elapsed, start.elapsed);
    assert_eq!(start.damage_work, 120.0);
    assert_eq!(start.elastic_energy, 0.0);
    assert!(event.released_elastic > 0.0);
    assert!(w.loading.as_ref().unwrap().episodes.is_empty());
    let bytes = w.snapshot_bytes().unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    for _ in 0..64 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert!(step.events.is_empty() && step.failures.is_empty());
        assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
        assert_eq!(
            w.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        let interval = &step.intervals[0];
        assert_eq!(interval.load.force, 0.0);
        assert_eq!(interval.load.impulse, 0.0);
        assert_eq!(interval.load.damage_work, 0.0);
        let s = step.summary.unwrap();
        assert_eq!(s.damage_work, start.damage_work);
        assert_eq!(s.plastic_indentation, start.plastic_indentation);
        assert_eq!(s.viscous_loss, start.viscous_loss);
        let t = s.elapsed - start.elapsed;
        assert!((s.velocity - start.velocity - 400.0 * t).abs() < 1e-5);
        assert!(
            (s.indentation - start.indentation - start.velocity * t - 200.0 * t * t).abs() < 1e-6
        );
        assert!(
            (s.gravity_work
                - s.kinetic_energy
                - s.damage_work
                - s.charge_error
                - s.viscous_loss
                - event.dissipated()
                - s.errors[0])
                .abs()
                < 1e-8
        );
    }
    assert_eq!(w.material_compression_failure(), Some(event));
}

#[test]
fn collapse_policy_restores_incoming_motion_and_pending_support_before_failure() {
    let mut w = fixture();
    w.set_pose(body(11), -Vec2::X / 64.0, 0.0, true);
    w.step_material_collapsing_support(DT).unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let mut impact = None;
    for _ in 0..1000 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
        assert_eq!(
            w.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        if let Some(event) = step
            .events
            .iter()
            .find(|e| e.event.transition == EpisodeTransition::Impact)
        {
            impact = Some(event.event.id);
            break;
        }
    }
    let id = impact.expect("platen reaches the support");
    assert_eq!(w.material_compression_failure(), None);
    assert!(
        matches!(&continuation(&mut w).status, Status::CollapsingSupport(a) if a.pending_support)
    );
    replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    let step = w.step_material_collapsing_support(DT).unwrap();
    assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(step.events.len(), 1);
    assert_eq!(step.events[0].event.id, id);
    assert_eq!(step.events[0].event.transition, EpisodeTransition::Support);
}

#[test]
fn collapse_extension_cannot_be_downgraded_or_revived_and_validates_after_removal() {
    let original = collapsed();
    for removed in [false, true] {
        let mut w = original.clone();
        if removed {
            w.remove_entity(body(11).entity);
            w.remove_entity(body(2).entity);
        }
        let bytes = w.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..4], &21_u32.to_le_bytes());
        let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(
            restored.material_compression_failure(),
            w.material_compression_failure()
        );
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        for version in 0_u32..21 {
            let mut bad = bytes.clone();
            bad[..4].copy_from_slice(&version.to_le_bytes());
            assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
        }
        for missing in [1, 8, 48] {
            assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - missing]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
        let changes: &[fn(&mut CompressionFailure)] = &[
            |e| e.elapsed = 0.0,
            |e| e.elapsed = f64::NAN,
            |e| e.elapsed *= 0.5,
            |e| e.source.patch += 1,
            |e| e.episode = EpisodeId(0),
            |e| e.indentation += 0.01,
            |e| e.plastic_indentation += 0.01,
            |e| e.velocity += 1.0,
            |e| e.released_elastic += 1.0,
            |e| e.excess_work += 1.0,
        ];
        for change in changes {
            let mut bad = w.clone();
            change(
                continuation(&mut bad)
                    .history
                    .failure
                    .as_mut()
                    .unwrap()
                    .event
                    .as_mut()
                    .unwrap(),
            );
            assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
        }
    }
    let mut w = original;
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.step_material_dynamic_compression(DT),
        Err(R::StaleContinuation)
    );
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(0.0),
        Err(R::InvalidCommand)
    );
    w.set_material_dynamic_compression_acceleration(400.0)
        .unwrap();
    w.step_material_collapsing_support(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    w.set_gravity(Vec2::ZERO);
    w.set_gravity(Vec2::X * 400.0);
    refused(&mut w, DT, R::StaleContinuation);
    let mut legacy = fixture();
    legacy.step_material_dynamic_compression(DT).unwrap();
    refused(&mut legacy, DT, R::StaleContinuation);
}

#[test]
fn collapse_and_end_records_are_atomic_when_output_fills() {
    let mut w = fixture();
    w.loading.as_mut().unwrap().config.max_records_per_step = 3;
    for tick in 0..1000 {
        let before = w.snapshot_bytes().unwrap();
        match w.step_material_collapsing_support(2.0 * DT) {
            Ok(_) => assert!(tick < 999),
            Err(reason) => {
                assert_eq!(reason, R::Limit);
                assert_eq!(w.snapshot_bytes().unwrap(), before);
                assert_eq!(w.material_compression_failure(), None);
                // Same internal subdivision, but enough output space per call.
                let a = w.step_material_collapsing_support(DT).unwrap();
                let b = w.step_material_collapsing_support(DT).unwrap();
                assert_eq!(a.failures.len() + b.failures.len(), 1);
                assert!(w.material_compression_failure().is_some());
                return;
            }
        }
    }
}

#[test]
fn collapse_policy_keeps_safe_loads_safe_and_refuses_paired_materials() {
    let mut w = fixture();
    w.set_gravity(Vec2::X * 156.25);
    w.set_pose(body(11), Vec2::X / 128.0, 0.0, true);
    for _ in 0..512 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert!(step.failures.is_empty());
        let s = step.summary.unwrap();
        assert_eq!(s.damage_work, 0.0);
        assert_eq!(s.plastic_indentation, 0.0);
        assert_eq!(s.velocity, 0.0);
    }
    assert_eq!(w.material_compression_failure(), None);
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&bytes)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        bytes
    );

    let mut moving = fixture();
    moving.set_gravity(Vec2::ZERO);
    moving.set_body_kind(body(2), BodyKind::Dynamic, true);
    moving.refresh_mass_properties(body(2));
    moving.set_velocity(body(11), Vec2::X, 0.0, true);
    let loading = moving.loading.as_mut().unwrap();
    let driver = loading
        .bindings
        .values()
        .find(|s| s.region.id.entity == body(11).entity)
        .unwrap()
        .region;
    let materials = &mut loading.response.as_mut().unwrap().materials;
    materials.insert(driver, materials.values().next().unwrap().clone());
    refused(&mut moving, DT, R::UnsupportedScene);
}

#[test]
fn moving_collapse_preserves_frozen_fixed_support_v21_histories() {
    let mut approaching = fixture();
    approaching.set_pose(body(11), -Vec2::X / 64.0, 0.0, true);
    approaching.step_material_collapsing_support(DT).unwrap();
    for original in [approaching, collapsed()] {
        for removed in [false, true] {
            let mut w = original.clone();
            if removed {
                w.remove_entity(body(11).entity);
                w.remove_entity(body(2).entity);
            }
            let c = continuation(&mut w);
            let mut frozen = bincode::serialize(&7_u32).unwrap();
            frozen.extend(model::frozen_v15_history(&c.history));
            match &c.status {
                Status::CollapsingSupport(a) => {
                    bincode::serialize_into(&mut frozen, &7_u32).unwrap();
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
                Status::Invalidated(_) => {
                    bincode::serialize_into(&mut frozen, &(1_u32, 0_u32)).unwrap();
                }
                _ => panic!("v21 lifecycle changed"),
            }
            assert_eq!(bincode::serialize(&c).unwrap(), frozen);
            let f = c.history.failure.as_ref().unwrap();
            let incoming = c.history.incoming.as_ref().map(|v| v.velocity);
            let event = f.event.as_ref().map(|e| {
                (
                    e.elapsed,
                    e.source,
                    e.episode,
                    e.indentation,
                    e.velocity,
                    e.plastic_indentation,
                    e.released_elastic,
                    e.excess_work,
                )
            });
            bincode::serialize_into(&mut frozen, &(incoming, f.source, event)).unwrap();
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &21_u32.to_le_bytes());
            assert!(bytes.ends_with(&frozen));
            let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            assert_eq!(
                restored.step_material_collapsing_support(DT),
                w.step_material_collapsing_support(DT)
            );
            assert_eq!(
                restored.snapshot_bytes().unwrap(),
                w.snapshot_bytes().unwrap()
            );
        }
    }
}
