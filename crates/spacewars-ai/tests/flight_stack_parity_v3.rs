//! The gameplay driver must fly an archived LF-100 evaluation case exactly as
//! the training bridge flew it under the v3 contract: same commands, with the
//! turn sense among them, same actuator outputs, same physics. The case is
//! from the grounded family: the ship comes to rest on the ground under
//! neutral controls, a settle the replay steps as recorded, and then the stack
//! turns it the way the ground leaves free and climbs. The environment
//! counted the settle toward its deadline, so the goal, issued when the stack
//! takes the ship, carries the time left, as a host's would. A host whose C
//! math library differs from the recording host's holds the flight to the
//! replay's tolerances instead; see `support/flight_stack_replay.rs`.
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightAction, FlightEnvironment, FlightTrace,
};
use spacewars_ai::flight_stack::{FlightStack, NavigationGoal};

#[path = "support/flight_stack_replay.rs"]
mod flight_stack_replay;

const TRACE: &str = include_str!("fixtures/learned-flight-stack-v3-grounded-case.json");

#[test]
fn runtime_driver_reproduces_the_archived_v3_grounded_closed_loop_trace() {
    let trace: FlightTrace = serde_json::from_str(TRACE).unwrap();
    let stack = FlightStack::retained_v3().unwrap();
    assert_eq!(stack.contract(), 3);
    assert!(trace.task.contact_allowed);
    let settle = trace
        .steps
        .iter()
        .take_while(|step| step.action == Some(FlightAction::default()))
        .count() as u32;
    assert!(settle > 0, "the case begins with a settle on the ground");
    let goal = |env: &FlightEnvironment| NavigationGoal {
        id: 7,
        position: env.frame().target_position,
        velocity: env.frame().target_velocity,
        arrival: trace.task.arrival,
        deadline_ticks: trace.task.deadline_ticks - settle,
    };
    flight_stack_replay::drive_archived_trace(&trace, &stack, goal);
}
