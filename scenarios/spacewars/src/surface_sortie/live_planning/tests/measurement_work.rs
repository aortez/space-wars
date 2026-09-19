use super::*;

#[test]
fn raw_measurements_survive_partial_retirement_and_are_not_recounted_on_delivery() {
    let mut state = state();
    let source = target(&state, 0);
    let mut live = LiveObjectivePlanner::new(
        1,
        Work {
            graph: 512,
            physics_queries: 1024,
        },
    )
    .with_route_dependencies();
    let mut partial = None;
    // Freeze the physical scene to isolate incremental accounting and lifecycle.
    for age in 0..MAX_SURVEY_AGE_TICKS {
        let mut o = source.clone();
        o.combat.recovery.flight.pilot.tick += age;
        state.world.tick = o.combat.recovery.flight.pilot.tick;
        live.observe(&state, 0, &mut o);
        live.advance(state.world.tick);
        let work = &live.telemetry.measurements_by_actor[&0];
        if work.successful_candidates > 0 && work.finished_surveys == 0 {
            partial = Some(work.clone());
            break;
        }
    }
    let partial = partial.expect("a candidate finishes before the whole survey");
    assert_eq!(live.telemetry.completed, 0);
    assert_eq!(live.telemetry.published, 0);
    live.remove(0);
    live.remove(0);
    assert_eq!(live.telemetry.retired_partial_successes_by_actor[&0], 1);
    assert_eq!(live.telemetry.measurements_by_actor[&0], partial);
    live.reset();
    assert!(live.telemetry.measurements_by_actor.is_empty());
    assert!(live.telemetry.retired_partial_successes_by_actor.is_empty());

    live.allowance = Work::UNLIMITED;
    let mut o = source;
    o.combat.recovery.flight.pilot.tick = state.world.tick;
    live.observe(&state, 0, &mut o);
    live.advance(state.world.tick);
    let measured = live.telemetry.measurements_by_actor[&0].clone();
    assert_eq!(measured.finished_surveys, 1);
    assert!(measured.successful_candidates > 0);
    assert_eq!(
        measured.finished_candidates,
        measured.successful_candidates + measured.failures.values().sum::<u64>()
    );
    assert_eq!(
        live.telemetry.completed, 0,
        "measurement precedes validation"
    );
    for _ in 0..3 {
        live.observe(&state, 0, &mut o);
        assert!(o.landing_objective.is_some());
        assert!(live.advance(state.world.tick).is_none());
        assert_eq!(live.telemetry.measurements_by_actor[&0], measured);
    }
    assert_eq!(live.telemetry.completed, 1);
    assert_eq!(live.telemetry.published, 3);
}
