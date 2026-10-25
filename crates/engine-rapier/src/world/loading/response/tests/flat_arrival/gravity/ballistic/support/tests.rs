use super::*;

#[test]
fn ballistic_support_strict_policy_holds_rest_and_zero_rebound_but_refuses_reimpact() {
    // With backend inverse mass 0.05, this speed stops with exactly impulse 1.
    // Full material damping alone does not guarantee an exactly rounded stop.
    for speed in [0.0, 0.05] {
        let (mut world, tokens) = fixture(&[4, 2], 2, true, 0.0, speed, 10.0);
        for region in world
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .values_mut()
        {
            region.parameters[4] = 1.0;
        }
        let capacity = balances(&world, &tokens);
        let mut tracker = departure::Tracker::default();
        let trace = step_mode(
            &mut world,
            0.01,
            MAX_ADVANCES,
            Some(&mut tracker),
            SupportMode::Rigid,
        )
        .unwrap();
        assert_eq!(trace.impacts.len(), usize::from(speed > 0.0));
        assert!(trace.settlements.is_empty());
        assert_eq!(trace.supports.len(), 1);
        assert_eq!(trace.events.len(), 1);
        let owner = trace.events[0].event.id;
        assert_eq!(trace.supports[0].episode, owner);
        assert_eq!(trace.supports[0].gravity_work, 0.0);
        assert_eq!(state(&world), (0.0, 0.0, 20.0));
        assert_eq!(balances(&world, &tokens), capacity);
        let trace = step_mode(
            &mut world,
            1.0,
            MAX_ADVANCES,
            Some(&mut tracker),
            SupportMode::Rigid,
        )
        .unwrap();
        assert!(
            trace.impacts.is_empty() && trace.events.is_empty() && trace.settlements.is_empty()
        );
        assert_eq!(trace.supports[0].episode, owner);
    }
    for (speed, gravity, expected) in [
        (0.01, 10.0, Rejection::Reimpact),
        (0.0, 10_000.0, Rejection::Support),
    ] {
        let (mut world, _) = fixture(&[1, 1], 1, false, 0.0, speed, gravity);
        let original = world.snapshot_bytes().unwrap();
        let mut tracker = departure::Tracker::default();
        let previous = tracker.clone();
        assert_eq!(
            step_mode(
                &mut world,
                0.01,
                MAX_ADVANCES,
                Some(&mut tracker),
                SupportMode::Rigid
            )
            .unwrap_err(),
            expected
        );
        assert_eq!(world.snapshot_bytes().unwrap(), original);
        assert_eq!(tracker, previous);
    }
}

fn verified(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    speed: f32,
    gravity: f32,
) -> (Verified, Vec<RegionToken>) {
    let (world, tokens) = fixture(parts, copies, reverse, 0.0, speed, gravity);
    (
        Verified {
            world,
            tracker: departure::Tracker::default(),
        },
        tokens,
    )
}

