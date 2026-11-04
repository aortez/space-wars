//! Public native API qualification against the independent three-body fixture.
use super::*;
use crate::world::loading::response::arrival::ArrivalRejection;
use crate::world::loading::response::line_arrival::{
    LineArrivalRejection as R, LineArrivalStep, LineEventPhase,
};

fn check_trace(step: &LineArrivalStep, world: &PhysicsWorld, dt: f32) {
    assert_eq!(step.advances.len(), step.previews.len());
    assert!(step.advances.len() <= MAX_ADVANCES);
    assert_eq!(step.advances.len(), step.durations.len());
    assert_eq!(step.durations.len(), world.loading_batches().len());
    let sum: f64 = step.durations.iter().map(|d| f64::from(*d)).sum();
    assert!((sum - f64::from(dt)).abs() < 2.0 * f64::from(f32::EPSILON * dt));
    for (i, b) in world.loading_batches().iter().enumerate() {
        assert_eq!(b.substep, i);
        assert_eq!(b.duration, step.durations[i]);
    }
    for e in &step.events {
        let end = e.substep + usize::from(e.phase == LineEventPhase::AfterDrift);
        assert_eq!(
            e.time_seconds,
            step.durations[..end]
                .iter()
                .map(|d| f64::from(*d))
                .sum::<f64>()
        );
        if e.phase == LineEventPhase::AfterDrift {
            assert_eq!(e.event.transition, EpisodeTransition::End);
        }
    }
    for pair in step.events.windows(2) {
        assert!(pair[0].time_seconds <= pair[1].time_seconds);
        if pair[0].time_seconds == pair[1].time_seconds {
            assert!(pair[0].substep <= pair[1].substep);
            if pair[0].substep == pair[1].substep && pair[0].phase == pair[1].phase {
                assert!(pair[0].event.sources <= pair[1].event.sources);
            }
        }
    }
    let backend_time: Duration = step
        .advances
        .iter()
        .chain(&step.previews)
        .map(|m| m.wall_time)
        .sum();
    assert!(step.wall_time >= backend_time);
}

#[test]
fn native_line_matches_fixtures_and_replays_all_outputs() {
    for case in [Case::Sequential, Case::Simultaneous, Case::Supported] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for parts in [1, 2, 4] {
                for shift in [0.0, 17.0] {
                    for reverse in [false, true] {
                        let (mut world, tokens) = fixture(case, parts, reverse, shift);
                        let mut reference = world.clone();
                        for _ in 0..4 {
                            let mut replay = world.clone();
                            let actual = world.step_material_line_arrivals(1.0 / hz).unwrap();
                            let restored = replay.step_material_line_arrivals(1.0 / hz).unwrap();
                            check_trace(&actual, &world, 1.0 / hz);
                            assert_eq!(actual.durations, restored.durations);
                            assert_eq!(actual.events, restored.events);
                            assert_eq!(
                                world.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            assert_eq!(world.loading_batches(), replay.loading_batches());
                            assert_eq!(
                                world.material_response_batches(),
                                replay.material_response_batches()
                            );
                            step(&mut reference, 1.0 / hz, MAX_ADVANCES).unwrap();
                            assert_eq!(balances(&world, &tokens), balances(&reference, &tokens));
                            assert_eq!(
                                world.material_response_batches(),
                                reference.material_response_batches()
                            );
                        }
                    }
                }
            }
        }
    }
}

fn rapid(parts: u16, reverse: bool, shift: f32) -> (PhysicsWorld, Vec<RegionToken>) {
    let (mut world, tokens) = fixture(Case::Sequential, parts, reverse, shift);
    for part in 0..parts {
        let h = world.collider_handle(collider(1, part)).unwrap();
        world.raw.colliders[h].set_mass(20.0 / f32::from(parts));
    }
    world.refresh_mass_properties(body(1));
    world.set_pose(body(3), Vec2::X * 2.02, 0.0, true);
    (world, tokens)
}

