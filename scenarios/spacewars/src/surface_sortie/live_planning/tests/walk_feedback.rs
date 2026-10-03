use super::*;
use requested_corridors::{make_job_mode, smooth_state, target_at_angle};

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

fn planner(enabled: bool, work: Work) -> LiveObjectivePlanner {
    let p = LiveObjectivePlanner::new(2, work)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates()
        .with_requested_corridors()
        .with_extended_corridors();
    if enabled { p.with_walk_feedback() } else { p }
}

fn source(
    state: &SurfaceSortieState,
    seat: usize,
    angle: f32,
) -> combat::TacticalSortieObservationV1 {
    let mut o = if angle == 0.0 {
        target(state, seat)
    } else {
        target_at_angle(state, seat, angle, true)
    };
    let p = &mut o.combat.recovery.flight.pilot;
    p.landing.phase = LandingPhase::Flying;
    p.sites.truncate(1);
    p.site_query = LandingSiteQuery::Selected(p.sites[0].id);
    // A real walking measurement with no certified boarding entrance must
    // finish unsuccessfully. This changes sensor metadata, never physics.
    p.sites[0].boarding_hatches = [None; 2];
    o
}

#[test]
fn finished_focused_and_long_walks_report_effort_without_a_route_permission() {
    for angle in [0.0, 0.65, 2.3] {
        let mut state = smooth_state();
        let source = source(&state, 0, angle);
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let tick = state.world.tick;
        let mut live = planner(true, WORK);
        let mut seen = false;
        for age in 0..=MAX_SURVEY_AGE_TICKS {
            state.world.tick = tick + age;
            let mut o = source.clone();
            o.combat.recovery.flight.pilot.tick = state.world.tick;
            live.observe(&state, 0, &mut o);
            let e = o.objective_evidence.unwrap();
            assert!(o.landing_objective.is_none());
            if let Some(attempt) = e.exhausted_walk {
                assert_eq!(
                    attempt.site,
                    source.combat.recovery.flight.pilot.sites[0].id
                );
                assert_eq!(attempt.actor, source.combat.recovery.flight.pilot.owner);
                assert_eq!(e.measurement_tick, Some(tick));
                assert_eq!(e.request_tick, Some(tick));
                assert!(e.publication.is_none());
                seen = true;
                break;
            }
            let charged = live.advance(state.world.tick).unwrap().charged;
            assert!(charged.graph <= 4 && charged.physics_queries <= 384);
        }
        assert!(seen, "angle {angle}");
        assert_eq!(live.telemetry.completed, 0);
        assert_eq!(live.telemetry.published, 0);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn two_new_scanned_probes_replace_only_completed_attempts_with_fresh_snapshots() {
    let mut state = smooth_state();
    let sources = [source(&state, 0, 2.3), source(&state, 1, 2.3)];
    let next = [target(&state, 0), target(&state, 1)].map(|o| o.combat.recovery.flight.pilot.sites);
    let start = state.world.tick;
    let mut live = planner(true, WORK);
    let mut receipts = [false; 2];
    for age in 0..40 {
        state.world.tick = start + age;
        for seat in 0..2 {
            let mut o = sources[seat].clone();
            o.combat.recovery.flight.pilot.tick = state.world.tick;
            live.observe(&state, seat, &mut o);
            receipts[seat] |= o.objective_evidence.unwrap().exhausted_walk.is_some();
        }
        let charged = live.advance(state.world.tick).unwrap().charged;
        assert!(charged.graph <= 4 && charged.physics_queries <= 384);
        if receipts.iter().all(|v| *v) {
            break;
        }
    }
    assert!(receipts.iter().all(|v| *v));
    let previous = live.requests.clone();
    state.world.tick += 1;
    for seat in 0..2 {
        let mut o = sources[seat].clone();
        let p = &mut o.combat.recovery.flight.pilot;
        let old = p.sites[0].id;
        let site = *next[seat].iter().find(|s| s.id != old).unwrap();
        p.tick = state.world.tick;
        p.site_query = LandingSiteQuery::Selected(site.id);
        p.sites.clear();
        live.observe(&state, seat, &mut o);
        assert_eq!(live.requests[&seat].token, previous[&seat].token);
        o.combat.recovery.flight.pilot.sites = vec![site];
        live.observe(&state, seat, &mut o);
        let request = &live.requests[&seat];
        assert_ne!(request.token, previous[&seat].token);
        assert!(!Arc::ptr_eq(&request.snapshot, &previous[&seat].snapshot));
        assert_eq!(request.measurement_tick, state.world.tick);
        assert!(o.objective_evidence.unwrap().exhausted_walk.is_none());
        assert!(o.landing_objective.is_none());
    }
    assert_eq!(live.telemetry.walk_probe_restarts, 2);
    assert_eq!(
        live.telemetry.retired_unpublished_graph,
        previous.values().map(|r| r.graph).sum::<u64>()
    );
    assert_eq!(
        live.telemetry.retired_unpublished_queries,
        previous.values().map(|r| r.physics_queries).sum::<u64>()
    );
    let charged = live.advance(state.world.tick).unwrap().charged;
    assert!(charged.graph <= 4 && charged.physics_queries <= 384);
    assert!(live.advance(state.world.tick).is_none());
}

#[test]
fn disabled_zero_work_expired_and_cancelled_jobs_supply_no_feedback() {
    for (enabled, work) in [(false, WORK), (true, Work::default()), (true, WORK)] {
        let mut state = smooth_state();
        let source = source(&state, 0, 2.3);
        let start = state.world.tick;
        let mut live = planner(enabled, work);
        for age in 0..10 {
            state.world.tick = start + age;
            let mut o = source.clone();
            o.combat.recovery.flight.pilot.tick = state.world.tick;
            live.observe(&state, 0, &mut o);
            if !enabled || work == Work::default() {
                assert!(o.objective_evidence.unwrap().exhausted_walk.is_none());
            }
            live.advance(state.world.tick);
        }
        let mut cloned = live.clone();
        cloned.remove(0);
        assert!(cloned.requests.is_empty() && !live.requests.is_empty());
        state.world.tick = start + MAX_SURVEY_AGE_TICKS + 1;
        let mut o = source.clone();
        o.combat.recovery.flight.pilot.tick = state.world.tick;
        live.observe(&state, 0, &mut o);
        let e = o.objective_evidence.unwrap();
        assert_eq!(e.invalidated_by, Some("expired"));
        assert!(e.exhausted_walk.is_none());
        live.reset();
        assert_eq!(live.uses_walk_feedback(), enabled);
        assert!(live.requests.is_empty());
    }
}

#[test]
fn unsuccessful_prefix_retains_native_fallback_and_actual_return_is_never_a_probe() {
    let state = smooth_state();
    let mut o = source(&state, 0, 2.3);
    let p = &o.combat.recovery.flight.pilot;
    let expected = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    let mut job = make_job_mode(&state, 0, &o, true);
    while job.measurement_work().corridor_completed == 0 {
        job.step();
    }
    assert!(job.exhausted_walk().is_some() && job.positive_candidates().is_none());
    while job.next_work().is_some() {
        job.step();
    }
    assert_eq!(job.output(), Some(&expected));
    let p = &mut o.combat.recovery.flight.pilot;
    let site = p.sites[0];
    p.landing.phase = LandingPhase::Landed;
    p.ship.position = site.vehicle_position;
    p.ship.angle = rotation_for_direction(site.normal);
    p.hatch = Some(site.hatch_position);
    p.boarding_hatches = [None; 2];
    p.sites.clear();
    let mut job = make_job_mode(&state, 0, &o, true);
    while job.measurement_work().corridor_completed == 0 {
        job.step();
    }
    assert!(job.exhausted_walk().is_none() && job.positive_candidates().is_none());
}

fn unsupported_source(
    state: &SurfaceSortieState,
    seat: usize,
) -> combat::TacticalSortieObservationV1 {
    let mut o = target(state, seat);
    let p = &mut o.combat.recovery.flight.pilot;
    p.landing.phase = LandingPhase::Flying;
    p.sites.truncate(1);
    p.site_query = LandingSiteQuery::Selected(p.sites[0].id);
    p.planet
        .claim
        .as_mut()
        .unwrap()
        .flag
        .as_mut()
        .unwrap()
        .position = p.planet.motion.position * 2.0 - p.sites[0].hatch_position;
    o
}

#[test]
fn unsupported_walks_are_distinct_and_opt_in_even_without_dispatch() {
    let mut state = smooth_state();
    for enabled in [false, true] {
        let mut o = unsupported_source(&state, 0);
        let mut live = planner(true, Work::default());
        if enabled {
            live = live.with_walk_bounds_feedback();
        }
        live.observe(&state, 0, &mut o);
        let request = live.requests[&0].clone();
        let job = live.queue.job(request.token).unwrap();
        let (site, bounds) = job.unsupported_walk().unwrap();
        assert_eq!(bounds.required_steps, 260);
        assert_eq!(bounds.max_steps, 224);
        assert_eq!(job.measurement_work().corridor_started, 0);
        assert_eq!(job.measurement_work().corridor_completed, 0);
        state.world.tick += 1;
        o.combat.recovery.flight.pilot.tick = state.world.tick;
        live.observe(&state, 0, &mut o);
        let e = o.objective_evidence.unwrap();
        assert_eq!(e.unsupported_walk.is_some(), enabled);
        if let Some(notice) = e.unsupported_walk {
            assert_eq!(notice.site, site);
            assert_eq!(notice.actor, o.combat.recovery.flight.pilot.owner);
            assert_eq!((notice.required_steps, notice.max_steps), (260, 224));
        }
        assert!(e.exhausted_walk.is_none() && e.publication.is_none());
        assert!(o.landing_objective.is_none());
        assert_eq!(e.measurement_tick, Some(request.measurement_tick));
        assert_eq!(live.telemetry.graph, 0);
        assert_eq!(live.telemetry.physics_queries, 0);
        live.reset();
        assert_eq!(live.uses_walk_bounds_feedback(), enabled);
    }
}

#[test]
fn unsupported_probes_replace_only_for_a_current_new_scan_and_expire_normally() {
    let mut state = smooth_state();
    let mut o = unsupported_source(&state, 0);
    let sites = target(&state, 0).combat.recovery.flight.pilot.sites;
    let mut live = planner(true, WORK).with_walk_bounds_feedback();
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let old = live.requests[&0].clone();
    let next = *sites
        .iter()
        .find(|s| s.id != o.combat.recovery.flight.pilot.sites[0].id)
        .unwrap();
    state.world.tick += 1;
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = state.world.tick;
    p.site_query = LandingSiteQuery::Selected(next.id);
    p.sites.clear();
    live.observe(&state, 0, &mut o);
    assert!(o.objective_evidence.unwrap().unsupported_walk.is_none());
    assert_eq!(live.requests[&0].token, old.token);
    o.combat.recovery.flight.pilot.sites = vec![next];
    live.observe(&state, 0, &mut o);
    let new = &live.requests[&0];
    assert_ne!(new.token, old.token);
    assert!(!Arc::ptr_eq(&new.snapshot, &old.snapshot));
    assert_eq!(new.measurement_tick, state.world.tick);
    assert_eq!(live.telemetry.unsupported_walk_probe_restarts, 1);
    assert_eq!(live.telemetry.walk_probe_restarts, 0);
    assert_eq!(live.telemetry.retired_unpublished_graph, old.graph);
    assert_eq!(
        live.telemetry.retired_unpublished_queries,
        old.physics_queries
    );
    state.world.tick += MAX_SURVEY_AGE_TICKS + 1;
    o.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe(&state, 0, &mut o);
    let e = o.objective_evidence.unwrap();
    assert_eq!(e.invalidated_by, Some("expired"));
    assert!(e.unsupported_walk.is_none() && e.exhausted_walk.is_none());
}

#[test]
fn unsupported_walks_retain_the_native_fallback_but_do_not_label_actual_hatches() {
    let state = smooth_state();
    let mut o = unsupported_source(&state, 0);
    let p = &o.combat.recovery.flight.pilot;
    let expected = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    let mut job = make_job_mode(&state, 0, &o, true);
    assert!(job.unsupported_walk().is_some() && job.exhausted_walk().is_none());
    while job.next_work().is_some() {
        job.step();
    }
    assert_eq!(job.output(), Some(&expected));
    assert_eq!(job.measurement_work().corridor_started, 0);
    let p = &mut o.combat.recovery.flight.pilot;
    let site = p.sites[0];
    p.landing.phase = LandingPhase::Landed;
    p.ship.position = site.vehicle_position;
    p.ship.angle = rotation_for_direction(site.normal);
    p.hatch = Some(site.hatch_position);
    p.sites.clear();
    let job = make_job_mode(&state, 0, &o, true);
    assert!(job.unsupported_walk().is_none() && job.exhausted_walk().is_none());
    assert_eq!(job.measurement_work().corridor_started, 0);
}