fn replayed(scene: &mut Verified, dt: f32) -> Trace {
    let mut replay = Verified::restore(&scene.snapshot());
    let trace = scene.step(dt, MAX_ADVANCES).unwrap();
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

#[test]
fn ballistic_support_holds_weight_without_work_or_damage() {
    for n in [1, 4, 8] {
        for hz in [1.0_f32, 30.0, 60.0, 120.0, 240.0] {
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut scene, tokens) = verified(parts, copies, reverse, 0.0, 10.0);
                        scene.world.raw.integration_parameters.num_solver_iterations = n;
                        let capacity = balances(&scene.world, &tokens);
                        let mut elapsed = 0.0;
                        let mut reaction = 0.0;
                        let mut owner = None;
                        for _ in 0..hz as usize {
                            let dt = 1.0 / hz;
                            let trace = replayed(&mut scene, dt);
                            assert!(trace.flights.is_empty());
                            assert!(trace.impacts.is_empty());
                            assert!(trace.settlements.is_empty());
                            assert_eq!(trace.supports.len(), 1);
                            let support = &trace.supports[0];
                            assert_eq!(support.duration, dt);
                            assert_eq!(support.force, 200.0);
                            assert_eq!(support.gravity_work, 0.0);
                            assert_eq!(support.projection_energy, support.kick_energy);
                            assert!(support.rounding_loss <= 1.0e-12 * support.projection_energy);
                            assert!(support.rounding_impulse.abs() <= 1.0e-6 * support.impulse);
                            near(support.impulse, 200.0 * f64::from(dt), 1.0e-5);
                            if let Some(id) = owner {
                                assert_eq!(support.episode, id);
                                assert!(trace.events.is_empty());
                            } else {
                                owner = Some(support.episode);
                                assert_eq!(trace.events.len(), 1);
                                assert_eq!(
                                    trace.events[0].event.transition,
                                    EpisodeTransition::Support
                                );
                                assert_eq!(trace.events[0].event.id, support.episode);
                            }
                            reaction += support.impulse;
                            elapsed += f64::from(dt);
                            assert_eq!(state(&scene.world), (0.0, 0.0, 20.0));
                            assert_eq!(balances(&scene.world, &tokens), capacity);
                            assert!(scene.world.material_response_batches().is_empty());
                        }
                        near(reaction, 200.0 * elapsed, 1.0e-5 * 200.0);
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_support_settles_one_small_hop_in_its_original_episode() {
    for n in [1, 4, 8] {
        for dt in [0.01, 0.0005, 0.0001] {
            for parts in [&[1, 1][..], &[4, 2]] {
                for copies in [1, 2] {
                    for reverse in [false, true] {
                        let (mut scene, tokens) = verified(parts, copies, reverse, 0.01, 10.0);
                        scene.world.raw.integration_parameters.num_solver_iterations = n;
                        let capacity = balances(&scene.world, &tokens);
                        let initial = mechanical(&scene.world, 10.0);
                        let mut impacts = Vec::new();
                        let mut settlements = Vec::new();
                        let mut events = Vec::new();
                        let mut work = 0.0;
                        let mut supports = Vec::new();
                        for _ in 0..(0.01_f32 / dt).round() as usize {
                            let trace = replayed(&mut scene, dt);
                            work += trace.flights.iter().map(|f| f.gravity_work).sum::<f64>();
                            impacts.extend(trace.impacts);
                            settlements.extend(trace.settlements);
                            events.extend(trace.events);
                            supports.extend(trace.supports);
                        }
                        assert_eq!(impacts.len(), 1);
                        assert_eq!(settlements.len(), 1);
                        assert_eq!(events.len(), 1);
                        assert_eq!(events[0].event.transition, EpisodeTransition::Impact);
                        let owner = events[0].event.id;
                        assert_eq!(impacts[0].response.roots, vec![owner]);
                        let settlement = &settlements[0];
                        assert_eq!(settlement.episode, owner);
                        assert!(settlement.dissipated > 0.0);
                        assert!(settlement.rounding_loss <= 1.0e-12 * settlement.dissipated);
                        near(
                            settlement.impulse + settlement.rounding_impulse,
                            libm::sqrt(40.0 * settlement.dissipated),
                            1.0e-5 * settlement.impulse,
                        );
                        assert!(settlement.dissipated <= settlement.available * (1.0 + 1.0e-5));
                        near(
                            settlement.available,
                            initial - loss(&impacts[0].response),
                            1.0e-5 * initial,
                        );
                        assert!(!supports.is_empty());
                        for support in supports {
                            assert_eq!(support.episode, owner);
                            assert_eq!(support.gravity_work, 0.0);
                        }
                        near(
                            mechanical(&scene.world, 10.0)
                                + loss(&impacts[0].response)
                                + settlement.dissipated,
                            initial,
                            1.0e-5 * initial,
                        );
                        near(
                            loss(&impacts[0].response) + settlement.dissipated,
                            initial + work,
                            1.0e-5 * initial,
                        );
                        assert_eq!(state(&scene.world).1, 0.0);
                        assert_eq!(balances(&scene.world, &tokens), capacity);
                    }
                }
            }
        }
    }
}

#[test]
fn ballistic_support_pause_limits_and_overload_are_atomic() {
    let (mut scene, _) = verified(&[4, 2], 2, true, 0.01, 10.0);
    let original = scene.snapshot();
    refused(&mut scene, 0.01, 2, Rejection::Limit);
    let trace = replayed(&mut scene, 0.01);
    assert_eq!(trace.settlements.len(), 1);
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
    let bytes = scene.snapshot();
    assert_eq!(scene.step(0.0, 0).unwrap(), Trace::default());
    assert_eq!(scene.snapshot(), bytes);
    assert!(scene.world.loading_batches().is_empty());
    assert!(scene.world.material_response_batches().is_empty());
    let continued = replayed(&mut scene, 1.0);
    assert!(continued.impacts.is_empty());
    assert!(continued.settlements.is_empty());
    assert_eq!(continued.supports.len(), 1);
    refused(&mut scene, 1.0, 0, Rejection::Limit);
    for speed in [0.0, 0.5] {
        let (mut overloaded, _) = verified(&[1, 1], 1, false, speed, 10_000.0);
        refused(&mut overloaded, 0.01, MAX_ADVANCES, Rejection::Support);
    }
    let (mut fresh, _) = verified(&[1, 1], 1, false, 0.0, 10.0);
    fresh
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = 1;
    refused(
        &mut fresh,
        1.0,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );

    // Failure remains disqualifying even though this material law keeps its
    // strength threshold constant while the remaining capacity decreases.
    let (mut failed, tokens) = verified(&[1, 1], 1, false, 0.0, 10.0);
    failed
        .world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .get_mut(&tokens[1])
        .unwrap()
        .remaining = 0.0;
    refused(&mut failed, 0.01, MAX_ADVANCES, Rejection::Support);

    for edit in 0..3 {
        let mut changed = Verified::restore(&bytes);
        match edit {
            0 => {
                changed.world.set_velocity(body(1), Vec2::X, 0.0, true);
            }
            1 => {
                changed.world.set_gravity(Vec2::X * 11.0);
            }
            _ => {
                assert!(changed.world.retire_loading_region(
                    changed.world.loading.as_ref().unwrap().bindings[&collider(1, 0)].region
                ));
            }
        }
        refused(&mut changed, 0.01, MAX_ADVANCES, Rejection::Evidence);
    }
}

#[test]
fn ballistic_support_settles_the_previously_uncertain_excursion() {
    // The eventual apex is about 1.90925e-5 from the face. The old unit-size
    // drift allowance straddled the 2e-5 exit band; the tighter bound does not.
    let (mut scene, tokens) = verified(&[1, 1], 1, false, 4.0, 10.0);
    scene.world.raw.integration_parameters.num_solver_iterations = 4;
    let initial = mechanical(&scene.world, 10.0);
    let mut impacts = Vec::new();
    for _ in 0..1000 {
        let trace = replayed(&mut scene, 1.0 / 240.0);
        impacts.extend(trace.impacts);
        if let Some(settlement) = trace.settlements.first() {
            assert_eq!(trace.settlements.len(), 1);
            assert!(impacts.len() > 3);
            assert_eq!(
                settlement.episode,
                impacts.last().unwrap().response.roots[0]
            );
            near(
                mechanical(&scene.world, 10.0)
                    + settlement.dissipated
                    + impacts.iter().map(|i| loss(&i.response)).sum::<f64>(),
                initial,
                1.0e-5 * initial,
            );
            let capacity = balances(&scene.world, &tokens);
            let rounding = scene.tracker.rounding.clone();
            let held = replayed(&mut scene, 0.1);
            assert!(held.impacts.is_empty() && held.settlements.is_empty());
            assert_eq!(balances(&scene.world, &tokens), capacity);
            assert_eq!(scene.tracker.rounding, rounding);
            return;
        }
    }
    panic!("expected the final excursion to settle");
}

#[test]
fn ballistic_support_still_refuses_genuinely_ambiguous_separation() {
    let (mut reference, _) = fixture(&[1, 1], 1, false, 0.0, 0.01, 0.0);
    reference.step(0.0001);
    let speed = state(&reference).1;
    let height = speed * speed / 20.0;
    let (mut scene, _) = verified(&[4, 2], 2, true, 0.01, 10.0);
    scene.world.loading.as_mut().unwrap().config.touch_tolerance = (height / 2.0) as f32;
    let apex = departure::apex_duration(speed, 10.0).unwrap();
    let trace = replayed(&mut scene, apex);
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.events.len(), 1);
    let flight = trace.flights.last().unwrap();
    let exit = 2.0 * f64::from(scene.world.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(flight.gap_upper > exit);
    assert!((-state(&scene.world).0).min(-flight.predicted_x) - flight.uncertainty <= exit);
    refused(&mut scene, 0.01, MAX_ADVANCES, Rejection::Reimpact);
}

#[test]
fn ballistic_support_follows_genuine_returns_without_new_resting_budgets() {
    for speed in [0.5, 3.0, 4.0] {
        let mut materials = None;
        let mut incoming = speed;
        let mut expected = Vec::new();
        let mut expected_capacity = Vec::new();
        let mut settling_at = 0.0;
        let mut settling_speed = 0.0;
        // Independent ordinary-contact sequence at the exact ballistic return
        // speeds. Stop at a hop strictly inside the capture exit band.
        for _ in 0..64 {
            let (mut reference, tokens) = fixture(&[1, 1], 1, false, 0.0, incoming, 0.0);
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
            let response = applied(&reference);
            let rebound = -state(&reference).1;
            expected.push(loss(response));
            expected_capacity = balances(&reference, &tokens);
            let exit = 2.0 * f64::from(reference.loading.as_ref().unwrap().config.touch_tolerance);
            let height = rebound * rebound / 20.0;
            settling_at += 2.0 * rebound / 10.0;
            if height < exit {
                assert!(
                    height < exit - 1.0e-8,
                    "ambiguous reference height {height}"
                );
                settling_speed = rebound;
                break;
            }
            assert!(
                height > exit + 1.0e-8,
                "ambiguous reference height {height}"
            );
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
                let (mut scene, tokens) = verified(&[4, 2], 2, true, speed, 10.0);
                scene.world.raw.integration_parameters.num_solver_iterations = n;
                let initial = mechanical(&scene.world, 10.0);
                let mut elapsed = 0.0;
                let mut impacts = Vec::new();
                let mut settlements = Vec::new();
                let mut events = Vec::new();
                let mut support_at = None;
                while elapsed < settling_at + 0.1 {
                    let trace = replayed(&mut scene, 1.0 / hz);
                    if let Some(support) = trace.supports.first() {
                        support_at.get_or_insert(elapsed + support.at);
                    }
                    impacts.extend(trace.impacts);
                    settlements.extend(trace.settlements);
                    events.extend(trace.events);
                    elapsed += f64::from(1.0 / hz);
                }
                assert_eq!(impacts.len(), expected.len());
                assert_eq!(settlements.len(), 1);
                // Touching admission includes TOUCH of geometric uncertainty.
                // It can stop the last approach just before exact face arrival.
                near(
                    support_at.unwrap(),
                    settling_at,
                    2.0 * TOUCH / settling_speed,
                );
                for (impact, expected) in impacts.iter().zip(&expected) {
                    near(loss(&impact.response), *expected, 1.0e-5 * initial);
                }
                assert_eq!(events.len(), 2 * impacts.len() - 1);
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
                assert_eq!(
                    settlements[0].episode,
                    impacts.last().unwrap().response.roots[0]
                );
                let spent = impacts.iter().map(|i| loss(&i.response)).sum::<f64>()
                    + settlements[0].dissipated;
                near(
                    mechanical(&scene.world, 10.0) + spent,
                    initial,
                    1.0e-5 * initial,
                );
                for (actual, expected) in balances(&scene.world, &tokens)
                    .iter()
                    .zip(&expected_capacity)
                {
                    near(*actual, *expected, 1.0e-5 * initial);
                }
                assert_eq!(state(&scene.world).1, 0.0);
            }
        }
    }
}
