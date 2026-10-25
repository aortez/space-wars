//! Native support policies compared against independent physical phases.
use super::*;
use crate::world::loading::response::gravity::{self as native, GravitySettling};

fn native_step(world: &mut PhysicsWorld, dt: f32, policy: GravitySettling) -> native::GravityStep {
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let trace = world
        .step_material_flat_gravity(dt, policy)
        .unwrap_or_else(|e| {
            panic!(
                "{e:?}: dt={dt:?}, motion={:?}, iterations={}, colliders={}",
                state(world),
                world.raw.integration_parameters.num_solver_iterations,
                world.raw.colliders.len()
            )
        });
    let repeated = replay.step_material_flat_gravity(dt, policy).unwrap();
    assert_eq!(trace.records, repeated.records);
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
    assert_eq!(trace.durations, repeated.durations);
    assert_eq!(trace.numerical, repeated.numerical);
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
    assert!(trace.wall_time >= trace.advances.iter().map(|m| m.wall_time).sum());
    trace
}

fn compare_support(actual: &native::GravityStep, expected: &Trace, tolerance: f64) {
    let actual_support: Vec<_> = actual
        .records
        .iter()
        .filter_map(|r| match &r.record {
            native::GravityRecord::Support(s) => Some(s),
            _ => None,
        })
        .collect();
    // Exact duration accounting can add a tiny held tail. Compare integrated
    // physical reaction/work while checking every auxiliary phase independently.
    assert_eq!(actual_support.is_empty(), expected.supports.is_empty());
    let sum =
        |field: fn(&support::Support) -> f64| expected.supports.iter().map(field).sum::<f64>();
    let reaction: f64 = actual_support
        .iter()
        .map(|s| s.impulse + s.rounding_impulse)
        .sum();
    near(reaction, sum(|s| s.impulse + s.rounding_impulse), tolerance);
    for s in actual_support {
        assert_eq!(s.episode, expected.supports[0].episode);
        assert_eq!(s.force, expected.supports[0].force);
        assert_eq!(s.gravity_work, 0.0);
        assert_eq!(s.kick_energy, s.projection_energy);
        assert!(s.rounding_loss <= 1.0e-12 * s.projection_energy);
        assert!(s.rounding_impulse.abs() <= 1.0e-6 * s.impulse);
        near(
            s.impulse + s.rounding_impulse,
            s.force * f64::from(s.duration),
            1.0e-5 * s.force * f64::from(s.duration),
        );
    }
}

