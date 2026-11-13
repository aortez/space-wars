use super::super::tests::{body, world};
use super::*;

const DT: f32 = 1.0 / 8192.0;
fn pair() -> PhysicsWorld {
    let mut w = world();
    w.set_gravity(Vec2::ZERO);
    w.set_body_kind(body(2), BodyKind::Dynamic, true);
    w.refresh_mass_properties(body(2));
    w.set_pose(body(11), -Vec2::X * 0.5, 0.0, true);
    w.set_pose(body(2), Vec2::X * 0.5, 0.0, true);
    w.set_velocity(body(11), Vec2::X * 7.0, 0.0, true);
    w.set_velocity(body(2), -Vec2::X, 0.0, true);
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
    material.volume = 0.5;
    material.remaining = 30.0;
    w
}
fn history(w: &mut PhysicsWorld) -> &mut History {
    &mut w
        .loading
        .as_mut()
        .unwrap()
        .dynamic_compression
        .as_mut()
        .unwrap()
        .history
}
fn collapsed() -> PhysicsWorld {
    let mut w = pair();
    for _ in 0..1000 {
        if !w
            .step_material_collapsing_support(DT)
            .unwrap()
            .failures
            .is_empty()
        {
            return w;
        }
    }
    panic!("moving support did not collapse");
}
fn refused(w: &mut PhysicsWorld, dt: f32, reason: R) {
    let bytes = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_collapsing_support(dt), Err(reason));
    assert_eq!(w.snapshot_bytes().unwrap(), bytes);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn moving_collapse_replays_pending_support_then_retains_both_boundary_trajectories() {
    let mut w = pair();
    let first = w.step_material_collapsing_support(DT).unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].event.transition, EpisodeTransition::Impact);
    let bytes = w.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..4], &22_u32.to_le_bytes());
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let second = w.step_material_collapsing_support(DT).unwrap();
    assert_eq!(second, replay.step_material_collapsing_support(DT).unwrap());
    assert_eq!(second.events.len(), 1);
    assert_eq!(
        second.events[0].event.transition,
        EpisodeTransition::Support
    );
    assert_eq!(second.events[0].event.id, first.events[0].event.id);
    let mut failures = 0;
    for _ in 0..1000 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
        assert_eq!(
            w.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        failures += step.failures.len();
        if failures != 0 {
            assert_eq!(step.events.len(), 1);
            assert_eq!(step.events[0].event.transition, EpisodeTransition::End);
            assert_eq!(step.events[0].event.id, first.events[0].event.id);
            break;
        }
    }
    assert_eq!(failures, 1);
    let boundary = w.material_moving_support_failure().unwrap();
    assert_eq!(
        Some(boundary.failure.clone()),
        w.material_compression_failure()
    );
    let at_failure = w.material_moving_support_summary().unwrap();
    assert_eq!(boundary.positions, at_failure.positions);
    assert_eq!(boundary.velocities, at_failure.velocities);
    replay = PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).unwrap();
    for _ in 0..64 {
        let step = w.step_material_collapsing_support(DT).unwrap();
        assert_eq!(step, replay.step_material_collapsing_support(DT).unwrap());
        assert!(step.events.is_empty() && step.failures.is_empty());
        for i in step.intervals {
            assert_eq!(i.load.impulse, 0.0);
            assert_eq!(i.load.driver_work, 0.0);
            assert_eq!(i.load.damage_work, 0.0);
            assert_eq!(i.failure_dissipated, 0.0);
        }
        let s = step.summary.unwrap();
        let m = w.material_moving_support_summary().unwrap();
        assert_eq!(s.damage_work, 30.0);
        assert_eq!(s.elastic_energy, 0.0);
        assert_eq!(m.velocities, boundary.velocities);
        for i in 0..2 {
            let expected = boundary.positions[i]
                + f64::from(boundary.velocities[i]) * (s.elapsed - boundary.failure.elapsed);
            assert!((m.positions[i] - expected).abs() <= m.absolute_errors[3 + i] + 1e-10);
        }
    }
    assert_eq!(Some(boundary), w.material_moving_support_failure());
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
}

