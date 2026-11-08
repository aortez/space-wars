//! Persistence, ownership and refusal boundaries of the native departure path.
use super::*;
use crate::world::loading::response::flat_arrival::{FlatArrivalRejection as R, FlatEventPhase};

type Proofs = BTreeMap<[ContactSource; 2], (EpisodeId, [Vec2; 2], [f64; 2])>;
type BallProofs = BTreeMap<[ContactSource; 2], (EpisodeId, [Vec2; 2])>;

fn wire(world: &PhysicsWorld) -> (Vec<u8>, usize, Proofs) {
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(bytes[0], 5);
    let mut reader = std::io::Cursor::new(bytes.as_slice());
    let _: crate::world::PhysicsSnapshot = bincode::deserialize_from(&mut reader).unwrap();
    let mut loading: LoadingState = bincode::deserialize_from(&mut reader).unwrap();
    loading.deserialize_response(&mut reader).unwrap();
    let _: Option<BallProofs> = bincode::deserialize_from(&mut reader).unwrap();
    let offset = reader.position() as usize;
    let proofs = bincode::deserialize_from(&mut reader).unwrap();
    assert_eq!(reader.position() as usize, bytes.len());
    (bytes, offset, proofs)
}

fn pending() -> PhysicsWorld {
    let (mut fixture, _) = rapid(
        &[1, 1, 1],
        1,
        false,
        0.37,
        Vec2::new(17.0, -11.0),
        0.5,
        true,
    );
    let mut world = fixture.world.clone();
    let first = tests::first_arrival(&mut fixture);
    world.step_material_flat_arrivals(first).unwrap();
    world
        .step_material_flat_arrivals(2835.0_f32 * 1.0e-9)
        .unwrap();
    assert_eq!(wire(&world).2.len(), 1);
    world
}

fn unchanged(
    world: &PhysicsWorld,
    bytes: &[u8],
    loading: &[ContactBatch],
    response: &[ResponseBatch],
) {
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), response);
}

#[test]
fn native_flat_departure_snapshot_validates_witnesses_and_preserves_old_versions() {
    let (initial, _) = rapid(&[1, 1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
    let legacy = initial.world.snapshot_bytes().unwrap();
    assert_eq!(legacy[0], 3);
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&legacy)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        legacy
    );
    let world = pending();
    let (before, offset, proofs) = wire(&world);
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&before)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        before
    );
    let (&pair, witness) = proofs.iter().next().unwrap();
    for case in 0..12 {
        let mut invalid = proofs.clone();
        match case {
            0 => invalid.get_mut(&pair).unwrap().0 = EpisodeId(0),
            1 => invalid.get_mut(&pair).unwrap().1[1].y = f32::NAN,
            2 => invalid.get_mut(&pair).unwrap().2[0] = f64::INFINITY,
            3 => invalid.get_mut(&pair).unwrap().2 = [0.0, 0.0],
            4 => invalid.get_mut(&pair).unwrap().2 = [1.0, 1.0],
            5 => {
                invalid.remove(&pair);
                invalid.insert([pair[1], pair[0]], *witness);
            }
            6 => {
                invalid.remove(&pair);
                let mut stale = pair;
                stale[0].region.incarnation += 100;
                invalid.insert(stale, *witness);
            }
            7 => {
                invalid.remove(&pair);
                let mut unbound = pair;
                unbound[0].patch = 1234;
                invalid.insert(unbound, *witness);
            }
            8 => {
                for n in 1..4 {
                    let mut extra = pair;
                    extra[0].patch = n;
                    invalid.insert(extra, *witness);
                }
            }
            9 => invalid.get_mut(&pair).unwrap().0 = EpisodeId(witness.0.0 + 10),
            10 => {
                invalid.remove(&pair);
                invalid.insert([pair[0], pair[0]], *witness);
            }
            _ => invalid.get_mut(&pair).unwrap().1[0].x = f32::NEG_INFINITY,
        }
        let mut bytes = before[..offset].to_vec();
        bincode::serialize_into(&mut bytes, &invalid).unwrap();
        assert!(
            PhysicsWorld::from_snapshot_bytes(&bytes).is_err(),
            "case {case}"
        );
    }
    assert!(PhysicsWorld::from_snapshot_bytes(&before[..before.len() - 1]).is_err());
    let mut trailing = before.clone();
    trailing.push(0);
    assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());

    // Both proof kinds cannot own a pending interval at the same time.
    let mut mixed = world.clone();
    let ball: BallProofs = proofs.iter().map(|(pair, w)| (*pair, (w.0, w.1))).collect();
    mixed.loading.as_mut().unwrap().arrival_departures =
        Some(bincode::deserialize(&bincode::serialize(&ball).unwrap()).unwrap());
    assert!(PhysicsWorld::from_snapshot_bytes(&mixed.snapshot_bytes().unwrap()).is_err());

    let mut restored = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
    let mut original = world;
    let a = original.step_material_flat_arrivals(0.006).unwrap();
    let b = restored.step_material_flat_arrivals(0.006).unwrap();
    assert_eq!(a.events, b.events);
    assert_eq!(
        original.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    assert_eq!(
        original.material_response_batches(),
        restored.material_response_batches()
    );
    assert_eq!(original.material_response_batches().len(), 3);
}

