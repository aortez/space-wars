//! The gameplay driver must fly an archived LF-083 evaluation case exactly as
//! the training bridge flew it: same commands, same actuator outputs, same
//! physics, through the live-state observation and motor-input paths.
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightEnvironment, FlightOutcome, FlightTrace,
};
use spacewars_ai::flight_stack::{FlightStack, FlightStackDriver, GoalOutcome, NavigationGoal};

const TRACE: &str = include_str!("fixtures/learned-flight-stack-clear-case.json");

#[test]
fn runtime_driver_reproduces_the_archived_closed_loop_trace() {
    let trace: FlightTrace = serde_json::from_str(TRACE).unwrap();
    FlightEnvironment::replay(&trace).unwrap();
    let stack = FlightStack::retained().unwrap();
    let mut env = FlightEnvironment::new(trace.task.clone()).unwrap();
    let mut driver = FlightStackDriver::new(0).unwrap();
    let goal = NavigationGoal {
        id: 7,
        position: trace.initial.target_position,
        velocity: trace.task.goal_velocity,
        arrival: trace.task.arrival,
        deadline_ticks: trace.task.deadline_ticks,
    };
    for (index, step) in trace.steps.iter().enumerate() {
        let decision = driver.decide(&stack, env.state(), goal).unwrap();
        assert_eq!(Some(decision.action), step.action, "tick {index}");
        assert_eq!(env.step(decision.action).unwrap(), step, "tick {index}");
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
    let decision = driver.decide(&stack, env.state(), goal).unwrap();
    assert_eq!(decision.progress.outcome, Some(GoalOutcome::Arrived));
    assert_eq!(decision.progress.elapsed_ticks as usize, trace.steps.len());
}
