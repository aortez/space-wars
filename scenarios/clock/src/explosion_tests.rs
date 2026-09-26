use super::*;
use engine_common::ClockFloorMode;
use engine_core::Vec2;
use engine_rapier::world::{BodyId, BodyRole, ColliderId, ColliderRole, PhysicsId};
use events::explosion::{BURST_TICKS, EXPLOSION_TICKS, MAX_EXPLOSION_CELLS, WARNING_TICKS};

fn ready(aspect: f32, seed: u64, format: ClockTimeFormat) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            time_format: format,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(23, 58, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}
fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}
fn cells(state: &ClockState) -> &[events::explosion::Cell] {
    let Some(ActiveEvent::Explosion(event)) = &state.active_event else {
        panic!("Explosion expected")
    };
    &event.cells
}
fn world(state: &ClockState) -> &engine_rapier::world::PhysicsWorld {
    state
        .duck_visit
        .as_deref()
        .or_else(|| {
            state
                .active_event
                .as_ref()
                .and_then(ActiveEvent::vacant_arena)
        })
        .expect("shared arena")
        .arena_world()
}
fn toggle(state: &mut ClockState, seat: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(seat)],
        Duration::ZERO,
    );
}

#[test]
fn fully_lit_face_respects_the_cell_cap_including_meridiem() {
    let mut state = ready(4.0 / 3.0, 5, ClockTimeFormat::TwelveHour);
    // Synthetic all-segment face exercises the cap beyond valid clock readings.
    state.display.digits = [Some(8); 4];
    state.display.meridiem = Some("AM");
    state.preview_event(ClockEventKind::Explosion);
    assert_eq!(cells(&state).len(), MAX_EXPLOSION_CELLS);
    ticks(&mut state, WARNING_TICKS);
    assert_eq!(state.body_count(), MAX_EXPLOSION_CELLS + 4);
    ticks(&mut state, EXPLOSION_TICKS - WARNING_TICKS);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}

#[test]
fn shared_world_steps_once_through_release_reformation_and_final_tick() {
    let mut baseline = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    for s in [&mut baseline, &mut state] {
        toggle(s, 1);
        ticks(s, 90);
    }
    state.preview_event(ClockEventKind::Explosion);
    for tick in 1..=EXPLOSION_TICKS + 1 {
        ticks(&mut state, 1);
        ticks(&mut baseline, 1);
        if tick == WARNING_TICKS {
            // Park this event's bodies away from the arena to isolate stepping
            // from physical contact; collision behavior has a separate test.
            let count = cells(&state).len();
            let world = state.duck_visit.as_mut().unwrap().arena_world_mut();
            for i in 0..count {
                let id = BodyId::new(PhysicsId::new(5001 + i as u64), BodyRole::PRIMARY);
                assert!(world.set_pose(
                    id,
                    Vec2::new(10_000.0 + i as f32 * 100.0, 10_000.0),
                    0.0,
                    true
                ));
                assert!(world.set_velocity(id, Vec2::ZERO, 0.0, true));
                assert!(world.set_gravity_scale(id, 0.0, true));
            }
        }
        assert_eq!(
            state.player_duck_state(),
            baseline.player_duck_state(),
            "tick={tick}"
        );
    }
    assert!(state.explosion_state().is_none());
    assert_eq!(state.body_count(), baseline.body_count());
}