#[test]
fn native_flat_departure_mode_changes_and_lifecycle_clear_only_committed_evidence() {
    let world = pending();
    for action in 0..3 {
        let mut changed = world.clone();
        match action {
            0 => {
                changed.step(1.0e-9);
            }
            1 => {
                let token = changed.loading.as_ref().unwrap().bindings[&collider(1, 0)].region;
                assert!(changed.retire_loading_region(token));
            }
            _ => changed.loading.as_mut().unwrap().unbind(collider(1, 0)),
        }
        assert!(wire(&changed).2.is_empty());
        assert!(PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_ok());
    }
    let mut changed = world;
    let before = changed.snapshot_bytes().unwrap();
    let loading = changed.loading_batches().to_vec();
    let response = changed.material_response_batches().to_vec();
    assert!(changed.step_material_three_ball_arrivals(1.0e-9).is_err());
    unchanged(&changed, &before, &loading, &response);
    for (_, c) in changed.raw.colliders.iter_mut() {
        c.set_shape(SharedShape::ball(0.5));
    }
    let before = changed.snapshot_bytes().unwrap();
    assert_eq!(
        changed.step_balls_with_limit(1.0e-9, 0, false).unwrap_err(),
        crate::world::loading::response::ball_arrival::BallArrivalRejection::AdvanceLimit
    );
    unchanged(&changed, &before, &loading, &response);
    changed.step_material_three_ball_arrivals(0.0).unwrap();
    unchanged(&changed, &before, &[], &[]);
    let mut replay = changed.clone();
    let a = changed.step_material_three_ball_arrivals(1.0e-9).unwrap();
    let b = replay.step_material_three_ball_arrivals(1.0e-9).unwrap();
    assert_eq!(a.events, b.events);
    assert!(wire(&changed).2.is_empty());
    assert_eq!(
        changed.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
}

#[test]
fn native_flat_departure_continuity_and_late_refusals_preserve_ownership() {
    for edit in 0..4 {
        let mut world = pending();
        let proofs = wire(&world).2;
        let (&pair, witness) = proofs.iter().next().unwrap();
        if edit < 2 {
            let mut p = world.motion(body(1)).unwrap().position;
            if edit == 0 {
                p.x -= 0.1;
            } else {
                p.y -= 0.1;
            }
            world.set_pose(body(1), p, 0.37, true);
        } else if edit == 2 {
            let v = world.motion(body(2)).unwrap().linear_velocity + rotate(Vec2::X * 0.001, 0.37);
            world.set_velocity(body(1), v, 0.0, true);
        } else {
            let shift = Vec2::new(17.0, -11.0);
            for n in 1..=3 {
                let m = world.motion(body(n)).unwrap();
                world.set_pose(body(n), m.position, 0.3701, true);
                world.set_velocity(
                    body(n),
                    shift + rotate(m.linear_velocity - shift, 0.0001),
                    0.0,
                    true,
                );
            }
        }
        // Stale geometry is allowed to roundtrip; it cannot continue the proof.
        let mut replay = world.clone();
        let a = world.step_material_flat_arrivals(0.0001).unwrap();
        let b = replay.step_material_flat_arrivals(0.0001).unwrap();
        assert_eq!(a.events, b.events);
        assert!(
            a.events
                .iter()
                .all(|e| e.phase != FlatEventPhase::AfterDrift)
        );
        assert!(wire(&world).2.is_empty());
        assert_eq!(
            world.loading.as_ref().unwrap().episodes[&pair].id,
            witness.0
        );
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
    }
    for limit in [1, 2] {
        let mut world = pending();
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        assert_eq!(
            world.step_flats_with_limit(0.006, limit).unwrap_err(),
            R::AdvanceLimit
        );
        unchanged(&world, &before, &loading, &response);
        for dt in [-1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                world.step_material_flat_arrivals(dt).unwrap_err(),
                R::InvalidTimestep
            );
            unchanged(&world, &before, &loading, &response);
        }
        let pause = world.step_material_flat_arrivals(0.0).unwrap();
        assert!(pause.events.is_empty() && pause.advances.is_empty());
        unchanged(&world, &before, &[], &[]);
    }
    let (fixture, _) = rapid(
        &[1, 1, 1],
        1,
        false,
        0.0,
        Vec2::new(17.0, -11.0),
        0.0,
        false,
    );
    let mut world = fixture.world;
    world.step_material_flat_arrivals(0.0015).unwrap();
    assert_eq!(world.material_response_batches().len(), 2);
    let before = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    assert_eq!(
        world.step_material_flat_arrivals(0.03).unwrap_err(),
        R::Response(ResponseRejection::Backend)
    );
    unchanged(&world, &before, &loading, &response);
}