#[test]
fn native_line_rounded_departures_persist_and_rollback() {
    for nanos in [5200.0_f32, 5228.0, 5229.0, 5230.0, 5260.0] {
        for parts in [1, 2, 4] {
            for reverse in [false, true] {
                let (mut world, _) = rapid(parts, reverse, 17.0);
                assert_eq!(world.snapshot_bytes().unwrap()[0], 3);
                let initial = energy(&world, 17.0);
                let initial_momentum = momentum(&world, 17.0);
                let mut responses = Vec::new();
                let mut events = Vec::new();
                for (i, dt) in [0.00025, nanos * 1e-9, 0.003].into_iter().enumerate() {
                    if i == 2 && nanos == 5229.0 {
                        let before = world.snapshot_bytes().unwrap();
                        let loading = world.loading_batches().to_vec();
                        let outputs = world.material_response_batches().to_vec();
                        assert_eq!(
                            world.step_line_with_limit(dt, 1).unwrap_err(),
                            R::AdvanceLimit
                        );
                        assert_unchanged(&world, &before, &loading, &outputs);
                    }
                    let mut replay = world.clone();
                    let actual = world.step_material_line_arrivals(dt).unwrap();
                    let restored = replay.step_material_line_arrivals(dt).unwrap();
                    assert_eq!(actual.events, restored.events);
                    assert_eq!(
                        world.snapshot_bytes().unwrap(),
                        replay.snapshot_bytes().unwrap()
                    );
                    assert_eq!(world.snapshot_bytes().unwrap()[0], 4);
                    check_trace(&actual, &world, dt);
                    events.extend(actual.events);
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
                for r in &responses {
                    let start = events
                        .iter()
                        .position(|e| {
                            e.event.id == r.roots[0]
                                && e.event.transition == EpisodeTransition::Impact
                        })
                        .unwrap();
                    let end = events
                        .iter()
                        .position(|e| {
                            e.event.id == r.roots[0] && e.event.transition == EpisodeTransition::End
                        })
                        .unwrap();
                    assert!(start < end);
                }
                assert!(
                    (initial - energy(&world, 17.0) - responses.iter().map(loss).sum::<f64>())
                        .abs()
                        < 1.0e-5 * initial
                );
                assert!((initial_momentum - momentum(&world, 17.0)).abs() < 0.002);
            }
        }
    }
}

#[test]
fn native_line_admission_and_late_failure_are_atomic() {
    for case in 0..21 {
        let (mut world, _) = fixture(Case::Supported, 1, false, 0.0);
        let bh = world.body_handle(body(1)).unwrap();
        let ch = world.collider_handle(collider(1, 0)).unwrap();
        let mut expected = R::Admission(ArrivalRejection::UnsupportedScene);
        match case {
            0 => {
                world.set_pose(body(1), Vec2::new(-0.01, 0.001), 0.0, true);
            }
            1 => {
                world.set_velocity(body(1), Vec2::new(40.0, 0.01), 0.0, true);
            }
            2 => world.raw.colliders[ch].set_shape(SharedShape::ball(0.6)),
            3 => {
                world.insert_body(
                    body(4),
                    BodySpec::default(),
                    &[ColliderSpec::ball(collider(4, 0), 0.5)],
                );
            }
            4 => world.raw.colliders[ch].set_friction(0.5),
            5 => world.raw.bodies[bh].enable_ccd(true),
            6 => world.raw.bodies[bh].lock_rotations(false, true),
            7 => world.raw.bodies[bh].set_linear_damping(0.1),
            8 => {
                world.set_gravity(Vec2::Y);
            }
            9 => world.raw.colliders[ch].set_translation_wrt_parent(Vector::X * 0.1),
            10 => world.raw.colliders[ch].set_sensor(true),
            11 => world.raw.colliders[ch].set_solver_groups(CollisionGroups::NONE.to_rapier()),
            12 => world.raw.bodies[bh].set_body_type(RigidBodyType::KinematicPositionBased, true),
            13 => world.raw.bodies[bh].add_force(Vector::X, true),
            14 => {
                world.raw.bodies[bh]
                    .activation_mut()
                    .normalized_linear_threshold = 0.4
            }
            15 => world.loading.as_mut().unwrap().unbind(collider(1, 0)),
            16 => world.raw.colliders[ch].set_mass_properties(MassProperties::new(
                Vector::X * 0.1,
                20.0,
                10.0,
            )),
            17 => {
                world.set_pose(body(1), Vec2::X * 0.1, 0.0, true);
                expected = R::Numerical;
            }
            18 => {
                world.loading.as_mut().unwrap().config.max_batches_per_step = 1;
                expected = R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit));
            }
            19 => {
                world.loading.as_mut().unwrap().config.max_records_per_step = 3;
                expected = R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit));
            }
            _ => {
                world.loading.as_mut().unwrap().response = None;
                expected = R::Admission(ArrivalRejection::ResponseDisabled);
            }
        }
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let outputs = world.material_response_batches().to_vec();
        assert_eq!(
            world.step_material_line_arrivals(DT).unwrap_err(),
            expected,
            "case {case}"
        );
        assert_unchanged(&world, &before, &loading, &outputs);
    }
}

