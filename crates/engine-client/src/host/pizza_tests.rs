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
        self.step_projected(elapsed, &[])
    }
    fn step_projected(
        &mut self,
        elapsed: Duration,
        projections: &[render::FrameProjection],
    ) -> HostStepResult {
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
            projections,
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
    /// The same normal run, advanced by a manually paced simulation worker.
    fn with_worker() -> Self {
        use crate::client_scenarios::{PizzaClientScenario, PizzaWorkerClientScenario};
        let mut session = Self::new();
        let direct = session
            .scenario
            .inner
            .as_any_mut()
            .downcast_mut::<PizzaClientScenario>()
            .unwrap();
        let state = std::mem::replace(
            &mut direct.state,
            <scenario_pizza::PizzaScenario as engine_common::Scenario>::init(Default::default(), 0),
        );
        let worker =
            PizzaWorkerClientScenario::spawn(state, None, BENCHMARK_VIEWPORT.aspect_ratio())
                .unwrap();
        worker.worker().use_manual_pacing();
        session.scenario = HostedScenario {
            inner: Box::new(worker),
            asset: ScenarioAsset::None,
        };
        session
    }
    fn advance_worker(&self, updates: u64) {
        use crate::client_scenarios::PizzaWorkerClientScenario;
        self.scenario
            .inner
            .as_any()
            .downcast_ref::<PizzaWorkerClientScenario>()
            .unwrap()
            .worker()
            .advance(updates);
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
fn pizza_capacity_host_pause_resize_restart_and_teardown() {
    use crate::client_scenarios::PizzaClientScenario;
    use scenario_pizza::{PizzaBounds, capacity_lab};
    let mut s = Session::new();
    s.settings.pizza.live_ball_limit = 5_000;
    let pizza = s
        .scenario
        .inner
        .as_any_mut()
        .downcast_mut::<PizzaClientScenario>()
        .unwrap();
    pizza.state =
        capacity_lab::mixed_size_fixture(5_000, 5_000, PizzaBounds::default(), 7).unwrap();
    s.pause();
    let before = s.status();
    assert_eq!(before.population.balls, 5_000);
    s.scenario.inner.set_viewport(Viewport::new(480.0, 800.0));
    s.step(Duration::from_secs(2));
    assert_eq!(s.status().observation, before.observation);
    assert_eq!(s.status().population, before.population);
    s.controls.request_resume();
    s.tick();
    s.tick();
    let population = s.status().population;
    assert_eq!(population.balls, population.physics_balls);
    assert!(population.balls <= 5_000);
    s.controls.request_restart();
    assert!(s.step(Duration::ZERO).scenario_replaced);
    assert_eq!(s.status().population.limit, 5_000);
    assert_eq!(s.status().population.balls, 0);
    assert_eq!(s.status().population.births, Default::default());
    // The launcher drops HostedScenario and clears its published controls.
    drop(s.scenario);
    s.controls.clear();
    assert!(s.controls.pizza_status().is_none());
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

#[test]
fn pizza_worker_host_forwards_input_and_pauses_between_updates() {
    let mut s = Session::with_worker();
    // The host resumes the worker but leaves its updates to the worker.
    assert_eq!(s.step(Duration::from_secs(1)).updates, 0);
    s.advance_worker(10);
    s.tick();
    assert_eq!(s.status().observation.tick, 10);

    // A paused trigger applies before the host resumes the worker.
    s.pause();
    assert!(s.controls.request_pizza_gravity());
    s.tick();
    assert!(!s.paused);
    assert_eq!(s.status().observation.tick, 10);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Idle {
            pending_manual: Some(PizzaEventKind::DirectionalGravity),
            ..
        }
    ));
    s.advance_worker(1);
    assert!(matches!(
        s.status().observation.event,
        PizzaEventState::Active {
            trigger: PizzaEventTrigger::Manual,
            elapsed_ticks: 1,
            ..
        }
    ));

    // Pointer input reaches the worker's next update; pausing cancels it.
    let frames = s
        .scenario
        .render_frames(RenderBackend::Vector, BENCHMARK_VIEWPORT);
    let projections =
        render::frame_projections(&frames, BENCHMARK_VIEWPORT, s.scenario.frame_layout());
    s.input
        .push_pointer_event(crate::input::ScreenPointerEvent {
            position: engine_common::RenderPoint::new(640.0, 200.0),
            phase: engine_common::PointerPhase::Press,
        });
    assert_eq!(
        s.step_projected(Duration::from_secs(1), &projections)
            .updates,
        0
    );
    s.advance_worker(1);
    assert!(s.status().held_ball_id.is_some());
    s.controls.request_pause();
    s.step_projected(Duration::ZERO, &projections);
    assert!(s.paused);
    let paused = s.status();
    assert!(paused.held_ball_id.is_none());
    assert_eq!(paused.observation.tick, 12);
    for _ in 0..10 {
        s.step(Duration::from_secs(2));
    }
    assert_eq!(s.status().observation, paused.observation);

    // Restart joins the worker and replaces the run.
    s.controls.request_restart();
    assert!(s.step(Duration::ZERO).scenario_replaced);
    assert_eq!(s.status().observation.tick, 0);
}

#[test]
fn worker_frames_are_presented_once_per_frame_and_viewport() {
    let wide = Viewport::new(1024.0, 768.0);
    let tall = Viewport::new(480.0, 800.0);
    // Scenarios without a worker present every tick.
    assert!(!worker_frame_on_screen(None, None));
    assert!(!worker_frame_on_screen(Some((1, wide)), None));
    assert!(worker_frame_on_screen(Some((1, wide)), Some((1, wide))));
    assert!(!worker_frame_on_screen(Some((2, wide)), Some((1, wide))));
    assert!(!worker_frame_on_screen(Some((2, tall)), Some((2, wide))));
}
