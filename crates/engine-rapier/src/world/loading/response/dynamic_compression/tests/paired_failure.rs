use super::super::super::tests::{body, world};
use super::super::tests::{continuation, pair};
use super::*;

const DT: f32 = 1.0 / 8192.0;
fn fixture(capacities: [f64; 2]) -> PhysicsWorld {
    let mut w = pair(8.0);
    let sources = Scene::read(&w).unwrap().plates.sources;
    for i in 0..2 {
        w.loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .get_mut(&sources[i].region)
            .unwrap()
            .remaining = capacities[i];
    }
    w
}
fn state(w: &mut PhysicsWorld) -> &mut PairedHistory {
    continuation(w).history.paired.as_mut().unwrap()
}
fn collapsed(capacities: [f64; 2]) -> PhysicsWorld {
    let mut w = fixture(capacities);
    for _ in 0..1000 {
        if !w
            .step_material_collapsing_pair(DT)
            .unwrap()
            .failures
            .is_empty()
        {
            return w;
        }
    }
    panic!("pair did not fail");
}
fn refused(w: &mut PhysicsWorld, dt: f32, reason: R) {
    let before = w.snapshot_bytes().unwrap();
    let capture = w.loading_batches().to_vec();
    let response = w.material_response_batches().to_vec();
    assert_eq!(w.step_material_collapsing_pair(dt), Err(reason));
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert_eq!(w.loading_batches(), capture);
    assert_eq!(w.material_response_batches(), response);
}