#[test]
fn native_line_retains_low_speed_and_precision_refusals() {
    let (mut world, _) = fixture(Case::Sequential, 1, false, 0.0);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_pose(body(3), Vec2::X * 3.0, 0.0, true);
    let h = world.collider_handle(collider(1, 0)).unwrap();
    world.raw.colliders[h].set_mass(1.0e12);
    world.refresh_mass_properties(body(1));
    let b = world.body_handle(body(2)).unwrap();
    world.raw.bodies[b].set_body_type(RigidBodyType::Fixed, true);
    world.set_velocity(body(1), Vec2::X * 0.00005, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.step_material_line_arrivals(DT).unwrap_err(),
        R::Unaccounted
    );
    assert_unchanged(&world, &before, &[], &[]);
    let initial = energy(&world, 0.0);
    world.loading.as_mut().unwrap().config.min_approach_speed = 0.0;
    world.step_material_line_arrivals(DT).unwrap();
    assert!((initial - energy(&world, 0.0) - loss(applied(&world))).abs() < 1.0e-5 * initial);

    let (mut world, _) = rapid(1, false, 1_000_000.0);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_pose(body(3), Vec2::X * 3.0, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 1_000_000.25, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        world.step_material_line_arrivals(DT).unwrap_err(),
        R::Response(ResponseRejection::Backend)
    );
    assert_unchanged(&world, &before, &[], &[]);
}

type WireProofs = BTreeMap<[ContactSource; 2], (EpisodeId, [Vec2; 2])>;

fn wire_proofs(world: &PhysicsWorld) -> (Vec<u8>, usize, WireProofs) {
    let bytes = world.snapshot_bytes().unwrap();
    assert_eq!(bytes[0], 4);
    let mut reader = std::io::Cursor::new(bytes.as_slice());
    let _: crate::world::PhysicsSnapshot = bincode::deserialize_from(&mut reader).unwrap();
    let mut loading: LoadingState = bincode::deserialize_from(&mut reader).unwrap();
    loading.deserialize_response(&mut reader).unwrap();
    let offset = reader.position() as usize;
    let proofs = bincode::deserialize_from(&mut reader).unwrap();
    assert_eq!(reader.position() as usize, bytes.len());
    (bytes, offset, proofs)
}

