use super::*;

const DT: Duration = Duration::from_nanos(16_666_667);

fn empty(seed: u64) -> PizzaState {
    PizzaScenario::init(
        PizzaConfig {
            desired_ball_count: 0,
            automatic_events: false,
            ..Default::default()
        },
        seed,
    )
}

// Isolate phase-dependent physics without moving test bodies through a warmup.
fn active(direction: PizzaEventDirection, elapsed: u32) -> PizzaState {
    let mut state = (0..128).find_map(|seed| {
        let mut state = empty(seed);
        state.events.request(PizzaEventKind::DirectionalGravity);
        state.events.begin_tick();
        matches!(state.event_state(), PizzaEventState::Active { direction: d, .. } if d == direction)
            .then_some(state)
    }).unwrap();
    for _ in 0..elapsed {
        state.events.finish_tick();
        state.events.begin_tick();
    }
    state
}

fn ball(state: &mut PizzaState, position: Vec2, radius: f32, velocity: Vec2) -> u64 {
    let mut ball = state.allocate_ball(position, radius, Color::GREEN);
    ball.velocity = velocity;
    let id = ball.id;
    state.push_ball(ball);
    id
}

fn near(actual: Vec2, expected: Vec2) {
    assert!(
        (actual - expected).length() < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn envelope_has_two_second_ramps_eleven_second_hold_and_zero_endpoints() {
    assert_eq!(envelope(0), 0.0);
    assert_eq!(envelope(60), 0.5);
    assert_eq!(envelope(840), 0.5);
    assert_eq!(envelope(900), 0.0);
    assert_eq!(envelope(u32::MAX), 0.0);
    for tick in 0..=900 {
        let value = envelope(tick);
        assert!((0.0..=1.0).contains(&value));
        assert!((value - envelope(900 - tick)).abs() < 1e-6);
        if (120..=780).contains(&tick) {
            assert_eq!(value, 1.0);
        }
        if tick < 120 {
            assert!(envelope(tick + 1) > value);
        }
    }
    // Smoothstep starts/ends flat rather than snapping to the linear ramp slope.
    assert!(envelope(1) < 0.001);
    assert!(1.0 - envelope(119) < 0.001);
}

#[test]
fn all_directions_add_equal_acceleration_to_different_masses_with_mutual_gravity() {
    for (direction, vector) in [
        (PizzaEventDirection::Left, Vec2::new(-1.0, 0.0)),
        (PizzaEventDirection::Right, Vec2::new(1.0, 0.0)),
        (PizzaEventDirection::Top, Vec2::new(0.0, 1.0)),
        (PizzaEventDirection::Bottom, Vec2::new(0.0, -1.0)),
    ] {
        for dt in [DT, DT / 2] {
            let mut pulled = active(direction, 120);
            let mut baseline = empty(pulled.seed());
            for state in [&mut pulled, &mut baseline] {
                ball(state, Vec2::new(0.3, 0.3), 0.015, Vec2::new(0.02, -0.01));
                ball(state, Vec2::new(0.7, 0.3), 0.06, Vec2::new(0.02, -0.01));
                PizzaScenario::step(state, &[], dt);
            }
            assert_ne!(
                baseline.balls[0].velocity, baseline.balls[1].velocity,
                "the control must include unequal mutual attraction"
            );
            for (actual, control) in pulled.balls.iter().zip(&baseline.balls) {
                near(
                    actual.velocity - control.velocity,
                    vector * (ACCELERATION * dt.as_secs_f32() * WORLD_TIME_SCALE),
                );
                assert_eq!(actual.hp, control.hp);
            }
        }
    }
}

#[test]
fn held_ball_accumulates_no_field_impulse_and_release_in_fade_responds_immediately() {
    let mut pulled = active(PizzaEventDirection::Right, 120);
    let mut baseline = empty(pulled.seed());
    let pointer = |phase, x| {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(x, 0.3),
            phase,
        })
    };
    for state in [&mut pulled, &mut baseline] {
        ball(state, Vec2::new(0.5, 0.3), 0.03, Vec2::ZERO);
        PizzaScenario::step(state, &[pointer(PointerPhase::Press, 0.5)], Duration::ZERO);
    }
    for _ in 0..720 {
        for state in [&mut pulled, &mut baseline] {
            PizzaScenario::step(state, &[pointer(PointerPhase::Drag, 0.55)], DT);
        }
        assert_eq!(pulled.balls, baseline.balls);
        assert_eq!(pulled.cursor_velocity, baseline.cursor_velocity);
    }
    assert!(matches!(
        pulled.event_state(),
        PizzaEventState::Active {
            elapsed_ticks: 840,
            ..
        }
    ));
    for state in [&mut pulled, &mut baseline] {
        PizzaScenario::step(
            state,
            &[pointer(PointerPhase::Release, 0.55)],
            Duration::ZERO,
        );
    }
    assert_eq!(pulled.balls, baseline.balls);
    for state in [&mut pulled, &mut baseline] {
        PizzaScenario::step(state, &[], DT);
    }
    near(
        pulled.balls[0].velocity - baseline.balls[0].velocity,
        Vec2::new(
            ACCELERATION * 0.5 * DT.as_secs_f32() * WORLD_TIME_SCALE,
            0.0,
        ),
    );
    PizzaScenario::step(
        &mut pulled,
        &[pointer(PointerPhase::Press, 0.55)],
        Duration::ZERO,
    );
    PizzaScenario::step(
        &mut pulled,
        &[pointer(PointerPhase::Cancel, 0.55)],
        Duration::ZERO,
    );
    assert_eq!(pulled.balls[0].velocity, Vec2::ZERO);
    assert!(!pulled.balls[0].invincible);
}