#[test]
fn seeded_bursts_replay_and_finish_with_bounded_physics_at_every_aspect() {
    for aspect in [0.25, 0.6, 0.75, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        for seed in 0..2 {
            for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
                let mut a = ready(aspect, seed, format);
                let mut b = ready(aspect, seed, format);
                let baseline = ClockScenario::render_frame(&a);
                a.preview_event(ClockEventKind::Explosion);
                b.preview_event(ClockEventKind::Explosion);
                let count = cells(&a).len();
                assert!(count > 20 && count <= MAX_EXPLOSION_CELLS);
                let mut saw_spin = false;
                let mut saw_spread = false;
                for t in 0..EXPLOSION_TICKS {
                    assert_eq!(cells(&a), cells(&b), "aspect={aspect} seed={seed} tick={t}");
                    assert_eq!(a.explosion_state(), b.explosion_state());
                    let expected = if t < WARNING_TICKS {
                        (EventPhase::Warning, 4)
                    } else if t < WARNING_TICKS + BURST_TICKS {
                        (EventPhase::Exploding, count + 4)
                    } else {
                        (EventPhase::Reforming, 0)
                    };
                    assert_eq!(a.event_phase(), Some(expected.0));
                    assert_eq!(a.body_count(), expected.1);
                    assert_eq!(a.collider_count(), expected.1);
                    for cell in cells(&a) {
                        assert!(
                            cell.position.x.is_finite()
                                && cell.position.y.is_finite()
                                && cell.angle.is_finite()
                        );
                        saw_spin |= cell.angle.abs() > 0.1;
                        saw_spread |=
                            (cell.position.x - cell.origin.x).abs() > Layout::new(aspect).pitch;
                    }
                    if t % 60 == 0 {
                        let frame = ClockScenario::render_frame(&a);
                        assert!(
                            frame
                                .layers
                                .iter()
                                .map(|l| l.primitives.len())
                                .sum::<usize>()
                                < 400
                        );
                        assert_eq!(frame, ClockScenario::render_frame(&b));
                    }
                    ticks(&mut a, 1);
                    ticks(&mut b, 1);
                }
                assert!(saw_spin && saw_spread);
                assert!(a.explosion_state().is_none());
                assert_eq!((a.body_count(), a.collider_count()), (0, 0));
                assert_eq!(a.floor_mode(), ClockFloorMode::Closed);
                assert_eq!(ClockScenario::render_frame(&a), baseline);
            }
        }
    }
}

#[test]
fn paused_reading_and_format_changes_preserve_debris_and_restore_the_latest_face() {
    for elapsed in [
        0,
        18,
        WARNING_TICKS,
        140,
        WARNING_TICKS + BURST_TICKS,
        EXPLOSION_TICKS - 1,
    ] {
        let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwelveHour);
        state.preview_event(ClockEventKind::Explosion);
        ticks(&mut state, elapsed);
        let original = cells(&state).to_vec();
        let counts = (state.body_count(), state.collider_count());
        let phase_tick = state.phase_tick();
        let id = state.event_id();
        let mut settings = state.settings();
        settings.time_format = ClockTimeFormat::TwentyFourHour;
        let reading = ClockReading::new(0, 1, 1).unwrap();
        for _ in 0..3 {
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::configure(settings),
                    ClockAction::set_reading(reading),
                ],
                Duration::ZERO,
            );
            assert_eq!(cells(&state), original);
            assert_eq!((state.body_count(), state.collider_count()), counts);
            assert_eq!(
                (
                    state.event_id(),
                    state.phase_tick(),
                    state.simulation_tick()
                ),
                (id, phase_tick, elapsed)
            );
        }
        ticks(&mut state, EXPLOSION_TICKS - elapsed);
        let mut expected = ready(4.0 / 3.0, 42, settings.time_format);
        ClockScenario::step(
            &mut expected,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        assert_eq!(
            ClockScenario::render_frame(&state),
            ClockScenario::render_frame(&expected)
        );
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
}

#[test]
fn shared_burst_survives_departure_rejoin_and_removes_only_its_own_bodies() {
    for responsive in [false, true] {
        let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwelveHour);
        if responsive {
            state.preview_event(ClockEventKind::Rain);
            ticks(&mut state, 300);
        }
        toggle(&mut state, 1);
        ticks(&mut state, 90);
        state.finish_event();
        let base = (state.body_count(), state.collider_count());
        let player = state.player_duck_state();
        state.preview_event(ClockEventKind::Explosion);
        assert_eq!(state.player_duck_state(), player);
        assert_eq!((state.body_count(), state.collider_count()), base);
        ticks(&mut state, WARNING_TICKS + 5);
        let count = cells(&state).len();
        assert_eq!(state.body_count(), base.0 + count);
        assert_eq!(state.collider_count(), base.1 + count);
        assert!(state.explosion_state().unwrap().shared_arena);
        let before = cells(&state).to_vec();
        let id = state.event_id();
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        assert!(state.duck_visit.is_none());
        assert_eq!(state.body_count(), base.0 + count - 1);
        assert_ne!(cells(&state), before, "retained arena keeps simulating");
        let before = world(&state).motions().collect::<Vec<_>>();
        toggle(&mut state, 2);
        assert_eq!(
            world(&state).motions().collect::<Vec<_>>(),
            before,
            "rejoin does not step or reconstruct debris"
        );
        assert_eq!(state.event_id(), id);
        while state.event_phase() != Some(EventPhase::Reforming) {
            ticks(&mut state, 1);
        }
        assert_eq!((state.body_count(), state.collider_count()), base);
        assert!(world(&state).motions().all(|m| m.id.entity.value() < 5000));
        ticks(&mut state, REFORMING_TICKS);
        assert_eq!(state.player_duck_session().unwrap().1, 2);
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        toggle(&mut state, 2);
        ticks(&mut state, 30);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}

