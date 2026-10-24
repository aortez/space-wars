//! Compare complete native trajectories with the independent ballistic fixture.
use super::*;
use crate::world::loading::response::gravity::{self as native, GravitySettling};

#[derive(Default)]
struct Observed {
    impacts: Vec<Impact>,
    events: Vec<TimedEvent>,
    work: f64,
    errors: [f64; 3],
}

impl Observed {
    fn independent(&mut self, trace: Trace, elapsed: f64) {
        for mut impact in trace.impacts {
            impact.at += elapsed;
            self.impacts.push(impact);
        }
        for mut event in trace.events {
            event.at += elapsed;
            self.events.push(event);
        }
        for flight in trace.flights {
            self.work += flight.gravity_work;
        }
    }

    fn native(&mut self, trace: native::GravityStep, elapsed: f64) {
        for item in trace.records {
            match item.record {
                native::GravityRecord::Flight(flight) => {
                    self.work += flight.gravity_work;
                    for (total, error) in self.errors.iter_mut().zip(flight.errors) {
                        *total += error;
                    }
                }
                native::GravityRecord::Episode(event) => self.events.push(TimedEvent {
                    at: elapsed + item.at,
                    event,
                }),
                native::GravityRecord::Impact(impact) => self.impacts.push(Impact {
                    at: elapsed + item.at,
                    incoming: impact.incoming,
                    outgoing: impact.outgoing,
                    response: impact.response,
                }),
                native::GravityRecord::Support(_) | native::GravityRecord::Settlement(_) => {
                    panic!("unexpected support in flight fixture")
                }
            }
        }
        let numeric = trace.numerical.unwrap();
        // End and subsequent impacts cannot erase or rebase earlier defects.
        assert_eq!(self.errors, numeric.errors);
        assert!(
            numeric
                .errors
                .iter()
                .zip(numeric.scales)
                .all(|(e, scale)| e.abs() <= 1.0e-5 * scale)
        );
    }
}

fn native_step(world: &mut PhysicsWorld, dt: f32) -> native::GravityStep {
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let trace = world
        .step_material_flat_gravity(dt, GravitySettling::Refuse)
        .unwrap();
    let replay_trace = replay
        .step_material_flat_gravity(dt, GravitySettling::Refuse)
        .unwrap();
    for entry in &trace.records {
        if let native::GravityRecord::Impact(impact) = &entry.record {
            assert_eq!(
                world.material_response_batches()[impact.response_batch]
                    .result
                    .as_ref()
                    .unwrap(),
                &impact.response
            );
        }
    }
    assert_eq!(trace.durations, replay_trace.durations);
    assert_eq!(trace.records, replay_trace.records);
    assert_eq!(trace.numerical, replay_trace.numerical);
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(world.loading_batches(), replay.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    assert_eq!(
        trace.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
        f64::from(dt)
    );
    assert!(trace.durations.iter().all(|d| *d > 0.0));
    assert!(
        trace
            .records
            .windows(2)
            .all(|r| r[0].at <= r[1].at && r[0].phase <= r[1].phase)
    );
    assert!(
        trace
            .records
            .iter()
            .all(|r| r.at <= f64::from(dt) && r.phase < trace.advances.len())
    );
    trace
}

fn compare(actual: &Observed, expected: &Observed, energy: f64) {
    assert_eq!(actual.impacts.len(), expected.impacts.len());
    assert_eq!(actual.events.len(), expected.events.len());
    for (actual, expected) in actual.impacts.iter().zip(&expected.impacts) {
        near(actual.at, expected.at, 3.0e-6);
        near(
            f64::from(actual.incoming),
            f64::from(expected.incoming),
            1.0e-5 * f64::from(expected.incoming),
        );
        near(
            f64::from(actual.outgoing),
            f64::from(expected.outgoing),
            1.0e-5 * f64::from(expected.outgoing).abs(),
        );
        near(
            loss(&actual.response),
            loss(&expected.response),
            1.0e-5 * energy,
        );
        assert_eq!(actual.response.roots, expected.response.roots);
    }
    for (actual, expected) in actual.events.iter().zip(&expected.events) {
        // End may be observed on a different tiny tail. Compare ownership/order,
        // while the impact timing checks compare the actual return trajectories.
        assert_eq!(actual.event.id, expected.event.id);
        assert_eq!(actual.event.sources, expected.event.sources);
        assert_eq!(actual.event.transition, expected.event.transition);
    }
    near(actual.work, expected.work, 1.0e-5 * energy);
}

#[test]
fn gravity_schedule_native_free_flight_and_landings_match_independent_motion() {
    let mut flights = 0;
    let mut landings = 0;
    for iterations in [1, 4, 8] {
        for acceleration in [1.0, 10.0, 100.0] {
            for speed in [-0.5, -4.0] {
                let (mut independent, _) = fixture(&[4, 2], 2, true, 1.0, speed, acceleration);
                independent.raw.integration_parameters.num_solver_iterations = iterations;
                let mut native = independent.clone();
                let dt = -1.5 * speed / acceleration;
                let mut tracker = departure::Tracker::default();
                let expected =
                    step_tracked(&mut independent, dt, MAX_ADVANCES, Some(&mut tracker)).unwrap();
                let actual = native_step(&mut native, dt);
                assert_eq!(actual.durations.len(), 2);
                assert_eq!(
                    actual.durations,
                    expected
                        .flights
                        .iter()
                        .map(|f| f.duration)
                        .collect::<Vec<_>>()
                );
                assert_eq!(state(&native), state(&independent));
                let h = f64::from(dt);
                let a = f64::from(acceleration);
                near(
                    state(&native).0,
                    -1.0 + f64::from(speed) * h + 0.5 * a * h * h,
                    3.0e-6,
                );
                near(state(&native).1, f64::from(speed) + a * h, 3.0e-6);
                assert!(native.loading_batches().is_empty());
                assert!(native.material_response_batches().is_empty());
                flights += 1;
            }
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut independent, tokens) =
                            fixture(parts, copies, reverse, 0.02, 0.0, acceleration);
                        independent.raw.integration_parameters.num_solver_iterations = iterations;
                        let initial = mechanical(&independent, f64::from(acceleration));
                        let mut native = independent.clone();
                        let dt = (1.25
                            * libm::sqrt(-2.0 * state(&native).0 / f64::from(acceleration)))
                            as f32;
                        let mut tracker = departure::Tracker::default();
                        let mut expected = Observed::default();
                        expected.independent(
                            step_tracked(&mut independent, dt, MAX_ADVANCES, Some(&mut tracker))
                                .unwrap(),
                            0.0,
                        );
                        let mut actual = Observed::default();
                        actual.native(native_step(&mut native, dt), 0.0);
                        assert_eq!(actual.impacts.len(), 1);
                        near(
                            10.0 * f64::from(actual.impacts[0].incoming).powi(2),
                            initial,
                            1.0e-5 * initial,
                        );
                        compare(&actual, &expected, initial);
                        near(state(&native).0, state(&independent).0, 3.0e-6);
                        near(
                            state(&native).1,
                            state(&independent).1,
                            1.0e-5 * f64::from(actual.impacts[0].incoming),
                        );
                        for (a, b) in balances(&native, &tokens)
                            .into_iter()
                            .zip(balances(&independent, &tokens))
                        {
                            near(a, b, 1.0e-5 * initial);
                        }
                        landings += 1;
                    }
                }
            }
        }
    }
    assert_eq!(flights, 18);
    assert_eq!(landings, 72);
}

