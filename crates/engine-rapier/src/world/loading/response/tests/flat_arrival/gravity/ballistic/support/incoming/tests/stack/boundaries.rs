use super::*;

fn projection_refused(scene: &mut Verified, dt: f32, projections: usize) {
    let bytes = scene.snapshot();
    let loading = scene.world.loading_batches().to_vec();
    let response = scene.world.material_response_batches().to_vec();
    assert_eq!(
        scene
            .step_with_projection_limit(dt, MAX_ADVANCES, projections)
            .unwrap_err(),
        Rejection::Limit
    );
    assert_eq!(scene.snapshot(), bytes);
    assert_eq!(scene.world.loading_batches(), loading);
    assert_eq!(scene.world.material_response_batches(), response);
}

#[test]
fn ballistic_stack_strict_support_accepts_an_exact_stop_but_refuses_a_contained_return() {
    let (mut stopped, tokens) = stacked(&[4, 2, 4], 2, true, 0.05, 10.0, false);
    for material in stopped
        .world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials
        .values_mut()
    {
        material.parameters[4] = 1.0;
    }
    let capacity = balances(&stopped.world, &tokens);
    let trace = replayed(&mut stopped, 0.01);
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.impacts[0].outgoing, 0.0);
    assert!(trace.stack_settlements.is_empty());
    assert_eq!(
        trace.stack_supports[0].episodes[0],
        trace.impacts[0].response.roots[0]
    );
    assert_eq!(balances(&stopped.world, &tokens), capacity);

    let (mut strict, _) = stacked(&[1, 1, 1], 1, false, 0.01, 10.0, false);
    replayed(&mut strict, 0.000001);
    refused(&mut strict, 0.01, MAX_ADVANCES, Rejection::Reimpact);
}

#[test]
fn ballistic_stack_checks_combined_weight_and_both_loaded_regions() {
    // B alone is safe (4000), but B-C cannot hold A and B together (8000).
    let (mut overloaded, _) = stacked(&[1, 1, 1], 1, false, 0.0, 200.0, true);
    let plates = Plates::new(&overloaded.world).unwrap();
    overloaded.safe_support(&plates, 0.01).unwrap();
    assert_eq!(
        overloaded.safe_stack(&plates, 0.01).unwrap_err(),
        Rejection::Support
    );
    refused(&mut overloaded, 0.01, MAX_ADVANCES, Rejection::Support);
    let (mut landing, _) = stacked(&[1, 1, 1], 1, false, 0.01, 200.0, true);
    replayed(&mut landing, 0.000001);
    refused(&mut landing, 0.01, MAX_ADVANCES, Rejection::Support);
    for side in 0..3 {
        let (mut failed, tokens) = stacked(&[4, 2, 4], 2, true, 0.0, 10.0, true);
        failed
            .world
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .get_mut(&tokens[side])
            .unwrap()
            .remaining = 0.0;
        refused(&mut failed, 0.01, MAX_ADVANCES, Rejection::Support);
    }
}

#[test]
fn ballistic_stack_limits_roll_back_capture_settlement_and_projection() {
    let (mut fresh, _) = stacked(&[4, 2, 4], 2, true, 0.0, 10.0, true);
    fresh.world.raw.integration_parameters.num_solver_iterations = 1;
    projection_refused(&mut fresh, 0.01, 0);
    projection_refused(&mut fresh, 0.01, 1);
    let fresh_bytes = fresh.snapshot();
    let trace = replayed(&mut fresh, 0.01);
    let solves = trace.stack_supports[0].solves;
    assert!(solves > 2);
    let mut limited = Verified::restore(&fresh_bytes);
    projection_refused(&mut limited, 0.01, solves - 1);

    let (mut scene, _) = stacked(&[4, 2, 4], 2, true, 0.01, 10.0, true);
    let first = replayed(&mut scene, 0.000001);
    assert_eq!(first.impacts.len(), 1);
    let bytes = scene.snapshot();
    let mut completed = Verified::restore(&bytes);
    let trace = replayed(&mut completed, 0.01);
    assert_eq!(trace.stack_settlements.len(), 1);
    let solves = trace
        .stack_settlements
        .iter()
        .map(|s| s.solves)
        .sum::<usize>()
        + trace.stack_supports.iter().map(|s| s.solves).sum::<usize>();
    projection_refused(&mut scene, 0.01, solves - 1);
    refused(&mut scene, 0.01, 2, Rejection::Limit);
    let mut record_limited = Verified::restore(&bytes);
    record_limited
        .world
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = completed.world.loading.as_ref().unwrap().records - 1;
    refused(
        &mut record_limited,
        0.01,
        MAX_ADVANCES,
        Rejection::Backend(Refusal::Response(ResponseRejection::Capture(
            CaptureRejection::StepLimit,
        ))),
    );
    for edit in 0..3 {
        let mut changed = Verified::restore(&bytes);
        match edit {
            0 => {
                changed
                    .world
                    .set_velocity(body(2), Vec2::X * 0.001, 0.0, true);
            }
            1 => {
                changed.witness.incoming = Some(EpisodeId(999));
            }
            _ => {
                changed.world.set_gravity(Vec2::X * 11.0);
            }
        }
        refused(
            &mut changed,
            0.01,
            MAX_ADVANCES,
            if edit == 0 {
                Rejection::Geometry
            } else {
                Rejection::Evidence
            },
        );
    }
}

