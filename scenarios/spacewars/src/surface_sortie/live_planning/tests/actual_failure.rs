use super::*;

fn planner() -> LiveObjectivePlanner {
    LiveObjectivePlanner::new(
        2,
        Work {
            graph: 4,
            physics_queries: 384,
        },
    )
    .with_route_dependencies()
    .with_early_candidates()
    .with_focused_candidates()
    .with_requested_corridors()
    .with_extended_corridors()
    .with_walk_feedback()
    .with_walk_bounds_feedback()
    .with_powered_corridors()
}

#[test]
fn actual_failure_is_historical_feedback_with_identical_disabled_queue_work() {
    let (mut state, mut source) = powered_corridors::fixture(false, 0);
    source.combat.recovery.flight.pilot.boarding_hatches = [None; 2];
    let before = state.world.physics.world.snapshot_bytes().unwrap();
    let start = state.world.tick;
    let mut control = planner();
    let mut live = control.clone().with_actual_failure_feedback([0]);
    let mut first = None;
    for age in 0..=120 {
        state.world.tick = start + age;
        source.combat.recovery.flight.pilot.tick = state.world.tick;
        let (mut a, mut b) = (source.clone(), source.clone());
        live.observe_with_planning(&state, 0, &mut a, ObjectivePlanning::JetpackRoundTrip);
        control.observe_with_planning(&state, 0, &mut b, ObjectivePlanning::JetpackRoundTrip);
        let evidence = a.objective_evidence.as_mut().unwrap();
        if let Some(attempt) = evidence.actual_local_failure.take() {
            assert_eq!(attempt.actor, PlayerId::PLAYER_1);
            assert_eq!(attempt.reason, "hatch_to_crossing");
            assert_eq!(evidence.measurement_tick, Some(start));
            assert_eq!(evidence.request_tick, Some(start));
            assert!(attempt.pose.matches(&source.combat.recovery.flight.pilot));
            assert_eq!(a.objective_work, Some(ObjectiveWorkState::Pending));
            assert!(a.landing_objective.is_none());
            first.get_or_insert_with(|| (state.world.tick, live.clone()));
        }
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
        let a = live.advance(state.world.tick).unwrap();
        let b = control.advance(state.world.tick).unwrap();
        assert_eq!(a, b);
        assert!(a.charged.graph <= 4 && a.charged.physics_queries <= 384);
    }
    assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
    let (tick, live) = first.expect("bounded local failure must be observed before expiry");
    for change in 0..8 {
        let mut branch = live.clone();
        let mut o = source.clone();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick = tick + 1;
        match change {
            0 => p.ship.position.x += 0.01,
            1 => p.ship.angle += 0.001,
            2 => p.hatch.as_mut().unwrap().x += 0.01,
            3 => p.planet.revision += 1,
            4 => p.tick = start + MAX_SURVEY_AGE_TICKS + 1,
            5 => p.landing.phase = LandingPhase::Flying,
            6 => p.location = PilotLocation::OnFoot,
            _ => branch.actual_failure_players.clear(),
        }
        state.world.tick = p.tick;
        branch.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
        assert!(
            o.objective_evidence
                .and_then(|e| e.actual_local_failure)
                .is_none(),
            "change {change}"
        );
    }
    let mut live = live;
    live.reset();
    assert!(live.actual_failure_players().contains(&0));
    assert!(live.requests.is_empty());
}

#[test]
fn unsupported_or_unfinished_actual_attempts_never_emit_failure() {
    let (mut state, source) = powered_corridors::fixture(false, 0);
    for unsupported in [false, true] {
        let mut live = planner().with_actual_failure_feedback([0]);
        live.allowance = Work::default();
        for _ in 0..5 {
            let mut o = source.clone();
            let p = &mut o.combat.recovery.flight.pilot;
            p.tick = state.world.tick;
            if unsupported {
                let flag = p.planet.claim.as_mut().unwrap().flag.as_mut().unwrap();
                flag.position =
                    p.planet.motion.position - (p.hatch.unwrap() - p.planet.motion.position);
            }
            live.observe_with_planning(&state, 0, &mut o, ObjectivePlanning::JetpackRoundTrip);
            assert!(o.objective_evidence.unwrap().actual_local_failure.is_none());
            assert_eq!(
                live.advance(state.world.tick).unwrap().charged,
                Work::default()
            );
            state.world.tick += 1;
        }
    }
}
