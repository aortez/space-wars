//! Complete fixture schedules qualify native holds/settlements before dispatch.
use super::flight::frame_with_policy;
use super::*;

#[test]
fn gravity_stack_support_matches_independent_rest_under_both_policies() {
    let mut cases = 0;
    for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
        for n in [1, 4, 8] {
            for warm in [0.0, 0.5, 1.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut w, tokens) = fixture(parts, copies, reverse, 0.0, 0.0, 10.0);
                            w.raw.integration_parameters.num_solver_iterations = n;
                            w.raw.integration_parameters.warmstart_coefficient = warm;
                            let mut native = unenrolled(&w);
                            let mut scene = Verified::with_stack(
                                w,
                                policy == GravitySettling::BoundedNumerical,
                            )
                            .unwrap();
                            let capacity = balances(&scene.world, &tokens);
                            let trace = frame_with_policy(&mut scene, &mut native, 0.01, policy);
                            assert_eq!(trace.events.len(), 1);
                            let owners = trace.stack_supports[0].episodes;
                            for dt in [1.0 / 30.0, 1.0 / 120.0, f32::from_bits(0x30000000)] {
                                let trace = frame_with_policy(&mut scene, &mut native, dt, policy);
                                assert!(
                                    trace.events.is_empty()
                                        && trace.impacts.is_empty()
                                        && trace.stack_settlements.is_empty()
                                );
                                assert_eq!(trace.stack_supports[0].episodes, owners);
                            }
                            assert_eq!(balances(&native, &tokens), capacity);
                            assert_eq!(
                                native.raw.integration_parameters.warmstart_coefficient,
                                warm
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 144);
}

#[test]
fn gravity_stack_support_matches_independent_small_hops_and_exact_stops() {
    let mut cases = 0;
    for n in [1, 4, 8] {
        for hz in [30.0, 120.0, 240.0] {
            for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut w, tokens) = fixture(parts, copies, reverse, 0.0, 0.01, 10.0);
                        w.raw.integration_parameters.num_solver_iterations = n;
                        let mut native = unenrolled(&w);
                        let mut scene = Verified::with_stack(w, true).unwrap();
                        let capacity = balances(&scene.world, &tokens);
                        let trace = frame_with_policy(
                            &mut scene,
                            &mut native,
                            1.0 / hz,
                            GravitySettling::BoundedNumerical,
                        );
                        assert_eq!(trace.impacts.len(), 1);
                        assert_eq!(trace.stack_settlements.len(), 1);
                        let owners = trace.stack_settlements[0].episodes;
                        let trace = frame_with_policy(
                            &mut scene,
                            &mut native,
                            1.0,
                            GravitySettling::BoundedNumerical,
                        );
                        assert!(
                            trace.impacts.is_empty()
                                && trace.stack_settlements.is_empty()
                                && trace.events.is_empty()
                        );
                        assert_eq!(trace.stack_supports[0].episodes, owners);
                        assert_eq!(balances(&native, &tokens), capacity);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 72);
    for n in [1, 4, 8] {
        for warm in [0.0, 0.5, 1.0] {
            for reverse in [false, true] {
                for policy in [GravitySettling::Refuse, GravitySettling::BoundedNumerical] {
                    let (mut w, _) = fixture(&[4, 2, 4], 2, reverse, 0.0, 0.05, 10.0);
                    w.raw.integration_parameters.num_solver_iterations = n;
                    w.raw.integration_parameters.warmstart_coefficient = warm;
                    for m in w
                        .loading
                        .as_mut()
                        .unwrap()
                        .response
                        .as_mut()
                        .unwrap()
                        .materials
                        .values_mut()
                    {
                        m.parameters[4] = 1.0;
                    }
                    let mut native = unenrolled(&w);
                    let mut scene =
                        Verified::with_stack(w, policy == GravitySettling::BoundedNumerical)
                            .unwrap();
                    let trace = frame_with_policy(&mut scene, &mut native, 0.01, policy);
                    assert_eq!(trace.impacts.len(), 1);
                    assert_eq!(trace.impacts[0].outgoing, 0.0);
                    assert!(trace.stack_settlements.is_empty());
                    assert_eq!(
                        trace.stack_supports[0].episodes[0],
                        trace.impacts[0].response.roots[0]
                    );
                }
            }
        }
    }
}

#[test]
fn gravity_stack_support_matches_complete_independent_bounce_to_rest_histories() {
    let mut cases = 0;
    let mut corrections = 0;
    for speed in [0.5, 3.0, 4.0] {
        for n in [1, 4, 8] {
            for hz in [120.0, 240.0] {
                for reverse in [false, true] {
                    let (mut w, _) = fixture(&[4, 2, 4], 2, reverse, 0.0, speed, 10.0);
                    w.raw.integration_parameters.num_solver_iterations = n;
                    let mut native = unenrolled(&w);
                    let mut scene = Verified::with_stack(w, true).unwrap();
                    let mut settled = false;
                    let mut hits = 0;
                    for _ in 0..2000 {
                        let trace = frame_with_policy(
                            &mut scene,
                            &mut native,
                            1.0 / hz,
                            GravitySettling::BoundedNumerical,
                        );
                        hits += trace.impacts.len();
                        corrections += trace.support_corrections.len();
                        if !trace.stack_settlements.is_empty() {
                            assert_eq!(trace.stack_settlements.len(), 1);
                            let owners = trace.stack_settlements[0].episodes;
                            for dt in [0.05, 1.0, f32::from_bits(0x30000000)] {
                                let trace = frame_with_policy(
                                    &mut scene,
                                    &mut native,
                                    dt,
                                    GravitySettling::BoundedNumerical,
                                );
                                assert!(
                                    trace.events.is_empty()
                                        && trace.impacts.is_empty()
                                        && trace.stack_settlements.is_empty()
                                );
                                assert_eq!(trace.stack_supports[0].episodes, owners);
                            }
                            settled = true;
                            break;
                        }
                    }
                    assert!(settled && hits > 3);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 36);
    assert!(corrections > 0);
}

#[test]
fn gravity_stack_support_frame_end_settlement_keeps_owners_across_pause() {
    let (w, _) = fixture(&[4, 2, 4], 2, true, 0.0, 0.01, 10.0);
    let mut native = unenrolled(&w);
    let mut scene = Verified::with_stack(w, true).unwrap();
    frame_with_policy(
        &mut scene,
        &mut native,
        0.000001,
        GravitySettling::BoundedNumerical,
    );
    let apex = departure::apex_duration(state(&scene.world).1, 10.0).unwrap();
    frame_with_policy(
        &mut scene,
        &mut native,
        apex,
        GravitySettling::BoundedNumerical,
    );
    let (x, v, _) = state(&scene.world);
    let duration = arrival(-x, v, 10.0) as f32;
    let trace = frame_with_policy(
        &mut scene,
        &mut native,
        duration,
        GravitySettling::BoundedNumerical,
    );
    assert_eq!(trace.stack_settlements.len(), 1);
    assert_eq!(trace.stack_settlements[0].at, f64::from(duration));
    assert!(trace.stack_supports.is_empty());
    let owners = trace.stack_settlements[0].episodes;
    let bytes = native.snapshot_bytes().unwrap();
    native
        .step_material_flat_gravity(0.0, GravitySettling::BoundedNumerical)
        .unwrap();
    assert_eq!(native.snapshot_bytes().unwrap(), bytes);
    native = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let trace = frame_with_policy(
        &mut scene,
        &mut native,
        f32::from_bits(0x30000000),
        GravitySettling::BoundedNumerical,
    );
    assert!(trace.stack_settlements.is_empty());
    assert_eq!(trace.stack_supports[0].episodes, owners);
}
