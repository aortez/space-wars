use super::*;
use requested_corridors::{make_job_mode, smooth_state, target_at_angle};

const WORK: Work = Work {
    graph: 4,
    physics_queries: 384,
};

fn planner(work: Work) -> LiveObjectivePlanner {
    LiveObjectivePlanner::new(2, work)
        .with_route_dependencies()
        .with_early_candidates()
        .with_focused_candidates()
        .with_requested_corridors()
        .with_extended_corridors()
}

#[test]
fn two_large_corridors_publish_before_expiry_with_the_original_shared_budget() {
    for planning in [
        ObjectivePlanning::JointRoundTrip,
        ObjectivePlanning::JetpackRoundTrip,
    ] {
        let mut state = smooth_state();
        let source = [
            target_at_angle(&state, 0, 2.65, true),
            target_at_angle(&state, 1, 2.65, true),
        ];
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let start = state.world.tick;
        let mut live = planner(WORK);
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
                    assert!(survey.validated_routes_only && survey.sites.len() == 1);
                    assert!(survey.sites[0].cost().is_some());
                    assert!(survey.sites[0].outbound.length > 120.0);
                    delivered[seat].get_or_insert(age);
                }
            }
            let report = live.advance(state.world.tick).unwrap();
            assert!(report.charged.graph <= 4 && report.charged.physics_queries <= 384);
            if delivered.iter().all(Option::is_some) {
                break;
            }
        }
        assert!(
            delivered.iter().all(Option::is_some),
            "{planning:?}: {delivered:?}"
        );
        assert_eq!(
            live.telemetry
                .measurements_by_actor
                .values()
                .map(|m| m.extended_successes)
                .sum::<u64>(),
            2
        );
        assert_eq!(live.telemetry.completed, 0);
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    }
}

#[test]
fn extended_handoff_preserves_native_completion_and_never_retimes_a_warm_snapshot() {
    let state = smooth_state();
    let mut source = target_at_angle(&state, 0, 2.3, true);
    let p = &source.combat.recovery.flight.pilot;
    assert!(
        make_job_mode(&state, 0, &source, false)
            .measurement_work()
            .corridor_started
            == 0
    );
    let expected = state
        .landing_objective_survey(0, p, &source.cover, ObjectivePlanning::JointRoundTrip)
        .unwrap();
    let mut job = make_job_mode(&state, 0, &source, true);
    while job.measurement_work().extended_completed == 0 {
        job.step();
    }
    assert!(job.positive_candidates().is_some());
    while job.next_work().is_some() {
        job.step();
    }
    assert_eq!(job.output(), Some(&expected));
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
    let source_tick = ground.measurement_tick();
    let p = &mut source.combat.recovery.flight.pilot;
    p.tick += 20;
    let mut warm = state
        .objective_job(
            0,
            p,
            &source.cover,
            snapshot,
            Some(ground.into_measurements()),
            true,
        )
        .unwrap()
        .with_focused_candidate(0)
        .with_extended_corridor(p.site_query);
    while warm.measurement_work().extended_completed == 0 {
        warm.step();
    }
    assert_eq!(warm.positive_candidates().unwrap().tick, source_tick);
    assert_eq!(warm.reused(), ReusedGroundWork::default());
}

#[test]
fn actual_long_return_and_zero_work_cancellation_keep_existing_permissions() {
    let mut state = smooth_state();
    let mut source = target_at_angle(&state, 0, 2.3, true);
    let p = &mut source.combat.recovery.flight.pilot;
    let site = p.sites[0];
    p.landing.phase = LandingPhase::Landed;
    p.ship.position = site.vehicle_position;
    p.ship.angle = rotation_for_direction(site.normal);
    p.hatch = Some(site.hatch_position);
    p.boarding_hatches = site.boarding_hatches;
    p.sites.clear();
    let mut job = make_job_mode(&state, 0, &source, true);
    while job.measurement_work().extended_completed == 0 {
        job.step();
    }
    let survey = job.positive_candidates().unwrap();
    assert!(survey.sites.is_empty());
    assert!(
        survey
            .actual
            .as_ref()
            .is_some_and(|r| r.cost().is_some() && r.returning.as_ref().unwrap().length > 100.0)
    );
    let mut live = planner(Work::default());
    let start = state.world.tick;
    live.observe(&state, 0, &mut source);
    assert_eq!(live.advance(start).unwrap().charged, Work::default());
    let mut cloned = live.clone();
    cloned.remove(0);
    assert!(cloned.requests.is_empty() && !live.requests.is_empty());
    state.world.tick += MAX_SURVEY_AGE_TICKS + 1;
    source.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe(&state, 0, &mut source);
    assert!(source.landing_objective.is_none());
    assert_eq!(
        source.objective_evidence.unwrap().invalidated_by,
        Some("expired")
    );
    live.reset();
    assert!(live.requests.is_empty() && live.uses_extended_corridors());
}
