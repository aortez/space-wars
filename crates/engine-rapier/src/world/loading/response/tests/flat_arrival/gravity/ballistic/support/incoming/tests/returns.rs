use super::*;

#[test]
fn ballistic_supported_returns_measure_tiny_support_intervals_after_larger_ones() {
    let (mut scene, tokens) = verified(&[4, 2, 4], 2, true, 0.02, 0.5, 10.0);
    let capacity = balances(&scene.world, &tokens);
    replayed(&mut scene, 0.01);
    let dt = f32::from_bits(0x30000000); // 2^-31 seconds.
    let trace = replayed(&mut scene, dt);
    assert!(trace.impacts.is_empty() && trace.events.is_empty());
    assert_eq!(trace.flights.len(), 1);
    assert_eq!(trace.flights[0].duration, dt);
    assert_eq!(trace.supports.len(), 1);
    let support = &trace.supports[0];
    let expected = 200.0 * f64::from(dt);
    assert!(support.impulse > 0.0);
    near(
        support.impulse + support.rounding_impulse,
        expected,
        1.0e-5 * expected,
    );
    assert_eq!(support.gravity_work, 0.0);
    assert_eq!(support.kick_energy, support.projection_energy);
    assert_eq!(balances(&scene.world, &tokens), capacity);
}

#[test]
fn ballistic_supported_returns_retain_a_rounded_departure_until_proven() {
    let (mut scene, tokens) = verified(&[1, 1, 1], 1, false, 0.0, 0.6, 10.0);
    scene.world.raw.integration_parameters.num_solver_iterations = 1;
    let mut coarse = Verified::restore(&scene.snapshot());
    let capacity = balances(&scene.world, &tokens);
    let first = 2.0e-6_f32;
    let trace = replayed(&mut scene, first);
    let owner = trace.impacts[0].response.roots[0];
    let old_support = scene.witness.support;
    let duration = f32::from_bits(0x3811c540);
    let trace = replayed(&mut scene, duration);
    assert!(trace.events.is_empty());
    assert_eq!(scene.witness.incoming, Some(owner));
    let flight = &trace.flights[0];
    let exit = 2.0 * f64::from(scene.world.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(-state(&scene.world).0 > exit + flight.uncertainty);
    assert!(-flight.predicted_x <= exit + flight.uncertainty);
    let bytes = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), bytes);
    scene = Verified::restore(&bytes);
    let trace = replayed(&mut scene, 0.0001);
    assert_eq!(trace.events.len(), 1);
    assert_eq!(trace.events[0].event.id, owner);
    assert_eq!(trace.events[0].event.transition, EpisodeTransition::End);
    assert!(scene.witness.incoming.is_none());
    let remaining = (0.12 - f64::from(first) - f64::from(duration) - f64::from(0.0001_f32)) as f32;
    let returned = replayed(&mut scene, remaining);
    assert_eq!(returned.impacts.len(), 1);
    assert!(returned.impacts[0].response.roots[0] > owner);
    assert_eq!(returned.impacts[0].response.impacts[1].episode, old_support);
    assert_eq!(replayed(&mut coarse, 0.12).impacts.len(), 2);
    near(state(&scene.world).0, state(&coarse.world).0, 3.0e-6);
    near(state(&scene.world).1, state(&coarse.world).1, 3.0e-6);
    assert_eq!(balances(&scene.world, &tokens), capacity);
    assert_eq!(balances(&coarse.world, &tokens), capacity);
}

