use super::*;

fn replayed(world: &mut PhysicsWorld, dt: f32) -> Trace {
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let trace = step(world, dt, MAX_ADVANCES).unwrap();
    assert_eq!(trace, step(&mut replay, dt, MAX_ADVANCES).unwrap());
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(world.loading_batches(), replay.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    trace
}

#[test]
fn ballistic_rounded_frame_endpoint_resolves_observed_arrival() {
    for (gap, dt) in [(0.0001, 0.0003996805), (0.02, 0.07015621), (1.0, 1.1861407)] {
        assert!(arrival(f64::from(gap), 0.25, 1.0) > f64::from(dt));
        let (mut world, _) = fixture(&[1, 1], 1, false, gap, 0.25, 1.0);
        world.raw.integration_parameters.num_solver_iterations = 1;
        let trace = replayed(&mut world, dt);
        assert_eq!(state(&world).0, 0.0);
        assert_eq!(trace.flights.len(), 1);
        assert!(trace.flights[0].predicted_x < 0.0);
        assert_eq!(trace.impacts.len(), 1);
        assert_eq!(trace.impacts[0].at, f64::from(dt));
        assert!(state(&world).1 < 0.0);
        assert_eq!(world.material_response_batches().len(), 1);
    }
}

#[test]
fn ballistic_free_flight_through_apex_preserves_actual_work() {
    for n in [1, 4, 8] {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for parts in [&[1, 1][..], &[4, 2]] {
                let (mut world, tokens) = fixture(parts, 2, true, 1.0, -2.0, 10.0);
                world.raw.integration_parameters.num_solver_iterations = n;
                let capacities = balances(&world, &tokens);
                let energy = mechanical(&world, 10.0);
                let mut elapsed = 0.0;
                let mut work = 0.0;
                let mut error = 0.0;
                loop {
                    let remaining = (0.3 - elapsed) as f32;
                    let dt = (1.0_f32 / hz).min(remaining);
                    let trace = replayed(&mut world, dt);
                    assert!(trace.impacts.is_empty());
                    for flight in trace.flights {
                        work += flight.gravity_work;
                        error += flight.work_error;
                    }
                    elapsed += f64::from(dt);
                    if dt == remaining {
                        break;
                    }
                }
                let (x, v, mass) = state(&world);
                near(x, -1.0 - 2.0 * elapsed + 5.0 * elapsed * elapsed, 4.0e-6);
                near(v, -2.0 + 10.0 * elapsed, 4.0e-6);
                near(work, mass * 10.0 * (x + 1.0), 1.0e-10);
                near(mechanical(&world, 10.0), energy, 1.0e-5 * energy);
                near(error, energy - mechanical(&world, 10.0), 1.0e-10);
                assert_eq!(balances(&world, &tokens), capacities);
                assert!(world.loading.as_ref().unwrap().episodes.is_empty());
                assert!(world.loading_batches().is_empty());
                assert_eq!(world.raw.gravity, Vector::X * 10.0);
            }
        }
    }
}

#[test]
fn ballistic_first_landing_matches_incoming_energy_and_rebound() {
    for speed in [0.0_f32, 40.0] {
        let gap = 0.02_f32;
        let incoming = libm::sqrt(f64::from(speed) * f64::from(speed) + 20.0 * f64::from(gap));
        let time = arrival(f64::from(gap), f64::from(speed), 10.0);
        // Independent zero-gravity contact at the analytically known incoming
        // speed, using ordinary stepping rather than the ballistic operation.
        let (mut reference, reference_tokens) =
            fixture(&[1, 1], 1, false, 0.0, incoming as f32, 0.0);
        reference.step(0.0001);
        let response = applied(&reference).clone();
        let expected_capacity = balances(&reference, &reference_tokens);
        let rebound = state(&reference).1;
        assert!(rebound < 0.0);
        let horizon = time + (-rebound / 10.0) * 0.5;
        for n in [1, 4, 8] {
            for hz in [30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1][..], &[4, 2]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut world, tokens) =
                                fixture(parts, copies, reverse, gap, speed, 10.0);
                            world.raw.integration_parameters.num_solver_iterations = n;
                            let initial = mechanical(&world, 10.0);
                            let mut elapsed = 0.0;
                            let mut impacts = Vec::new();
                            let mut gravity_work = 0.0;
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = (1.0_f32 / hz).min(remaining);
                                let trace = replayed(&mut world, dt);
                                for mut impact in trace.impacts {
                                    impact.at += elapsed;
                                    impacts.push(impact);
                                }
                                gravity_work +=
                                    trace.flights.iter().map(|f| f.gravity_work).sum::<f64>();
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(impacts.len(), 1);
                            let impact = &impacts[0];
                            near(impact.at, time, 2.0e-6);
                            near(f64::from(impact.incoming), incoming, 1.0e-5 * incoming);
                            near(f64::from(impact.outgoing), rebound, 1.0e-5 * rebound.abs());
                            near(loss(&impact.response), loss(&response), 1.0e-5 * initial);
                            for (actual, expected) in
                                balances(&world, &tokens).iter().zip(&expected_capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            if speed == 0.0 {
                                assert_eq!(balances(&world, &tokens), expected_capacity);
                                assert!(
                                    impact
                                        .response
                                        .impacts
                                        .iter()
                                        .all(|i| i.outcome.consumed == [0.0; 2])
                                );
                            } else {
                                assert!(
                                    impact
                                        .response
                                        .impacts
                                        .iter()
                                        .any(|i| i.outcome.consumed[1] > 0.0)
                                );
                            }
                            let (x, v, mass) = state(&world);
                            let after_time = elapsed - time;
                            near(
                                x,
                                rebound * after_time + 5.0 * after_time * after_time,
                                3.0e-6,
                            );
                            near(v, rebound + 10.0 * after_time, 1.0e-5);
                            near(
                                mechanical(&world, 10.0) + loss(&impact.response),
                                initial,
                                1.0e-5 * initial,
                            );
                            near(
                                0.5 * mass * v * v + loss(&impact.response),
                                0.5 * mass * f64::from(speed) * f64::from(speed) + gravity_work,
                                1.0e-5 * initial,
                            );
                            assert_eq!(world.loading.as_ref().unwrap().episodes.len(), 1);
                        }
                    }
                }
            }
        }
    }
}

