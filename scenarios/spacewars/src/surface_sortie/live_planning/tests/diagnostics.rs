use super::*;

#[test]
fn negative_results_need_the_whole_hypothesis_and_preserve_the_rejection_evidence() {
    let (state, source, _) = route_dependencies::fixture();
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    let mut o = source.clone();
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let request = live.requests[&0].clone();
    let job = live.queue.job(request.token).unwrap();
    let mut negative = job.output().unwrap().clone();
    assert!(!negative.sites.is_empty());
    for route in &mut negative.sites {
        route.outbound.failure = Some(ground_navigation::GroundRouteFailure::Disconnected);
        route.returning = None;
    }
    negative.actual = None;
    let p = &source.combat.recovery.flight.pilot;
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    for changed in [false, true] {
        let mut request = request.clone();
        if changed {
            request.gravity += 2.0;
        }
        let (result, evidence) = LiveObjectivePlanner::locally_validated_with_evidence(
            &state,
            0,
            p,
            &request,
            job,
            negative.clone(),
            &mut LivePlanningTelemetry::default(),
        );
        assert_eq!(evidence.source.usable, 0);
        assert_eq!(evidence.source.disconnected, negative.sites.len());
        assert_eq!(evidence.whole_region_valid, Some(true));
        assert_eq!(evidence.scalar_gravity_valid, Some(!changed));
        assert_eq!(
            evidence.decision,
            if changed {
                PublicationDecision::NoValidatedRoutes
            } else {
                PublicationDecision::WholeSurvey
            }
        );
        assert_eq!(result.is_some(), !changed);
    }
    assert_eq!(before, state.world.physics.world.snapshot_bytes().unwrap());
}

#[test]
fn invalidation_binds_the_examined_request_instead_of_its_replacement() {
    let state = state();
    let mut live = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 1,
            physics_queries: 1,
        },
    )
    .with_route_dependencies();
    let mut source = target(&state, 0);
    live.observe(&state, 0, &mut source);
    let original = source.objective_evidence.unwrap();
    assert_eq!(original.measurement_tick, Some(state.world.tick));
    assert!(original.generation.is_some());
    let mut changed = target(&state, 0);
    changed
        .combat
        .recovery
        .flight
        .pilot
        .planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position += Vec2::X;
    live.observe(&state, 0, &mut changed);
    let evidence = changed.objective_evidence.unwrap();
    assert_eq!(evidence.invalidated_by, Some("objective_changed"));
    assert_eq!(evidence.generation, original.generation);
    assert_eq!(evidence.source_objective, original.source_objective);
    assert_ne!(evidence.objective, original.objective);
    assert_ne!(
        live.requests[&0].token.generation,
        evidence.generation.unwrap()
    );
    changed.combat.recovery.flight.pilot.queries_ready = false;
    live.observe(&state, 0, &mut changed);
    assert!(changed.objective_evidence.is_none());
    assert!(changed.objective_work.is_none());
}

#[test]
fn publication_keeps_measurement_age_and_partial_actual_return_is_distinct() {
    let (state, source, _) = route_dependencies::fixture();
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    let mut o = source.clone();
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let request = live.requests[&0].clone();
    o = source.clone();
    live.observe(&state, 0, &mut o);
    let evidence = o.objective_evidence.unwrap();
    assert_eq!(evidence.measurement_tick, Some(request.measurement_tick));
    assert_eq!(evidence.generation, Some(request.token.generation));
    assert!(evidence.publication.unwrap().source.usable > 0);
    assert_eq!(o.objective_work, Some(ObjectiveWorkState::Ready));

    let job = live.queue.job(request.token).unwrap();
    let mut partial = job.output().unwrap().clone();
    partial.validated_routes_only = true;
    partial.actual = None;
    let mut landed = request;
    landed.actual = Some(ActualLanding {
        vehicle: Vec2::ZERO,
        angle: 0.0,
        exit: Vec2::ZERO,
        boarding_hatches: [None; 2],
    });
    let (result, evidence) = LiveObjectivePlanner::locally_validated_with_evidence(
        &state,
        0,
        &source.combat.recovery.flight.pilot,
        &landed,
        job,
        partial,
        &mut LivePlanningTelemetry::default(),
    );
    assert!(result.is_none());
    assert_eq!(evidence.decision, PublicationDecision::MissingActualReturn);
    assert!(evidence.whole_region_valid.is_none());
    assert!(evidence.partial_survey);
}
