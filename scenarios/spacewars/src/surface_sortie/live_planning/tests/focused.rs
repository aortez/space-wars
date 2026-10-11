use super::*;

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

#[test]
fn focused_cache_reuse_is_counted_and_never_disappears_at_handoff() {
    let state = state();
    let o = target(&state, 0);
    let p = &o.combat.recovery.flight.pilot;
    let snapshot = Arc::new(state.world.physics.world.query_snapshot());
    let mut ground = state
        .ground_survey_job(
            0,
            p.planet.index,
            state.objective_gravity(p),
            Arc::clone(&snapshot),
        )
        .unwrap();
    while ground.next_work().is_some() {
        ground.step();
    }
    let mut job = state
        .objective_job(
            0,
            p,
            &o.cover,
            snapshot,
            Some(ground.into_measurements()),
            true,
        )
        .unwrap()
        .with_focused_candidate(0);
    let mut previous = ReusedGroundWork::default();
    while job.measurement_work().focused_completed == 0 {
        assert!(job.next_work().is_some());
        job.step();
        let current = job.reused();
        assert!(
            current.nodes >= previous.nodes
                && current.walks >= previous.walks
                && current.physics_queries >= previous.physics_queries
        );
        previous = current;
    }
    assert!(previous.nodes > 0 && previous.walks > 0 && previous.physics_queries > 0);
    while job.next_work().is_some() {
        job.step();
        let current = job.reused();
        assert!(
            current.nodes >= previous.nodes
                && current.walks >= previous.walks
                && current.physics_queries >= previous.physics_queries
        );
        previous = current;
    }
}

#[test]
fn focused_routes_deliver_for_two_actors_before_expiry_under_the_shared_allowance() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        let mut state = state();
        let source = [target(&state, 0), target(&state, 1)];
        let start = state.world.tick;
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let mut live = LiveObjectivePlanner::new(2, WORK)
            .with_route_dependencies()
            .with_early_candidates()
            .with_focused_candidates();
        let mut delivered = [None; 2];
        for age in 0..=MAX_SURVEY_AGE_TICKS {
            state.world.tick = start + age;
            for seat in 0..2 {
                let mut o = source[seat].clone();
                o.combat.recovery.flight.pilot.tick = state.world.tick;
                live.observe_with_planning(&state, seat, &mut o, planning);
                if let Some(survey) = o.landing_objective {
                    assert_eq!(survey.tick, start);
                    assert_eq!(survey.validated_tick, Some(start + age));
                    assert!(survey.validated_routes_only);
                    assert!(survey.sites.iter().any(|r| r.cost().is_some()));
                    assert!(
                        survey
                            .sites
                            .iter()
                            .all(|r| r.cost().is_some() && r.crossing.is_none())
                    );
                    delivered[seat].get_or_insert(age);
                }
            }
            let report = live.advance(state.world.tick).unwrap();
            assert!(report.charged.graph <= WORK.graph);
            assert!(report.charged.physics_queries <= WORK.physics_queries);
            if delivered.iter().all(Option::is_some) {
                break;
            }
        }
        assert!(
            delivered.iter().all(Option::is_some),
            "{planning:?}: {delivered:?}, {:?}",
            live.telemetry()
        );
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(
            live.telemetry.completed, 0,
            "early success is not a complete survey"
        );
    }
}

#[test]
fn focused_pass_preserves_the_full_survey_and_never_publishes_patch_failures() {
    let state = state();
    let o = target(&state, 0);
    let p = &o.combat.recovery.flight.pilot;
    let expected = state
        .landing_objective_survey(0, p, &o.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    for cursor in 0..landing_objective::MAX_OBJECTIVE_SITES {
        let snapshot = Arc::new(state.world.physics.world.query_snapshot());
        let mut job = state
            .objective_job(0, p, &o.cover, snapshot, None, true)
            .unwrap()
            .with_focused_candidate(cursor);
        while job.next_work().is_some() {
            job.step();
            if let Some(partial) = job.positive_candidates() {
                assert!(partial.validated_routes_only);
                assert!(partial.sites.iter().all(|r| r.cost().is_some()));
                assert!(partial.actual.is_none_or(|r| r.cost().is_some()));
            }
        }
        assert_eq!(job.output(), Some(&expected), "cursor {cursor}");
        assert_eq!(
            job.measurement_work().finished_candidates as usize,
            expected.sites.len() + usize::from(expected.actual.is_some())
        );
    }
}

#[test]
fn focused_work_keeps_expiry_and_cancellation_semantics() {
    let mut state = state();
    let mut source = target(&state, 0);
    let start = state.world.tick;
    let mut live = LiveObjectivePlanner::new(2, WORK)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates();
    live.observe(&state, 0, &mut source);
    live.advance(start);
    state.world.tick = start + MAX_SURVEY_AGE_TICKS + 1;
    let mut o = target(&state, 0);
    live.observe(&state, 0, &mut o);
    assert_eq!(
        o.objective_evidence.unwrap().invalidated_by,
        Some("expired")
    );
    assert!(o.landing_objective.is_none());
    assert_eq!(live.focused_cursor[&0].1, 2);
    let mut cloned = live.clone();
    cloned.remove(0);
    assert!(cloned.requests.is_empty() && cloned.focused_cursor.is_empty());
    assert!(!live.requests.is_empty());
    live.reset();
    assert!(live.requests.is_empty() && live.focused_cursor.is_empty());
    assert!(live.uses_focused_candidates());
}