#[test]
fn native_flat_departure_orders_simultaneous_ends_and_reflected_returns() {
    for cap in [7, 8] {
        let (fixture, _) = rapid(&[1, 1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
        let mut world = fixture.world;
        world.set_pose(body(3), Vec2::X * 2.01, 0.0, true);
        world.set_velocity(body(3), Vec2::X * -40.0, 0.0, true);
        world.loading.as_mut().unwrap().config.max_records_per_step = cap;
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        let result = world.step_material_flat_arrivals(DT);
        if cap == 7 {
            assert_eq!(
                result.unwrap_err(),
                R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            );
            unchanged(&world, &before, &loading, &response);
        } else {
            let result = result.unwrap();
            let ends: Vec<_> = result
                .events
                .iter()
                .filter(|e| e.phase == FlatEventPhase::AfterDrift)
                .collect();
            assert_eq!(ends.len(), 2);
            assert_eq!(ends[0].time_seconds, ends[1].time_seconds);
            assert!(ends[0].event.sources < ends[1].event.sources);
        }
    }
    for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
        let (mut fixture, _) = rapid(parts, 2, true, 0.37, Vec2::new(17.0, -11.0), 0.5, true);
        let mut world = fixture.world.clone();
        let first = tests::first_arrival(&mut fixture);
        for n in 1..=3 {
            let m = world.motion(body(n)).unwrap();
            world.set_pose(body(n), -m.position, 0.37, true);
            world.set_velocity(body(n), -m.linear_velocity, 0.0, true);
        }
        let initial = flat_tests::kinetic(&world, Vec2::ZERO);
        let mut responses = Vec::new();
        let mut events = Vec::new();
        for (i, dt) in [first, 2835.0_f32 * 1.0e-9, 0.006].into_iter().enumerate() {
            let mut replay = world.clone();
            let a = world.step_material_flat_arrivals(dt).unwrap();
            let b = replay.step_material_flat_arrivals(dt).unwrap();
            assert_eq!(a.events, b.events);
            if i == 1 {
                let proofs = wire(&world).2;
                assert_eq!(proofs.len(), 1);
                for (pair, witness) in proofs {
                    assert_eq!(
                        witness.1,
                        pair.map(|s| {
                            let owner = world.loading.as_ref().unwrap().regions[&s.region.id].body;
                            world.motion(owner).unwrap().position
                        })
                    );
                }
            }
            assert_eq!(
                world.snapshot_bytes().unwrap(),
                replay.snapshot_bytes().unwrap()
            );
            events.extend(a.events);
            responses.extend(
                world
                    .material_response_batches()
                    .iter()
                    .map(|b| b.result.as_ref().unwrap().clone()),
            );
        }
        assert_eq!(responses.len(), 4);
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event.transition == EpisodeTransition::Impact)
                .count(),
            4
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event.transition == EpisodeTransition::End)
                .count(),
            4
        );
        assert!(
            (initial
                - flat_tests::kinetic(&world, Vec2::ZERO)
                - responses.iter().map(loss).sum::<f64>())
            .abs()
                < 1.0e-5 * initial
        );
    }
}