#[test]
fn a_pointer_fling_can_travel_against_the_full_strength_field() {
    let mut state = active(PizzaEventDirection::Top, 120);
    ball(&mut state, Vec2::new(0.5, 0.35), 0.04, Vec2::ZERO);
    let pointer = |phase, y| {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(0.5, y),
            phase,
        })
    };
    PizzaScenario::step(
        &mut state,
        &[pointer(PointerPhase::Press, 0.35)],
        Duration::ZERO,
    );
    PizzaScenario::step(&mut state, &[pointer(PointerPhase::Drag, 0.15)], DT);
    let fling = state.cursor_velocity;
    assert!(fling.y < -0.20);
    PizzaScenario::step(&mut state, &[pointer(PointerPhase::Release, 0.15)], DT);
    near(
        state.balls[0].velocity,
        fling + Vec2::new(0.0, ACCELERATION * DT.as_secs_f32() * WORLD_TIME_SCALE),
    );
    assert!(state.balls[0].velocity.y < -0.20);
}

#[test]
fn newborns_and_real_contact_fragments_join_the_field_without_changing_lifecycle() {
    let mut state = active(PizzaEventDirection::Top, 120);
    state.config.desired_ball_count = 1;
    state.config.ball_spawn_rate = MAX_SPAWN_RATE;
    for _ in 0..10 {
        PizzaScenario::step(&mut state, &[], DT);
        if !state.balls.is_empty() {
            break;
        }
    }
    assert_eq!(state.balls.len(), 1);
    assert!(state.balls[0].velocity.y > 0.0);

    // A real overlapping, approaching pair loses health through Rapier contact
    // impulses. Low initial hp ensures that contact, not a direct explosion call,
    // creates fragments on the normal step path.
    let mut state = active(PizzaEventDirection::Top, 120);
    ball(&mut state, Vec2::new(0.46, 0.3), 0.05, Vec2::new(0.3, 0.0));
    ball(&mut state, Vec2::new(0.54, 0.3), 0.05, Vec2::new(-0.3, 0.0));
    for b in &mut state.balls {
        b.hp = 1e-8;
    }
    PizzaScenario::step(&mut state, &[], DT);
    assert!(state.last_step_metrics.rapier.contacts > 0);
    assert!(state.balls.iter().all(|b| b.id > 2));
    assert!(state.balls.len() > 2);
    // Compare each real fragment in isolation so further inter-fragment contact
    // and mutual attraction cannot mask its first field response.
    for fragment in state.balls {
        let mut pulled = active(PizzaEventDirection::Top, 121);
        let mut baseline = empty(pulled.seed());
        for scene in [&mut pulled, &mut baseline] {
            scene.push_ball(fragment);
            PizzaScenario::step(scene, &[], DT);
        }
        near(
            pulled.balls[0].velocity - baseline.balls[0].velocity,
            Vec2::new(0.0, ACCELERATION * DT.as_secs_f32() * WORLD_TIME_SCALE),
        );
    }
}