#[test]
fn moving_collapse_saves_require_the_boundary_motion_even_after_removal() {
    let original = collapsed();
    let corruptions: &[fn(&mut FailureHistory)] = &[
        |f| f.motion = None,
        |f| f.event = None,
        |f| f.motion.as_mut().unwrap().positions[0] += 0.01,
        |f| {
            // Exact in f32, so only the per-body trajectory check can refuse it.
            f.motion
                .as_mut()
                .unwrap()
                .positions
                .iter_mut()
                .for_each(|x| *x += 1.0 / 128.0)
        },
        |f| {
            f.motion
                .as_mut()
                .unwrap()
                .velocities
                .iter_mut()
                .for_each(|v| *v += 1.0)
        },
        |f| f.motion.as_mut().unwrap().velocities[1] = f32::NAN,
        |f| f.event.as_mut().unwrap().elapsed *= 0.5,
        |f| f.event.as_mut().unwrap().released_elastic += 1.0,
        |f| f.event.as_mut().unwrap().source.patch += 1,
    ];
    for coast in [false, true] {
        let mut sample = original.clone();
        if coast {
            sample.step_material_collapsing_support(16.0 * DT).unwrap();
        }
        for removed in [false, true] {
            let mut w = sample.clone();
            if removed {
                w.remove_entity(body(11).entity);
                w.remove_entity(body(2).entity);
            }
            let bytes = w.snapshot_bytes().unwrap();
            let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            assert_eq!(
                restored.material_moving_support_failure(),
                w.material_moving_support_failure()
            );
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            for version in 0_u32..22 {
                let mut bad = bytes.clone();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
            }
            for cut in [1, 8, 25] {
                assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
            for corrupt in corruptions {
                let mut bad = w.clone();
                corrupt(history(&mut bad).failure.as_mut().unwrap());
                assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
            }
        }
    }
    let mut pending = pair();
    pending.step_material_collapsing_support(DT).unwrap();
    let motion = history(&mut pending)
        .moving
        .as_ref()
        .unwrap()
        .current
        .clone();
    history(&mut pending).failure.as_mut().unwrap().motion = Some(motion);
    assert!(PhysicsWorld::from_snapshot_bytes(&pending.snapshot_bytes().unwrap()).is_err());
}

#[test]
fn moving_collapse_limits_and_policy_changes_roll_back_both_bodies() {
    let mut w = pair();
    w.loading.as_mut().unwrap().config.max_records_per_step = 3;
    w.step_material_collapsing_support(DT).unwrap();
    w.step_material_collapsing_support(DT).unwrap();
    let mut reached_failure = false;
    for _ in 0..1000 {
        let before = w.snapshot_bytes().unwrap();
        if let Err(reason) = w.step_material_collapsing_support(2.0 * DT) {
            assert_eq!(reason, R::Limit);
            assert_eq!(w.snapshot_bytes().unwrap(), before);
            assert!(w.material_moving_support_failure().is_none());
            let a = w.step_material_collapsing_support(DT).unwrap();
            let b = w.step_material_collapsing_support(DT).unwrap();
            assert_eq!(a.failures.len() + b.failures.len(), 1);
            reached_failure = true;
            break;
        }
    }
    assert!(reached_failure);
    let mut w = collapsed();
    w.step_material_collapsing_support(64.0 * DT).unwrap();
    refused(&mut w, 1.0 / 30.0, R::Limit);
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.step_material_dynamic_compression(DT),
        Err(R::StaleContinuation)
    );
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(1.0),
        Err(R::InvalidCommand)
    );
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    w.step_material_collapsing_support(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    w.set_velocity(body(2), Vec2::X, 0.0, true);
    refused(&mut w, DT, R::StaleContinuation);
    let mut legacy = pair();
    legacy.step_material_dynamic_compression(DT).unwrap();
    refused(&mut legacy, DT, R::StaleContinuation);
    let mut gravity = pair();
    gravity.set_gravity(Vec2::X);
    refused(&mut gravity, DT, R::UnsupportedScene);
}