#[test]
fn ballistic_supported_returns_roll_back_late_limits_and_reject_stale_owners() {
    let (mut scene, tokens) = verified(&[4, 2, 4], 2, true, 0.0, 4.0, 10.0);
    let first = replayed(&mut scene, 0.000001);
    assert_eq!(first.events.len(), 1);
    let bytes = scene.snapshot();
    let owner = scene.witness.incoming.unwrap();
    let (x, v, _) = state(&scene.world);
    let duration = (arrival(-x, v, 10.0) + 0.001) as f32;
    let mut complete = Verified::restore(&bytes);
    let trace = replayed(&mut complete, duration);
    assert_eq!(trace.impacts.len(), 1);
    assert!(trace.impacts[0].response.roots[0] > owner);
    assert_eq!(trace.events[0].event.transition, EpisodeTransition::End);
    assert_eq!(trace.events[0].event.id, owner);
    assert!(loss(&trace.impacts[0].response) > 0.0);
    // The apex retires the old arrival; the second flight applies the new
    // impact. The required outbound interval then exceeds this limit.
    refused(&mut scene, duration, 2, Rejection::Limit);
    let mut limited = Verified::restore(&bytes);
    limited
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = complete.world.loading.as_ref().unwrap().records - 1;
    refused(
        &mut limited,
        duration,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );
    for edit in 0..4 {
        let mut changed = Verified::restore(&bytes);
        match edit {
            0 => changed.witness.incoming = Some(EpisodeId(999)),
            1 => changed.witness.incoming = None,
            2 => {
                changed.world.set_pose(body(1), -Vec2::X * 0.001, 0.0, true);
            }
            _ => {
                changed.world.retire_loading_region(tokens[0]);
            }
        }
        refused(&mut changed, duration, MAX_ADVANCES, Rejection::Evidence);
    }
}

