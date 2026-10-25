//! Full translated stack transactions versus independent physical scheduling.
use super::*;
use crate::world::loading::response::gravity::GravityRejection;

#[test]
fn translated_stack_matches_independent_physics_or_refuses_atomically() {
    let mut counts = [0; 2];
    for x in [
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
    ] {
        for n in [1, 4, 8] {
            for speed in [0.0, 0.01, 0.5, 4.0] {
                for parts in [[1; 3], [4, 2, 4]] {
                    let gap = if speed < 0.5 { 0.0 } else { 0.02 };
                    let (mut world, tokens) = translated_fixture(
                        &[(parts[0], 13.0), (parts[1], 31.0), (parts[2], 20.0)],
                        2,
                        true,
                        gap,
                        speed,
                        10.0,
                        Vec2::new(x, 3.25),
                    );
                    world.raw.integration_parameters.num_solver_iterations = n;
                    let initial = mechanical(&world, 10.0);
                    let (start_x, v, _) = state(&world);
                    let mut actual = unenrolled(&world);
                    let before = actual.snapshot_bytes().unwrap();
                    let output = (
                        actual.loading_batches().to_vec(),
                        actual.material_response_batches().to_vec(),
                    );
                    let mut replay = PhysicsWorld::from_snapshot_bytes(&before).unwrap();
                    let mut expected = Verified::with_stack(world, true).unwrap();
                    let fixture_before = expected.snapshot();
                    let mut fixture_replay = Verified::restore(&fixture_before);
                    let independent = expected.step(0.05, MAX_ADVANCES);
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
                            flight::check_report(&actual, &trace, &step, true);
                            assert_eq!(trace, fixture_replay.step(0.05, MAX_ADVANCES).unwrap());
                            assert_eq!(expected.snapshot(), fixture_replay.snapshot());
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
                                &11_u32.to_le_bytes()
                            );
                            assert_eq!(state(&actual), state(&expected.world));
                            assert_eq!(
                                balances(&actual, &tokens),
                                balances(&expected.world, &tokens)
                            );
                            let losses: f64 = trace.impacts.iter().map(|i| loss(&i.response)).sum();
                            let settled: f64 =
                                trace.stack_settlements.iter().map(|s| s.dissipated).sum();
                            let correction: f64 =
                                trace.support_corrections.iter().map(|s| s.dissipated).sum();
                            near(
                                mechanical(&actual, 10.0) + losses + settled + correction,
                                initial,
                                1.0e-5 * initial,
                            );
                            if let Some(impact) = trace.impacts.first() {
                                let incoming = libm::sqrt(v * v - 20.0 * start_x);
                                near(f64::from(impact.incoming), incoming, 1.0e-5 * incoming);
                                let (mut reference, _) = fixture_with_masses(
                                    &[(1, 13.0), (1, 31.0), (1, 20.0)],
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
                                assert_ne!(impact.response.roots[0], expected.witness.support);
                                assert_eq!(
                                    impact.response.impacts[1].episode,
                                    expected.witness.support
                                );
                            }
                            assert_eq!(step.numerical.unwrap().scales[0], initial);
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
                            assert_eq!(expected.snapshot(), fixture_before);
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
    assert_eq!(counts, [186, 78]);
}

#[test]
fn translated_stack_repeated_returns_preserve_owners_and_original_limits() {
    let mut cases = 0;
    for masses in [[5.0, 20.0], [20.0, 5.0], [13.0, 31.0], [80.0, 20.0]] {
        for offset in [
            Vec2::new(-0.125, -16.0),
            Vec2::new(0.125, 16.0),
            Vec2::new(0.0, 3.25),
        ] {
            for hz in [30.0, 240.0] {
                let (world, _) = translated_fixture(
                    &[(4, masses[0]), (2, masses[1]), (4, 20.0)],
                    2,
                    true,
                    0.0,
                    4.0,
                    10.0,
                    offset,
                );
                let mut actual = unenrolled(&world);
                let mut expected = Verified::with_stack(world, true).unwrap();
                let support = expected.witness.support;
                let initial = mechanical(&actual, 10.0);
                let mut roots = Vec::new();
                let mut ended = Vec::new();
                let mut loss_total = 0.0;
                for _ in 0..1000 {
                    let trace = flight::frame_with_policy(
                        &mut expected,
                        &mut actual,
                        1.0 / hz,
                        GravitySettling::BoundedNumerical,
                    );
                    for impact in trace.impacts {
                        assert_eq!(impact.response.impacts[1].episode, support);
                        roots.push(impact.response.roots[0]);
                        loss_total += loss(&impact.response);
                    }
                    loss_total += trace
                        .support_corrections
                        .iter()
                        .map(|c| c.dissipated)
                        .sum::<f64>();
                    ended.extend(
                        trace
                            .events
                            .into_iter()
                            .filter(|e| e.event.transition == EpisodeTransition::End)
                            .map(|e| e.event.id),
                    );
                    if roots.len() == 3 {
                        break;
                    }
                }
                assert_eq!(roots.len(), 3);
                assert!(roots.windows(2).all(|w| w[0] < w[1]));
                assert_eq!(&ended[..2], &roots[..2]);
                near(
                    mechanical(&actual, 10.0) + loss_total,
                    initial,
                    1.0e-5 * initial,
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 24);
}
