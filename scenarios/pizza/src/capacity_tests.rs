use super::*;
use std::collections::HashSet;

const DT: Duration = Duration::from_nanos(16_666_667);

fn fixture(count: usize, limit: usize) -> PizzaState {
    capacity_lab::mixed_size_fixture(count, limit, PizzaBounds::default(), 7).unwrap()
}

fn assert_coherent(state: &PizzaState) {
    let population = state.population();
    assert!(population.balls <= population.limit);
    assert_eq!(population.physics_balls, population.balls);
    assert_eq!(population.ball_colliders, population.balls);
    let ids: HashSet<_> = state.balls.iter().map(|b| b.id).collect();
    assert_eq!(ids.len(), state.balls.len());
    let indices = state.ball_indices.as_ref().unwrap();
    assert_eq!(indices.len(), state.balls.len());
    for (index, ball) in state.balls.iter().enumerate() {
        assert_eq!(indices.get(&ball.id), Some(&index));
    }
    let physics = state.physics.as_ref().unwrap();
    for ball in &state.balls {
        assert!(ball.position.x.is_finite() && ball.position.y.is_finite());
        assert!(ball.velocity.x.is_finite() && ball.velocity.y.is_finite());
        assert!(ball.hp.is_finite());
        assert!(physics.contains_body(ball_body_id(ball.id)));
    }
    let frame = PizzaScenario::render_frame(state);
    let rendered = frame
        .layers
        .iter()
        .filter(|l| l.z == BALL_LAYER)
        .map(|l| l.primitives.len())
        .sum::<usize>();
    assert_eq!(rendered, state.balls.len());
}

fn press(state: &mut PizzaState, position: Vec2) {
    PizzaScenario::step(
        state,
        &[Action::Pointer(PointerAction {
            position: RenderPoint::new(position.x, position.y),
            phase: PointerPhase::Press,
        })],
        Duration::ZERO,
    );
}

#[test]
fn pizza_capacity_spawn_and_pointer_boundaries_include_the_held_body() {
    for limit in [500, MAX_BALLS] {
        let mut state = fixture(limit - 1, limit);
        state.config.desired_ball_count = limit;
        state.config.ball_spawn_rate = MAX_SPAWN_RATE;
        for _ in 0..100 {
            state.advance_spawner();
        }
        assert_eq!(state.balls.len(), limit);
        assert_eq!(state.population().births.spawned, 1);
        assert_coherent(&state);
        state.swap_remove_ball(0);
        // This gap is between bands, outside every initial circle.
        let empty = Vec2::new(0.5, state.config.bounds.height * 0.62);
        press(&mut state, empty);
        let held = state.held_ball_id.unwrap();
        assert_eq!(state.balls.len(), limit);
        assert_eq!(state.population().births.pointer_replaced, 0);
        for _ in 0..10 {
            state.advance_spawner();
        }
        assert!(
            state
                .balls
                .iter()
                .any(|ball| ball.id == held && ball.invincible)
        );
        state.set_aspect_ratio(480.0 / 800.0);
        assert_eq!(state.held_ball_id, Some(held));
        state.cancel_pointer();
        assert_eq!(state.held_ball_id, None);
        assert!(state.ball_mut(held).unwrap().moving);
        assert_coherent(&state);
        // Outside the playfield guarantees a new pointer ball, just as an
        // out-of-bounds pointer request does before containment on the next step.
        press(&mut state, Vec2::new(-1.0, -1.0));
        assert_ne!(state.held_ball_id, Some(held));
        assert_eq!(state.population().births.pointer_replaced, 1);
        state.release_pointer();
        state.sync_from_rapier();
        assert_coherent(&state);
    }
}

#[test]
fn pizza_capacity_simultaneous_explosions_obey_both_budgets_and_free_removed_slots() {
    for limit in [500, MAX_BALLS] {
        let mut state = fixture(limit, limit);
        // Isolate lifecycle overflow here; the fixture test below obtains deaths
        // through healthy-body contacts instead of writing health.
        let last = state.balls.len() - 1;
        state.balls[last].hp = 0.0;
        state.balls[last - 1].hp = 0.0;
        explode_dead_balls(&mut state);
        assert_eq!(state.balls.len(), limit);
        let counts = state.population().births;
        assert_eq!(counts.exploded, 2);
        assert_eq!(counts.fragments_created, 2);
        assert_eq!(counts.fragment_capacity_stops, 1);
        assert_eq!(counts.fragment_parents_skipped, 1);
        state.sync_from_rapier();
        assert_coherent(&state);
        state.swap_remove_ball(0);
        assert_eq!(state.population().physics_balls, limit - 1);
    }
}

