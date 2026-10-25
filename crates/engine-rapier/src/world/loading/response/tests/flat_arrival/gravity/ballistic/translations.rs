//! World-space rounding stays charged to local physical scales.
use super::*;
use crate::world::loading::response::gravity::{GravityRecord, GravityRejection, GravitySettling};

const OFFSETS: [f32; 11] = [
    -16.0,
    -8.0,
    -1.25,
    -0.125,
    -1.0 / 8192.0,
    0.0,
    1.0 / 8192.0,
    0.125,
    1.25,
    8.0,
    16.0,
];

#[test]
fn quarter_turn_translated_pair_matches_independent_physics_or_refuses_atomically() {
    for turn in 0..4 {
        check_pair_matches_independent_physics_or_refuses_atomically(turn);
    }
}

fn check_pair_matches_independent_physics_or_refuses_atomically(turn: usize) {
    let mut counts = [0; 2];
    for x in OFFSETS {
        for n in [1, 4, 8] {
            for speed in [0.0, 0.01, 0.5, 4.0] {
                for parts in [[1, 1], [4, 2]] {
                    let gap = if speed < 0.5 { 0.0 } else { 0.02 };
                    let (mut expected, tokens) = translated_fixture(
                        &[(parts[0], 13.0), (parts[1], 20.0)],
                        2,
                        true,
                        gap,
                        speed,
                        10.0,
                        Vec2::new(x, 3.25),
                    );
                    expected.raw.integration_parameters.num_solver_iterations = n;
                    let independent_before = expected.snapshot_bytes().unwrap();
                    let mut actual = expected.clone();
                    turn_world(&mut actual, turn);
                    let before = actual.snapshot_bytes().unwrap();
                    let output = (
                        actual.loading_batches().to_vec(),
                        actual.material_response_batches().to_vec(),
                    );
                    let mut replay = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
                    let initial = cardinal_mechanical(&actual, turn, 10.0);
                    let (start_x, v, mass) = cardinal_state(&actual, turn);
                    let mut tracker = departure::Tracker::default();
                    let independent = step_mode(
                        &mut expected,
                        0.05,
                        MAX_ADVANCES,
                        Some(&mut tracker),
                        SupportMode::Settling,
                    );
                    let policy = GravitySettling::BoundedNumerical;
                    let native = actual.step_material_flat_gravity(0.05, policy);
                    assert_eq!(
                        independent.is_ok(),
                        native.is_ok(),
                        "x={x} n={n} v={speed}: {independent:?} {native:?}"
                    );
                    match (independent, native) {
                        (Ok(trace), Ok(step)) => {
                            counts[0] += 1;
                            let again = replay.step_material_flat_gravity(0.05, policy).unwrap();
                            assert_eq!(step.records, again.records);
                            assert_eq!(step.numerical, again.numerical);
                            assert_eq!(step.durations, again.durations);
                            assert_eq!(
                                actual.snapshot_bytes().unwrap(),
                                replay.snapshot_bytes().unwrap()
                            );
                            assert_eq!(
                                step.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
                                f64::from(0.05_f32)
                            );
                            assert_eq!(
                                &actual.snapshot_bytes().unwrap()[..4],
                                &(if turn == 0 { 10_u32 } else { 12 }).to_le_bytes()
                            );
                            let losses: f64 = trace.impacts.iter().map(|i| loss(&i.response)).sum();
                            let settled: f64 = trace.settlements.iter().map(|s| s.dissipated).sum();
                            near(
                                mechanical(&expected, 10.0) + losses + settled,
                                initial,
                                1.0e-5 * initial,
                            );
                            let work: f64 = trace.flights.iter().map(|f| f.gravity_work).sum();
                            near(work, mass * 10.0 * (state(&expected).0 - start_x), 1.0e-10);
                            near(cardinal_state(&actual, turn).0, state(&expected).0, 3.0e-6);
                            near(
                                cardinal_state(&actual, turn).1,
                                state(&expected).1,
                                1.0e-5 * v,
                            );
                            for (a, b) in balances(&actual, &tokens)
                                .iter()
                                .zip(balances(&expected, &tokens))
                            {
                                near(*a, b, 1.0e-5 * initial);
                            }
                            let impacts: Vec<_> = step
                                .records
                                .iter()
                                .filter_map(|r| match &r.record {
                                    GravityRecord::Impact(i) => Some(i),
                                    _ => None,
                                })
                                .collect();
                            assert_eq!(impacts.len(), trace.impacts.len());
                            for (a, b) in impacts.iter().zip(&trace.impacts) {
                                near(f64::from(a.incoming), f64::from(b.incoming), 1.0e-5 * v);
                                near(loss(&a.response), loss(&b.response), 1.0e-5 * initial);
                            }
                            if let Some(impact) = trace.impacts.first() {
                                // Analytic arrival uses the actual rounded starting gap.
                                let incoming = libm::sqrt(v * v - 20.0 * start_x);
                                near(f64::from(impact.incoming), incoming, 1.0e-5 * incoming);
                                let (mut reference, _) = fixture_with_masses(
                                    &[(1, 13.0), (1, 20.0)],
                                    1,
                                    false,
                                    0.0,
                                    incoming as f32,
                                    0.0,
                                );
                                reference.step(0.00001);
                                near(
                                    loss(&impact.response),
                                    loss(applied(&reference)),
                                    1.0e-5 * initial,
                                );
                            }
                            let numeric = step.numerical.unwrap();
                            assert_eq!(
                                numeric.scales,
                                [
                                    initial,
                                    libm::sqrt(2.0 * mass * initial),
                                    start_x.abs().max(1.0)
                                ]
                            );
                            assert!(
                                numeric
                                    .errors
                                    .iter()
                                    .zip(numeric.scales)
                                    .all(|(e, s)| e.abs() <= 1.0e-5 * s)
                            );
                            if speed == 0.0 {
                                assert_eq!(numeric.scales, [0.0, 0.0, 1.0]);
                                assert!(trace.impacts.is_empty());
                                assert_eq!(trace.supports.len(), 1);
                            }
                        }
                        (Err(reason), Err(native)) => {
                            counts[1] += 1;
                            assert!(matches!(
                                reason,
                                Rejection::Energy | Rejection::Geometry | Rejection::Reimpact
                            ));
                            assert!(
                                matches!(
                                    native,
                                    GravityRejection::Numerical
                                        | GravityRejection::NumericalAllowance { .. }
                                        | GravityRejection::AmbiguousDeparture
                                        | GravityRejection::UnaccountedApproach
                                ),
                                "x={x} n={n} speed={speed}: {reason:?} / {native:?}"
                            );
                            assert!(speed > 0.0 && x.abs() >= 0.125);
                            assert_eq!(actual.snapshot_bytes().unwrap(), before);
                            assert_eq!(expected.snapshot_bytes().unwrap(), independent_before);
                            assert_eq!(actual.loading_batches(), output.0);
                            assert_eq!(actual.material_response_batches(), output.1);
                            assert_eq!(
                                replay.step_material_flat_gravity(0.05, policy).unwrap_err(),
                                native
                            );
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    assert_eq!(counts, [188, 76]);
}