#[test]
fn field_wakes_a_sleeping_dynamic_body() {
    let mut state = active(PizzaEventDirection::Top, 120);
    let id = ball(&mut state, Vec2::new(0.5, 0.3), 0.02, Vec2::ZERO);
    let physics = state.physics.as_mut().unwrap();
    physics.remove_entity(PhysicsId::new(id));
    assert!(physics.insert_body(
        ball_body_id(id),
        BodySpec {
            position: Vec2::new(0.5, 0.3),
            can_sleep: true,
            ..Default::default()
        },
        &[ColliderSpec::ball(ball_collider_id(id), 0.02)]
    ));
    let mut metrics = PhysicsStepMetrics::default();
    for _ in 0..600 {
        metrics = physics.step(DT.as_secs_f32() * WORLD_TIME_SCALE);
    }
    assert_eq!(metrics.sleeping_bodies, 1);
    apply(&mut state);
    let physics = state.physics.as_mut().unwrap();
    let metrics = physics.step(DT.as_secs_f32() * WORLD_TIME_SCALE);
    assert_eq!(metrics.sleeping_bodies, 0);
    assert!(physics.motion(ball_body_id(id)).unwrap().linear_velocity.y > 0.0);
}

#[test]
fn completion_preserves_momentum_and_leaves_no_force_or_cue() {
    let mut state = active(PizzaEventDirection::Right, 898);
    ball(&mut state, Vec2::new(0.5, 0.3), 0.02, Vec2::new(0.02, 0.0));
    for _ in 0..2 {
        PizzaScenario::step(&mut state, &[], DT);
    }
    assert!(matches!(state.event_state(), PizzaEventState::Idle { .. }));
    let survivor = state.balls[0];
    assert!(survivor.velocity.x > 0.02);
    assert_eq!(survivor.hp, survivor.radius);
    let mut control = empty(state.seed());
    control.push_ball(survivor);
    for _ in 0..30 {
        for scene in [&mut state, &mut control] {
            PizzaScenario::step(scene, &[], DT);
        }
        assert_eq!(state.balls, control.balls);
        assert_eq!(
            PizzaScenario::render_frame(&state),
            PizzaScenario::render_frame(&control)
        );
    }
}

#[test]
fn pause_and_resize_preserve_phase_direction_and_cue_in_every_orientation() {
    for direction in PizzaEventDirection::ALL {
        let mut state = active(direction, 60);
        let before = state.event_state();
        for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
            state.set_aspect_ratio(aspect);
            for _ in 0..5 {
                PizzaScenario::step(&mut state, &[], Duration::ZERO);
            }
            assert_eq!(state.event_state(), before);
            let frame = PizzaScenario::render_frame(&state);
            let cue = frame
                .layers
                .iter()
                .find(|layer| layer.z == CUE_LAYER)
                .unwrap();
            assert_eq!(cue.primitives.len(), 2);
            let RenderPrimitive::Circle(badge) = &cue.primitives[0] else {
                panic!()
            };
            assert_eq!(badge.fill.unwrap().color.a, 0.85 * 0.5);
            let (direction, _) = field(before);
            let bounds = state.config.bounds;
            let center = Vec2::new(badge.center.x, badge.center.y);
            assert!((center - bounds.center()).dot(direction) > 0.0);
            assert!(center.x - badge.radius > 0.0 && center.x + badge.radius < bounds.width);
            assert!(center.y - badge.radius > 0.0 && center.y + badge.radius < bounds.height);
            let RenderPrimitive::Polygon(arrow) = &cue.primitives[1] else {
                panic!()
            };
            let tip = Vec2::new(arrow.points[3].x, arrow.points[3].y);
            assert!((tip - center).dot(direction) > 0.0);
        }
    }
}