fn refused(world: &mut PhysicsWorld, dt: f32, limit: usize, expected: Rejection) {
    let snapshot = world.snapshot_bytes().unwrap();
    let loading = world.loading_batches().to_vec();
    let responses = world.material_response_batches().to_vec();
    assert_eq!(step(world, dt, limit).unwrap_err(), expected);
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(world.loading_batches(), loading);
    assert_eq!(world.material_response_batches(), responses);
}

#[test]
fn ballistic_refusals_preserve_motion_capacity_and_prior_outputs() {
    let (mut world, _) = fixture(&[4, 2], 2, true, 0.0, 40.0, 10.0);
    replayed(&mut world, 0.001);
    assert_eq!(world.material_response_batches().len(), 1);
    refused(&mut world, -1.0, MAX_ADVANCES, Rejection::InvalidTime);
    refused(&mut world, f32::NAN, MAX_ADVANCES, Rejection::InvalidTime);
    refused(&mut world, 0.01, 0, Rejection::Limit);
    // No departure tracker exists yet: a later return must not mint a budget.
    refused(&mut world, 0.3, MAX_ADVANCES, Rejection::Reimpact);
    world.set_pose(body(1), Vec2::ZERO, 0.0, true);
    world.set_velocity(body(1), Vec2::ZERO, 0.0, true);
    refused(&mut world, 0.01, MAX_ADVANCES, Rejection::Support);
    let snapshot = world.snapshot_bytes().unwrap();
    assert_eq!(step(&mut world, 0.0, 0).unwrap(), Trace::default());
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert!(world.loading_batches().is_empty() && world.material_response_batches().is_empty());

    // A late failure rolls back an already-applied damaging arrival too.
    let (mut late, _) = fixture(&[1, 1], 1, false, 0.02, 40.0, 10.0);
    refused(&mut late, 0.01, 1, Rejection::Limit);
    // A gravity kick smaller than the stored velocity can represent is refused.
    let (mut tiny, _) = fixture(&[1, 1], 1, false, 1.0, -40.0, 10.0);
    refused(&mut tiny, 1.0e-9, MAX_ADVANCES, Rejection::Energy);
    // Already-touching approaches below capture's threshold still need accounting.
    let (mut slow, _) = fixture(&[1, 1], 1, false, 0.0, 0.00005, 10.0);
    refused(
        &mut slow,
        0.01,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Unaccounted),
    );
}

#[test]
fn ballistic_frame_endpoint_resolves_before_pause_and_continuation() {
    let (mut world, tokens) = fixture(&[4, 2], 2, true, 0.5, 0.0, 16.0);
    let initial = mechanical(&world, 16.0);
    let capacity = balances(&world, &tokens);
    let trace = replayed(&mut world, 0.25);
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.impacts[0].at, 0.25);
    assert_eq!(trace.impacts[0].incoming, 4.0);
    assert_eq!(state(&world).0, 0.0);
    assert_eq!(world.loading_batches().len(), 1);
    assert_eq!(world.loading_batches()[0].duration, 0.0);
    let damaged = balances(&world, &tokens);
    for i in 0..2 {
        near(
            capacity[i] - damaged[i],
            f64::from(trace.impacts[0].response.impacts[0].outcome.consumed[i]),
            1.0e-5,
        );
    }
    assert!(damaged[1] < capacity[1]);
    let dissipated = loss(&trace.impacts[0].response);
    near(
        mechanical(&world, 16.0) + dissipated,
        initial,
        1.0e-5 * initial,
    );
    let snapshot = world.snapshot_bytes().unwrap();
    assert_eq!(step(&mut world, 0.0, 0).unwrap(), Trace::default());
    assert_eq!(world.snapshot_bytes().unwrap(), snapshot);
    assert!(world.loading_batches().is_empty() && world.material_response_batches().is_empty());
    let next = replayed(&mut world, 0.01);
    assert!(next.impacts.is_empty());
    assert_eq!(balances(&world, &tokens), damaged);
    near(
        mechanical(&world, 16.0) + dissipated,
        initial,
        1.0e-5 * initial,
    );
}
