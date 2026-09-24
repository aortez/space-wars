use super::*;
use std::cell::Cell;

fn partial_request(
    state: &SurfaceSortieState,
    live: &mut LiveObjectivePlanner,
    seat: usize,
) -> combat::TacticalSortieObservationV1 {
    let mut o = target(state, seat);
    o.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    live.observe(state, seat, &mut o);
    // Stop at an exact job boundary to exercise publication independently of
    // scheduling. Integration tests below run the ordinary shared dispatch.
    let old = live.requests[&seat].token;
    let mut job = live.queue.take(old).unwrap();
    while job.positive_candidates().is_none() {
        assert!(job.next_work().is_some());
        job.step();
    }
    assert!(job.output().is_none());
    let token = live
        .queue
        .submit(seat as u64, state.world.tick, JobLimits::default(), job)
        .unwrap();
    live.requests.get_mut(&seat).unwrap().token = token;
    o
}

fn deferred(o: &mut combat::TacticalSortieObservationV1, tick: u64) {
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = tick;
    p.sites.clear();
    p.site_query = LandingSiteQuery::Deferred {
        next_tick: tick + 30,
    };
}

#[test]
fn partial_candidates_have_both_legs_and_dependencies_without_finishing_the_survey() {
    let state = state();
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED).with_route_dependencies();
    let o = partial_request(&state, &mut live, 0);
    let token = live.requests[&0].token;
    let mut job = live.queue.take(token).unwrap();
    let measured = job.measurement_work().clone();
    let partial = job.positive_candidates().unwrap();
    assert!(partial.validated_routes_only);
    assert!(partial.validated_tick.is_none());
    assert!(!partial.sites.is_empty());
    assert!(partial.sites.iter().all(|r| {
        r.cost().is_some()
            && r.returning.is_some()
            && job
                .dependencies()
                .iter()
                .any(|(id, areas)| *id == r.site && !areas.is_empty())
    }));
    assert!(job.next_work().is_some());
    assert_eq!(job.positive_candidates(), Some(partial));
    assert_eq!(job.measurement_work(), &measured);
    while job.next_work().is_some() {
        job.step();
    }
    let expected = state
        .landing_objective_survey(
            0,
            &o.combat.recovery.flight.pilot,
            &o.cover,
            ObjectivePlanning::JointRoundTrip,
        )
        .unwrap();
    assert_eq!(job.output(), Some(&expected));
}

#[test]
fn unfinished_actual_return_blocks_partial_prospective_publication_in_an_unchanged_world() {
    let state = state();
    let mut o = target(&state, 0);
    let p = &mut o.combat.recovery.flight.pilot;
    p.landing.phase = LandingPhase::Landed;
    p.hatch = Some(p.sites[0].hatch_position);
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED)
        .with_route_dependencies()
        .with_early_candidates();
    live.observe(&state, 0, &mut o);
    let request = &live.requests[&0];
    assert!(request.actual.is_some());
    let mut job = live.queue.job(request.token).unwrap().clone();
    while job.positive_candidates().is_none() {
        job.step();
    }
    let partial = job.positive_candidates().unwrap();
    assert!(partial.actual.is_none());
    assert!(
        LiveObjectivePlanner::locally_validated(
            &state,
            0,
            &o.combat.recovery.flight.pilot,
            request,
            &job,
            partial,
            &mut LivePlanningTelemetry::default(),
        )
        .is_none()
    );
}

#[test]
fn budgeted_site_checks_match_normal_geometry_and_stop_at_query_exhaustion() {
    let state = state();
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let o = target(&state, 0);
    let planet = o.combat.recovery.flight.pilot.planet.index;
    let mut clear = 0;
    for bearing in 0..pilot::LANDING_SITE_COUNT {
        let id = LandingSiteId { planet, bearing };
        let expected = state.vehicle_landing_site(0, id, false);
        let used = Cell::new(0);
        let observed = state.vehicle_landing_site_with_queries(0, id, false, || {
            used.set(used.get() + 1);
            true
        });
        assert_eq!(observed, expected);
        assert!(used.get() <= 192, "{bearing}: {}", used.get());
        if expected.is_some() {
            clear += 1;
            for limit in [0, 1, used.get() - 1] {
                let used = Cell::new(0);
                let exhausted = Cell::new(false);
                let result = state.vehicle_landing_site_with_queries(0, id, false, || {
                    if used.get() == limit {
                        exhausted.set(true);
                        false
                    } else {
                        used.set(used.get() + 1);
                        true
                    }
                });
                assert!(exhausted.get());
                assert_eq!(used.get(), limit);
                // A truncated check is unknown even when its already measured
                // exit would let the ordinary function return a partial site.
                assert!(result.filter(|_| !exhausted.get()).is_none());
            }
        }
    }
    assert!(clear > 0);
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
}

