use super::*;

const DT: Duration = Duration::from_nanos(16_666_667);

fn trigger() -> Action {
    PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode()
}

fn empty(automatic_events: bool, seed: u64) -> PizzaState {
    PizzaScenario::init(
        PizzaConfig {
            desired_ball_count: 0,
            automatic_events,
            ..Default::default()
        },
        seed,
    )
}

fn observed(state: &PizzaState) -> PizzaEventState {
    let observation = PizzaScenario::observe(state);
    assert!(observation.payload.len() < 256);
    let decoded: PizzaObservation = serde_json::from_slice(&observation.payload).unwrap();
    assert_eq!(decoded.version, PIZZA_OBSERVATION_VERSION);
    assert_eq!(decoded.tick, state.tick);
    assert_eq!(decoded.event, state.event_state());
    decoded.event
}

#[test]
fn zero_duration_queues_once_without_advancing_and_keeps_pointer_controls() {
    let mut state = empty(false, 7);
    let mut replay = empty(false, 7);
    let pointer = |phase| {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(0.5, 0.25),
            phase,
        })
    };
    PizzaScenario::step(
        &mut state,
        &[trigger(), trigger(), pointer(PointerPhase::Press)],
        Duration::ZERO,
    );
    PizzaScenario::step(
        &mut replay,
        &[trigger(), pointer(PointerPhase::Press)],
        Duration::ZERO,
    );
    assert_eq!(state.tick, 0);
    assert!(state.held_ball_id.is_some());
    assert_eq!(
        observed(&state),
        PizzaEventState::Idle {
            automatic_wait_ticks: None,
            pending_manual: Some(PizzaEventKind::DirectionalGravity),
        }
    );
    let paused = observed(&state);
    for _ in 0..100 {
        PizzaScenario::step(&mut state, &[trigger()], Duration::ZERO);
        PizzaScenario::render_frame(&state);
        assert_eq!(observed(&state), paused);
    }
    for scene in [&mut state, &mut replay] {
        PizzaScenario::step(scene, &[pointer(PointerPhase::Cancel)], Duration::ZERO);
        assert!(scene.held_ball_id.is_none());
        assert_eq!(scene.balls[0].velocity, Vec2::ZERO);
        PizzaScenario::step(scene, &[], DT);
    }
    assert_eq!(state.balls, replay.balls);
    assert_eq!(observed(&state), observed(&replay));
    assert!(matches!(
        state.event_state(),
        PizzaEventState::Active {
            trigger: PizzaEventTrigger::Manual,
            elapsed_ticks: 1,
            remaining_ticks: 899,
            ..
        }
    ));
    let paused = observed(&state);
    for _ in 0..100 {
        PizzaScenario::step(&mut state, &[trigger()], Duration::ZERO);
        assert_eq!(observed(&state), paused);
    }
    for _ in 1..DIRECTIONAL_GRAVITY_TICKS {
        PizzaScenario::step(&mut state, &[trigger()], DT);
    }
    assert_eq!(state.tick, 900);
    assert_eq!(
        observed(&state),
        PizzaEventState::Idle {
            automatic_wait_ticks: None,
            pending_manual: None,
        }
    );
}

#[test]
fn resize_preserves_pending_active_and_idle_state_while_new_runs_reset_it() {
    let mut state = empty(true, 42);
    let initial = observed(&state);
    for phase in 0..3 {
        match phase {
            1 => {
                PizzaScenario::step(&mut state, &[trigger()], Duration::ZERO);
            }
            2 => {
                PizzaScenario::step(&mut state, &[], DT);
            }
            _ => {}
        }
        let before = observed(&state);
        for aspect in [0.6, 800.0 / 480.0, 1024.0 / 768.0] {
            state.set_aspect_ratio(aspect);
            assert_eq!(observed(&state), before);
        }
        assert_eq!(observed(&empty(true, 42)), initial);
    }
    drop(state);
    assert_eq!(observed(&empty(true, 42)), initial);
}