#[test]
fn gravity_support_native_safe_rest_matches_both_independent_policies() {
    let mut cases = 0;
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        let mode = if policy == GravitySettling::Refuse {
            SupportMode::Rigid
        } else {
            SupportMode::Settling
        };
        for n in [1, 4, 8] {
            for hz in [1.0_f32, 30.0, 60.0, 120.0, 240.0] {
                for parts in [&[1, 1][..], &[4, 2]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut independent, tokens) =
                                fixture(parts, copies, reverse, 0.0, 0.0, 10.0);
                            independent.raw.integration_parameters.num_solver_iterations = n;
                            let capacity = balances(&independent, &tokens);
                            let mut native = independent.clone();
                            let mut tracker = departure::Tracker::default();
                            let mut owner = None;
                            for _ in 0..3 {
                                let expected = step_mode(
                                    &mut independent,
                                    1.0 / hz,
                                    MAX_ADVANCES,
                                    Some(&mut tracker),
                                    mode,
                                )
                                .unwrap();
                                let actual = native_step(&mut native, 1.0 / hz, policy);
                                compare_support(&actual, &expected, 1.0e-5);
                                assert_eq!(state(&native), (0.0, 0.0, 20.0));
                                assert_eq!(balances(&native, &tokens), capacity);
                                assert!(native.material_response_batches().is_empty());
                                assert!(native.contact_events().is_empty());
                                let summary = actual.numerical.unwrap();
                                assert_eq!(summary.scales, [0.0, 0.0, 1.0]);
                                assert_eq!(summary.errors, [0.0; 3]);
                                let events: Vec<_> = actual
                                    .records
                                    .iter()
                                    .filter_map(|r| match r.record {
                                        native::GravityRecord::Episode(e) => Some(e),
                                        _ => None,
                                    })
                                    .collect();
                                let supports: Vec<_> = actual
                                    .records
                                    .iter()
                                    .filter_map(|r| match &r.record {
                                        native::GravityRecord::Support(s) => Some(s),
                                        _ => None,
                                    })
                                    .collect();
                                assert_eq!(supports.len(), 1);
                                if let Some(id) = owner {
                                    assert!(events.is_empty());
                                    assert_eq!(supports[0].episode, id);
                                    assert_eq!(actual.advances.len(), 3);
                                } else {
                                    assert_eq!(events.len(), 1);
                                    assert_eq!(events[0].transition, EpisodeTransition::Support);
                                    assert_eq!(events[0].id, supports[0].episode);
                                    owner = Some(events[0].id);
                                    assert_eq!(actual.advances.len(), 4);
                                }
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 240);
}

#[test]
fn gravity_support_native_small_hops_settle_once_without_new_budgets() {
    let mut cases = 0;
    for n in [1, 4, 8] {
        for dt in [0.01, 0.0005, 0.0001] {
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut independent, tokens) =
                            fixture(parts, copies, reverse, 0.0, 0.01, 10.0);
                        independent.raw.integration_parameters.num_solver_iterations = n;
                        let mut native = independent.clone();
                        let mut tracker = departure::Tracker::default();
                        let initial = mechanical(&native, 10.0);
                        let capacity = balances(&native, &tokens);
                        let mut impacts = 0;
                        let mut settlements = 0;
                        let mut spent = 0.0;
                        let mut owner = None;
                        let mut errors = [0.0; 3];
                        for _ in 0..(0.01_f32 / dt).round() as usize {
                            let expected = step_mode(
                                &mut independent,
                                dt,
                                MAX_ADVANCES,
                                Some(&mut tracker),
                                SupportMode::Settling,
                            )
                            .unwrap();
                            let actual =
                                native_step(&mut native, dt, GravitySettling::BoundedNumerical);
                            compare_support(&actual, &expected, 1.0e-5 * 200.0 * f64::from(dt));
                            let mut expected_settlement = expected.settlements.iter();
                            for record in actual.records {
                                match record.record {
                                    native::GravityRecord::Flight(f) => {
                                        for (sum, e) in errors.iter_mut().zip(f.errors) {
                                            *sum += e;
                                        }
                                    }
                                    native::GravityRecord::Impact(i) => {
                                        impacts += 1;
                                        owner = Some(i.event.id);
                                        spent += loss(&i.response);
                                    }
                                    native::GravityRecord::Settlement(s) => {
                                        let other = expected_settlement.next().unwrap();
                                        assert_eq!(Some(s.episode), owner);
                                        assert_eq!(s.episode, other.episode);
                                        assert!(
                                            s.dissipated > 0.0
                                                && s.dissipated <= s.available * (1.0 + 1.0e-5)
                                        );
                                        near(s.available, other.available, 1.0e-5 * initial);
                                        near(s.dissipated, other.dissipated, 1.0e-5 * initial);
                                        near(
                                            s.impulse + s.rounding_impulse,
                                            libm::sqrt(40.0 * s.dissipated),
                                            1.0e-5 * s.impulse,
                                        );
                                        assert!(s.rounding_loss <= 1.0e-12 * s.dissipated);
                                        settlements += 1;
                                        spent += s.dissipated;
                                    }
                                    native::GravityRecord::Episode(e) => {
                                        assert_eq!(e.transition, EpisodeTransition::Impact)
                                    }
                                    native::GravityRecord::StackSupport(_)
                                    | native::GravityRecord::StackSettlement(_)
                                    | native::GravityRecord::SupportCorrection(_) => {
                                        panic!("unexpected stack record")
                                    }
                                    native::GravityRecord::Support(s) => {
                                        assert_eq!(Some(s.episode), owner)
                                    }
                                }
                            }
                            assert!(expected_settlement.next().is_none());
                            assert_eq!(actual.numerical.unwrap().errors, errors);
                            near(state(&native).0, state(&independent).0, 3.0e-6);
                            near(state(&native).1, state(&independent).1, 1.0e-7);
                        }
                        assert_eq!((impacts, settlements), (1, 1));
                        assert_eq!(state(&native).1, 0.0);
                        near(mechanical(&native, 10.0) + spent, initial, 1.0e-5 * initial);
                        assert_eq!(balances(&native, &tokens), capacity);
                        let trace =
                            native_step(&mut native, 1.0, GravitySettling::BoundedNumerical);
                        assert_eq!(trace.records.len(), 1);
                        assert!(matches!(
                            trace.records[0].record,
                            native::GravityRecord::Support(_)
                        ));
                        assert_eq!(trace.numerical.unwrap().errors, errors);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 72);
}

#[test]
fn gravity_support_native_genuine_returns_reach_rest_with_finite_work() {
    let mut cases = 0;
    for speed in [0.5, 3.0, 4.0] {
        for n in [1, 4, 8] {
            for hz in [120.0_f32, 240.0] {
                let (mut independent, tokens) = fixture(&[4, 2], 2, true, 0.0, speed, 10.0);
                independent.raw.integration_parameters.num_solver_iterations = n;
                let mut native = independent.clone();
                let initial = mechanical(&native, 10.0);
                let mut tracker = departure::Tracker::default();
                let mut actual_losses = Vec::new();
                let mut expected_losses = Vec::new();
                let mut actual_events = Vec::new();
                let mut expected_events = Vec::new();
                let mut actual_settled = Vec::new();
                let mut expected_settled = Vec::new();
                for frame in 0..1000 {
                    let expected = step_mode(
                        &mut independent,
                        1.0 / hz,
                        MAX_ADVANCES,
                        Some(&mut tracker),
                        SupportMode::Settling,
                    )
                    .unwrap();
                    let actual =
                        native_step(&mut native, 1.0 / hz, GravitySettling::BoundedNumerical);
                    expected_losses.extend(expected.impacts.iter().map(|i| loss(&i.response)));
                    expected_events.extend(
                        expected
                            .events
                            .iter()
                            .map(|e| (e.event.id, e.event.transition)),
                    );
                    expected_settled.extend(expected.settlements.iter().map(|s| s.dissipated));
                    for record in actual.records {
                        match record.record {
                            native::GravityRecord::Impact(i) => {
                                actual_losses.push(loss(&i.response))
                            }
                            native::GravityRecord::Episode(e) => {
                                actual_events.push((e.id, e.transition))
                            }
                            native::GravityRecord::Settlement(s) => {
                                assert!(s.dissipated <= s.available * (1.0 + 1.0e-5));
                                actual_settled.push(s.dissipated);
                            }
                            native::GravityRecord::Support(s) => {
                                assert_eq!(s.gravity_work, 0.0);
                                near(
                                    s.impulse + s.rounding_impulse,
                                    s.force * f64::from(s.duration),
                                    1.0e-5 * s.force * f64::from(s.duration),
                                );
                            }
                            native::GravityRecord::Flight(_) => {}
                            native::GravityRecord::StackSupport(_)
                            | native::GravityRecord::StackSettlement(_)
                            | native::GravityRecord::SupportCorrection(_) => {
                                panic!("unexpected stack record")
                            }
                        }
                    }
                    if !actual_settled.is_empty() && !expected_settled.is_empty() {
                        break;
                    }
                    assert!(frame < 999, "did not reach rest");
                }
                assert!(actual_losses.len() > 3);
                assert_eq!(actual_losses.len(), expected_losses.len());
                assert_eq!(actual_events, expected_events);
                assert_eq!(actual_events.len(), 2 * actual_losses.len() - 1);
                for (a, b) in actual_losses.iter().zip(expected_losses) {
                    near(*a, b, 1.0e-5 * initial);
                }
                assert_eq!((actual_settled.len(), expected_settled.len()), (1, 1));
                near(actual_settled[0], expected_settled[0], 1.0e-5 * initial);
                near(
                    mechanical(&native, 10.0)
                        + actual_losses.iter().sum::<f64>()
                        + actual_settled[0],
                    initial,
                    1.0e-5 * initial,
                );
                let capacity = balances(&native, &tokens);
                for (a, b) in capacity.iter().zip(balances(&independent, &tokens)) {
                    near(*a, b, 1.0e-5 * initial);
                }
                let held = native_step(&mut native, 0.1, GravitySettling::BoundedNumerical);
                assert_eq!(held.records.len(), 1);
                assert!(matches!(
                    held.records[0].record,
                    native::GravityRecord::Support(_)
                ));
                assert_eq!(balances(&native, &tokens), capacity);
                assert_eq!(state(&native).1, 0.0);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 18);
}