#[test]
fn ballistic_stack_frame_end_settlement_preserves_owners_through_pause() {
    let (mut scene, tokens) = stacked(&[4, 2, 4], 2, true, 0.01, 10.0, true);
    let capacity = balances(&scene.world, &tokens);
    let first = replayed(&mut scene, 0.000001);
    let owner = first.impacts[0].response.roots[0];
    let apex = departure::apex_duration(state(&scene.world).1, 10.0).unwrap();
    assert!(replayed(&mut scene, apex).stack_settlements.is_empty());
    let (x, v, _) = state(&scene.world);
    let duration = arrival(-x, v, 10.0) as f32;
    let trace = replayed(&mut scene, duration);
    assert_eq!(trace.stack_settlements.len(), 1);
    assert_eq!(trace.stack_settlements[0].at, f64::from(duration));
    assert_eq!(
        trace.stack_settlements[0].episodes,
        [owner, scene.witness.support]
    );
    assert!(trace.stack_supports.is_empty());
    let snapshot = scene.snapshot();
    assert_eq!(
        scene.step_with_projection_limit(0.0, 0, 0).unwrap(),
        Trace::default()
    );
    assert_eq!(scene.snapshot(), snapshot);
    let held = replayed(&mut scene, f32::from_bits(0x30000000));
    assert!(held.impacts.is_empty() && held.stack_settlements.is_empty() && held.events.is_empty());
    assert_eq!(
        held.stack_supports[0].episodes,
        [owner, scene.witness.support]
    );
    assert_eq!(balances(&scene.world, &tokens), capacity);
}

#[test]
fn ballistic_stack_recoil_correction_has_fixed_cumulative_limits() {
    let (mut scene, _) = stacked(&[4, 2, 4], 2, false, 0.5, 10.0, true);
    scene.world.raw.integration_parameters.num_solver_iterations = 1;
    let dt = 1.0 / 120.0;
    for _ in 0..1000 {
        let bytes = scene.snapshot();
        let mut trial = Verified::restore(&bytes);
        let trace = trial.step(dt, MAX_ADVANCES).unwrap();
        if let Some(correction) = trace.support_corrections.first() {
            assert!(correction.dissipated > 0.0);
            for axis in 0..2 {
                let mut limited = Verified::restore(&bytes);
                let h = limited.witness.stack.as_mut().unwrap();
                if axis == 0 {
                    h.correction_impulse = correction.impulse.signum()
                        * 1.0e-5
                        * limited.witness.rounding.momentum_scale;
                } else {
                    h.correction_work = 1.0e-5 * limited.witness.rounding.energy_scale;
                }
                refused(&mut limited, dt, MAX_ADVANCES, Rejection::Energy);
            }
            // A materially moving support cannot be stopped by this numerical
            // correction. Its whole velocity must fit the residual bound.
            let mut excessive = Verified::restore(&bytes);
            let plates = Plates::new(&excessive.world).unwrap();
            excessive.world.raw.bodies[plates.handles[1]].set_linvel(Vector::X * 0.001, true);
            let snapshot = excessive.snapshot();
            let mut output = Trace::default();
            assert_eq!(
                excessive
                    .correct_support_recoil(&plates, &mut output, 0.0, 1.0)
                    .unwrap_err(),
                Rejection::Energy
            );
            assert_eq!(excessive.snapshot(), snapshot);
            assert_eq!(output, Trace::default());
            return;
        }
        scene = trial;
    }
    panic!("expected the reproduced support recoil");
}

#[test]
fn ballistic_stack_refuses_an_ambiguous_excursion_without_new_work() {
    let (mut reference, _) = fixture(&[1, 1, 1], 1, false, 0.0, 0.01, 0.0);
    reference.step(0.00001);
    let rebound = state(&reference).1;
    let height = rebound * rebound / 20.0;
    let (mut scene, _) = stacked(&[4, 2, 4], 2, true, 0.01, 10.0, true);
    scene.world.loading.as_mut().unwrap().config.touch_tolerance = (height / 2.0) as f32;
    let trace = replayed(&mut scene, departure::apex_duration(rebound, 10.0).unwrap());
    assert_eq!(trace.impacts.len(), 1);
    assert_eq!(trace.events.len(), 1);
    let flight = trace.flights.last().unwrap();
    let exit = 2.0 * f64::from(scene.world.loading.as_ref().unwrap().config.touch_tolerance);
    assert!(flight.gap_upper > exit);
    assert!((-state(&scene.world).0).min(-flight.predicted_x) - flight.uncertainty <= exit);
    refused(&mut scene, 0.01, MAX_ADVANCES, Rejection::Reimpact);
}
