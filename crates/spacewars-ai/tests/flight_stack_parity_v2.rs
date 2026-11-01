//! The gameplay driver must fly an archived LF-093 evaluation case exactly as
//! the training bridge flew it under the v2 contract: same commands, same
//! actuator outputs, same physics, with the goal issued each tick where the
//! environment's target is, as a host issues a goal that rides its planet.
use scenario_spacewars::surface_sortie::flight_environment::{
    FlightEnvironment, FlightOutcome, FlightTrace,
};
use spacewars_ai::flight_stack::{FlightStack, FlightStackDriver, GoalOutcome, NavigationGoal};

const TRACE: &str = include_str!("fixtures/learned-flight-stack-v2-clear-case.json");

#[test]
fn runtime_driver_reproduces_the_archived_v2_closed_loop_trace() {
    let trace: FlightTrace = serde_json::from_str(TRACE).unwrap();
    FlightEnvironment::replay(&trace).unwrap();
    let stack = FlightStack::retained_v2().unwrap();
    assert_eq!(stack.contract(), 2);
    let mut env = FlightEnvironment::new(trace.task.clone()).unwrap();
    let mut driver = FlightStackDriver::new(0).unwrap();
    let goal_at = |env: &FlightEnvironment| NavigationGoal {
        id: 7,
        position: env.frame().target_position,
        velocity: env.frame().target_velocity,
        arrival: trace.task.arrival,
        deadline_ticks: trace.task.deadline_ticks,
    };
    for (index, step) in trace.steps.iter().enumerate() {
        let decision = driver.decide(&stack, env.state(), goal_at(&env)).unwrap();
        assert_eq!(Some(decision.action), step.action, "tick {index}");
        assert_eq!(env.step(decision.action).unwrap(), step, "tick {index}");
    }
    assert_eq!(env.progress().outcome, Some(FlightOutcome::Success));
    let decision = driver.decide(&stack, env.state(), goal_at(&env)).unwrap();
    assert_eq!(decision.progress.outcome, Some(GoalOutcome::Arrived));
    assert_eq!(decision.progress.elapsed_ticks as usize, trace.steps.len());
}
