use super::*;

mod boundaries;

fn stacked(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    speed: f32,
    gravity: f32,
    settling: bool,
) -> (Verified, Vec<RegionToken>) {
    let (world, tokens) = fixture(parts, copies, reverse, 0.0, speed, gravity);
    (Verified::with_stack(world, settling).unwrap(), tokens)
}

#[test]
fn ballistic_stack_support_balances_both_weights_without_work_or_damage() {
    for settling in [false, true] {
        for n in [1, 4, 8] {
            for warm in [0.0, 0.5, 1.0] {
                for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                    for copies in [1, 2] {
                        for reverse in [false, true] {
                            let (mut scene, tokens) =
                                stacked(parts, copies, reverse, 0.0, 10.0, settling);
                            scene.world.raw.integration_parameters.num_solver_iterations = n;
                            scene.world.raw.integration_parameters.warmstart_coefficient = warm;
                            let capacity = balances(&scene.world, &tokens);
                            let old = scene.witness.support;
                            let initial = scene.witness.motion.clone();
                            let trace = replayed(&mut scene, 0.01);
                            assert_eq!(trace.events.len(), 1);
                            assert_eq!(
                                trace.events[0].event.transition,
                                EpisodeTransition::Support
                            );
                            assert_eq!(trace.stack_supports.len(), 1);
                            assert!(trace.impacts.is_empty() && trace.stack_settlements.is_empty());
                            let support = &trace.stack_supports[0];
                            assert_eq!(support.episodes, [trace.events[0].event.id, old]);
                            assert_eq!(support.forces, [200.0, 400.0]);
                            for i in 0..2 {
                                near(
                                    support.impulses[i] + support.rounding_impulses[i],
                                    support.forces[i] * f64::from(support.duration),
                                    1.0e-5 * support.forces[i] * f64::from(support.duration),
                                );
                            }
                            assert_eq!(support.gravity_work, 0.0);
                            assert_eq!(support.kick_energy, support.projection_energy);
                            assert_eq!(scene.witness.motion, initial);
                            assert_eq!(balances(&scene.world, &tokens), capacity);
                            for dt in [1.0 / 30.0, 1.0 / 120.0, f32::from_bits(0x30000000)] {
                                let held = replayed(&mut scene, dt);
                                assert!(
                                    held.events.is_empty()
                                        && held.impacts.is_empty()
                                        && held.stack_settlements.is_empty()
                                );
                                let held = &held.stack_supports[0];
                                assert_eq!(held.episodes, support.episodes);
                                for i in 0..2 {
                                    let expected = held.forces[i] * f64::from(dt);
                                    near(
                                        held.impulses[i] + held.rounding_impulses[i],
                                        expected,
                                        1.0e-5 * expected,
                                    );
                                }
                                assert_eq!(held.duration, dt);
                                assert_eq!(held.gravity_work, 0.0);
                                assert_eq!(held.kick_energy, held.projection_energy);
                                assert_eq!(scene.witness.motion, initial);
                                assert_eq!(balances(&scene.world, &tokens), capacity);
                                assert_eq!(
                                    scene.world.raw.integration_parameters.warmstart_coefficient,
                                    warm
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_stack_settles_one_contained_return_without_a_new_root() {
    for n in [1, 4, 8] {
        for hz in [30.0_f32, 120.0, 240.0] {
            for parts in [&[1, 1, 1][..], &[4, 2, 4]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut scene, tokens) = stacked(parts, copies, reverse, 0.01, 10.0, true);
                        scene.world.raw.integration_parameters.num_solver_iterations = n;
                        let capacity = balances(&scene.world, &tokens);
                        let initial = mechanical(&scene.world, 10.0);
                        let trace = replayed(&mut scene, 1.0 / hz);
                        assert_eq!(trace.impacts.len(), 1);
                        assert_eq!(trace.stack_settlements.len(), 1);
                        assert_eq!(trace.events.len(), 1);
                        let settlement = &trace.stack_settlements[0];
                        assert_eq!(
                            settlement.episodes,
                            [trace.impacts[0].response.roots[0], scene.witness.support]
                        );
                        assert!(
                            settlement.dissipated > 0.0
                                && settlement.dissipated <= settlement.available * (1.0 + 1.0e-5)
                        );
                        near(
                            mechanical(&scene.world, 10.0)
                                + settlement.dissipated
                                + loss(&trace.impacts[0].response),
                            initial,
                            1.0e-5 * initial,
                        );
                        assert_eq!(balances(&scene.world, &tokens), capacity);
                        assert_eq!(
                            trace
                                .flights
                                .iter()
                                .map(|f| f64::from(f.duration))
                                .chain(trace.stack_supports.iter().map(|s| f64::from(s.duration)))
                                .sum::<f64>(),
                            f64::from(1.0 / hz)
                        );
                        assert!(settlement.solves > 0 && settlement.solves <= 32);
                        let incoming = libm::sqrt(settlement.dissipated / 10.0);
                        for i in 0..2 {
                            near(
                                settlement.impulses[i] + settlement.rounding_impulses[i],
                                20.0 * incoming,
                                1.0e-5 * 20.0 * incoming,
                            );
                        }
                        let bytes = scene.snapshot();
                        assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
                        assert_eq!(scene.snapshot(), bytes);
                        let rounding = scene.witness.rounding.clone();
                        let held = replayed(&mut scene, 1.0);
                        assert!(
                            held.impacts.is_empty()
                                && held.stack_settlements.is_empty()
                                && held.events.is_empty()
                        );
                        assert_eq!(held.stack_supports[0].episodes, settlement.episodes);
                        assert_eq!(balances(&scene.world, &tokens), capacity);
                        assert_eq!(scene.witness.rounding, rounding);
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_stack_bounce_histories_match_independent_impacts_then_settle() {
    for speed in [0.5, 3.0, 4.0] {
        let mut materials = None;
        let mut incoming = speed;
        let mut expected = Vec::new();
        let mut expected_capacity = Vec::new();
        let mut settling_at = 0.0;
        let mut settling_speed = 0.0;
        for _ in 0..64 {
            let (mut reference, tokens) = fixture(&[1, 1, 1], 1, false, 0.0, incoming, 0.0);
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
            reference.step(0.00001);
            let response = applied(&reference);
            let rebound = -state(&reference).1;
            expected.push(loss(response));
            expected_capacity = balances(&reference, &tokens);
            let exit = 2.0 * f64::from(reference.loading.as_ref().unwrap().config.touch_tolerance);
            let height = rebound * rebound / 20.0;
            settling_at += 2.0 * rebound / 10.0;
            if height < exit {
                assert!(height < exit - 1.0e-8);
                settling_speed = rebound;
                break;
            }
            assert!(height > exit + 1.0e-8);
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
            incoming = rebound as f32;
        }
        assert!(expected.len() > 3 && expected.len() < 64);
        for n in [1, 4, 8] {
            for hz in [120.0_f32, 240.0] {
                for reverse in [false, true] {
                    let (mut scene, tokens) = stacked(&[4, 2, 4], 2, reverse, speed, 10.0, true);
                    scene.world.raw.integration_parameters.num_solver_iterations = n;
                    let old = scene.witness.support;
                    let initial = mechanical(&scene.world, 10.0);
                    let mut elapsed = 0.0;
                    let mut impacts = Vec::new();
                    let mut settlements = Vec::new();
                    let mut events = Vec::new();
                    let mut support_at = None;
                    let mut reaction = 0.0;
                    let mut work = 0.0;
                    let mut correction = [0.0; 2];
                    while elapsed < settling_at + 0.05 {
                        let dt = 1.0 / hz;
                        let trace = replayed(&mut scene, dt);
                        for c in &trace.support_corrections {
                            assert_eq!(c.episode, old);
                            correction[0] += c.impulse;
                            correction[1] += c.dissipated;
                            assert!(c.dissipated > 0.0);
                        }
                        let h = scene.witness.stack.as_ref().unwrap();
                        assert_eq!(correction, [h.correction_impulse, h.correction_work]);
                        assert!(
                            correction[0].abs() <= 1.0e-5 * scene.witness.rounding.momentum_scale
                        );
                        assert!(correction[1] <= 1.0e-5 * initial);
                        assert_eq!(
                            trace
                                .flights
                                .iter()
                                .map(|f| f64::from(f.duration))
                                .chain(trace.stack_supports.iter().map(|s| f64::from(s.duration)))
                                .sum::<f64>(),
                            f64::from(dt)
                        );
                        work += trace.flights.iter().map(|f| f.gravity_work).sum::<f64>();
                        reaction += trace
                            .supports
                            .iter()
                            .map(|s| s.impulse + s.rounding_impulse)
                            .sum::<f64>();
                        for support in &trace.stack_supports {
                            support_at.get_or_insert(elapsed + support.at);
                            assert_eq!(support.episodes[1], old);
                            assert_eq!(support.forces, [200.0, 400.0]);
                            reaction += support.impulses[1] + support.rounding_impulses[1];
                            assert_eq!(support.gravity_work, 0.0);
                        }
                        impacts.extend(trace.impacts);
                        settlements.extend(trace.stack_settlements);
                        events.extend(trace.events);
                        elapsed += f64::from(dt);
                    }
                    assert_eq!(impacts.len(), expected.len());
                    assert_eq!(settlements.len(), 1);
                    assert_eq!(events.len(), 2 * impacts.len() - 1);
                    near(
                        support_at.unwrap(),
                        settling_at,
                        2.0 * TOUCH / settling_speed,
                    );
                    for (impact, expected) in impacts.iter().zip(&expected) {
                        near(loss(&impact.response), *expected, 1.0e-5 * initial);
                        assert_eq!(impact.response.impacts[1].episode, old);
                    }
                    for (i, event) in events.iter().enumerate() {
                        assert_eq!(
                            event.event.transition,
                            if i % 2 == 0 {
                                EpisodeTransition::Impact
                            } else {
                                EpisodeTransition::End
                            }
                        );
                        assert_eq!(event.event.id, impacts[i / 2].response.roots[0]);
                    }
                    let settlement = &settlements[0];
                    assert_eq!(
                        settlement.episodes,
                        [impacts.last().unwrap().response.roots[0], old]
                    );
                    let spent = impacts.iter().map(|i| loss(&i.response)).sum::<f64>()
                        + settlement.dissipated
                        + correction[1];
                    near(
                        mechanical(&scene.world, 10.0) + spent,
                        initial,
                        1.0e-5 * initial,
                    );
                    near(spent, initial + work, 1.0e-5 * initial);
                    let impact_reaction = impacts
                        .iter()
                        .map(|i| f64::from(i.response.impacts[1].normal_impulse))
                        .sum::<f64>();
                    near(
                        -20.0 * f64::from(speed)
                            + reaction
                            + impact_reaction
                            + settlement.impulses[1]
                            + settlement.rounding_impulses[1]
                            + correction[0],
                        400.0 * elapsed,
                        1.0e-5 * 400.0 * elapsed,
                    );
                    for (actual, expected) in balances(&scene.world, &tokens)
                        .iter()
                        .zip(&expected_capacity)
                    {
                        near(*actual, *expected, 1.0e-5 * initial);
                    }
                    assert_eq!(state(&scene.world).1, 0.0);
                    let capacity = balances(&scene.world, &tokens);
                    let rounding = scene.witness.rounding.clone();
                    let trace = replayed(&mut scene, 0.1);
                    assert!(
                        trace.impacts.is_empty()
                            && trace.stack_settlements.is_empty()
                            && trace.events.is_empty()
                    );
                    assert_eq!(balances(&scene.world, &tokens), capacity);
                    assert_eq!(scene.witness.rounding, rounding);
                }
            }
        }
    }
}
