use super::*;

fn verified(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    gap: f32,
    speed: f32,
    gravity: f32,
) -> (Verified, Vec<RegionToken>) {
    let (world, tokens) = fixture(parts, copies, reverse, gap, speed, gravity);
    (Verified::new(world).unwrap(), tokens)
}

fn replayed(scene: &mut Verified, dt: f32) -> Trace {
    let mut replay = Verified::restore(&scene.snapshot());
    let trace = scene.step(dt, MAX_ADVANCES).unwrap_or_else(|e| {
        panic!(
            "dt={dt} g={} state={:?}: {e:?}",
            scene.witness.gravity,
            state(&scene.world)
        )
    });
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

fn reference(speed: f32) -> (AppliedResponse, Vec<f64>, f64) {
    let (mut world, tokens) = fixture(&[1, 1, 1], 1, false, 0.0, speed, 0.0);
    world.step(0.00001);
    (
        applied(&world).clone(),
        balances(&world, &tokens),
        state(&world).1,
    )
}

#[test]
fn ballistic_supported_arrival_preserves_shared_work_and_old_owner() {
    for speed in [0.5_f32, 4.0] {
        let gap = 0.02_f32;
        let arrival_speed = libm::sqrt(f64::from(speed).powi(2) + 20.0 * f64::from(gap));
        let arrival_at = arrival(f64::from(gap), f64::from(speed), 10.0);
        let (expected, expected_capacity, expected_outgoing) = reference(arrival_speed as f32);
        let horizon = arrival_at + 0.005;
        for n in [1, 4, 8] {
            for hz in [30.0_f32, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut scene, tokens) =
                                verified(parts, copies, reverse, gap, speed, 10.0);
                            scene.world.raw.integration_parameters.num_solver_iterations = n;
                            let initial = mechanical(&scene.world, 10.0);
                            let old = scene.witness.support;
                            let mut elapsed = 0.0;
                            let mut work = 0.0;
                            let mut support_impulse = 0.0;
                            let mut impacts = Vec::new();
                            let mut events = Vec::new();
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = (1.0 / hz).min(remaining);
                                let trace = replayed(&mut scene, dt);
                                assert!(trace.settlements.is_empty());
                                assert_eq!(trace.flights.len(), trace.supports.len());
                                for (flight, support) in trace.flights.iter().zip(&trace.supports) {
                                    assert_eq!(flight.duration, support.duration);
                                    assert_eq!(support.episode, old);
                                    assert_eq!(support.force, 200.0);
                                    assert_eq!(support.gravity_work, 0.0);
                                    assert_eq!(support.kick_energy, support.projection_energy);
                                    support_impulse += support.impulse + support.rounding_impulse;
                                    work += flight.gravity_work;
                                }
                                for mut impact in trace.impacts {
                                    impact.at += elapsed;
                                    impacts.push(impact);
                                }
                                events.extend(trace.events);
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(impacts.len(), 1);
                            assert_eq!(events.len(), 1);
                            let impact = &impacts[0];
                            let response = &impact.response;
                            near(impact.at, arrival_at, 2.0e-6);
                            near(
                                f64::from(impact.incoming),
                                arrival_speed,
                                1.0e-5 * arrival_speed,
                            );
                            near(
                                f64::from(impact.outgoing),
                                expected_outgoing,
                                1.0e-5 * arrival_speed,
                            );
                            assert_eq!(events[0].event.transition, EpisodeTransition::Impact);
                            assert_eq!(response.roots, vec![events[0].event.id]);
                            assert_ne!(events[0].event.id, old);
                            assert_eq!(response.impacts[1].episode, old);
                            assert_eq!(response.budget.contacts[1].closing_speed, 0.0);
                            assert!(response.budget.contacts[1].energy > 0.0);
                            assert_eq!(response.prescribed_work, 0.0);
                            for (actual, expected) in response.impacts.iter().zip(&expected.impacts)
                            {
                                assert_eq!(actual.sources, expected.sources);
                                near(f64::from(actual.geometry.width), 2.0, 1.0e-6);
                                near(
                                    f64::from(actual.outcome.ledger.available),
                                    f64::from(expected.outcome.ledger.available),
                                    1.0e-5 * initial,
                                );
                                for side in 0..2 {
                                    near(
                                        f64::from(actual.outcome.consumed[side]),
                                        f64::from(expected.outcome.consumed[side]),
                                        1.0e-5 * initial,
                                    );
                                }
                            }
                            let (x, v, mass) = state(&scene.world);
                            near(
                                mechanical(&scene.world, 10.0) + loss(response),
                                initial,
                                1.0e-5 * initial,
                            );
                            near(
                                0.5 * mass * v * v + loss(response),
                                0.5 * mass * f64::from(speed).powi(2) + work,
                                1.0e-5 * initial,
                            );
                            near(work, mass * 10.0 * (x + f64::from(gap)), 1.0e-10);
                            near(support_impulse, 200.0 * elapsed, 1.0e-5 * 200.0 * elapsed);
                            // Momentum includes gravity on both dynamic bodies and
                            // two distinct reactions at the base: weight and impact.
                            let impact_reaction = f64::from(response.impacts[1].normal_impulse);
                            near(
                                mass * (v - f64::from(speed)) + support_impulse + impact_reaction,
                                400.0 * elapsed,
                                1.0e-5 * mass * arrival_speed,
                            );
                            for (actual, expected) in balances(&scene.world, &tokens)
                                .iter()
                                .zip(&expected_capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            let plates = Plates::new(&scene.world).unwrap();
                            assert_eq!(motion(&scene.world, &plates)[1], (1.0, 0.0, 20.0));
                            assert_eq!(
                                scene.world.loading.as_ref().unwrap().episodes
                                    [&[scene.witness.sources[1], scene.witness.sources[2]]]
                                    .id,
                                old
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_supported_arrival_at_frame_end_preserves_pause_and_support() {
    // Exactly representable time/velocity/position under a=8.
    let (mut scene, tokens) = verified(&[4, 2, 4], 2, true, 0.015625, 0.0, 8.0);
    let old = scene.witness.support;
    let trace = replayed(&mut scene, 0.0625);
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.impacts[0].at, 0.0625);
    assert_eq!(trace.impacts[0].incoming, 0.5);
    let capacity = balances(&scene.world, &tokens);
    let bytes = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), bytes);
    assert!(scene.world.loading_batches().is_empty());
    assert!(scene.world.material_response_batches().is_empty());
    let trace = replayed(&mut scene, 0.005);
    assert!(trace.impacts.is_empty());
    assert!(trace.events.is_empty());
    assert!(trace.supports.iter().all(|s| s.episode == old));
    assert_eq!(balances(&scene.world, &tokens), capacity);
}

#[test]
fn ballistic_supported_arrival_limits_failure_and_stale_evidence_roll_back() {
    let (mut scene, tokens) = verified(&[4, 2, 4], 2, true, 0.002, 4.0, 10.0);
    let original = scene.snapshot();
    let capacity = balances(&scene.world, &tokens);
    refused(&mut scene, 0.01, 1, Rejection::Limit);
    refused(&mut scene, 1.0, MAX_ADVANCES, Rejection::Reimpact);
    refused(&mut scene, -1.0, MAX_ADVANCES, Rejection::InvalidTime);
    refused(&mut scene, f32::NAN, MAX_ADVANCES, Rejection::InvalidTime);
    replayed(&mut scene, 0.01);
    assert_ne!(balances(&scene.world, &tokens), capacity);
    let mut limited = Verified::restore(&original);
    limited
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = scene.world.loading.as_ref().unwrap().records - 1;
    refused(
        &mut limited,
        0.01,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );
    let mut failed = Verified::restore(&original);
    failed
        .world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .get_mut(&tokens[2])
        .unwrap()
        .remaining = 0.001;
    refused(&mut failed, 0.01, MAX_ADVANCES, Rejection::Support);
    let (mut overloaded, _) = verified(&[1, 1, 1], 1, false, 0.002, 4.0, 10_000.0);
    refused(&mut overloaded, 0.01, MAX_ADVANCES, Rejection::Support);
    for edit in 0..5 {
        let mut changed = Verified::restore(&original);
        match edit {
            0 => {
                changed.world.set_velocity(body(1), Vec2::X, 0.0, true);
            }
            1 => {
                changed.world.set_gravity(Vec2::X * 11.0);
            }
            2 => {
                assert!(changed.world.retire_loading_region(tokens[1]));
            }
            3 => {
                changed.witness.support = EpisodeId(999);
            }
            _ => {
                changed.world.set_pose(body(1), -Vec2::X * 0.003, 0.0, true);
            }
        }
        refused(&mut changed, 0.01, MAX_ADVANCES, Rejection::Evidence);
    }
}

#[test]
fn ballistic_supported_arrival_budget_ignores_support_history() {
    let (expected, capacity, _) = reference(2.0);
    for gravity in [1.0_f32, 10.0, 100.0] {
        for speed in [0.0_f32, 1.0, 1.9] {
            for hz in [0.0_f32, 120.0] {
                let gap = (4.0 - speed * speed) / (2.0 * gravity);
                let (mut scene, tokens) = verified(&[4, 2, 4], 2, true, gap, speed, gravity);
                let at = arrival(f64::from(gap), f64::from(speed), f64::from(gravity));
                let mut elapsed = 0.0;
                let mut impacts = Vec::new();
                let mut reaction = 0.0;
                loop {
                    let remaining = (at + 0.0005 - elapsed) as f32;
                    let dt = if hz == 0.0 {
                        remaining
                    } else {
                        remaining.min(1.0 / hz)
                    };
                    let trace = replayed(&mut scene, dt);
                    reaction += trace
                        .supports
                        .iter()
                        .map(|s| s.impulse + s.rounding_impulse)
                        .sum::<f64>();
                    impacts.extend(trace.impacts);
                    elapsed += f64::from(dt);
                    if dt == remaining {
                        break;
                    }
                }
                assert_eq!(impacts.len(), 1);
                let response = &impacts[0].response;
                near(response.budget.components[0].available, 40.0, 4.0e-4);
                near(loss(response), loss(&expected), 4.0e-4);
                for (actual, expected) in balances(&scene.world, &tokens).iter().zip(&capacity) {
                    near(*actual, *expected, 4.0e-4);
                }
                near(
                    reaction,
                    20.0 * f64::from(gravity) * elapsed,
                    1.0e-5 * reaction,
                );
            }
        }
    }
}

#[test]
fn ballistic_supported_arrival_accounts_for_an_unrepresentable_frame_remainder() {
    // Endpoint observed during a two-second fall at 120 Hz under g=1.
    // The next frame has about 12 ns left after impact; neither half-kick
    // can change the rebound velocity. Account for both work and impulse error.
    let gap = 0.016631931_f32;
    let speed = 1.9916679_f32;
    let (mut scene, _) = verified(&[4, 2, 4], 2, true, gap, speed, 1.0);
    let mut split = Verified::restore(&scene.snapshot());
    let initial_scale = scene.witness.rounding.energy_scale;
    let dt = 1.0_f32 / 120.0;
    let trace = replayed(&mut scene, dt);
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.flights.len(), 2);
    assert_eq!(
        trace
            .flights
            .iter()
            .map(|f| f64::from(f.duration))
            .sum::<f64>(),
        f64::from(dt)
    );
    let tail = &trace.flights[1];
    assert!(tail.duration < 2.0e-8);
    assert_eq!(state(&scene.world).1, f64::from(trace.impacts[0].outgoing));
    assert_eq!(tail.impulse_error, 20.0 * f64::from(tail.duration));
    assert_ne!(tail.work_error, 0.0);
    assert_eq!(
        scene.witness.rounding.work_error,
        trace.flights.iter().map(|f| f.work_error).sum::<f64>()
    );
    assert_eq!(
        scene.witness.rounding.impulse_error,
        trace.flights.iter().map(|f| f.impulse_error).sum::<f64>()
    );
    assert_eq!(scene.witness.rounding.energy_scale, initial_scale);
    let bytes = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), bytes);
    let toi = arrival(f64::from(gap), f64::from(speed), 1.0) as f32;
    assert_eq!(replayed(&mut split, toi).impacts.len(), 1);
    assert!(replayed(&mut split, dt - toi).impacts.is_empty());
    assert_eq!(state(&scene.world), state(&split.world));
    assert_eq!(scene.witness.rounding, split.witness.rounding);
    let rounding = scene.witness.rounding.clone();
    let trace = replayed(&mut scene, 0.0005);
    assert!(trace.impacts.is_empty());
    assert_eq!(trace.supports.len(), 1);
    assert_eq!(
        scene.witness.rounding.work_error,
        rounding.work_error + trace.flights[0].work_error
    );
    assert_eq!(
        scene.witness.rounding.impulse_error,
        rounding.impulse_error + trace.flights[0].impulse_error
    );
    assert_eq!(scene.witness.rounding.energy_scale, initial_scale);
}
