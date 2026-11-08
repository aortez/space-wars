//! Pair qualification uses actual refreshed mass, not the fixture's nominal sum.
use super::*;

fn replayed(
    world: &mut PhysicsWorld,
    tracker: &mut departure::Tracker,
    dt: f32,
    mode: SupportMode,
) -> Trace {
    let bytes = bincode::serialize(&(world.snapshot_bytes().unwrap(), &*tracker)).unwrap();
    let (physics, mut history): (Vec<u8>, departure::Tracker) =
        bincode::deserialize(&bytes).unwrap();
    let mut replay = PhysicsWorld::from_snapshot_bytes(&physics).unwrap();
    let trace = step_mode(world, dt, MAX_ADVANCES, Some(tracker), mode).unwrap();
    assert_eq!(
        trace,
        step_mode(&mut replay, dt, MAX_ADVANCES, Some(&mut history), mode).unwrap()
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    assert_eq!(*tracker, history);
    assert_eq!(world.loading_batches(), replay.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    trace
}

#[test]
fn unequal_mass_pair_arrivals_match_analytic_and_ordinary_contact_work() {
    let mut cases = 0;
    for nominal in [5.0, 13.0, 20.0, 31.0, 80.0] {
        for speed in [0.5_f32, 4.0] {
            let gap = 0.02_f32;
            let incoming = libm::sqrt(f64::from(speed).powi(2) + 20.0 * f64::from(gap));
            let at = arrival(f64::from(gap), f64::from(speed), 10.0);
            let (mut reference, tokens) = fixture_with_masses(
                &[(1, nominal), (1, 20.0)],
                1,
                false,
                0.0,
                incoming as f32,
                0.0,
            );
            reference.step(0.00001);
            let response = applied(&reference).clone();
            let capacity = balances(&reference, &tokens);
            let rebound = state(&reference).1;
            let horizon = at + 0.005;
            for n in [1, 4, 8] {
                for hz in [30.0_f32, 120.0, 240.0] {
                    for partitioned in [false, true] {
                        for reverse in [false, true] {
                            let parts = if partitioned { [4, 2] } else { [1, 1] };
                            let (mut world, tokens) = fixture_with_masses(
                                &[(parts[0], nominal), (parts[1], 20.0)],
                                if partitioned { 2 } else { 1 },
                                reverse,
                                gap,
                                speed,
                                10.0,
                            );
                            world.raw.integration_parameters.num_solver_iterations = n;
                            let mut tracker = departure::Tracker::default();
                            let initial = mechanical(&world, 10.0);
                            let mut elapsed = 0.0;
                            let mut work = 0.0;
                            let mut impacts = Vec::new();
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = (1.0 / hz).min(remaining);
                                let trace =
                                    replayed(&mut world, &mut tracker, dt, SupportMode::Disabled);
                                // This older fixture rounds its last f32
                                // interval. Use measured time for the physical
                                // reference, and bound the scheduling defect.
                                let advanced: f64 =
                                    trace.flights.iter().map(|f| f64::from(f.duration)).sum();
                                near(
                                    advanced,
                                    f64::from(dt),
                                    f64::from(f32::EPSILON) * f64::from(dt),
                                );
                                work += trace.flights.iter().map(|f| f.gravity_work).sum::<f64>();
                                for mut impact in trace.impacts {
                                    impact.at += elapsed;
                                    impacts.push(impact);
                                }
                                elapsed += advanced;
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(impacts.len(), 1);
                            let impact = &impacts[0];
                            near(impact.at, at, 2.0e-6);
                            near(f64::from(impact.incoming), incoming, 1.0e-5 * incoming);
                            near(f64::from(impact.outgoing), rebound, 1.0e-5 * incoming);
                            near(loss(&impact.response), loss(&response), 1.0e-5 * initial);
                            let (x, v, mass) = state(&world);
                            near(
                                mechanical(&world, 10.0) + loss(&impact.response),
                                initial,
                                1.0e-5 * initial,
                            );
                            near(
                                0.5 * mass * v * v + loss(&impact.response),
                                0.5 * mass * f64::from(speed).powi(2) + work,
                                1.0e-5 * initial,
                            );
                            near(work, mass * 10.0 * (x + f64::from(gap)), 1.0e-10);
                            near(
                                mass * (v - f64::from(speed))
                                    + f64::from(impact.response.impacts[0].normal_impulse),
                                mass * 10.0 * elapsed,
                                1.0e-5 * mass * incoming,
                            );
                            for (actual, expected) in
                                balances(&world, &tokens).iter().zip(&capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 360);
}

#[test]
fn unequal_mass_pair_holds_or_settles_without_new_damage() {
    let mut cases = 0;
    for nominal in [5.0, 13.0, 20.0, 31.0, 80.0] {
        for n in [1, 4, 8] {
            for partitioned in [false, true] {
                for reverse in [false, true] {
                    for speed in [0.0, 0.01] {
                        let parts = if partitioned { [4, 2] } else { [1, 1] };
                        let (mut world, tokens) = fixture_with_masses(
                            &[(parts[0], nominal), (parts[1], 20.0)],
                            if partitioned { 2 } else { 1 },
                            reverse,
                            0.0,
                            speed,
                            10.0,
                        );
                        world.raw.integration_parameters.num_solver_iterations = n;
                        let mass = state(&world).2;
                        let initial = mechanical(&world, 10.0);
                        let capacity = balances(&world, &tokens);
                        let mut tracker = departure::Tracker::default();
                        let mode = if speed == 0.0 {
                            SupportMode::Rigid
                        } else {
                            SupportMode::Settling
                        };
                        let first = replayed(&mut world, &mut tracker, 1.0 / 120.0, mode);
                        assert_eq!(first.events.len(), 1);
                        assert_eq!(first.impacts.len(), usize::from(speed > 0.0));
                        assert_eq!(first.settlements.len(), usize::from(speed > 0.0));
                        let owner = first.events[0].event.id;
                        let loss: f64 = first.impacts.iter().map(|i| loss(&i.response)).sum();
                        let settled: f64 = first.settlements.iter().map(|s| s.dissipated).sum();
                        near(loss + settled, initial, 1.0e-5 * initial);
                        assert_eq!(balances(&world, &tokens), capacity);
                        assert_eq!(state(&world).1, 0.0);
                        let rounding = tracker.rounding.clone();
                        for dt in [1.0 / 30.0, 1.0 / 240.0, f32::from_bits(0x30000000)] {
                            let trace = replayed(&mut world, &mut tracker, dt, mode);
                            assert!(
                                trace.impacts.is_empty()
                                    && trace.events.is_empty()
                                    && trace.settlements.is_empty()
                            );
                            assert_eq!(trace.supports.len(), 1);
                            let support = &trace.supports[0];
                            assert_eq!(support.episode, owner);
                            assert_eq!(support.force, mass * 10.0);
                            assert_eq!(support.gravity_work, 0.0);
                            assert_eq!(support.kick_energy, support.projection_energy);
                            let expected = mass * 10.0 * f64::from(dt);
                            near(
                                support.impulse + support.rounding_impulse,
                                expected,
                                1.0e-5 * expected,
                            );
                            assert_eq!(balances(&world, &tokens), capacity);
                            assert_eq!(tracker.rounding, rounding);
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 120);
}
