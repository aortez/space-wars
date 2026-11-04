//! The gameplay driver must fly an archived LF-083 evaluation case exactly as
//! the training bridge flew it: same commands, same actuator outputs, same
//! physics, through the live-state observation and motor-input paths. A host
//! whose C math library differs from the recording host's holds the flight to
//! the replay's tolerances instead; see `support/flight_stack_replay.rs`.
use scenario_spacewars::surface_sortie::flight_environment::{FlightEnvironment, FlightTrace};
use spacewars_ai::flight_stack::{FlightStack, NavigationGoal};

#[path = "support/flight_stack_replay.rs"]
mod flight_stack_replay;

const TRACE: &str = include_str!("fixtures/learned-flight-stack-clear-case.json");

#[test]
fn runtime_driver_reproduces_the_archived_closed_loop_trace() {
    let trace: FlightTrace = serde_json::from_str(TRACE).unwrap();
    let stack = FlightStack::retained().unwrap();
    // The v1 goal stands where the case's target started, at its velocity.
    let goal = |_: &FlightEnvironment| NavigationGoal {
        id: 7,
        position: trace.initial.target_position,
        velocity: trace.task.goal_velocity,
        arrival: trace.task.arrival,
        deadline_ticks: trace.task.deadline_ticks,
    };
    flight_stack_replay::drive_archived_trace(&trace, &stack, goal);
}
