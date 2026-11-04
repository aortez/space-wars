//! The gameplay driver must fly an archived LF-095 evaluation case exactly as
//! the training bridge flew it under the v2 contract: same commands, same
//! actuator outputs, same physics, with the goal issued each tick where the
//! environment's target is, as a host issues a goal that rides its planet. A
//! host whose C math library differs from the recording host's holds the
//! flight to the replay's tolerances instead; see
//! `support/flight_stack_replay.rs`.
use scenario_spacewars::surface_sortie::flight_environment::{FlightEnvironment, FlightTrace};
use spacewars_ai::flight_stack::{FlightStack, NavigationGoal};

#[path = "support/flight_stack_replay.rs"]
mod flight_stack_replay;

const TRACE: &str = include_str!("fixtures/learned-flight-stack-v2-clear-case.json");

#[test]
fn runtime_driver_reproduces_the_archived_v2_closed_loop_trace() {
    let trace: FlightTrace = serde_json::from_str(TRACE).unwrap();
    let stack = FlightStack::retained_v2().unwrap();
    assert_eq!(stack.contract(), 2);
    let goal = |env: &FlightEnvironment| NavigationGoal {
        id: 7,
        position: env.frame().target_position,
        velocity: env.frame().target_velocity,
        arrival: trace.task.arrival,
        deadline_ticks: trace.task.deadline_ticks,
    };
    flight_stack_replay::drive_archived_trace(&trace, &stack, goal);
}
