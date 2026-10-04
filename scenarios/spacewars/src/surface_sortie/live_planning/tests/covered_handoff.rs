use super::requested_corridors::smooth_state;
use super::*;

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

fn planner(players: &[usize], work: Work) -> LiveObjectivePlanner {
    LiveObjectivePlanner::new(2, work)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates()
        .with_requested_corridors()
        .with_extended_corridors()
        .with_walk_feedback()
        .with_walk_bounds_feedback()
        .with_powered_corridors()
        .with_covered_request_handoff(players.iter().copied())
}

fn sources(
    state: &SurfaceSortieState,
    seat: usize,
) -> (
    combat::TacticalSortieObservationV1,
    combat::TacticalSortieObservationV1,
) {
    let mut selected = requested_corridors::target_at_angle(state, seat, 0.65, true);
    let p = &mut selected.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.landing.supported_feet = 0;
    p.landing.phase = LandingPhase::Flying;
    selected.combat.recovery.flight.flight.enabled = true;
    let id = p.sites[0].id;
    selected.cover = vec![combat::LandingCover {
        site: id,
        grounded: true,
        approach: true,
        departure: false,
    }];
    let mut ordinary = selected.clone();
    let p = &mut ordinary.combat.recovery.flight.pilot;
    // Sensor fixtures vary the request, never the physical terrain. The full
    // replay corpus separately checks real scans and moving cover.
    p.site_query = LandingSiteQuery::Survey;
    p.sites = target(state, seat)
        .combat
        .recovery
        .flight
        .pilot
        .sites
        .into_iter()
        .filter(|s| s.id != id)
        .collect();
    (ordinary, selected)
}

#[test]
fn two_handoffs_bind_fresh_snapshots_keep_charged_work_and_deliver_within_shared_quota() {
    let mut state = smooth_state();
    let source = [sources(&state, 0), sources(&state, 1)];
    let start = state.tick();
    let physical = state.world.physics.world.snapshot_bytes().unwrap();
    let mut live = planner(
        &[0, 1],
        Work {
            graph: 1,
            physics_queries: 1,
        },
    );
    for (seat, (ordinary, _)) in source.iter().enumerate() {
        live.observe(&state, seat, &mut ordinary.clone());
    }
    live.advance(start).unwrap();
    let old = live.requests.clone();
    live.allowance = WORK;
    let mut delivered = [false; 2];
    for age in 1..=MAX_SURVEY_AGE_TICKS {
        state.world.tick = start + age;
        for (seat, (_, selected)) in source.iter().enumerate() {
            let mut o = selected.clone();
            o.combat.recovery.flight.pilot.tick = state.tick();
            live.observe(&state, seat, &mut o);
            let e = o.objective_evidence.unwrap();
            let h = e.covered_handoff.unwrap();
            assert_eq!(h.tick, start + 1);
            assert_eq!(h.previous_generation, old[&seat].token.generation);
            assert_eq!(h.deadline_tick, start + MAX_SURVEY_AGE_TICKS);
            assert_eq!(e.measurement_tick, Some(start + 1));
            assert_eq!(e.request_tick, Some(start + 1));
            assert_ne!(e.generation, Some(old[&seat].token.generation));
            assert!(!Arc::ptr_eq(
                &live.requests[&seat].snapshot,
                &old[&seat].snapshot
            ));
            if let Some(survey) = o.landing_objective {
                assert!(survey.validated_routes_only && survey.is_current(state.tick()));
                assert!(
                    survey
                        .sites
                        .iter()
                        .any(|r| r.site == Some(h.site) && r.cost().is_some())
                );
                delivered[seat] = true;
            }
        }
        let work = live.advance(state.tick()).unwrap().charged;
        assert!(work.graph <= 4 && work.physics_queries <= 384);
        assert!(live.advance(state.tick()).is_none());
        if delivered.iter().all(|&v| v) {
            break;
        }
    }
    assert!(delivered.iter().all(|&v| v));
    assert_eq!(
        live.telemetry.covered_request_handoffs,
        BTreeMap::from([(0, 1), (1, 1)])
    );
    assert_eq!(
        live.telemetry.retired_unpublished_graph,
        old.values().map(|r| r.graph).sum::<u64>()
    );
    assert_eq!(
        live.telemetry.retired_unpublished_queries,
        old.values().map(|r| r.physics_queries).sum::<u64>()
    );
    assert_eq!(
        state.world.physics.world.snapshot_bytes().unwrap(),
        physical
    );
}

