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
fn detached_actual_job_finds_the_native_crossing_without_changing_live_work_or_physics() {
    let (mut state, mut source) = powered_corridors::fixture(true, 0);
    let mut live = planner();
    live.observe_with_planning(&state, 0, &mut source, ObjectivePlanning::JetpackRoundTrip);
    live.advance(state.world.tick);
    let physical = state.world.physics.world.snapshot_bytes().unwrap();
    let source_probe = live.diagnose_actual_request(0, 0).unwrap();
    let source_telemetry = serde_json::to_value(live.telemetry()).unwrap();
    let mut control = live.clone();
    let result = live.diagnose_actual_request(0, 1_000_000).unwrap();
    assert_eq!(result["stop"], "actual_positive");
    assert!(!result["positive_actual"]["crossing"].is_null());
    assert_eq!(result["measurement_tick"], state.world.tick);
    assert_eq!(result["request_tick"], state.world.tick);
    assert_eq!(result["observed_tick"], state.world.tick);
    assert!(result["steps"].as_u64().unwrap() > 0);
    assert_eq!(live.diagnose_actual_request(0, 0).unwrap(), source_probe);
    assert_eq!(
        serde_json::to_value(live.telemetry()).unwrap(),
        source_telemetry
    );
    assert_eq!(
        state.world.physics.world.snapshot_bytes().unwrap(),
        physical
    );
    // Continuing both live queues must still consume the same work and produce
    // the same result. A detached footprint or forecast must not leak back.
    for _ in 0..120 {
        state.world.tick += 1;
        source.combat.recovery.flight.pilot.tick = state.world.tick;
        let (mut a, mut b) = (source.clone(), source.clone());
        live.observe_with_planning(&state, 0, &mut a, ObjectivePlanning::JetpackRoundTrip);
        control.observe_with_planning(&state, 0, &mut b, ObjectivePlanning::JetpackRoundTrip);
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
        assert_eq!(
            live.advance(state.world.tick),
            control.advance(state.world.tick)
        );
    }
    assert_eq!(
        state.world.physics.world.snapshot_bytes().unwrap(),
        physical
    );
}

#[test]
fn detached_probe_is_bounded_and_absent_without_an_actual_touchdown_request() {
    let (state, mut source) = powered_corridors::fixture(false, 0);
    let mut live = planner();
    assert!(live.diagnose_actual_request(0, 1).is_none());
    live.observe_with_planning(&state, 0, &mut source, ObjectivePlanning::JetpackRoundTrip);
    for cap in [0, 1, 7] {
        let result = live.diagnose_actual_request(0, cap).unwrap();
        assert_eq!(result["stop"], "work_limit");
        assert_eq!(result["steps"], cap);
        assert_eq!(
            result["work"]["graph"].as_u64().unwrap()
                + result["work"]["physics_queries"].as_u64().unwrap(),
            u64::from(cap)
        );
        assert!(result["positive_actual"].is_null());
        assert_eq!(
            result["live_charged"],
            serde_json::json!({"graph":0,"physics_queries":0})
        );
    }
    live.reset();
    let mut source = target(&state, 0);
    source.combat.recovery.flight.pilot.landing.phase = LandingPhase::Flying;
    live.observe_with_planning(&state, 0, &mut source, ObjectivePlanning::JetpackRoundTrip);
    assert!(live.requests.contains_key(&0));
    assert!(live.diagnose_actual_request(0, 7).is_none());
}