#[test]
fn native_line_snapshot_validation_pause_and_cleanup() {
    let (mut world, tokens) = rapid(1, false, 17.0);
    let legacy = world.snapshot_bytes().unwrap();
    assert_eq!(legacy[0], 3);
    assert_eq!(
        PhysicsWorld::from_snapshot_bytes(&legacy)
            .unwrap()
            .snapshot_bytes()
            .unwrap(),
        legacy
    );
    world.step_material_line_arrivals(0.00025).unwrap();
    world
        .step_material_line_arrivals(5229.0_f32 * 1e-9)
        .unwrap();
    let (before, offset, proofs) = wire_proofs(&world);
    assert_eq!(proofs.len(), 1);
    let loading = world.loading_batches().to_vec();
    let outputs = world.material_response_batches().to_vec();
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            world.step_material_line_arrivals(dt).unwrap_err(),
            R::InvalidTimestep
        );
        assert_unchanged(&world, &before, &loading, &outputs);
    }
    let paused = world.step_material_line_arrivals(0.0).unwrap();
    assert!(paused.events.is_empty() && paused.advances.is_empty() && paused.previews.is_empty());
    assert_eq!(world.snapshot_bytes().unwrap(), before);
    assert!(world.loading_batches().is_empty());
    let (&pair, witness) = proofs.iter().next().unwrap();
    for case in 0..7 {
        let mut invalid = proofs.clone();
        match case {
            0 => invalid.get_mut(&pair).unwrap().0 = EpisodeId(0),
            1 => invalid.get_mut(&pair).unwrap().1[0].x = f32::NAN,
            2 => {
                invalid.remove(&pair);
                invalid.insert([pair[1], pair[0]], *witness);
            }
            3 => {
                invalid.remove(&pair);
                let mut stale = pair;
                stale[0].region.incarnation += 100;
                invalid.insert(stale, *witness);
            }
            4 => {
                invalid.remove(&pair);
                let mut unbound = pair;
                unbound[0].patch = 1234;
                invalid.insert(unbound, *witness);
            }
            5 => {
                for n in 1..4 {
                    let mut extra = pair;
                    extra[0].patch = n;
                    invalid.insert(extra, *witness);
                }
            }
            _ => invalid.get_mut(&pair).unwrap().0 = EpisodeId(witness.0.0 + 10),
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
    for action in 0..3 {
        let mut changed = world.clone();
        match action {
            0 => {
                changed.step(1.0e-9);
            }
            1 => {
                assert!(changed.retire_loading_region(tokens[0]));
            }
            _ => changed.loading.as_mut().unwrap().unbind(collider(1, 0)),
        }
        assert!(wire_proofs(&changed).2.is_empty());
        assert!(PhysicsWorld::from_snapshot_bytes(&changed.snapshot_bytes().unwrap()).is_ok());
    }
    let mut restored = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
    let a = world.step_material_line_arrivals(0.003).unwrap();
    let b = restored.step_material_line_arrivals(0.003).unwrap();
    assert_eq!(a.events, b.events);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        restored.snapshot_bytes().unwrap()
    );
    assert_eq!(world.material_response_batches().len(), 3);
}

#[test]
fn native_line_frame_end_end_order_and_prescribed_supports() {
    let (mut world, tokens) = fixture(Case::Sequential, 1, false, 0.0);
    world.set_pose(body(1), -Vec2::X, 0.0, true);
    world.set_pose(body(3), Vec2::X * 3.0, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 120.0, 0.0, true);
    let before = balances(&world, &tokens);
    assert!(
        world
            .step_material_line_arrivals(DT)
            .unwrap()
            .events
            .is_empty()
    );
    assert_eq!(balances(&world, &tokens), before);
    let next = world.step_material_line_arrivals(DT).unwrap();
    assert_eq!(next.events[0].time_seconds, 0.0);
    assert_eq!(next.events[0].event.transition, EpisodeTransition::Impact);

    for cap in [5, 6] {
        let (mut world, _) = fixture(Case::Simultaneous, 1, true, 0.0);
        world.loading.as_mut().unwrap().config.max_records_per_step = cap;
        let before = world.snapshot_bytes().unwrap();
        let result = world.step_material_line_arrivals(DT);
        if cap == 5 {
            assert_eq!(
                result.unwrap_err(),
                R::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
            );
            assert_unchanged(&world, &before, &[], &[]);
        } else {
            let result = result.unwrap();
            check_trace(&result, &world, DT);
            assert_eq!(result.events.len(), 4);
            assert!(
                result.events[2..]
                    .iter()
                    .all(|e| e.phase == LineEventPhase::AfterDrift)
            );
            assert_eq!(result.events[2].time_seconds, result.events[3].time_seconds);
            assert!(result.events[2].event.sources < result.events[3].event.sources);
        }
    }
    for shift in [0.0, 17.0] {
        let (mut world, _) = fixture(Case::Supported, 2, true, shift);
        let h = world.body_handle(body(3)).unwrap();
        world.raw.bodies[h].set_body_type(
            if shift == 0.0 {
                RigidBodyType::Fixed
            } else {
                RigidBodyType::KinematicVelocityBased
            },
            true,
        );
        world.refresh_mass_properties(body(3));
        world.collect_events = true;
        let initial = energy(&world, shift);
        let global = energy(&world, 0.0);
        world.step_material_line_arrivals(DT).unwrap();
        let response = applied(&world);
        assert_eq!(response.impacts.len(), 2);
        assert!((initial - energy(&world, shift) - loss(response)).abs() < 1.0e-5 * initial);
        assert!(
            (global - energy(&world, 0.0) + response.prescribed_work - loss(response)).abs()
                < 1.0e-5 * global
        );
    }
}

