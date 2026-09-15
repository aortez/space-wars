use super::*;

#[test]
fn changed_flight_environment_preserves_pending_ground_routes_but_never_powered_answers() {
    let mut state = SurfaceSortieScenario::init_material_flag_crossing_trial(42, 0);
    let mut source =
        state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    // Isolate proposed sites: this test is about evidence, not a touchdown action.
    source.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    let mut live = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 16384,
            physics_queries: 1024,
        },
    )
    .with_route_dependencies();
    let mut observed = source.clone();
    live.observe_with_planning(
        &state,
        0,
        &mut observed,
        ObjectivePlanning::JetpackRoundTrip,
    );
    let token = live.requests[&0].token;
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let mut changed = false;
    let mut published = None;
    // Hold collider poses fixed while changing just the observed spin. This
    // separates flight validity from terrain, hatch and scalar jump gravity.
    for age in 0..MAX_SURVEY_AGE_TICKS {
        observed = source.clone();
        observed.combat.recovery.flight.pilot.tick += age;
        state.world.tick = observed.combat.recovery.flight.pilot.tick;
        changed |= live.requests[&0].flight_dependent;
        if changed {
            observed.combat.recovery.flight.pilot.planet.motion.spin += 0.001;
        }
        live.observe_with_planning(
            &state,
            0,
            &mut observed,
            ObjectivePlanning::JetpackRoundTrip,
        );
        assert_eq!(
            live.requests[&0].token, token,
            "independent ground work was restarted"
        );
        if observed.landing_objective.is_some() {
            published = observed.landing_objective;
            break;
        }
        let report = live
            .advance(observed.combat.recovery.flight.pilot.tick)
            .unwrap();
        assert!(report.charged.graph <= 16384 && report.charged.physics_queries <= 1024);
    }
    assert!(changed);
    let published = published.expect("ground routes complete at the unchanged quota");
    assert!(published.validated_routes_only);
    assert!(!published.sites.is_empty());
    assert!(
        published
            .sites
            .iter()
            .all(|r| r.cost().is_some() && r.crossing.is_none())
    );
    let job = live.queue.job(token).unwrap();
    let original = job.output().unwrap();
    assert!(
        original.sites.iter().any(|r| r.crossing.is_some()),
        "{original:?}"
    );
    assert!(original.sites.len() > published.sites.len());
    assert_eq!(live.telemetry.flight_environment_mismatches, 1);
    assert_eq!(live.telemetry.flight_independent_validations, 1);
    assert!(live.telemetry.withheld_flight_routes > 0);
    assert_eq!(
        live.telemetry.flight_forecasts.approved,
        job.flight_work().approved
    );
    assert_eq!(
        live.telemetry.flight_forecasts.started,
        live.telemetry.flight_forecasts.approved
            + live
                .telemetry
                .flight_forecasts
                .rejected
                .values()
                .sum::<u64>()
    );
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);

    // At the original environment the same completed hypothesis still includes
    // its powered result. Filtering did not mutate retained queue output.
    let request = &live.requests[&0];
    let p = &source.combat.recovery.flight.pilot;
    let restored = LiveObjectivePlanner::locally_validated(
        &state,
        0,
        p,
        request,
        job,
        original.clone(),
        &mut LivePlanningTelemetry::default(),
    )
    .unwrap();
    assert_eq!(&restored, original);
    assert!(LiveObjectivePlanner::valid(&state, 0, p, request, request.objective, false).is_ok());
    let mut moved = p.clone();
    moved.planet.motion.spin += 0.001;
    assert_eq!(
        LiveObjectivePlanner::valid(&state, 0, &moved, request, request.objective, false),
        Err("flight_environment_changed"),
        "whole-region mode retains its strict contract"
    );
}

#[test]
fn stale_powered_actual_route_cannot_be_replaced_by_an_unrelated_landing() {
    let state = SurfaceSortieScenario::init_material_flag_crossing_trial(42, 0);
    let mut o = state.tactical_sortie_observation_for_live_planning(0, LandingSiteQuery::Survey);
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    live.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
    live.advance(o.combat.recovery.flight.pilot.tick);
    let request = &live.requests[&0];
    let job = live.queue.job(request.token).unwrap();
    let survey = job.output().unwrap();
    assert!(survey.actual.as_ref().unwrap().crossing.is_some());
    assert!(
        survey
            .sites
            .iter()
            .any(|r| r.cost().is_some() && r.crossing.is_none())
    );
    let mut moved = o.combat.recovery.flight.pilot.clone();
    moved.planet.motion.spin += 0.001;
    let mut telemetry = LivePlanningTelemetry::default();
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            &moved,
            request,
            job,
            survey.clone(),
            &mut telemetry
        )
        .is_none()
    );
    assert!(telemetry.withheld_flight_routes > 0);
    assert_eq!(telemetry.flight_independent_validations, 0);
}
