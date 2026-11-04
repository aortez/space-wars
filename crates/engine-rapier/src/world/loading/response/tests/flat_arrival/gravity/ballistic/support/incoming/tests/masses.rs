//! Unequal-mass qualification independent of native gravity admission.
use super::*;

const MASSES: [[f32; 2]; 8] = [
    [5.0, 5.0],
    [5.0, 20.0],
    [20.0, 5.0],
    [13.0, 31.0],
    [31.0, 13.0],
    [20.0, 80.0],
    [80.0, 20.0],
    [80.0, 80.0],
];

fn scene(
    masses: [f32; 2],
    partitioned: bool,
    reverse: bool,
    gap: f32,
    speed: f32,
    gravity: f32,
) -> (Verified, Vec<RegionToken>) {
    let parts = if partitioned { [4, 2, 4] } else { [1; 3] };
    let (world, tokens) = fixture_with_masses(
        &[
            (parts[0], masses[0]),
            (parts[1], masses[1]),
            (parts[2], 20.0),
        ],
        if partitioned { 2 } else { 1 },
        reverse,
        gap,
        speed,
        gravity,
    );
    let result = Verified::with_stack(world, true).unwrap();
    for (i, nominal) in masses.into_iter().enumerate() {
        near(
            f64::from(result.witness.motion[i].2),
            f64::from(nominal),
            2.0 * f64::from(f32::EPSILON) * f64::from(nominal),
        );
    }
    (result, tokens)
}