#[test]
fn early_clearance_is_charged_once_to_the_shared_allowance_even_after_cancellation() {
    let mut state = state();
    let allowance = Work {
        graph: 512,
        physics_queries: 1024,
    };
    let mut live = LiveObjectivePlanner::new(2, allowance)
        .with_route_dependencies()
        .with_early_candidates();
    let mut sources: Vec<_> = (0..2)
        .map(|seat| partial_request(&state, &mut live, seat))
        .collect();
    let tokens = [live.requests[&0].token, live.requests[&1].token];
    state.world.tick += 1;
    for (seat, o) in sources.iter_mut().enumerate() {
        deferred(o, state.world.tick);
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        live.observe(&state, seat, o);
        assert_eq!(o.objective_work, Some(ObjectiveWorkState::Ready));
        assert_eq!(live.requests[&seat].token, tokens[seat]);
        assert!(!live.requests[&seat].completed);
        assert_eq!(live.telemetry.completed, 0);
        assert_eq!(
            o.landing_objective.as_ref().unwrap().tick,
            state.world.tick - 1
        );
        assert!(matches!(
            o.combat.recovery.flight.pilot.site_query,
            LandingSiteQuery::Selected(_)
        ));
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        let spent = live.telemetry.early_candidates.physics_queries;
        deferred(o, state.world.tick);
        live.observe(&state, seat, o);
        assert_eq!(live.telemetry.early_candidates.physics_queries, spent);
        assert!(o.combat.recovery.flight.pilot.sites.is_empty());
    }
    assert_eq!(live.telemetry.early_candidates.site_checks, 2);
    let early = live.telemetry.early_candidates.physics_queries;
    assert!(early > 0 && early <= 384);
    // Removing a job between sensing and dispatch must retain its spent fuel.
    live.remove(0);
    assert_eq!(live.telemetry.retired_unpublished_queries, 0);
    let report = live.advance(state.world.tick).unwrap();
    assert!(report.charged.physics_queries <= allowance.physics_queries);
    assert!(report.charged.graph <= allowance.graph);
    assert_eq!(report.allowance, allowance);
    assert_eq!(
        report.charged.physics_queries,
        report
            .jobs
            .iter()
            .map(|j| j.charged.physics_queries)
            .sum::<u32>()
    );
    assert_eq!(
        live.telemetry.physics_queries,
        u64::from(report.charged.physics_queries)
    );
    assert!(
        report
            .jobs
            .iter()
            .any(|j| j.request == tokens[0] && j.charged.physics_queries > 0)
    );
    assert!(live.advance(state.world.tick).is_none());
    // A post-dispatch observation cannot open a second allowance.
    deferred(&mut sources[1], state.world.tick);
    live.observe(&state, 1, &mut sources[1]);
    assert_eq!(live.telemetry.early_candidates.physics_queries, early);
    live.reset();
    assert_eq!(live.query_budget.charged_queries(), 0);
    assert_eq!(live.telemetry.early_candidates.site_checks, 0);
}

#[test]
fn small_budgets_and_absent_demand_publish_paths_without_granting_fresh_clearance() {
    for (budget, query) in [
        (0, LandingSiteQuery::Deferred { next_tick: 210 }),
        (191, LandingSiteQuery::Deferred { next_tick: 210 }),
        (1024, LandingSiteQuery::NotRequested),
    ] {
        let state = state();
        let mut live = LiveObjectivePlanner::new(
            1,
            Work {
                graph: 0,
                physics_queries: budget,
            },
        )
        .with_route_dependencies()
        .with_early_candidates();
        let mut o = partial_request(&state, &mut live, 0);
        deferred(&mut o, state.world.tick);
        o.combat.recovery.flight.pilot.site_query = query;
        live.observe(&state, 0, &mut o);
        assert!(o.landing_objective.is_some());
        assert!(o.combat.recovery.flight.pilot.sites.is_empty());
        assert_eq!(o.combat.recovery.flight.pilot.site_query, query);
        assert_eq!(live.telemetry.early_candidates.site_checks, 0);
    }
}

#[test]
fn a_revoked_partial_route_reports_stale_once_without_restarting_remaining_work() {
    let (mut state, _, body) = route_dependencies::fixture();
    let mut live = LiveObjectivePlanner::new(1, Work::UNLIMITED)
        .with_route_dependencies()
        .with_early_candidates();
    let mut o = partial_request(&state, &mut live, 0);
    live.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_some());
    let request = live.requests[&0].clone();
    let job = live.queue.job(request.token).unwrap();
    let area = *job.dependencies()[0]
        .1
        .iter()
        .find(|a| a.groups == SurfaceSortieState::spec().collision_groups)
        .unwrap();
    let frame = o.combat.recovery.flight.pilot.planet.motion;
    let blocked =
        frame.position + ((area.minimum + area.maximum) * 0.5).rotate_radians(frame.angle);
    route_dependencies::move_body(&mut state, body, blocked);
    o.combat.recovery.flight.pilot.tick = state.world.tick;
    for expected in [ObjectiveWorkState::Stale, ObjectiveWorkState::Pending] {
        live.observe(&state, 0, &mut o);
        assert_eq!(o.objective_work, Some(expected));
        assert!(o.landing_objective.is_none());
        assert_eq!(live.requests[&0].token, request.token);
        assert_eq!(live.requests[&0].measurement_tick, request.measurement_tick);
        assert!(live.queue.job(request.token).unwrap().output().is_none());
    }
    live.advance(state.world.tick);
    assert!(
        live.queue.job(request.token).unwrap().output().is_some(),
        "other candidates continue"
    );
    // Completed or partial delivery never extends the source's lifetime.
    state.world.tick = request.measurement_tick + MAX_SURVEY_AGE_TICKS + 1;
    deferred(&mut o, state.world.tick);
    live.observe(&state, 0, &mut o);
    assert!(o.landing_objective.is_none());
    assert_eq!(live.telemetry.invalidations.get("expired"), Some(&1));
    assert_eq!(
        live.queue.poll(request.token, &request.tick),
        JobPoll::Stale
    );
}
