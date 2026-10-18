use super::*;
use scenario_pizza::{PizzaEventKind, PizzaEventState, PizzaEventTrigger};

struct Session {
    scenario: HostedScenario,
    controls: ScenarioControls,
    settings: Settings,
    input: ClientInput,
    paused: bool,
    benchmark: bool,
    accumulator: Duration,
    revision: u64,
}

impl Session {
    fn new() -> Self {
        let mut settings = Settings::default();
        settings.pizza.desired_balls = 0;
        settings.pizza.automatic_events = false;
        Self {
            scenario: HostedScenario::new(
                "pizza",
                7,
                &settings,
                BENCHMARK_VIEWPORT,
                ScenarioStartMode::Normal,
            )
            .unwrap(),
            controls: ScenarioControls::default(),
            settings,
            input: ClientInput::default(),
            paused: false,
            benchmark: false,
            accumulator: Duration::ZERO,
            revision: 1,
        }
    }
    fn status(&self) -> PizzaStatus {
        self.scenario.inner.pizza_status().unwrap()
    }
    fn step(&mut self, elapsed: Duration) -> HostStepResult {
        let tick_model = self.scenario.tick_model();
        let result = step_scenario(
            &mut self.scenario,
            "pizza",
            7,
            tick_model,
            Some(Duration::from_secs_f64(1.0 / 60.0)),
            elapsed,
            &mut self.accumulator,
            &mut self.input,
            &mut self.controls,
            &mut self.paused,
            &mut self.benchmark,
            false,
            &self.settings,
            BENCHMARK_VIEWPORT,
            &[],
        );
        if result.scenario_replaced {
            self.revision += 1;
        }
        let mut status = self.status();
        status.scenario_revision = self.revision;
        status.paused = self.paused;
        self.controls.pizza_status = Some(status);
        result
    }
    fn tick(&mut self) {
        self.step(Duration::from_secs_f64(1.0 / 60.0));
    }
    fn pause(&mut self) {
        self.controls.request_pause();
        self.tick();
        assert!(self.paused);
    }
}

#[test]
fn pizza_menu_trigger_consumes_input_and_preserves_pause_resize_and_completion() {
    let mut s = Session::new();
    s.pause();
    assert!(s.controls.request_pizza_gravity());
    assert!(!s.controls.request_pizza_gravity());
    // Even a pointer press queued in the same dispatch as activation is consumed.
    s.input
        .push_pointer_event(crate::input::ScreenPointerEvent {
            position: engine_common::RenderPoint::new(320.0, 200.0),
            phase: engine_common::PointerPhase::Press,
        });
    s.tick();
    assert!(!s.paused);
    assert_eq!(s.status().observation.tick, 0);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Idle {
            pending_manual: Some(PizzaEventKind::DirectionalGravity),
            ..
        }
    ));
    s.step(Duration::ZERO);
    assert_eq!(s.status().observation.tick, 0);
    s.tick();
    assert_eq!(s.status().ball_count, 0);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Active {
            trigger: PizzaEventTrigger::Manual,
            elapsed_ticks: 1,
            ..
        }
    ));
    for target in [60, 360, 840] {
        while s.status().observation.tick < target {
            s.tick();
        }
        s.pause();
        let before = s.status().observation;
        assert!(!s.controls.request_pizza_gravity());
        s.scenario.inner.set_viewport(Viewport::new(480.0, 800.0));
        for _ in 0..10 {
            s.step(Duration::from_secs(2));
        }
        assert_eq!(s.status().observation, before);
        s.controls.request_resume();
        s.tick();
        assert_eq!(s.status().observation, before);
    }
    while s.status().observation.tick < 900 {
        s.tick();
    }
    assert_eq!(
        s.status().observation.event,
        PizzaEventState::Idle {
            automatic_wait_ticks: None,
            pending_manual: None
        }
    );
    // Off stays off beyond the longest automatic gap, including busy refusals.
    for _ in 0..7201 {
        s.tick();
    }
    assert_eq!(
        s.status().observation.event,
        PizzaEventState::Idle {
            automatic_wait_ticks: None,
            pending_manual: None
        }
    );
}

#[test]
fn pizza_requests_do_not_survive_restart_benchmark_clear_or_revision_change() {
    let mut s = Session::new();
    s.pause();
    assert!(s.controls.request_pizza_gravity());
    s.controls.request_restart();
    assert!(s.step(Duration::ZERO).scenario_replaced);
    s.tick();
    assert_eq!(s.revision, 2);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Idle {
            pending_manual: None,
            ..
        }
    ));
    s.pause();
    assert!(s.controls.request_pizza_gravity());
    s.controls.pizza_status.as_mut().unwrap().scenario_revision += 1;
    s.tick();
    assert!(s.paused);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Idle {
            pending_manual: None,
            ..
        }
    ));
    assert!(s.controls.request_pizza_gravity());
    s.controls.clear();
    s.tick();
    assert!(s.paused);
    assert!(s.controls.request_pizza_gravity());
    s.controls.request_benchmark();
    s.tick();
    assert_eq!(s.status().observation.event, PizzaEventState::Disabled);
    s.pause();
    assert!(!s.controls.request_pizza_gravity());
    // A fresh normal run takes current settings, even after a benchmark.
    s.settings.pizza.automatic_events = true;
    s.controls.request_restart();
    s.tick();
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Idle {
            automatic_wait_ticks: Some(_),
            pending_manual: None
        }
    ));
    s.controls.clear();
    assert!(s.controls.pizza_status().is_none());
    assert!(!s.controls.request_pizza_gravity());
}