#[test]
fn joining_each_phase_preserves_existing_cells_and_time_then_resize_cleans_up() {
    for elapsed in [0, 18, 36, 80, 245, 246, 300] {
        let mut state = ready(0.6, 7, ClockTimeFormat::TwelveHour);
        state.preview_event(ClockEventKind::Explosion);
        ticks(&mut state, elapsed);
        let before = cells(&state).to_vec();
        let counts = (state.body_count(), state.collider_count());
        let phase_tick = state.phase_tick();
        toggle(&mut state, 1);
        assert_eq!(cells(&state), before);
        assert_eq!(state.phase_tick(), phase_tick);
        // The arena is rebuilt only if physics was already released for reformation.
        if elapsed < WARNING_TICKS + BURST_TICKS {
            assert_eq!((state.body_count(), state.collider_count()), counts);
        }
        ticks(&mut state, 2);
        state.set_aspect_ratio(5.0 / 3.0);
        assert!(state.explosion_state().is_none() && state.player_duck_state().is_none());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}

#[test]
fn replacement_and_duplicate_preview_keep_resource_counts_bounded() {
    for elapsed in [0, 20, 100, 260] {
        let mut state = ready(5.0 / 3.0, 99, ClockTimeFormat::TwelveHour);
        toggle(&mut state, 1);
        ticks(&mut state, 90);
        let base = state.body_count();
        for _ in 0..4 {
            state.preview_event(ClockEventKind::Explosion);
            ticks(&mut state, elapsed);
            assert!(state.body_count() <= base + MAX_EXPLOSION_CELLS);
        }
        let player = state.player_duck_state().unwrap();
        let retained = world(&state)
            .motions()
            .filter(|m| m.id.entity.value() < 5000)
            .collect::<Vec<_>>();
        state.preview_event(ClockEventKind::ColorCycle);
        assert_eq!(state.body_count(), base);
        assert_eq!(world(&state).motions().collect::<Vec<_>>(), retained);
        let after = state.player_duck_state().unwrap();
        assert_eq!(
            (
                after.session_id,
                after.player,
                after.move_milli,
                after.jump_held
            ),
            (
                player.session_id,
                player.player,
                player.move_milli,
                player.jump_held
            )
        );
        // Grounding may change immediately if the removed debris was support;
        // no physical pose, velocity or controller ownership should change.
        assert!(world(&state).motions().all(|m| m.id.entity.value() < 5000));
    }
}

#[test]
fn explosion_steps_the_visit_once_and_real_cells_can_hit_the_duck() {
    let mut baseline = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    for s in [&mut baseline, &mut state] {
        toggle(s, 1);
        ticks(s, 90);
    }
    state.preview_event(ClockEventKind::Explosion);
    // Before release there are no extra colliders/forces: controller and gravity
    // must remain byte-for-byte equal, even at the first event tick.
    for _ in 0..WARNING_TICKS - 1 {
        ticks(&mut state, 1);
        ticks(&mut baseline, 1);
        assert_eq!(state.player_duck_state(), baseline.player_duck_state());
    }
    ticks(&mut state, 1);
    let character = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    let target = BodyId::new(PhysicsId::new(5001), BodyRole::PRIMARY);
    let physics = state.duck_visit.as_mut().unwrap().arena_world_mut();
    let position = physics.motion(character).unwrap().position;
    physics.set_pose(target, position + Vec2::new(0.0, 40.0), 0.0, true);
    physics.set_velocity(target, Vec2::new(0.0, -150.0), 0.0, true);
    let mut contact = false;
    for _ in 0..60 {
        ticks(&mut state, 1);
        contact |= world(&state)
            .surface_contacts(ColliderId::new(character.entity, ColliderRole::PRIMARY, 0))
            .any(|contact| contact.collider.entity == target.entity);
    }
    assert!(
        contact,
        "Explosion cells must collide in the actual duck world"
    );
}
