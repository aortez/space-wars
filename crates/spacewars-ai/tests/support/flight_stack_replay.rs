//! The gameplay driver flies an archived closed-loop trace of the learned
//! stack: the training harness's evaluation of a development case, replayed
//! through the live-state observation and motor-input paths with the goal
//! issued each tick as a host issues it.
//!
//! The archived actions are bound to the C math library of the host that
//! recorded them. The networks are tanh layers, and a libm whose `tanhf`
//! rounds differently (glibc 2.41 adopted the correctly rounded CORE-MATH
//! functions) moves actions in their last digits, usually from the first
//! tick, though not always: under glibc 2.39 the LF-100 grounded case
//! reproduced its first action and parted from the archive two ticks later,
//! by 4e-12 of thrust. The replay therefore holds the flight exactly for as
//! long as this host reproduces the archived actions and steps, and from the
//! first difference on it must still fly the task to the same success within
//! a few ticks of the archive, with every action and position within a small
//! tolerance of it. Under glibc 2.36 the archived cases stray by at most 5e-5
//! in action and 3e-5 in position and arrive on the archived tick (measured
//! 2026-11-03, the LF-099 case 2026-11-06, the LF-100 case 2026-11-07). A
//! closed loop can amplify that difference, so an archived case is one that
//! holds to the tolerances under glibc 2.36.
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightAction, FlightEnvironment, FlightOutcome, FlightTrace,
};
use spacewars_ai::flight_stack::{FlightStack, FlightStackDriver, GoalOutcome, NavigationGoal};

/// The largest difference an action may show from the archived one on a host
/// whose math library does not reproduce the archived inference.
pub const ACTION_TOLERANCE: f32 = 1e-3;
/// The largest distance of the ship's center of mass from its archived
/// position on such a host.
pub const POSITION_TOLERANCE: f32 = 1e-2;
/// How many ticks the arrival may move on such a host.
pub const TICK_TOLERANCE: usize = 4;

/// Flies the archived task with the driver and holds it to the archive:
/// exactly while this host reproduces the archived inference, within the
/// tolerances from the first difference on. The archive's leading neutral
/// steps, a settle the harness flew under neutral controls before the stack
/// took the ship, are stepped as recorded and must reproduce exactly, since
/// nothing in them is inferred. `goal` issues the goal for the environment's
/// current state, as the host that flew the case issued it.
pub fn drive_archived_trace(
    trace: &FlightTrace,
    stack: &FlightStack,
    goal: impl Fn(&FlightEnvironment) -> NavigationGoal,
) {
    FlightEnvironment::replay(trace).unwrap();
    let mut env = FlightEnvironment::new(trace.task.clone()).unwrap();
    let settle = trace
        .steps
        .iter()
        .take_while(|step| step.action == Some(FlightAction::default()))
        .count();
    for (tick, archived) in trace.steps[..settle].iter().enumerate() {
        let step = env.step(FlightAction::default()).unwrap();
        assert_eq!(step, archived, "settle tick {tick}");
    }
    let mut driver = FlightStackDriver::new(0).unwrap();
    let mut exact = true;
    let mut flown = settle;
    while env.progress().outcome.is_none() {
        let tick = flown;
        let decision = driver.decide(stack, env.state(), goal(&env)).unwrap();
        let archived = trace.steps.get(tick);
        let step = env.step(decision.action).unwrap();
        if exact
            && archived
                .is_none_or(|archived| archived.action != Some(decision.action) || archived != step)
        {
            // The first difference is this host's math library parting from
            // the recording host's; from here the flight is held to the
            // tolerances.
            exact = false;
            eprintln!(
                "tick {tick}: this host's math library does not reproduce the archived \
                 inference ({:?} against the archived {:?}); holding the flight to the \
                 tolerances",
                decision.action,
                archived.and_then(|step| step.action)
            );
        }
        if exact {
            // Equal to the archive, action and step alike.
        } else if let Some(archived) = archived {
            if let Some(action) = archived.action {
                assert!(
                    within(decision.action, action, ACTION_TOLERANCE),
                    "tick {tick}: the action {:?} strays from the archived {action:?}",
                    decision.action
                );
            }
            if let (Some(frame), Some(archived)) = (&step.frame, &archived.frame) {
                let distance = frame.center_of_mass.distance_to(archived.center_of_mass);
                assert!(
                    distance <= POSITION_TOLERANCE,
                    "tick {tick}: the ship is {distance} from its archived position"
                );
            }
        } else {
            assert!(
                tick < trace.steps.len() + TICK_TOLERANCE,
                "tick {tick}: the flight runs past the archive"
            );
        }
        flown += 1;
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
    if exact {
        assert_eq!(flown, trace.steps.len());
    } else {
        assert!(
            flown.abs_diff(trace.steps.len()) <= TICK_TOLERANCE,
            "flown {flown} ticks against the archived {}",
            trace.steps.len()
        );
    }
    let decision = driver.decide(stack, env.state(), goal(&env)).unwrap();
    assert_eq!(decision.progress.outcome, Some(GoalOutcome::Arrived));
    assert_eq!(decision.progress.elapsed_ticks as usize, flown - settle);
}

fn within(a: FlightAction, b: FlightAction, tolerance: f32) -> bool {
    (a.steering - b.steering).abs() <= tolerance
        && (a.thrust - b.thrust).abs() <= tolerance
        && (a.brake - b.brake).abs() <= tolerance
}