#[test]
fn current_selected_cover_and_source_identity_are_required() {
    let mut state = smooth_state();
    let (ordinary, selected) = sources(&state, 0);
    let mut live = planner(&[0], WORK);
    live.observe(&state, 0, &mut ordinary.clone());
    state.world.tick += 1;
    for mutation in 0..12 {
        let mut o = selected.clone();
        let mut live = live.clone();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = state.tick();
        match mutation {
            0 => live.covered_handoff_players.clear(),
            1 => {
                live.covered_handoff_players = [1].into_iter().collect();
            }
            2 => p.sites.clear(),
            3 => p.sites[0].revision += 1,
            4 => p.site_query = LandingSiteQuery::Survey,
            5 => o.cover.clear(),
            6 => o.cover[0].grounded = false,
            7 => o.cover[0].approach = false,
            8 => p.landing.supported_feet = 1,
            9 => p.controls_armed = false,
            10 => {
                p.planet
                    .claim
                    .as_mut()
                    .unwrap()
                    .flag
                    .as_mut()
                    .unwrap()
                    .position += Vec2::X * 10.0
            }
            11 => p.tick -= 1,
            _ => unreachable!(),
        }
        let objective = LandingObjective::read(&o.combat.recovery.flight.pilot).unwrap();
        assert!(
            live.covered_handoff_candidate(&state, 0, &o, objective)
                .is_none(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn targeted_jobs_do_not_churn_or_extend_the_original_expiry() {
    let mut state = smooth_state();
    let (ordinary, mut selected) = sources(&state, 0);
    let start = state.tick();
    let mut live = planner(&[0], Work::default());
    live.observe(&state, 0, &mut ordinary.clone());
    state.world.tick = start + 40;
    selected.combat.recovery.flight.pilot.tick = state.tick();
    live.observe(&state, 0, &mut selected);
    let token = live.requests[&0].token;
    assert_eq!(live.requests[&0].measurement_tick, start + 40);
    assert_eq!(
        live.requests[&0].covered_handoff.unwrap().deadline_tick,
        start + 120
    );
    let mut switched = ordinary.clone();
    let id = switched.combat.recovery.flight.pilot.sites[0].id;
    switched.combat.recovery.flight.pilot.sites.truncate(1);
    switched.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(id);
    switched.cover = vec![combat::LandingCover {
        site: id,
        grounded: true,
        approach: true,
        departure: true,
    }];
    for age in 41..=120 {
        state.world.tick = start + age;
        switched.combat.recovery.flight.pilot.tick = state.tick();
        live.observe(&state, 0, &mut switched);
        assert_eq!(live.requests[&0].token, token);
        assert!(switched.landing_objective.is_none());
        assert_eq!(live.advance(state.tick()).unwrap().charged, Work::default());
    }
    state.world.tick += 1;
    switched.combat.recovery.flight.pilot.tick = state.tick();
    live.observe(&state, 0, &mut switched);
    assert_eq!(
        switched.objective_evidence.unwrap().invalidated_by,
        Some("expired")
    );
    assert_ne!(live.requests[&0].token, token);
    assert_eq!(live.telemetry.covered_request_handoffs[&0], 1);
    live.reset();
    assert_eq!(live.covered_handoff_players, [0].into_iter().collect());
    assert!(live.telemetry.covered_request_handoffs.is_empty() && live.requests.is_empty());
}

#[test]
fn existing_target_and_positive_results_keep_their_work() {
    let mut state = smooth_state();
    let (ordinary, mut selected) = sources(&state, 0);
    let mut live = planner(&[0], WORK);
    live.observe(&state, 0, &mut selected.clone());
    let token = live.requests[&0].token;
    state.world.tick += 1;
    selected.combat.recovery.flight.pilot.tick = state.tick();
    let objective = LandingObjective::read(&selected.combat.recovery.flight.pilot).unwrap();
    assert!(
        live.covered_handoff_candidate(&state, 0, &selected, objective)
            .is_none()
    );
    assert_eq!(live.requests[&0].token, token);
    // A completed ordinary survey must be published/revalidated normally,
    // rather than discarded because a later selected request is covered.
    let mut live = planner(&[0], Work::UNLIMITED);
    let mut o = ordinary;
    o.combat.recovery.flight.pilot.tick = state.tick();
    live.observe(&state, 0, &mut o);
    live.advance(state.tick()).unwrap();
    let token = live.requests[&0].token;
    assert!(live.queue.job(token).unwrap().output().is_some());
    state.world.tick += 1;
    selected.combat.recovery.flight.pilot.tick = state.tick();
    assert!(
        live.covered_handoff_candidate(&state, 0, &selected, objective)
            .is_none()
    );
}

#[test]
fn already_focused_and_partial_positive_work_is_never_preempted() {
    let mut state = smooth_state();
    let mut source = target(&state, 0);
    let p = &mut source.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    let mut live = planner(&[0], WORK);
    live.observe(&state, 0, &mut source.clone());
    let old = live.requests[&0].token;
    let priority = source
        .combat
        .recovery
        .flight
        .pilot
        .sites
        .iter()
        .find(|s| live.queue.job(old).unwrap().prioritizes(s.id))
        .copied()
        .unwrap();
    let mut selected = source.clone();
    selected.combat.recovery.flight.pilot.sites = vec![priority];
    selected.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(priority.id);
    selected.cover = vec![combat::LandingCover {
        site: priority.id,
        grounded: true,
        approach: true,
        departure: true,
    }];
    state.world.tick += 1;
    selected.combat.recovery.flight.pilot.tick = state.tick();
    let objective = LandingObjective::read(&selected.combat.recovery.flight.pilot).unwrap();
    assert!(
        live.covered_handoff_candidate(&state, 0, &selected, objective)
            .is_none()
    );
    for _ in 0..120 {
        source.combat.recovery.flight.pilot.tick = state.tick();
        live.observe(&state, 0, &mut source.clone());
        live.advance(state.tick()).unwrap();
        let job = live.queue.job(live.requests[&0].token).unwrap();
        if job.positive_candidates().is_some() {
            assert!(job.output().is_none());
            assert!(!live.requests[&0].published && !live.requests[&0].partial_visible);
            let other = *source
                .combat
                .recovery
                .flight
                .pilot
                .sites
                .iter()
                .find(|s| s.id != priority.id)
                .unwrap();
            selected.combat.recovery.flight.pilot.sites = vec![other];
            selected.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(other.id);
            selected.cover[0].site = other.id;
            state.world.tick += 1;
            selected.combat.recovery.flight.pilot.tick = state.tick();
            assert!(
                live.covered_handoff_candidate(&state, 0, &selected, objective)
                    .is_none()
            );
            return;
        }
        state.world.tick += 1;
    }
    panic!("no partial positive fixture");
}
