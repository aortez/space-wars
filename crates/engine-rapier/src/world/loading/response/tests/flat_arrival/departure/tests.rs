use super::*;

pub(super) fn first_arrival(experiment: &mut Verified) -> f32 {
    let plates = Plates::new(&experiment.world).unwrap();
    let v: Vec<_> = plates
        .handles
        .iter()
        .map(|h| experiment.world.raw.bodies[*h].linvel())
        .collect();
    let duration = plates.next_time(&experiment.world, &v).unwrap().unwrap() as f32;
    experiment.step(duration, MAX_ADVANCES).unwrap();
    assert!(experiment.world.material_response_batches().is_empty());
    duration
}

fn pending() -> Verified {
    let (mut experiment, _) = rapid(
        &[1, 1, 1],
        1,
        false,
        0.37,
        Vec2::new(17.0, -11.0),
        0.5,
        true,
    );
    first_arrival(&mut experiment);
    experiment.step(2835.0_f32 * 1.0e-9, MAX_ADVANCES).unwrap();
    assert_eq!(experiment.tracker.pending.len(), 1);
    experiment
}

fn unchanged(
    experiment: &Verified,
    snapshot: &[u8],
    loading: &[ContactBatch],
    response: &[ResponseBatch],
) {
    assert_eq!(experiment.snapshot(), snapshot);
    assert_eq!(experiment.world.loading_batches(), loading);
    assert_eq!(experiment.world.material_response_batches(), response);
}

fn equivalent(actual: &[AppliedResponse], expected: &[AppliedResponse]) {
    assert_eq!(actual.len(), expected.len());
    // Every fixture starts with mass20 at relative speed40: KE=16000.
    let tolerance = 1.0e-5 * 16000.0;
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.roots, expected.roots);
        assert_eq!(actual.impacts[0].sources, expected.impacts[0].sources);
        assert!((loss(actual) - loss(expected)).abs() < tolerance);
        for side in 0..2 {
            assert!(
                (actual.impacts[0].after[side] - expected.impacts[0].after[side]).abs() < tolerance
            );
        }
    }
}

fn exercise(
    mut experiment: Verified,
    dt: f32,
    horizon: f32,
    shift: Vec2,
    dynamic: bool,
) -> Vec<AppliedResponse> {
    let mut native = experiment.world.clone();
    let initial = flat_tests::kinetic(&experiment.world, shift);
    let global = flat_tests::kinetic(&experiment.world, Vec2::ZERO);
    let initial_momentum = flat_tests::momentum(&experiment.world, shift);
    let mut responses = Vec::new();
    let mut ended = BTreeMap::new();
    let mut started = BTreeMap::new();
    let mut elapsed = 0.0;
    let mut frame = 0;
    loop {
        let remaining = (f64::from(horizon) - elapsed) as f32;
        let duration = dt.min(remaining);
        let mut replay = Verified::restore(&experiment.snapshot());
        let trace = experiment.step(duration, MAX_ADVANCES).unwrap_or_else(|e| {
            panic!("dt={dt} frame={frame} dynamic={dynamic} shift={shift:?}: {e:?}")
        });
        super::super::native::compare(
            &mut native,
            &experiment.world,
            &experiment.tracker,
            &trace,
            duration,
        );
        assert_eq!(trace, replay.step(duration, MAX_ADVANCES).unwrap());
        assert_eq!(experiment.snapshot(), replay.snapshot());
        assert_eq!(
            experiment.world.loading_batches(),
            replay.world.loading_batches()
        );
        assert_eq!(
            experiment.world.material_response_batches(),
            replay.world.material_response_batches()
        );
        let mut batch_time = elapsed;
        for batch in experiment.world.loading_batches() {
            for event in &batch.events {
                match event.transition {
                    EpisodeTransition::Impact => {
                        assert!(
                            started
                                .insert(event.id, (event.sources, batch_time))
                                .is_none()
                        );
                    }
                    EpisodeTransition::End => {
                        assert!(ended.insert(event.id, batch_time).is_none());
                    }
                    _ => {}
                }
            }
            batch_time += f64::from(batch.duration);
        }
        for d in trace.departures {
            assert!(d.lower > 2.0e-5);
            assert!(d.at > 0.0 && d.at <= f64::from(duration) * (1.0 + f64::from(f32::EPSILON)));
            assert!(ended.insert(d.event.id, elapsed + d.at).is_none());
        }
        responses.extend(
            experiment
                .world
                .material_response_batches()
                .iter()
                .map(|b| b.result.as_ref().unwrap().clone()),
        );
        elapsed += f64::from(duration);
        if duration == remaining {
            break;
        }
        frame += 1;
    }
    let lost: f64 = responses.iter().map(loss).sum();
    let work: f64 = responses.iter().map(|r| r.prescribed_work).sum();
    assert!(
        (initial - flat_tests::kinetic(&experiment.world, shift) - lost).abs() < 1.0e-5 * initial
    );
    assert!(
        (global - flat_tests::kinetic(&experiment.world, Vec2::ZERO) + work - lost).abs()
            < 1.0e-5 * initial
    );
    if dynamic {
        let after = flat_tests::momentum(&experiment.world, shift);
        assert!((0..2).all(|axis| (after[axis] - initial_momentum[axis]).abs() < 0.002));
    }
    assert_eq!(started.len(), responses.len());
    assert_eq!(ended.len(), responses.len());
    for (i, response) in responses.iter().enumerate() {
        assert_eq!(response.roots.len(), 1);
        assert_eq!(response.impacts.len(), 1);
        let id = response.roots[0];
        assert!(ended[&id] > started[&id].1);
        assert_eq!(response.impacts[0].sources, started[&id].0);
        if i >= 2 {
            assert_ne!(id, responses[i - 2].roots[0]);
            assert!(ended[&responses[i - 2].roots[0]] <= started[&id].1);
        }
    }
    assert!(experiment.tracker.pending.is_empty());
    responses
}