#[test]
fn scheduling_alone_does_not_change_spawning_fragmentation_or_motion() {
    let config = PizzaConfig {
        desired_ball_count: 4,
        ..Default::default()
    };
    let mut enabled = PizzaScenario::init(config.clone(), 31);
    let mut disabled = PizzaScenario::init(
        PizzaConfig {
            automatic_events: false,
            ..config
        },
        31,
    );
    let mut fragmented = false;
    for tick in 0..1000 {
        // Force one ordinary explosion to exercise fragment RNG, in addition
        // to the normal spawner and real contact/motion path.
        if tick == 100 {
            for state in [&mut enabled, &mut disabled] {
                assert!(!state.balls.is_empty());
                state.balls[0].hp = 0.0;
            }
            fragmented = true;
        }
        let actions = if tick == 1 || tick == 950 {
            vec![trigger()]
        } else {
            vec![]
        };
        PizzaScenario::step(&mut enabled, &actions, DT);
        PizzaScenario::step(&mut disabled, &[], DT);
        assert_eq!(enabled.balls, disabled.balls, "tick {tick}");
        assert_eq!(
            enabled.rng.clone().random::<u64>(),
            disabled.rng.clone().random::<u64>()
        );
    }
    assert!(fragmented);
}

#[test]
fn ball_randomness_does_not_change_event_choices() {
    let mut quiet = empty(true, 42);
    let mut busy = empty(true, 42);
    for visit in 0..8 {
        let parent = busy.allocate_ball(Vec2::new(0.5, 0.25), 0.08, Color::GREEN);
        busy.push_ball(parent);
        busy.balls.last_mut().unwrap().hp = 0.0;
        explode_dead_balls(&mut busy);
        assert!(!busy.balls.is_empty());
        assert_ne!(
            quiet.rng.clone().random::<u64>(),
            busy.rng.clone().random::<u64>()
        );
        for tick in 0..DIRECTIONAL_GRAVITY_TICKS {
            let actions = if tick == 0 { vec![trigger()] } else { vec![] };
            for state in [&mut quiet, &mut busy] {
                PizzaScenario::step(state, &actions, DT);
            }
            assert_eq!(
                quiet.event_state(),
                busy.event_state(),
                "visit {visit} tick {tick}"
            );
        }
        while !busy.balls.is_empty() {
            busy.swap_remove_ball(0);
        }
    }
}

#[test]
fn every_controlled_benchmark_ignores_event_settings_and_requests() {
    for backend in [PizzaPhysicsBackend::Classic, PizzaPhysicsBackend::Rapier] {
        for workload in [
            PizzaBenchmarkWorkload::Sparse,
            PizzaBenchmarkWorkload::Dense,
            PizzaBenchmarkWorkload::Churn,
        ] {
            let config = PizzaConfig::benchmark(
                PizzaBenchmarkConfig {
                    backend,
                    workload,
                    ball_count: 8,
                    ..Default::default()
                },
                PizzaBounds::default(),
            );
            let mut baseline = PizzaScenario::init(config.clone(), 19);
            let mut requested = PizzaScenario::init(
                PizzaConfig {
                    automatic_events: true,
                    ..config
                },
                19,
            );
            assert!(!requested.config.automatic_events);
            assert_eq!(baseline.balls, requested.balls);
            for _ in 0..=BENCHMARK_CHURN_INTERVAL_TICKS {
                PizzaScenario::step(&mut requested, &[trigger()], Duration::ZERO);
                PizzaScenario::step(&mut requested, &[trigger()], DT);
                PizzaScenario::step(&mut baseline, &[], DT);
                assert_eq!(observed(&requested), PizzaEventState::Disabled);
                assert_eq!(requested.balls, baseline.balls);
                assert_eq!(requested.benchmark_counts(), baseline.benchmark_counts());
            }
        }
    }
}