#[test]
fn paired_failure_records_use_source_order_and_charge_both_live_regions() {
    for capacities in [[20.0, 1200.0], [1200.0, 20.0], [20.0, 20.0]] {
        let mut w = fixture(capacities);
        let mut reached = false;
        for _ in 0..1000 {
            let step = w.step_material_collapsing_pair(DT).unwrap();
            if step.failures.is_empty() {
                continue;
            }
            reached = true;
            let event = w.material_paired_compression_failure().unwrap();
            let mut expected: Vec<_> = event
                .layers
                .iter()
                .filter(|l| l.failed)
                .map(|l| l.source)
                .collect();
            expected.sort();
            // This fixture's incoming entity ID is larger than the support's.
            assert_eq!(
                step.failures.iter().map(|e| e.source).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(step.events.len(), 1);
            assert_eq!(step.events[0].event.transition, EpisodeTransition::End);
            assert!(w.loading.as_ref().unwrap().episodes.is_empty());
            assert_eq!(
                step.intervals.last().unwrap().failure_dissipated,
                event.dissipated()
            );
            for (i, layer) in w
                .material_paired_compression_summary()
                .unwrap()
                .iter()
                .enumerate()
            {
                assert_eq!(
                    w.loading_material_remaining(event.layers[i].source.region),
                    Some(layer.remaining)
                );
            }
            break;
        }
        assert!(reached);
    }
}

#[test]
fn paired_collapse_saves_validate_both_dispositions_and_exact_common_motion_after_removal() {
    let original = collapsed([20.0, 1200.0]);
    let corruptions: &[fn(&mut PairedCompressionFailure)] = &[
        |e| e.elapsed = 0.0,
        |e| e.elapsed *= 0.5,
        |e| e.episode = EpisodeId(0),
        |e| e.layers[0].failed = false,
        |e| e.layers[1].failed = true,
        |e| e.layers[0].excess_work += 1.0,
        |e| e.layers[1].released_elastic += 1.0,
        |e| e.layers[1].excess_work += 1.0,
        |e| e.layers[0].plastic_indentation += 0.01,
        |e| e.layers[1].indentation += 0.01,
        |e| e.layers[0].source.patch += 1,
        // Exact common translations and velocities preserve relative motion.
        |e| e.positions.iter_mut().for_each(|x| *x += 1.0 / 128.0),
        |e| e.velocities.iter_mut().for_each(|v| *v += 1.0),
    ];
    for coast in [false, true] {
        let mut sample = original.clone();
        if coast {
            sample.step_material_collapsing_pair(16.0 * DT).unwrap();
        }
        for removed in [false, true] {
            let mut w = sample.clone();
            if removed {
                w.remove_entity(body(11).entity);
                w.remove_entity(body(2).entity);
            }
            let bytes = w.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..4], &23_u32.to_le_bytes());
            let restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
            assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
            assert_eq!(
                restored.material_paired_compression_failure(),
                w.material_paired_compression_failure()
            );
            for version in 0_u32..23 {
                let mut bad = bytes.clone();
                bad[..4].copy_from_slice(&version.to_le_bytes());
                assert!(PhysicsWorld::from_snapshot_bytes(&bad).is_err());
            }
            for cut in [1, 8, 48] {
                assert!(PhysicsWorld::from_snapshot_bytes(&bytes[..bytes.len() - cut]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
            for corrupt in corruptions {
                let mut bad = w.clone();
                corrupt(
                    state(&mut bad)
                        .failure
                        .as_mut()
                        .unwrap()
                        .event
                        .as_mut()
                        .unwrap(),
                );
                assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
            }
            let mut bad = w.clone();
            state(&mut bad).layers[1].remaining += 1.0;
            assert!(PhysicsWorld::from_snapshot_bytes(&bad.snapshot_bytes().unwrap()).is_err());
            state(&mut w).failure.as_mut().unwrap().event = None;
            assert!(PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err());
        }
    }
}

#[test]
fn paired_collapse_restores_pending_support_and_refuses_policy_switches() {
    let mut w = fixture([20.0, 1200.0]);
    let first = w.step_material_collapsing_pair(DT).unwrap();
    assert_eq!(first.events[0].event.transition, EpisodeTransition::Impact);
    let bytes = w.snapshot_bytes().unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let step = w.step_material_collapsing_pair(DT).unwrap();
    assert_eq!(step, replay.step_material_collapsing_pair(DT).unwrap());
    assert_eq!(step.events[0].event.transition, EpisodeTransition::Support);
    assert_eq!(step.events[0].event.id, first.events[0].event.id);
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(
        w.step_material_dynamic_compression(DT),
        Err(R::StaleContinuation)
    );
    assert_eq!(
        w.step_material_collapsing_support(DT),
        Err(R::StaleContinuation)
    );
    assert_eq!(
        w.set_material_dynamic_compression_acceleration(1.0),
        Err(R::InvalidCommand)
    );
    w.set_material_dynamic_compression_acceleration(0.0)
        .unwrap();
    w.step_material_collapsing_pair(0.0).unwrap();
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    let mut legacy = fixture([20.0, 1200.0]);
    legacy.step_material_dynamic_compression(DT).unwrap();
    refused(&mut legacy, DT, R::StaleContinuation);
    refused(&mut world(), DT, R::UnsupportedScene);
    w.set_gravity(Vec2::X);
    w.set_gravity(Vec2::ZERO);
    refused(&mut w, DT, R::StaleContinuation);
}

#[test]
fn paired_failure_output_and_later_coasting_limits_are_atomic() {
    for (capacities, records, failures) in [([20.0, 1200.0], 5, 1), ([20.0, 20.0], 6, 2)] {
        let mut w = fixture(capacities);
        w.loading.as_mut().unwrap().config.max_records_per_step = records;
        w.step_material_collapsing_pair(DT).unwrap();
        w.step_material_collapsing_pair(DT).unwrap();
        let mut hit = false;
        for _ in 0..1000 {
            let before = w.snapshot_bytes().unwrap();
            if let Err(reason) = w.step_material_collapsing_pair(2.0 * DT) {
                assert_eq!(reason, R::Limit);
                assert_eq!(w.snapshot_bytes().unwrap(), before);
                assert!(w.material_paired_compression_failure().is_none());
                let a = w.step_material_collapsing_pair(DT).unwrap();
                let b = w.step_material_collapsing_pair(DT).unwrap();
                assert_eq!(a.failures.len() + b.failures.len(), failures);
                hit = true;
                break;
            }
        }
        assert!(hit);
    }
    let mut w = collapsed([20.0, 1200.0]);
    let failure = w.material_paired_compression_failure();
    for tick in 0..16 {
        let bytes = w.snapshot_bytes().unwrap();
        if let Err(reason) = w.step_material_collapsing_pair(1.0 / 30.0) {
            assert!(matches!(reason, R::Limit | R::Numerical));
            assert_eq!(w.snapshot_bytes().unwrap(), bytes);
            assert_eq!(w.material_paired_compression_failure(), failure);
            return;
        }
        assert!(tick < 15);
    }
}
