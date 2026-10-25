use super::*;

#[test]
fn native_mass_pair_arrivals_match_independent_work_and_exact_native_time() {
    let mut cases = 0;
    for mass in [5.0, 13.0, 20.0, 31.0, 80.0] {
        for n in [1, 4, 8] {
            for hz in [30.0_f32, 120.0, 240.0] {
                for partitioned in [false, true] {
                    for speed in [0.5, 4.0] {
                        let parts = if partitioned { [4, 2] } else { [1, 1] };
                        let (mut independent, tokens) = fixture_with_masses(
                            &[(parts[0], mass), (parts[1], 20.0)],
                            if partitioned { 2 } else { 1 },
                            partitioned,
                            0.02,
                            speed,
                            10.0,
                        );
                        independent.raw.integration_parameters.num_solver_iterations = n;
                        let mut world = independent.clone();
                        let initial = mechanical(&world, 10.0);
                        let mut tracker = departure::Tracker::default();
                        let mut actual = Observed::default();
                        let mut expected = Observed::default();
                        let horizon = arrival(f64::from(0.02_f32), f64::from(speed), 10.0) + 0.005;
                        let mut elapsed = 0.0;
                        loop {
                            let remaining = (horizon - elapsed) as f32;
                            let dt = remaining.min(1.0 / hz);
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
                            actual.native(native_step(&mut world, dt), elapsed);
                            elapsed += f64::from(dt);
                            if dt == remaining {
                                break;
                            }
                        }
                        compare(&actual, &expected, initial);
                        assert_eq!(actual.impacts.len(), 1);
                        near(state(&world).0, state(&independent).0, 3.0e-6);
                        near(
                            state(&world).1,
                            state(&independent).1,
                            1.0e-5 * f64::from(speed),
                        );
                        for (a, b) in balances(&world, &tokens)
                            .iter()
                            .zip(balances(&independent, &tokens))
                        {
                            near(*a, b, 1.0e-5 * initial);
                        }
                        let version: u32 = if mass == 20.0 { 6 } else { 8 };
                        assert_eq!(
                            &world.snapshot_bytes().unwrap()[..4],
                            &version.to_le_bytes()
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 180);
}