#[test]
fn unequal_mass_stack_support_measures_each_weight_without_damage() {
    let mut cases = 0;
    for masses in MASSES {
        for n in [1, 4, 8] {
            for gravity in [1.0, 10.0, 32.0] {
                for partitioned in [false, true] {
                    for reverse in [false, true] {
                        let (mut scene, tokens) =
                            scene(masses, partitioned, reverse, 0.0, 0.0, gravity);
                        scene.world.raw.integration_parameters.num_solver_iterations = n;
                        let initial = scene.witness.motion.clone();
                        let capacity = balances(&scene.world, &tokens);
                        let old = scene.witness.support;
                        let actual = [initial[0].2, initial[1].2].map(f64::from);
                        let expected = [
                            actual[0] * f64::from(gravity),
                            (actual[0] + actual[1]) * f64::from(gravity),
                        ];
                        let mut owners = None;
                        for dt in [0.01, 1.0 / 30.0, 1.0 / 120.0, f32::from_bits(0x30000000)] {
                            let trace = replayed(&mut scene, dt);
                            assert!(trace.impacts.is_empty() && trace.stack_settlements.is_empty());
                            assert_eq!(trace.events.len(), usize::from(owners.is_none()));
                            assert_eq!(trace.stack_supports.len(), 1);
                            let support = &trace.stack_supports[0];
                            assert_eq!(support.forces, expected);
                            assert_eq!(support.episodes[1], old);
                            assert_eq!(*owners.get_or_insert(support.episodes), support.episodes);
                            assert_eq!(support.gravity_work, 0.0);
                            assert_eq!(support.kick_energy, support.projection_energy);
                            assert!(support.solves <= 64);
                            for (i, force) in expected.iter().enumerate() {
                                let impulse = force * f64::from(dt);
                                near(
                                    support.impulses[i] + support.rounding_impulses[i],
                                    impulse,
                                    1.0e-5 * impulse,
                                );
                            }
                            assert_eq!(scene.witness.motion, initial);
                            assert_eq!(balances(&scene.world, &tokens), capacity);
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 288);
}

#[test]
fn unequal_mass_stack_arrival_closes_work_momentum_and_capacity() {
    let mut cases = 0;
    for masses in MASSES {
        for speed in [0.5_f32, 4.0] {
            let gap = 0.02_f32;
            let incoming = libm::sqrt(f64::from(speed).powi(2) + 20.0 * f64::from(gap));
            let at = arrival(f64::from(gap), f64::from(speed), 10.0);
            // Ordinary contact with analytically known incoming speed, no
            // ballistic scheduler or support projection in the reference.
            let (mut reference, tokens) = fixture_with_masses(
                &[(1, masses[0]), (1, masses[1]), (1, 20.0)],
                1,
                false,
                0.0,
                incoming as f32,
                0.0,
            );
            reference.step(0.00001);
            let expected = applied(&reference).clone();
            let expected_capacity = balances(&reference, &tokens);
            let rebound = state(&reference).1;
            let horizon = at + 0.005;
            for n in [1, 4, 8] {
                for hz in [30.0_f32, 120.0, 240.0] {
                    for partitioned in [false, true] {
                        for reverse in [false, true] {
                            let (mut scene, tokens) =
                                scene(masses, partitioned, reverse, gap, speed, 10.0);
                            scene.world.raw.integration_parameters.num_solver_iterations = n;
                            let initial = mechanical(&scene.world, 10.0);
                            let old = scene.witness.support;
                            let support_mass = f64::from(scene.witness.motion[1].2);
                            let mut elapsed = 0.0;
                            let mut work = 0.0;
                            let mut reaction = 0.0;
                            let mut corrections = 0.0;
                            let mut impacts = Vec::new();
                            let mut events = Vec::new();
                            loop {
                                let remaining = (horizon - elapsed) as f32;
                                let dt = (1.0 / hz).min(remaining);
                                let trace = replayed(&mut scene, dt);
                                assert!(
                                    trace.stack_settlements.is_empty()
                                        && trace.stack_supports.is_empty()
                                );
                                assert_eq!(trace.flights.len(), trace.supports.len());
                                assert_eq!(
                                    trace
                                        .flights
                                        .iter()
                                        .map(|f| f64::from(f.duration))
                                        .sum::<f64>(),
                                    f64::from(dt)
                                );
                                for flight in trace.flights {
                                    work += flight.gravity_work;
                                }
                                for support in trace.supports {
                                    assert_eq!(support.episode, old);
                                    assert_eq!(support.force, support_mass * 10.0);
                                    assert_eq!(support.gravity_work, 0.0);
                                    assert_eq!(support.kick_energy, support.projection_energy);
                                    reaction += support.impulse + support.rounding_impulse;
                                }
                                for correction in trace.support_corrections {
                                    assert_eq!(correction.episode, old);
                                    corrections += correction.dissipated;
                                    reaction += correction.impulse;
                                }
                                for mut impact in trace.impacts {
                                    impact.at += elapsed;
                                    reaction +=
                                        f64::from(impact.response.impacts[1].normal_impulse);
                                    impacts.push(impact);
                                }
                                events.extend(trace.events);
                                elapsed += f64::from(dt);
                                if dt == remaining {
                                    break;
                                }
                            }
                            assert_eq!(impacts.len(), 1);
                            assert_eq!(events.len(), 2);
                            assert_eq!(events[0].event.transition, EpisodeTransition::Impact);
                            assert_eq!(events[1].event.transition, EpisodeTransition::End);
                            assert_eq!(events[0].event.id, events[1].event.id);
                            let impact = &impacts[0];
                            assert_eq!(impact.response.roots, [events[0].event.id]);
                            assert_eq!(impact.response.impacts[1].episode, old);
                            assert_eq!(impact.response.prescribed_work, 0.0);
                            near(impact.at, at, 2.0e-6);
                            near(f64::from(impact.incoming), incoming, 1.0e-5 * incoming);
                            near(f64::from(impact.outgoing), rebound, 1.0e-5 * incoming);
                            near(loss(&impact.response), loss(&expected), 1.0e-5 * initial);
                            let (x, v, mass) = state(&scene.world);
                            near(
                                mechanical(&scene.world, 10.0)
                                    + loss(&impact.response)
                                    + corrections,
                                initial,
                                1.0e-5 * initial,
                            );
                            near(
                                0.5 * mass * v * v + loss(&impact.response) + corrections,
                                0.5 * mass * f64::from(speed).powi(2) + work,
                                1.0e-5 * initial,
                            );
                            near(work, mass * 10.0 * (x + f64::from(gap)), 1.0e-10);
                            near(
                                mass * (v - f64::from(speed)) + reaction,
                                (mass + support_mass) * 10.0 * elapsed,
                                1.0e-5 * mass * incoming,
                            );
                            for (actual, expected) in balances(&scene.world, &tokens)
                                .iter()
                                .zip(&expected_capacity)
                            {
                                near(*actual, *expected, 1.0e-5 * initial);
                            }
                            assert_eq!(scene.witness.motion[1], (1.0, 0.0, support_mass as f32));
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 576);
}

#[test]
fn unequal_mass_stack_contained_return_settles_once_without_damage() {
    let mut cases = 0;
    for masses in MASSES {
        for n in [1, 4, 8] {
            for partitioned in [false, true] {
                for reverse in [false, true] {
                    let (mut scene, tokens) = scene(masses, partitioned, reverse, 0.0, 0.01, 10.0);
                    scene.world.raw.integration_parameters.num_solver_iterations = n;
                    let initial = mechanical(&scene.world, 10.0);
                    let capacity = balances(&scene.world, &tokens);
                    let old = scene.witness.support;
                    let trace = replayed(&mut scene, 1.0 / 120.0);
                    assert_eq!(trace.impacts.len(), 1);
                    assert_eq!(trace.stack_settlements.len(), 1);
                    assert_eq!(trace.events.len(), 1);
                    let settlement = &trace.stack_settlements[0];
                    assert_eq!(settlement.episodes, [trace.events[0].event.id, old]);
                    assert!(
                        settlement.dissipated > 0.0
                            && settlement.dissipated <= settlement.available * (1.0 + 1.0e-5)
                    );
                    let correction: f64 =
                        trace.support_corrections.iter().map(|c| c.dissipated).sum();
                    near(
                        loss(&trace.impacts[0].response) + settlement.dissipated + correction,
                        initial,
                        1.0e-5 * initial,
                    );
                    assert_eq!(state(&scene.world).1, 0.0);
                    assert_eq!(balances(&scene.world, &tokens), capacity);
                    let held = replayed(&mut scene, 1.0 / 30.0);
                    assert!(
                        held.impacts.is_empty()
                            && held.events.is_empty()
                            && held.stack_settlements.is_empty()
                    );
                    assert_eq!(held.stack_supports[0].episodes, settlement.episodes);
                    assert_eq!(balances(&scene.world, &tokens), capacity);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 96);
}

#[test]
fn unequal_mass_stack_limits_overload_and_edits_roll_back() {
    // Resting 8:1 and impacting 16:1 exceed existing finite solve limits.
    // Neither may be made successful by silently increasing those limits.
    let (mut rest, _) = scene([40.0, 5.0], true, true, 0.0, 0.0, 10.0);
    rest.world.raw.integration_parameters.num_solver_iterations = 1;
    refused(&mut rest, 0.01, MAX_ADVANCES, Rejection::Limit);
    let (mut impact, _) = scene([80.0, 5.0], true, true, 0.0, 4.0, 10.0);
    refused(
        &mut impact,
        0.01,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Budget(
            engine_material::shared_impact::BudgetError::NotConverged,
        ))),
    );
    // B alone is safe (2000), but A+B overload the wood base (10000).
    let (mut overload, _) = scene([80.0, 20.0], true, true, 0.0, 0.0, 100.0);
    refused(&mut overload, 0.01, MAX_ADVANCES, Rejection::Support);
    // Positive outputs already exist before each refusal.
    let (mut changed, _) = scene([13.0, 31.0], true, true, 0.02, 0.5, 10.0);
    replayed(&mut changed, 0.001);
    for role in [0, 1] {
        let mut edited = Verified::restore(&changed.snapshot());
        let id = body(role + 1);
        let handle = edited.world.body_handle(id).unwrap();
        edited.world.raw.bodies[handle].set_additional_mass(1.0, true);
        edited.world.refresh_mass_properties(id);
        refused(&mut edited, 0.001, MAX_ADVANCES, Rejection::Evidence);
    }
    refused(&mut changed, 0.1, 0, Rejection::Limit);
}

#[test]
fn unequal_mass_stack_three_returns_preserve_work_and_support_owner() {
    let mut cases = 0;
    for masses in MASSES {
        let mut saved = None;
        let mut speed = 4.0_f32;
        let mut at = 0.0;
        let mut expected = Vec::new();
        let mut expected_capacity = Vec::new();
        for _ in 0..3 {
            let (mut reference, tokens) = fixture_with_masses(
                &[(1, masses[0]), (1, masses[1]), (1, 20.0)],
                1,
                false,
                0.0,
                speed,
                0.0,
            );
            if let Some(materials) = saved.take() {
                reference
                    .loading
                    .as_mut()
                    .unwrap()
                    .response
                    .as_mut()
                    .unwrap()
                    .materials = materials;
            }
            reference.step(0.00001);
            let response = applied(&reference).clone();
            let outgoing = state(&reference).1;
            assert!(outgoing < 0.0);
            expected.push((at, speed, outgoing, loss(&response)));
            expected_capacity = balances(&reference, &tokens);
            saved = Some(
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
            at -= 2.0 * outgoing / 10.0;
            speed = -outgoing as f32;
        }
        let horizon = expected[2].0 - expected[2].2 / 20.0;
        for n in [1, 4, 8] {
            for hz in [0.0_f32, 30.0, 120.0, 240.0] {
                for partitioned in [false, true] {
                    let (mut scene, tokens) =
                        scene(masses, partitioned, partitioned, 0.0, 4.0, 10.0);
                    scene.world.raw.integration_parameters.num_solver_iterations = n;
                    let initial = mechanical(&scene.world, 10.0);
                    let old = scene.witness.support;
                    let mass = state(&scene.world).2;
                    let support_mass = f64::from(scene.witness.motion[1].2);
                    let scales = [
                        scene.witness.rounding.energy_scale,
                        scene.witness.rounding.momentum_scale,
                        scene.witness.rounding.distance_scale,
                    ];
                    let mut elapsed = 0.0;
                    let mut impacts = Vec::new();
                    let mut events = Vec::new();
                    let mut reaction = 0.0;
                    let mut corrections = 0.0;
                    let mut work = 0.0;
                    loop {
                        let remaining = (horizon - elapsed) as f32;
                        let dt = if hz == 0.0 {
                            remaining
                        } else {
                            remaining.min(1.0 / hz)
                        };
                        let trace = replayed(&mut scene, dt);
                        assert!(
                            trace.stack_supports.is_empty() && trace.stack_settlements.is_empty()
                        );
                        assert_eq!(
                            trace
                                .flights
                                .iter()
                                .map(|f| f64::from(f.duration))
                                .sum::<f64>(),
                            f64::from(dt)
                        );
                        for flight in trace.flights {
                            work += flight.gravity_work;
                        }
                        for support in trace.supports {
                            assert_eq!(support.episode, old);
                            reaction += support.impulse + support.rounding_impulse;
                        }
                        for correction in trace.support_corrections {
                            assert_eq!(correction.episode, old);
                            reaction += correction.impulse;
                            corrections += correction.dissipated;
                        }
                        for mut impact in trace.impacts {
                            impact.at += elapsed;
                            reaction += f64::from(impact.response.impacts[1].normal_impulse);
                            impacts.push(impact);
                        }
                        events.extend(trace.events);
                        elapsed += f64::from(dt);
                        if dt == remaining {
                            break;
                        }
                    }
                    assert_eq!(impacts.len(), 3);
                    assert_eq!(events.len(), 6);
                    for (i, (impact, expected)) in impacts.iter().zip(&expected).enumerate() {
                        near(impact.at, expected.0, 3.0e-6);
                        near(
                            f64::from(impact.incoming),
                            f64::from(expected.1),
                            1.0e-5 * f64::from(expected.1),
                        );
                        near(
                            f64::from(impact.outgoing),
                            expected.2,
                            1.0e-5 * expected.2.abs(),
                        );
                        near(loss(&impact.response), expected.3, 1.0e-5 * initial);
                        let start = events[2 * i].event;
                        let end = events[2 * i + 1].event;
                        assert_eq!(start.transition, EpisodeTransition::Impact);
                        assert_eq!(end.transition, EpisodeTransition::End);
                        assert_eq!(start.id, end.id);
                        assert_eq!(impact.response.roots, [start.id]);
                        assert_eq!(impact.response.impacts[1].episode, old);
                        assert_ne!(start.id, old);
                        if i > 0 {
                            assert!(start.id > events[2 * i - 2].event.id);
                        }
                    }
                    let loss = impacts.iter().map(|i| loss(&i.response)).sum::<f64>();
                    let (_, v, _) = state(&scene.world);
                    near(
                        mechanical(&scene.world, 10.0) + loss + corrections,
                        initial,
                        1.0e-5 * initial,
                    );
                    near(
                        0.5 * mass * v * v + loss + corrections,
                        initial + work,
                        1.0e-5 * initial,
                    );
                    near(
                        mass * (v - 4.0) + reaction,
                        (mass + support_mass) * 10.0 * elapsed,
                        1.0e-5 * mass * 4.0,
                    );
                    for (actual, expected) in balances(&scene.world, &tokens)
                        .iter()
                        .zip(&expected_capacity)
                    {
                        near(*actual, *expected, 1.0e-5 * initial);
                    }
                    assert!(scene.witness.incoming.is_none());
                    assert_eq!(
                        scales,
                        [
                            scene.witness.rounding.energy_scale,
                            scene.witness.rounding.momentum_scale,
                            scene.witness.rounding.distance_scale
                        ]
                    );
                    scene.witness.rounding.validate(&scene.world, 10.0).unwrap();
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 192);
}