#[test]
fn ballistic_supported_returns_cannot_rebudget_a_contained_hop_or_hold_a_stack() {
    let (mut scene, _) = verified(&[1, 1, 1], 1, false, 0.0, 0.01, 10.0);
    replayed(&mut scene, 0.000001);
    let (x, v, _) = state(&scene.world);
    assert!(v < 0.0);
    let exit = 2.0 * f64::from(scene.world.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(-x + v * v / 20.0 < exit);
    refused(&mut scene, 0.01, MAX_ADVANCES, Rejection::Reimpact);
    let (mut rest, _) = verified(&[1, 1, 1], 1, false, 0.0, 0.0, 10.0);
    refused(&mut rest, 0.01, MAX_ADVANCES, Rejection::Support);
}

#[test]
fn ballistic_supported_returns_account_for_rounded_endpoint_arrivals() {
    for (gap, dt) in [(0.0001, 0.0003996805), (0.02, 0.07015621), (1.0, 1.1861407)] {
        let (mut scene, _) = verified(&[1, 1, 1], 1, false, gap, 0.25, 1.0);
        scene.world.raw.integration_parameters.num_solver_iterations = 1;
        assert!(arrival(f64::from(gap), 0.25, 1.0) > f64::from(dt));
        let trace = replayed(&mut scene, dt);
        assert_eq!(trace.impacts.len(), 1);
        assert_eq!(trace.impacts[0].at, f64::from(dt));
        assert_eq!(trace.flights.len(), 1);
        assert!(state(&scene.world).1 < 0.0);
        let owner = scene.witness.incoming.unwrap();
        assert_eq!(trace.impacts[0].response.roots, vec![owner]);
        let next = replayed(&mut scene, 0.000001);
        assert!(next.impacts.is_empty());
        assert_eq!(scene.witness.incoming, Some(owner));
    }
}

#[test]
fn ballistic_supported_returns_preserve_work_and_base_ownership() {
    for speed in [0.5_f32, 4.0] {
        // Independent instantaneous contacts carry only material state forward.
        // The return speed/time come from ballistic motion to the same height,
        // without consulting the departure or scheduling code under test.
        let mut saved = None;
        let mut incoming = speed;
        let mut at = 0.0;
        let mut expected = Vec::new();
        let mut expected_capacity = Vec::new();
        for _ in 0..3 {
            let (mut world, tokens) = fixture(&[1, 1, 1], 1, false, 0.0, incoming, 0.0);
            if let Some(materials) = saved.take() {
                world
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials = materials;
            }
            world.step(0.00001);
            let response = applied(&world).clone();
            let outgoing = state(&world).1;
            assert!(outgoing < 0.0);
            expected.push((at, incoming, outgoing, loss(&response)));
            expected_capacity = balances(&world, &tokens);
            saved = Some(
                world
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
            for hz in [0.0_f32, 30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut scene, tokens) =
                                verified(parts, copies, reverse, 0.0, speed, 10.0);
                            scene.world.raw.integration_parameters.num_solver_iterations = n;
                            let old = scene.witness.support;
                            let initial = mechanical(&scene.world, 10.0);
                            let scales = (
                                scene.witness.rounding.energy_scale,
                                scene.witness.rounding.momentum_scale,
                                scene.witness.rounding.distance_scale,
                            );
                            let mut elapsed = 0.0;
                            let mut work = 0.0;
                            let mut numerical = [0.0; 3];
                            let mut reaction = 0.0;
                            let mut impacts = Vec::new();
                            let mut events = Vec::new();
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = if hz == 0.0 {
                                    remaining
                                } else {
                                    remaining.min(1.0 / hz)
                                };
                                let trace = replayed(&mut scene, dt);
                                assert_eq!(
                                    trace
                                        .flights
                                        .iter()
                                        .map(|f| f64::from(f.duration))
                                        .sum::<f64>(),
                                    f64::from(dt),
                                );
                                assert!(trace.settlements.is_empty());
                                assert_eq!(trace.flights.len(), trace.supports.len());
                                for (flight, support) in trace.flights.iter().zip(&trace.supports) {
                                    work += flight.gravity_work;
                                    for (total, error) in numerical.iter_mut().zip([
                                        flight.work_error,
                                        flight.impulse_error,
                                        flight.position_error,
                                    ]) {
                                        *total += error;
                                    }
                                    reaction += support.impulse + support.rounding_impulse;
                                    assert_eq!(support.episode, old);
                                    assert_eq!(support.duration, flight.duration);
                                    assert_eq!(support.force, 200.0);
                                    assert_eq!(support.gravity_work, 0.0);
                                    assert_eq!(support.kick_energy, support.projection_energy);
                                }
                                assert_eq!(
                                    numerical,
                                    [
                                        scene.witness.rounding.work_error,
                                        scene.witness.rounding.impulse_error,
                                        scene.witness.rounding.position_error,
                                    ],
                                );
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
                                let impact_event = &events[2 * i];
                                let end = &events[2 * i + 1];
                                assert_eq!(
                                    impact_event.event.transition,
                                    EpisodeTransition::Impact
                                );
                                assert_eq!(end.event.transition, EpisodeTransition::End);
                                assert_eq!(end.event.id, impact_event.event.id);
                                assert_eq!(end.event.sources, impact_event.event.sources);
                                assert!(impact_event.at < end.at);
                                assert_eq!(impact.response.roots, vec![impact_event.event.id]);
                                assert_ne!(impact_event.event.id, old);
                                assert_eq!(impact.response.impacts[1].episode, old);
                                assert_eq!(impact.response.budget.contacts[1].closing_speed, 0.0);
                                assert!(impact.response.budget.contacts[1].energy > 0.0);
                                assert_eq!(impact.response.prescribed_work, 0.0);
                                near(
                                    impact.response.budget.components[0].available,
                                    10.0 * f64::from(impact.incoming).powi(2),
                                    1.0e-5 * initial,
                                );
                                if i > 0 {
                                    assert!(events[2 * i - 1].at <= impact_event.at);
                                    assert!(events[2 * i - 1].event.id < impact_event.event.id);
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
                            near(reaction, 200.0 * elapsed, 1.0e-5 * 200.0 * elapsed);
                            let impact_reaction: f64 = impacts
                                .iter()
                                .map(|i| f64::from(i.response.impacts[1].normal_impulse))
                                .sum();
                            near(
                                mass * (v - f64::from(speed)) + reaction + impact_reaction,
                                400.0 * elapsed,
                                1.0e-5 * mass * f64::from(speed),
                            );
                            assert!(spent <= initial);
                            for (actual, expected) in balances(&scene.world, &tokens)
                                .iter()
                                .zip(&expected_capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            assert_eq!(scene.world.loading.as_ref().unwrap().episodes.len(), 1);
                            assert_eq!(
                                scene.world.loading.as_ref().unwrap().episodes
                                    [&[scene.witness.sources[1], scene.witness.sources[2]]]
                                    .id,
                                old
                            );
                            assert!(scene.witness.incoming.is_none());
                            assert_eq!(
                                scales,
                                (
                                    scene.witness.rounding.energy_scale,
                                    scene.witness.rounding.momentum_scale,
                                    scene.witness.rounding.distance_scale
                                )
                            );
                        }
                    }
                }
            }
        }
    }
}