#[test]
fn native_line_source_order_is_independent_of_spatial_order() {
    // Reflect the rapid case: sources 3,2,1 now run left-to-right. Witness
    // endpoints must follow source order, not accidental spatial ordering.
    let (mut world, _) = rapid(2, true, 17.0);
    for n in 1..=3 {
        let motion = world.motion(body(n)).unwrap();
        world.set_pose(body(n), -motion.position, 0.0, true);
        world.set_velocity(body(n), -motion.linear_velocity, 0.0, true);
    }
    let initial = energy(&world, -17.0);
    let mut responses = Vec::new();
    for dt in [0.00025, 5229.0_f32 * 1e-9, 0.003] {
        let mut replay = world.clone();
        let a = world.step_material_line_arrivals(dt).unwrap();
        let b = replay.step_material_line_arrivals(dt).unwrap();
        assert_eq!(a.events, b.events);
        assert_eq!(
            world.snapshot_bytes().unwrap(),
            replay.snapshot_bytes().unwrap()
        );
        responses.extend(
            world
                .material_response_batches()
                .iter()
                .map(|b| b.result.as_ref().unwrap().clone()),
        );
    }
    assert_eq!(responses.len(), 4);
    assert!(
        (initial - energy(&world, -17.0) - responses.iter().map(loss).sum::<f64>()).abs()
            < 1.0e-5 * initial
    );
}

#[test]
fn native_line_chatter_and_pending_continuity_do_not_mint_roots() {
    let (mut world, tokens) = fixture(Case::Supported, 2, true, 0.0);
    world.set_pose(body(1), Vec2::X * -10.0, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    let before = balances(&world, &tokens);
    let episode = world
        .loading
        .as_ref()
        .unwrap()
        .episodes
        .values()
        .next()
        .unwrap()
        .id;
    for i in 0..64 {
        world.set_pose(
            body(2),
            Vec2::X * (1.0 + if i % 2 == 0 { 0.25e-6 } else { -0.25e-6 }),
            0.0,
            true,
        );
        let result = world.step_material_line_arrivals(1.0 / 240.0).unwrap();
        assert!(result.events.is_empty());
        assert!(world.material_response_batches().is_empty());
        assert_eq!(balances(&world, &tokens), before);
        assert_eq!(
            world
                .loading
                .as_ref()
                .unwrap()
                .episodes
                .values()
                .next()
                .unwrap()
                .id,
            episode
        );
    }
    world.set_pose(body(2), Vec2::X, 0.0, true);
    for speed in [0.00005, 1.0] {
        world.set_velocity(body(2), Vec2::X * speed, 0.0, true);
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        assert_eq!(
            world.step_material_line_arrivals(DT).unwrap_err(),
            R::Unaccounted
        );
        assert_unchanged(&world, &before, &loading, &[]);
    }
    for reversal in [false, true] {
        let (mut world, _) = rapid(1, false, 17.0);
        world.step_material_line_arrivals(0.00025).unwrap();
        world
            .step_material_line_arrivals(5229.0_f32 * 1e-9)
            .unwrap();
        let (_, _, pending) = wire_proofs(&world);
        let (&pair, &(episode, _)) = pending.iter().next().unwrap();
        if reversal {
            let b_speed = world.motion(body(2)).unwrap().linear_velocity.x;
            world.set_velocity(body(1), Vec2::X * (b_speed + 0.001), 0.0, true);
        } else {
            let position = world.motion(body(1)).unwrap().position;
            world.set_pose(body(1), position - Vec2::X * 0.1, 0.0, true);
        }
        assert!(
            world
                .step_material_line_arrivals(0.0001)
                .unwrap()
                .events
                .is_empty()
        );
        assert!(wire_proofs(&world).2.is_empty());
        assert_eq!(world.loading.as_ref().unwrap().episodes[&pair].id, episode);
    }
}