#[test]
fn pizza_capacity_preserves_defaults_and_lower_population_replays() {
    assert_eq!(PizzaConfig::default().live_ball_limit, 500);
    assert_eq!(PizzaConfig::default().desired_ball_count, 24);
    let mut low = fixture(64, 500);
    let mut high = fixture(64, 5000);
    for _ in 0..12 {
        PizzaScenario::step(&mut low, &[], DT);
        PizzaScenario::step(&mut high, &[], DT);
        assert_eq!(low.balls, high.balls);
        assert!(low.balls.len() < 500);
    }
    assert_eq!(
        PizzaConfig::benchmark(
            PizzaBenchmarkConfig {
                ball_count: MAX_BENCHMARK_BALLS,
                ..Default::default()
            },
            PizzaBounds::default()
        )
        .desired_ball_count,
        10_000
    );
}

#[test]
fn pizza_capacity_fixture_runs_real_contact_damage_at_5000_and_replays() {
    let mut left = fixture(MAX_BALLS, MAX_BALLS);
    let mut right = fixture(MAX_BALLS, MAX_BALLS);
    assert!(left.benchmark_config().is_none());
    assert!(left.balls.iter().all(|b| b.hp == b.radius && !b.invincible));
    let mut contacts = 0;
    let mut explosion_ticks = 0;
    let mut exploded = 0;
    for _ in 0..24 {
        PizzaScenario::step(&mut left, &[], DT);
        PizzaScenario::step(&mut right, &[], DT);
        assert_eq!(left.balls, right.balls);
        assert_eq!(left.population(), right.population());
        contacts += left.last_step_metrics.rapier.contacts;
        explosion_ticks += usize::from(left.birth_counts.exploded > exploded);
        exploded = left.birth_counts.exploded;
        assert_coherent(&left);
    }
    assert!(contacts > 0);
    assert!(explosion_ticks > 1);
    assert!(left.birth_counts.fragments_created > 0);
}

#[test]
fn pizza_capacity_fixture_rejects_invalid_requests_and_supports_target_aspects() {
    assert!(capacity_lab::mixed_size_fixture(5001, 5000, PizzaBounds::default(), 7).is_err());
    assert!(capacity_lab::mixed_size_fixture(500, 499, PizzaBounds::default(), 7).is_err());
    assert!(
        capacity_lab::mixed_size_fixture(5000, 5000, PizzaBounds::from_aspect_ratio(100.0), 7)
            .is_err()
    );
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        let state =
            capacity_lab::mixed_size_fixture(5000, 5000, PizzaBounds::from_aspect_ratio(aspect), 7)
                .unwrap();
        assert_coherent(&state);
    }
}

// Independent old linear lookup as an oracle. Reapply the same real contacts
// to cloned metadata, then restore HP so this check cannot change the cascade.
#[test]
fn indexed_contact_damage_matches_linear_lookup_through_fragment_churn() {
    for count in [64, 500] {
        let mut state = fixture(count, 500);
        let mut contact_updates = 0;
        for tick in 0..180 {
            if tick == 20 {
                let position = state.balls[0].position;
                press(&mut state, position);
            } else if tick == 40 {
                state.release_pointer();
            } else if tick == 60 {
                state.set_aspect_ratio(480.0 / 800.0);
            }
            PizzaScenario::step(&mut state, &[], DT);
            let before = state.balls.clone();
            let mut expected = before.clone();
            let physics = state.physics.as_ref().unwrap();
            contact_updates += usize::from(!physics.contact_events().is_empty());
            for contact in physics.contact_events() {
                if contact.collider_a.role != BALL_COLLIDER_ROLE
                    || contact.collider_b.role != BALL_COLLIDER_ROLE
                {
                    continue;
                }
                for entity in [contact.collider_a.entity, contact.collider_b.entity] {
                    let mass = physics
                        .body_mass(ball_body_id(entity.value()))
                        .unwrap_or(1.0)
                        .max(f32::EPSILON);
                    if let Some(ball) = expected.iter_mut().find(|b| b.id == entity.value())
                        && !ball.invincible
                    {
                        ball.hp -= contact.impulse_magnitude / mass * DAMAGE_SCALAR;
                    }
                }
            }
            apply_contact_damage(&mut state);
            assert_eq!(state.balls, expected);
            state.balls = before;
            assert_coherent(&state);
        }
        assert!(contact_updates > 10);
        assert!(state.birth_counts.exploded > 0);
        assert!(state.birth_counts.fragments_created > 0);
    }
}
