use super::*;

fn verified(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    speed: f32,
    gravity: f32,
) -> (Verified, Vec<RegionToken>) {
    let (world, tokens) = fixture(parts, copies, reverse, 0.0, speed, gravity);
    (
        Verified {
            world,
            tracker: Tracker::default(),
        },
        tokens,
    )
}

fn replayed(scene: &mut Verified, dt: f32) -> Trace {
    let mut replay = Verified::restore(&scene.snapshot());
    let trace = scene.step(dt, MAX_ADVANCES).unwrap();
    assert_eq!(trace, replay.step(dt, MAX_ADVANCES).unwrap());
    assert_eq!(scene.snapshot(), replay.snapshot());
    assert_eq!(
        scene.world.loading_batches(),
        replay.world.loading_batches()
    );
    assert_eq!(
        scene.world.material_response_batches(),
        replay.world.material_response_batches()
    );
    trace
}

fn refused(scene: &mut Verified, dt: f32, limit: usize, expected: Rejection) {
    let bytes = scene.snapshot();
    let loading = scene.world.loading_batches().to_vec();
    let responses = scene.world.material_response_batches().to_vec();
    assert_eq!(scene.step(dt, limit).unwrap_err(), expected);
    assert_eq!(scene.snapshot(), bytes);
    assert_eq!(scene.world.loading_batches(), loading);
    assert_eq!(scene.world.material_response_batches(), responses);
}