#[test]
fn flat_departure_rearms_real_returns_across_partitions_and_timesteps() {
    let mut reference: BTreeMap<u32, Vec<AppliedResponse>> = BTreeMap::new();
    for parts in [
        &[1, 1, 1][..],
        &[2, 2, 2],
        &[4, 4, 4],
        &[1, 4, 2],
        &[4, 2, 4],
    ] {
        for copies in [1, 2] {
            for reverse in [false, true] {
                for angle in [0.0, 0.37] {
                    for shift in [Vec2::ZERO, Vec2::new(17.0, -11.0)] {
                        for offset in [0.0_f32, 0.5] {
                            for hz in [30.0, 60.0, 120.0, 240.0] {
                                let (experiment, _) =
                                    rapid(parts, copies, reverse, angle, shift, offset, true);
                                let results =
                                    exercise(experiment, 1.0 / hz, 1.0 / 30.0, shift, true);
                                assert_eq!(results.len(), 4);
                                if let Some(previous) = reference.get(&offset.to_bits()) {
                                    equivalent(&results, previous);
                                } else {
                                    reference.insert(offset.to_bits(), results);
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
fn flat_departure_support_and_fine_subdivision_keep_work_checks() {
    for offset in [0.0, 0.5] {
        let mut reference: Option<Vec<AppliedResponse>> = None;
        for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
            for copies in [1, 2] {
                for reverse in [false, true] {
                    for hz in [30.0, 60.0, 120.0, 240.0, 2000.0, 10000.0] {
                        let (experiment, _) =
                            rapid(parts, copies, reverse, 0.0, Vec2::ZERO, offset, false);
                        let results = exercise(experiment, 1.0 / hz, 0.02, Vec2::ZERO, false);
                        assert_eq!(results.len(), 7);
                        if let Some(previous) = &reference {
                            equivalent(&results, previous);
                        } else {
                            reference = Some(results);
                        }
                    }
                }
            }
        }
        for angle in [0.0, 0.37] {
            for shift in [Vec2::ZERO, Vec2::new(17.0, -11.0)] {
                for hz in [2000.0, 10000.0] {
                    let (experiment, _) = rapid(&[4, 2, 4], 2, true, angle, shift, offset, true);
                    assert_eq!(exercise(experiment, 1.0 / hz, 0.006, shift, true).len(), 4);
                }
            }
        }
    }
    // Departure proof does not waive the existing rounded-energy validator.
    let (mut experiment, _) = rapid(
        &[1, 1, 1],
        1,
        false,
        0.0,
        Vec2::new(17.0, -11.0),
        0.0,
        false,
    );
    experiment.step(0.0015, MAX_ADVANCES).unwrap();
    assert_eq!(experiment.world.material_response_batches().len(), 2);
    let before = experiment.snapshot();
    let loading = experiment.world.loading_batches().to_vec();
    let response = experiment.world.material_response_batches().to_vec();
    assert_eq!(
        experiment.step(0.03, MAX_ADVANCES),
        Err(Refusal::Response(ResponseRejection::Backend))
    );
    unchanged(&experiment, &before, &loading, &response);
}

#[test]
fn flat_departure_rounded_crossing_preserves_pending_evidence() {
    for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
        for copies in [1, 2] {
            for reverse in [false, true] {
                for nanos in [2800.0_f32, 2834.0, 2835.0, 2836.0, 2870.0] {
                    let (mut experiment, _) = rapid(
                        parts,
                        copies,
                        reverse,
                        0.37,
                        Vec2::new(17.0, -11.0),
                        0.5,
                        true,
                    );
                    let mut native = experiment.world.clone();
                    let first = first_arrival(&mut experiment);
                    native.step_material_flat_arrivals(first).unwrap();
                    let early = experiment.step(nanos * 1.0e-9, MAX_ADVANCES).unwrap();
                    super::super::native::compare(
                        &mut native,
                        &experiment.world,
                        &experiment.tracker,
                        &early,
                        nanos * 1.0e-9,
                    );
                    assert_eq!(experiment.world.material_response_batches().len(), 1);
                    let mut replay = Verified::restore(&experiment.snapshot());
                    let late = experiment.step(0.006, MAX_ADVANCES).unwrap();
                    super::super::native::compare(
                        &mut native,
                        &experiment.world,
                        &experiment.tracker,
                        &late,
                        0.006,
                    );
                    assert_eq!(late, replay.step(0.006, MAX_ADVANCES).unwrap());
                    assert_eq!(experiment.snapshot(), replay.snapshot());
                    assert_eq!(early.departures.len() + late.departures.len(), 4);
                    assert_eq!(experiment.world.material_response_batches().len(), 3);
                }
            }
        }
    }
    let experiment = pending();
    let mut dropped = Verified::restore(&experiment.snapshot());
    dropped.tracker.pending.clear();
    assert_eq!(dropped.step(0.006, MAX_ADVANCES), Err(Refusal::Unaccounted));
    let mut native = experiment.world.clone();
    assert_eq!(
        native.step_material_flat_arrivals(0.006).unwrap_err(),
        crate::world::loading::response::flat_arrival::FlatArrivalRejection::Unaccounted
    );
}

#[test]
fn flat_departure_continuity_includes_normal_and_both_coordinates() {
    for edit in 0..4 {
        let mut experiment = pending();
        let (&pair, witness) = experiment.tracker.pending.iter().next().unwrap();
        let episode = witness.episode;
        match edit {
            0 | 1 => {
                let mut p = experiment.world.motion(body(1)).unwrap().position;
                if edit == 0 {
                    p.x -= 0.1;
                } else {
                    p.y -= 0.1;
                }
                experiment.world.set_pose(body(1), p, 0.37, true);
            }
            2 => {
                let v = experiment.world.motion(body(2)).unwrap().linear_velocity
                    + rotate(Vec2::X * 0.001, 0.37);
                experiment.world.set_velocity(body(1), v, 0.0, true);
            }
            _ => {
                let shift = Vec2::new(17.0, -11.0);
                for n in 1..=3 {
                    let motion = experiment.world.motion(body(n)).unwrap();
                    experiment
                        .world
                        .set_pose(body(n), motion.position, 0.3701, true);
                    experiment.world.set_velocity(
                        body(n),
                        shift + rotate(motion.linear_velocity - shift, 0.0001),
                        0.0,
                        true,
                    );
                }
                // Omitting the orientation check would reuse this old witness
                // even though no body center moved and the face plane did.
                let mut forged = Verified::restore(&experiment.snapshot());
                let normal = Plates::new(&forged.world).unwrap().normal;
                forged.tracker.pending.get_mut(&pair).unwrap().normal = normal;
                assert_eq!(
                    forged.step(0.0001, MAX_ADVANCES).unwrap().departures.len(),
                    1
                );
            }
        }
        assert!(
            experiment
                .step(0.0001, MAX_ADVANCES)
                .unwrap()
                .departures
                .is_empty()
        );
        assert!(experiment.tracker.pending.is_empty());
        assert_eq!(
            experiment.world.loading.as_ref().unwrap().episodes[&pair].id,
            episode
        );
    }
}

#[test]
fn flat_departure_limits_pause_and_end_notifications_are_atomic() {
    for limit in [1, 2] {
        let mut experiment = pending();
        let before = experiment.snapshot();
        let loading = experiment.world.loading_batches().to_vec();
        let response = experiment.world.material_response_batches().to_vec();
        assert_eq!(
            experiment.step(0.006, limit),
            Err(Refusal::Limit { applied: limit - 1 })
        );
        unchanged(&experiment, &before, &loading, &response);
    }
    let mut experiment = pending();
    let before = experiment.snapshot();
    let loading = experiment.world.loading_batches().to_vec();
    let response = experiment.world.material_response_batches().to_vec();
    for dt in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(experiment.step(dt, MAX_ADVANCES), Err(Refusal::InvalidTime));
        unchanged(&experiment, &before, &loading, &response);
    }
    let paused = experiment.step(0.0, MAX_ADVANCES).unwrap();
    assert!(paused.durations.is_empty() && paused.departures.is_empty());
    unchanged(&experiment, &before, &[], &[]);

    for cap in [7, 8] {
        let (mut experiment, _) = rapid(&[1, 1, 1], 1, false, 0.0, Vec2::ZERO, 0.0, true);
        experiment
            .world
            .set_pose(body(3), Vec2::X * 2.01, 0.0, true);
        experiment
            .world
            .set_velocity(body(3), Vec2::X * -40.0, 0.0, true);
        experiment
            .world
            .loading
            .as_mut()
            .unwrap()
            .config
            .max_records_per_step = cap;
        let before = experiment.snapshot();
        let loading = experiment.world.loading_batches().to_vec();
        let response = experiment.world.material_response_batches().to_vec();
        let result = experiment.step(DT, MAX_ADVANCES);
        if cap == 7 {
            assert_eq!(
                result,
                Err(Refusal::Response(ResponseRejection::Capture(
                    CaptureRejection::StepLimit
                )))
            );
            unchanged(&experiment, &before, &loading, &response);
        } else {
            let trace = result.unwrap();
            assert_eq!(trace.departures.len(), 2);
            assert_eq!(trace.departures[0].at, trace.departures[1].at);
            assert!(trace.departures[0].event.sources < trace.departures[1].event.sources);
        }
    }
}

#[test]
fn flat_departure_rejects_bad_drift_and_keeps_resting_contacts_owned() {
    let mut experiment = pending();
    let plates = Plates::new(&experiment.world).unwrap();
    let start = plates.positions(&experiment.world);
    let velocity: Vec<_> = plates
        .handles
        .iter()
        .map(|h| experiment.world.raw.bodies[*h].linvel())
        .collect();
    for (i, p) in start.iter().enumerate() {
        let mut end = from_rapier(*p + velocity[i] * 0.00001);
        if i == 0 {
            end.x += 0.001;
        }
        experiment
            .world
            .set_pose(body(i as u64 + 1), end, 0.37, true);
    }
    let before = experiment.snapshot();
    assert_eq!(
        retire(
            &mut experiment.world,
            &mut experiment.tracker,
            &plates,
            &start,
            &velocity,
            0.00001,
            0.00001
        ),
        Err(Refusal::Geometry)
    );
    assert_eq!(experiment.snapshot(), before);

    let (mut world, tokens) = fixture(&[4, 2], 2, true, 0.37, Vec2::ZERO, 0.5, false);
    world.set_pose(body(1), Vec2::ZERO, 0.37, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    let mut experiment = Verified {
        world,
        tracker: Tracker::default(),
    };
    experiment.step(DT, MAX_ADVANCES).unwrap();
    let episode = experiment
        .world
        .loading
        .as_ref()
        .unwrap()
        .episodes
        .values()
        .next()
        .unwrap()
        .id;
    let capacity = balances(&experiment.world, &tokens);
    for i in 0..64 {
        experiment.world.set_pose(
            body(1),
            rotate(Vec2::X * if i % 2 == 0 { 0.25e-6 } else { -0.25e-6 }, 0.37),
            0.37,
            true,
        );
        let trace = experiment.step(1.0 / 240.0, MAX_ADVANCES).unwrap();
        assert!(trace.departures.is_empty() && experiment.tracker.pending.is_empty());
        assert!(experiment.world.material_response_batches().is_empty());
        assert_eq!(balances(&experiment.world, &tokens), capacity);
        assert_eq!(
            experiment
                .world
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
}
