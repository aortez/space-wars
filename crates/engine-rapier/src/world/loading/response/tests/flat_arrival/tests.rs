use super::*;

pub(super) fn kinetic(world: &PhysicsWorld, shift: Vec2) -> f64 {
    world
        .raw
        .bodies
        .iter()
        .filter(|(_, b)| b.is_dynamic())
        .map(|(_, b)| {
            let v = delta(b.linvel(), to_rapier(shift));
            0.5 * f64::from(b.mass()) * dot(v, v)
        })
        .sum()
}

pub(super) fn momentum(world: &PhysicsWorld, shift: Vec2) -> [f64; 2] {
    let mut result = [0.0; 2];
    for (_, b) in world.raw.bodies.iter().filter(|(_, b)| b.is_dynamic()) {
        let v = delta(b.linvel(), to_rapier(shift));
        for axis in 0..2 {
            result[axis] += f64::from(b.mass()) * v[axis];
        }
    }
    result
}

fn unchanged(
    world: &PhysicsWorld,
    snapshot: &[u8],
    loading: &[ContactBatch],
    responses: &[ResponseBatch],
) {
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), responses);
}

fn qualify(layouts: &[&[u16]], offsets: &[f32]) {
    for parts in layouts {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for copies in [1, 2] {
                for reverse in [false, true] {
                    for angle in [0.0, 0.37] {
                        for shift in [Vec2::ZERO, Vec2::new(17.0, -11.0)] {
                            for &offset in offsets {
                                for dynamic in [false, true] {
                                    let (mut world, tokens) = fixture(
                                        parts, copies, reverse, angle, shift, offset, dynamic,
                                    );
                                    let initial = kinetic(&world, shift);
                                    let mut native = world.clone();
                                    let mut tracker = departure::Tracker::default();
                                    let global = kinetic(&world, Vec2::ZERO);
                                    let p = momentum(&world, shift);
                                    let (mut reference, reference_tokens) = fixture(
                                        &vec![1; parts.len()],
                                        1,
                                        false,
                                        angle,
                                        shift,
                                        offset,
                                        dynamic,
                                    );
                                    reference.set_pose(body(1), Vec2::ZERO, angle, true);
                                    reference.step(1.0 / hz);
                                    let expected = applied(&reference).clone();
                                    let reference_balances =
                                        balances(&reference, &reference_tokens);
                                    let mut results = Vec::new();
                                    let mut times = Vec::new();
                                    let mut elapsed = 0.0;
                                    for frame in 0..2 {
                                        let mut replay = world.clone();
                                        let mut replay_tracker = tracker.clone();
                                        let dt = 1.0 / hz;
                                        let trace = step_tracked(&mut world, dt, MAX_ADVANCES, true, Some(&mut tracker)).unwrap_or_else(|e| panic!(
                                            "parts={parts:?} hz={hz} copies={copies} reverse={reverse} angle={angle} shift={shift:?} offset={offset} dynamic={dynamic} frame={frame}: {e:?}"));
                                        native::compare(&mut native, &world, &tracker, &trace, dt);
                                        assert_eq!(
                                            trace,
                                            step_tracked(
                                                &mut replay,
                                                dt,
                                                MAX_ADVANCES,
                                                true,
                                                Some(&mut replay_tracker)
                                            )
                                            .unwrap()
                                        );
                                        assert_eq!(
                                            world.snapshot_bytes().unwrap(),
                                            replay.snapshot_bytes().unwrap()
                                        );
                                        assert_eq!(
                                            world.loading_batches(),
                                            replay.loading_batches()
                                        );
                                        assert_eq!(
                                            world.material_response_batches(),
                                            replay.material_response_batches()
                                        );
                                        assert_eq!(
                                            trace.durations.len(),
                                            world.loading_batches().len()
                                        );
                                        let sum: f64 =
                                            trace.durations.iter().map(|t| f64::from(*t)).sum();
                                        assert!(
                                            (sum - f64::from(dt)).abs()
                                                < 2.0 * f64::from(f32::EPSILON * dt)
                                        );
                                        for (i, batch) in world.loading_batches().iter().enumerate()
                                        {
                                            assert_eq!(batch.substep, i);
                                            assert_eq!(batch.duration, trace.durations[i]);
                                        }
                                        times
                                            .extend(trace.responses_at.iter().map(|t| t + elapsed));
                                        elapsed += f64::from(dt);
                                        results.extend(
                                            world
                                                .material_response_batches()
                                                .iter()
                                                .map(|r| r.result.as_ref().unwrap().clone()),
                                        );
                                    }
                                    assert_eq!(times.len(), 1);
                                    assert!((times[0] - 0.0005).abs() < 1.0e-6 / 40.0);
                                    assert_eq!(results.len(), 1);
                                    let response = &results[0];
                                    assert_eq!(response.roots.len(), 1);
                                    assert_eq!(response.impacts.len(), parts.len() - 1);
                                    let tolerance = 1.0e-5 * initial;
                                    assert!(
                                        (initial - kinetic(&world, shift) - loss(response)).abs()
                                            < tolerance
                                    );
                                    assert!(
                                        (global - kinetic(&world, Vec2::ZERO)
                                            + response.prescribed_work
                                            - loss(response))
                                        .abs()
                                            < tolerance
                                    );
                                    if dynamic {
                                        let after = momentum(&world, shift);
                                        assert!(
                                            (0..2)
                                                .all(|axis| (after[axis] - p[axis]).abs() < 0.002)
                                        );
                                    }
                                    for (i, (actual, expected)) in
                                        response.impacts.iter().zip(&expected.impacts).enumerate()
                                    {
                                        assert_eq!(actual.sources, expected.sources);
                                        let width = if i == 0 { 2.0 - offset } else { 2.0 };
                                        assert!((actual.geometry.width - width).abs() < 1.0e-5);
                                        assert_eq!(actual.geometry.a, expected.geometry.a);
                                        assert_eq!(actual.geometry.b, expected.geometry.b);
                                        assert!(
                                            (actual.outcome.ledger.available
                                                - expected.outcome.ledger.available)
                                                .abs()
                                                < tolerance as f32
                                        );
                                        for side in 0..2 {
                                            assert!(
                                                (actual.outcome.consumed[side]
                                                    - expected.outcome.consumed[side])
                                                    .abs()
                                                    < tolerance as f32
                                            );
                                        }
                                    }
                                    for (actual, expected) in
                                        balances(&world, &tokens).iter().zip(reference_balances)
                                    {
                                        assert!((actual - expected).abs() < tolerance);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn flat_arrival_pair_matches_touching_work_width_and_replay() {
    qualify(&[&[1, 1], &[2, 2], &[4, 4], &[1, 4], &[4, 2]], &[0.0, 0.5]);
}

#[test]
fn flat_arrival_supported_chain_preserves_one_root_and_patch_work() {
    qualify(
        &[&[1, 1, 1], &[2, 2, 2], &[4, 4, 4], &[1, 4, 2], &[4, 2, 4]],
        &[0.0, 0.5],
    );
}

#[test]
fn flat_arrival_internal_seams_need_exterior_boundary_evidence() {
    for parts in [&[4, 2][..], &[4, 2, 4]] {
        let (mut world, _) = fixture(parts, 2, true, 0.37, Vec2::new(17.0, -11.0), 0.5, true);
        let initial = kinetic(&world, Vec2::new(17.0, -11.0));
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        assert_eq!(
            step_with_policy(&mut world, DT, MAX_ADVANCES, false),
            Err(Refusal::Response(ResponseRejection::AmbiguousPatch))
        );
        unchanged(&world, &before, &loading, &response);
        let trace = step(&mut world, DT, MAX_ADVANCES).unwrap();
        assert!(trace.seams_removed > 0);
        let response = applied(&world);
        assert_eq!(response.impacts.len(), parts.len() - 1);
        assert!((response.impacts[0].geometry.width - 1.5).abs() < 1.0e-5);
        if parts.len() == 3 {
            assert!((response.impacts[1].geometry.width - 2.0).abs() < 1.0e-5);
        }
        assert!(
            (initial - kinetic(&world, Vec2::new(17.0, -11.0)) - loss(response)).abs()
                < 1.0e-5 * initial
        );
    }
}

#[test]
fn flat_arrival_supported_response_is_independent_of_impulse_targets() {
    // Equal per-point shares used to leave B-C closing by ~9.6e-16 and
    // 9.536743e-7 respectively, while each unsplit reference succeeded.
    for (parts, copies, angle, offset, dynamic) in [
        (&[1, 1, 1][..], 2, 0.37, 0.0, false),
        (&[2, 2, 2][..], 1, 0.0, 0.5, true),
    ] {
        let (mut world, _) = fixture(parts, copies, false, angle, Vec2::ZERO, offset, dynamic);
        let initial = kinetic(&world, Vec2::ZERO);
        let mut whole_frame = world.clone();
        step(&mut whole_frame, DT, MAX_ADVANCES).unwrap();
        let plates = Plates::new(&world).unwrap();
        let speed: Vec<_> = plates
            .handles
            .iter()
            .map(|h| world.raw.bodies[*h].linvel())
            .collect();
        let arrival = plates.next_time(&world, &speed).unwrap().unwrap() as f32;
        step(&mut world, arrival, MAX_ADVANCES).unwrap();
        assert!(world.material_response_batches().is_empty());

        let mut witness = world.clone();
        witness.loading.as_mut().unwrap().begin_step();
        let (speed, _) = advance(&mut witness, &plates, DT, true, true).unwrap();
        assert_eq!(applied(&witness).impacts.len(), 2);
        assert_eq!(speed[1], speed[2]);
        assert_eq!(plates.next_time(&world, &speed), Ok(None));
        step(&mut world, DT, MAX_ADVANCES).unwrap();
        let response = applied(&world);
        assert!((initial - kinetic(&world, Vec2::ZERO) - loss(response)).abs() < 1.0e-5 * initial);

        let (mut reference, reference_tokens) =
            fixture(&[1, 1, 1], 1, false, angle, Vec2::ZERO, offset, dynamic);
        step(&mut reference, DT, MAX_ADVANCES).unwrap();
        assert_eq!(response, applied(&reference));
        assert_eq!(response, applied(&whole_frame));
        for token in reference_tokens {
            assert_eq!(
                world.loading_material_remaining(token),
                reference.loading_material_remaining(token)
            );
        }
        for n in 1..=3 {
            let a = world.body_handle(body(n)).unwrap();
            let b = reference.body_handle(body(n)).unwrap();
            assert_eq!(
                world.raw.bodies[a].linvel(),
                reference.raw.bodies[b].linvel()
            );
            assert_eq!(world.raw.bodies[a].angvel(), 0.0);
        }
    }
}

#[test]
fn flat_arrival_unrepresentable_rebound_still_refuses_atomically() {
    for parts in [&[1, 1][..], &[4, 2]] {
        for copies in [1, 2] {
            for shift in [0.0, 17.0, -1_000_000.0, 1_000_000.0] {
                let (mut world, tokens) =
                    fixture(parts, copies, true, 0.0, Vec2::X * shift, 0.5, false);
                world.set_pose(body(1), Vec2::ZERO, 0.0, true);
                world.set_velocity(body(1), Vec2::X * (shift + 0.25), 0.0, true);
                let before = world.snapshot_bytes().unwrap();
                let capacity = balances(&world, &tokens);
                let initial = kinetic(&world, Vec2::X * shift);
                let result = step(&mut world, DT, MAX_ADVANCES);
                if shift.abs() == 1_000_000.0 {
                    assert_eq!(result, Err(Refusal::Response(ResponseRejection::Backend)));
                    unchanged(&world, &before, &[], &[]);
                } else {
                    result.unwrap();
                    let response = applied(&world);
                    assert!(loss(response) > 0.0);
                    assert!(
                        (initial - kinetic(&world, Vec2::X * shift) - loss(response)).abs()
                            < 1.0e-5 * initial
                    );
                }
                // This gentle impact dissipates energy without permanent damage.
                assert_eq!(balances(&world, &tokens), capacity);
            }
        }
    }
}

#[test]
fn flat_arrival_endpoint_pause_and_late_failures_preserve_outputs() {
    let (mut world, tokens) = fixture(&[2, 2], 1, false, 0.0, Vec2::ZERO, 0.5, true);
    world.set_pose(body(1), Vec2::X * -0.03125, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 32.0, 0.0, true);
    let before = balances(&world, &tokens);
    let trace = step(&mut world, 1.0 / 1024.0, MAX_ADVANCES).unwrap();
    assert!(trace.responses_at.is_empty());
    assert!(world.material_response_batches().is_empty());
    assert_eq!(balances(&world, &tokens), before);
    let trace = step(&mut world, 1.0e-6, MAX_ADVANCES).unwrap();
    assert_eq!(trace.responses_at, [0.0]);
    let snapshot = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let response = world.material_response_batches().to_vec();
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            step(&mut world, dt, MAX_ADVANCES),
            Err(Refusal::InvalidTime)
        );
        unchanged(&world, &snapshot, &loading, &response);
    }
    assert!(
        step(&mut world, 0.0, MAX_ADVANCES)
            .unwrap()
            .durations
            .is_empty()
    );
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert!(world.loading_batches().is_empty());
    assert!(world.material_response_batches().is_empty());

    for capture_limit in [false, true] {
        let (mut world, _) = fixture(&[1, 1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
        // The resting support produced real capture output. Arrange an
        // arrival that spends work before the second advance hits a limit.
        assert!(!world.loading_batches().is_empty());
        if !capture_limit {
            world.set_pose(body(1), Vec2::ZERO, 0.0, true);
            world.set_pose(body(3), Vec2::X * 2.02, 0.0, true);
        }
        if capture_limit {
            world.loading.as_mut().unwrap().config.max_batches_per_step = 1;
        }
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        let expected = if capture_limit {
            Refusal::Response(ResponseRejection::Capture(CaptureRejection::StepLimit))
        } else {
            Refusal::Limit { applied: 1 }
        };
        assert_eq!(
            step(&mut world, DT, if capture_limit { MAX_ADVANCES } else { 1 }),
            Err(expected)
        );
        unchanged(&world, &before, &loading, &response);
    }
}

#[test]
fn flat_arrival_only_certified_interior_faces_can_be_removed() {
    for angle in [0.0, 0.37] {
        let (world, _) = fixture(&[4, 2], 1, false, angle, Vec2::ZERO, 0.5, true);
        Plates::new(&world).unwrap();
        for part in 0..4 {
            let h = world.collider_handle(collider(1, part)).unwrap();
            let c = &world.raw.colliders[h];
            let n = to_rapier(rotate(Vec2::Y, angle));
            assert_eq!(internal_tangent_face(c, n), part != 3);
            assert_eq!(internal_tangent_face(c, -n), part != 0);
            let n = to_rapier(rotate(Vec2::X, angle));
            assert!(!internal_tangent_face(c, n));
            assert!(!internal_tangent_face(c, -n));
        }
    }

    let (mut original, _) = fixture(&[4, 2], 1, false, 0.0, Vec2::ZERO, 0.5, true);
    step(&mut original, DT, MAX_ADVANCES).unwrap();
    assert!(!original.material_response_batches().is_empty());
    for case in 0..8 {
        let mut world = original.clone();
        let b = world.body_handle(body(1)).unwrap();
        let c = world.collider_handle(collider(1, 0)).unwrap();
        match case {
            0 => {
                let middle = world.collider_handle(collider(1, 1)).unwrap();
                world.raw.colliders[middle].set_translation_wrt_parent(Vector::new(0.0, -0.125));
            }
            1 => world.raw.colliders[c].set_translation_wrt_parent(Vector::new(0.01, -0.75)),
            2 => world.raw.colliders[c].set_rotation_wrt_parent(0.01),
            3 => {
                world.set_pose(body(1), Vec2::ZERO, 0.01, true);
            }
            4 => world.raw.bodies[b].lock_rotations(false, true),
            5 => world.raw.colliders[c].set_shape(SharedShape::ball(0.5)),
            6 => {
                let v = world.raw.bodies[b].linvel() + Vector::Y;
                world.raw.bodies[b].set_linvel(v, true);
            }
            _ => {
                world.set_pose(body(2), Vec2::new(1.0, 2.0), 0.0, true);
            }
        }
        let before = world.snapshot_bytes().unwrap();
        let loading = world.loading_batches().to_vec();
        let response = world.material_response_batches().to_vec();
        assert_eq!(
            step(&mut world, DT, MAX_ADVANCES),
            Err(Refusal::Geometry),
            "case {case}"
        );
        unchanged(&world, &before, &loading, &response);
    }

    let (mut world, _) = fixture(&[1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::X * 0.00005, 0.0, true);
    let before = world.snapshot_bytes().unwrap();
    assert_eq!(
        step(&mut world, DT, MAX_ADVANCES),
        Err(Refusal::Unaccounted)
    );
    unchanged(&world, &before, &[], &[]);
}