#[test]
fn ballistic_departure_rearms_returns_with_finite_work() {
    for speed in [0.5, 4.0] {
        // Independent contact sequence: ballistic return to the same height
        // has the preceding rebound's speed. Carry material capacity into a
        // fresh ordinary zero-gravity contact, without using the tested tracker.
        let mut saved = None;
        let mut incoming = speed;
        let mut at = 0.0;
        let mut expected = Vec::new();
        let mut expected_capacity = Vec::new();
        for _ in 0..3 {
            let (mut reference, tokens) = fixture(&[1, 1], 1, false, 0.0, incoming, 0.0);
            if let Some(materials) = saved.take() {
                reference
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials = materials;
            }
            reference.step(0.0001);
            let response = applied(&reference).clone();
            let outgoing = state(&reference).1;
            assert!(outgoing < 0.0);
            expected.push((at, incoming, outgoing, loss(&response)));
            expected_capacity = balances(&reference, &tokens);
            saved = Some(
                reference
                    .loading
                    .as_ref()
                    .unwrap()
                    .response
                    .as_ref()
                    .unwrap()
                    .materials
                    .clone(),
            );
            at += -2.0 * outgoing / 10.0;
            incoming = -outgoing as f32;
        }
        let horizon = expected[2].0 - expected[2].2 / 20.0;
        for n in [1, 4, 8] {
            for hz in [0.0, 30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1][..], &[4, 2]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut scene, tokens) = verified(parts, copies, reverse, speed, 10.0);
                            scene.world.raw.integration_parameters.num_solver_iterations = n;
                            let initial = mechanical(&scene.world, 10.0);
                            let mut elapsed = 0.0;
                            let mut impacts = Vec::new();
                            let mut events = Vec::new();
                            let mut work = 0.0;
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = if hz == 0.0 {
                                    remaining
                                } else {
                                    (1.0_f32 / hz).min(remaining)
                                };
                                let trace = replayed(&mut scene, dt);
                                work += trace.flights.iter().map(|f| f.gravity_work).sum::<f64>();
                                for mut impact in trace.impacts {
                                    impact.at += elapsed;
                                    impacts.push(impact);
                                }
                                for mut event in trace.events {
                                    event.at += elapsed;
                                    events.push(event);
                                }
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(impacts.len(), 3);
                            assert_eq!(events.len(), 6);
                            for (i, (impact, expected)) in impacts.iter().zip(&expected).enumerate()
                            {
                                near(impact.at, expected.0, 3.0e-6);
                                near(
                                    f64::from(impact.incoming),
                                    f64::from(expected.1),
                                    1.0e-5 * f64::from(expected.1),
                                );
                                near(
                                    f64::from(impact.outgoing),
                                    expected.2,
                                    1.0e-5 * expected.2.abs(),
                                );
                                near(loss(&impact.response), expected.3, 1.0e-5 * initial);
                                assert_eq!(impact.response.roots, vec![events[2 * i].event.id]);
                                assert_eq!(
                                    events[2 * i].event.transition,
                                    EpisodeTransition::Impact
                                );
                                assert_eq!(
                                    events[2 * i + 1].event.transition,
                                    EpisodeTransition::End
                                );
                                assert_eq!(events[2 * i].event.id, events[2 * i + 1].event.id);
                                assert_eq!(
                                    events[2 * i].event.sources,
                                    events[2 * i + 1].event.sources
                                );
                                assert!(events[2 * i].at < events[2 * i + 1].at);
                                if i > 0 {
                                    assert!(events[2 * i - 1].at <= events[2 * i].at);
                                    assert!(events[2 * i - 1].event.id < events[2 * i].event.id);
                                }
                            }
                            let spent: f64 = impacts.iter().map(|i| loss(&i.response)).sum();
                            let (x, v, mass) = state(&scene.world);
                            near(
                                mechanical(&scene.world, 10.0) + spent,
                                initial,
                                1.0e-5 * initial,
                            );
                            near(0.5 * mass * v * v + spent, initial + work, 1.0e-5 * initial);
                            near(work, mass * 10.0 * x, 1.0e-10);
                            assert!(spent <= initial);
                            for (actual, expected) in balances(&scene.world, &tokens)
                                .iter()
                                .zip(&expected_capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            assert!(scene.world.loading.as_ref().unwrap().episodes.is_empty());
                            assert!(scene.tracker.pending.is_none());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_departure_preserves_pending_evidence_and_rejects_edits() {
    let (mut scene, tokens) = verified(&[4, 2], 2, true, 0.5, 10.0);
    let first = replayed(&mut scene, 0.000001);
    assert_eq!(first.impacts.len(), 1);
    assert_eq!(first.events.len(), 1);
    assert!(scene.tracker.pending.is_some());
    let bytes = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), bytes);
    assert!(scene.world.loading_batches().is_empty());
    assert!(scene.world.material_response_batches().is_empty());
    let capacity = balances(&scene.world, &tokens);
    for edit in 0..5 {
        let mut edited = Verified::restore(&bytes);
        match edit {
            0 => edited
                .world
                .set_pose(body(1), Vec2::new(-0.01, 0.0), 0.0, true),
            1 => edited
                .world
                .set_velocity(body(1), Vec2::X * -0.1, 0.0, true),
            2 => {
                edited.world.set_gravity(Vec2::X * 11.0);
                true
            }
            3 => {
                edited.tracker.pending.as_mut().unwrap().episode = EpisodeId(999);
                true
            }
            _ => {
                let id = collider(1, 0);
                let source = edited.world.loading.as_ref().unwrap().bindings[&id];
                assert!(edited.world.retire_loading_region(source.region));
                true
            }
        };
        refused(&mut edited, 0.001, MAX_ADVANCES, Rejection::Evidence);
    }
    let continued = replayed(&mut scene, 0.001);
    assert_eq!(continued.events.len(), 1);
    assert_eq!(continued.events[0].event.transition, EpisodeTransition::End);
    assert_eq!(balances(&scene.world, &tokens), capacity);
}

#[test]
fn ballistic_departure_limits_roll_back_end_and_damage() {
    let (mut scene, _) = verified(&[4, 2], 2, true, 4.0, 10.0);
    let original = scene.snapshot();
    // A full call would apply damage and retire its episode at the apex, then
    // return. The second drift exceeds this limit: all staged changes roll back.
    refused(&mut scene, 0.2, 1, Rejection::Limit);
    let first = replayed(&mut scene, 0.000001);
    assert_eq!(first.impacts.len(), 1);
    let records = scene.world.loading.as_ref().unwrap().records;
    let mut limited = Verified::restore(&original);
    limited
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = records;
    refused(
        &mut limited,
        0.001,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );
    scene
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = 1;
    refused(
        &mut scene,
        0.2,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );
}

#[test]
fn ballistic_departure_resolves_the_previously_uncertain_crossing() {
    let (mut scene, tokens) = verified(&[1, 1], 1, false, 0.5, 10.0);
    let mut coarse = Verified::restore(&scene.snapshot());
    let capacity = balances(&scene.world, &tokens);
    let first = 0.000002_f32;
    let initial = replayed(&mut scene, first);
    let owner = initial.impacts[0].response.roots[0];
    // About 44.614077 microseconds: retain the exact f32 boundary reproduction.
    let duration = f32::from_bits(0x383b2000);
    let crossed = replayed(&mut scene, duration);
    assert_eq!(crossed.events.len(), 1);
    assert_eq!(crossed.events[0].event.id, owner);
    assert_eq!(crossed.events[0].event.transition, EpisodeTransition::End);
    assert!(scene.tracker.pending.is_none());
    let flight = &crossed.flights[0];
    let exit = 2.0 * f64::from(scene.world.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(-state(&scene.world).0 > exit + flight.uncertainty);
    // The old unit-size allowance straddled this crossing. A bound based on
    // the actual drift scale proves it at this earlier endpoint.
    assert!(-flight.predicted_x > exit + flight.uncertainty);
    let snapshot = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), snapshot);
    scene = Verified::restore(&snapshot);
    let end = replayed(&mut scene, 0.0001);
    assert!(end.events.is_empty());
    let remaining = (0.1 - f64::from(first) - f64::from(duration) - f64::from(0.0001_f32)) as f32;
    let returned = replayed(&mut scene, remaining);
    assert_eq!(returned.impacts.len(), 1);
    assert!(returned.impacts[0].response.roots[0] > owner);
    let comparison = replayed(&mut coarse, 0.1);
    assert_eq!(comparison.impacts.len(), 2);
    let (x, v, _) = state(&scene.world);
    near(x, state(&coarse.world).0, 3.0e-6);
    near(v, state(&coarse.world).1, 3.0e-6);
    assert_eq!(balances(&scene.world, &tokens), capacity);
    assert_eq!(balances(&coarse.world, &tokens), capacity);
}

#[test]
fn ballistic_departure_short_excursion_cannot_create_another_budget() {
    let (mut scene, _) = verified(&[1, 1], 1, false, 0.5, 10000.0);
    // The apex remains inside the exit band. Observing a turn or spending time
    // away from contact alone cannot retire the episode and grant a new root.
    refused(&mut scene, 0.001, MAX_ADVANCES, Rejection::Reimpact);
}