#[test]
fn gravity_schedule_native_repeated_bounces_match_independent_trajectories() {
    let mut cases = 0;
    for speed in [0.5, 4.0] {
        // Obtain a three-impact horizon from fresh ordinary zero-gravity solves,
        // carrying material capacity between them. Neither scheduler chooses it.
        let mut materials = None;
        let mut incoming = speed;
        let mut at = 0.0;
        let mut horizon = 0.0;
        for _ in 0..3 {
            let (mut reference, _) = fixture(&[1, 1], 1, false, 0.0, incoming, 0.0);
            if let Some(saved) = materials.take() {
                reference
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials = saved;
            }
            reference.step(0.0001);
            let outgoing = state(&reference).1;
            horizon = at - outgoing / 20.0;
            at += -2.0 * outgoing / 10.0;
            incoming = -outgoing as f32;
            materials = Some(
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
        }
        for iterations in [1, 4, 8] {
            for hz in [0.0, 30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1][..], &[4, 2]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut independent, tokens) =
                                fixture(parts, copies, reverse, 0.0, speed, 10.0);
                            independent.raw.integration_parameters.num_solver_iterations =
                                iterations;
                            let mut native = independent.clone();
                            let mut tracker = departure::Tracker::default();
                            let initial = mechanical(&native, 10.0);
                            let mut expected = Observed::default();
                            let mut actual = Observed::default();
                            let mut elapsed = 0.0;
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = if hz == 0.0 {
                                    remaining
                                } else {
                                    (1.0_f32 / hz).min(remaining)
                                };
                                expected.independent(
                                    step_tracked(
                                        &mut independent,
                                        dt,
                                        MAX_ADVANCES,
                                        Some(&mut tracker),
                                    )
                                    .unwrap(),
                                    elapsed,
                                );
                                actual.native(native_step(&mut native, dt), elapsed);
                                near(state(&native).0, state(&independent).0, 3.0e-6);
                                near(
                                    state(&native).1,
                                    state(&independent).1,
                                    1.0e-5 * f64::from(speed),
                                );
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(actual.impacts.len(), 3);
                            assert_eq!(actual.events.len(), 6);
                            compare(&actual, &expected, initial);
                            let spent: f64 = actual.impacts.iter().map(|i| loss(&i.response)).sum();
                            near(mechanical(&native, 10.0) + spent, initial, 1.0e-5 * initial);
                            near(
                                actual.work,
                                state(&native).2 * 10.0 * state(&native).0,
                                1.0e-10,
                            );
                            assert!(spent <= initial);
                            for (a, b) in balances(&native, &tokens)
                                .into_iter()
                                .zip(balances(&independent, &tokens))
                            {
                                near(a, b, 1.0e-5 * initial);
                            }
                            for pair in actual.events.chunks_exact(2) {
                                assert_eq!(pair[0].event.transition, EpisodeTransition::Impact);
                                assert_eq!(pair[1].event.transition, EpisodeTransition::End);
                                assert_eq!(pair[0].event.id, pair[1].event.id);
                                assert!(pair[0].at < pair[1].at);
                            }
                            assert!(native.loading.as_ref().unwrap().episodes.is_empty());
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 240);
}
