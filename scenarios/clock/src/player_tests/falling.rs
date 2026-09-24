use super::*;
use engine_core::Vec2;
use engine_rapier::world::{BodyId, BodyRole, ColliderId, ColliderRole, PhysicsId};

const CHARACTER: BodyId = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
const HULL: ColliderId = ColliderId::new(PhysicsId::new(1), ColliderRole::PRIMARY, 0);

fn player(aspect: f32, responsive: bool) -> ClockState {
    let mut state = ready(aspect, 42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    settings.time_format = engine_common::ClockTimeFormat::TwelveHour;
    settings.rain_amount = engine_common::ClockRainAmount::Heavy;
    state.configure(settings);
    if responsive {
        state.preview_event(ClockEventKind::Rain);
        ticks(&mut state, 480);
    }
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state
}

fn world(state: &ClockState) -> &engine_rapier::world::PhysicsWorld {
    if let Some(duck) = &state.duck_visit {
        duck.arena_world()
    } else if let Some(ActiveEvent::Falling(event)) = &state.active_event {
        event.vacant_arena().unwrap().arena_world()
    } else {
        panic!("shared world expected");
    }
}

fn world_mut(state: &mut ClockState) -> &mut engine_rapier::world::PhysicsWorld {
    state.duck_visit.as_mut().unwrap().arena_world_mut()
}

fn batch(state: &ClockState) -> Vec<(BodyId, Vec2)> {
    world(state)
        .motions()
        .filter(|r| r.id.entity.value() >= 3000)
        .map(|r| (r.id, r.motion.position))
        .collect()
}

// Isolate an actual Falling bar for controlled contact tests. Every body remains
// owned by the real event; only its initial pose changes, never the solver.
fn park_batch(state: &mut ClockState) {
    let bodies = batch(state);
    for (index, (id, _)) in bodies.into_iter().enumerate() {
        world_mut(state).set_pose(
            id,
            Vec2::new(10000.0 + index as f32 * 200.0, 1000.0),
            0.0,
            true,
        );
        world_mut(state).set_velocity(id, Vec2::ZERO, 0.0, true);
    }
}

#[test]
fn falling_shares_one_world_through_dismiss_rejoin_reformation_and_completion() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for responsive in [false, true] {
            let mut state = player(aspect, responsive);
            let before = state.player_duck_state().unwrap();
            let base = state.duck_visit.as_ref().unwrap().physics_counts();
            state.preview_event(ClockEventKind::Falling);
            assert_eq!(state.player_duck_state(), Some(before.clone()));
            assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
            let count = batch(&state).len();
            assert!(count > 0 && count <= 30);
            assert_eq!(state.body_count(), base.0 + count);
            assert!(state.collider_count() <= 128);
            ticks(&mut state, 12);
            let id = state.event_id();
            let initial_batch = batch(&state);
            toggle(&mut state, 1);
            assert_eq!(
                state.body_count(),
                base.0 + count - 1,
                "only the character leaves"
            );
            ticks(&mut state, 30);
            assert!(state.duck_visit.is_none());
            assert_eq!(state.event_id(), id);
            assert_eq!(state.phase_tick(), 42);
            assert_eq!(state.body_count(), base.0 + count - 1);
            assert_ne!(batch(&state), initial_batch, "vacant arena still simulates");
            let retained = batch(&state);
            toggle(&mut state, 2);
            assert_eq!(
                batch(&state),
                retained,
                "no rebuild, step, or teleport on rejoin"
            );
            assert_eq!(state.event_id(), id);
            assert_eq!(state.player_duck_state().unwrap().player, 2);
            let stale = ClockAction::player_duck_input(ClockDuckInput {
                session_id: before.session_id,
                player: 1,
                move_milli: 1000,
                jump: true,
            });
            tick(&mut state, &[stale]);
            ticks(&mut state, 40);
            assert_eq!(state.player_duck_state().unwrap().move_milli, 0);
            assert!(world(&state).motion(CHARACTER).is_some());
            while state.event_phase() == Some(EventPhase::Falling) {
                tick(&mut state, &[]);
            }
            assert!(
                batch(&state).is_empty(),
                "no invisible bars during reformation"
            );
            assert_eq!(state.body_count(), base.0);
            assert_eq!(state.collider_count(), base.1);
            ticks(&mut state, REFORMING_TICKS as usize);
            assert_eq!(state.event_kind(), None);
            assert_eq!(state.player_duck_session().unwrap().1, 2);
            assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
            toggle(&mut state, 2);
            ticks(&mut state, 30);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        }
    }
}

#[test]
fn falling_steps_the_player_once_even_at_entry_and_the_last_event_tick() {
    for aspect in [4.0 / 3.0, 0.6] {
        for opening in [false, true] {
            let mut composed = player(aspect, false);
            let mut baseline = player(aspect, false);
            if opening {
                for state in [&mut composed, &mut baseline] {
                    toggle(state, 1);
                    ticks(state, 30);
                    toggle(state, 1);
                }
            }
            composed.preview_event(ClockEventKind::Falling);
            park_batch(&mut composed);
            for elapsed in 0..330 {
                for state in [&mut composed, &mut baseline] {
                    let action = input(
                        state,
                        if elapsed % 60 < 10 { 500 } else { 0 },
                        elapsed == 80,
                    );
                    tick(state, &[action]);
                }
                assert_eq!(
                    composed.player_duck_state(),
                    baseline.player_duck_state(),
                    "aspect={aspect} opening={opening} tick={elapsed}: no doubled gravity/movement"
                );
            }
        }
    }
}

#[test]
fn falling_pause_live_reading_and_replacement_release_only_the_event_batch() {
    for elapsed in [0, 1, 35, 209, 210, 299] {
        for leave in [false, true] {
            let mut state = player(4.0 / 3.0, false);
            let base = state.body_count();
            state.preview_event(ClockEventKind::Falling);
            park_batch(&mut state);
            ticks(&mut state, elapsed);
            if leave {
                toggle(&mut state, 1);
            }
            let frame = ClockScenario::render_frame(&state);
            let player = state.player_duck_state();
            let motions = world(&state).motions().collect::<Vec<_>>();
            for _ in 0..20 {
                ClockScenario::step(&mut state, &[], Duration::ZERO);
            }
            assert_eq!(ClockScenario::render_frame(&state), frame);
            assert_eq!(state.player_duck_state(), player);
            assert_eq!(world(&state).motions().collect::<Vec<_>>(), motions);
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(0, 0, 0).unwrap(),
                )],
                Duration::ZERO,
            );
            let mut settings = state.settings();
            settings.time_format = engine_common::ClockTimeFormat::TwentyFourHour;
            state.configure(settings);
            state.preview_event(ClockEventKind::ColorCycle);
            assert_eq!(state.player_duck_state(), player);
            assert_eq!(state.body_count(), if leave { 0 } else { base });
            state.set_aspect_ratio(0.6);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        }
    }
}

fn contact_fixture() -> (ClockState, BodyId) {
    let mut state = ready(4.0 / 3.0, 42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    state.configure(settings);
    toggle(&mut state, 1);
    let duck = state.duck_visit.as_mut().unwrap();
    duck.course.as_mut().unwrap().surfaces = vec![Surface {
        start: 0.0,
        end: duck.width,
        height: 0.0,
    }];
    ticks(&mut state, 90);
    let bar = state
        .segments
        .iter()
        .find(|s| s.lit && s.id.kind == digits::SegmentKind::Top)
        .unwrap()
        .id;
    state.preview_event(ClockEventKind::Falling);
    park_batch(&mut state);
    (state, crate::physics::body_id(bar))
}

#[test]
fn actual_falling_bar_pushes_the_duck_through_rapier_contacts() {
    let (mut state, bar) = contact_fixture();
    let duck = state.duck_visit.as_ref().unwrap();
    let r = duck.radius;
    let floor = duck.layout.floor_y;
    let half = duck.layout.pitch * 0.4;
    world_mut(&mut state).set_pose(CHARACTER, Vec2::new(0.0, floor + r * 1.05), 0.0, true);
    world_mut(&mut state).set_pose(bar, Vec2::new(-60.0, floor + half + 0.5), 0.0, true);
    world_mut(&mut state).set_velocity(bar, Vec2::new(160.0, 0.0), 0.0, true);
    let mut contact = false;
    let mut displacement = 0.0_f32;
    for _ in 0..45 {
        tick(&mut state, &[]);
        contact |= world(&state)
            .surface_contacts(HULL)
            .any(|c| c.collider.entity == bar.entity);
        displacement = displacement.max(world(&state).motion(CHARACTER).unwrap().position.x);
    }
    assert!(contact, "event and character must really collide");
    assert!(
        displacement > r,
        "bar should push the character, not pass through: {displacement}"
    );
}

#[test]
fn actual_dynamic_falling_bar_is_a_grounded_jump_surface() {
    let (mut state, bar) = contact_fixture();
    let duck = state.duck_visit.as_ref().unwrap();
    let r = duck.radius;
    let floor = duck.layout.floor_y;
    let half = duck.layout.pitch * 0.4;
    world_mut(&mut state).set_pose(bar, Vec2::new(0.0, floor + half + 0.1), 0.0, true);
    world_mut(&mut state).set_pose(
        CHARACTER,
        Vec2::new(0.0, floor + 2.0 * half + r + 0.2),
        0.0,
        true,
    );
    ticks(&mut state, 30);
    assert!(state.player_duck_state().unwrap().duck.grounded);
    assert!(
        world(&state)
            .surface_contacts(HULL)
            .any(|c| c.collider.entity == bar.entity && c.normal.y > 0.7)
    );
    let action = input(&state, 0, true);
    tick(&mut state, &[action]);
    assert_eq!(state.player_duck_state().unwrap().duck.jumps, 1);
    assert!(world(&state).motion(CHARACTER).unwrap().linear_velocity.y > 40.0);
    ticks(&mut state, 100);
    assert_eq!(
        state.player_duck_state().unwrap().duck.jumps,
        1,
        "holding is not auto-hop"
    );
}

#[test]
fn automatic_falling_is_available_and_its_first_frame_matches_shared_physics() {
    let mut state = player(4.0 / 3.0, false);
    let mut baseline = player(4.0 / 3.0, false);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events = ClockEvents {
        falling: true,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: false,
    };
    state.configure(settings);
    assert!(!state.automatic_events_suspended());
    assert!(!state.event_blocked_by_player(ClockEventKind::Falling));
    let wait = state.next_event_tick().unwrap() - state.simulation_tick();
    ticks(&mut state, wait as usize);
    ticks(&mut baseline, wait as usize);
    assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
    assert_eq!(state.phase_tick(), 1);
    assert_eq!(state.player_duck_state(), baseline.player_duck_state());
    for segment in &state.segments {
        if let SegmentRepresentation::Rigid { position, angle } = segment.representation {
            let motion = world(&state)
                .motion(crate::physics::body_id(segment.id))
                .unwrap();
            assert_eq!(position, motion.position);
            assert_eq!(angle, motion.angle);
        }
    }
    assert_eq!(state.settings(), settings);
    let id = state.event_id();
    state.next_event();
    assert_eq!(state.event_kind(), Some(ClockEventKind::Falling));
    assert_eq!(
        state.event_id(),
        id + 1,
        "Next replaces the shared batch safely"
    );
}

#[test]
fn falling_out_or_walking_out_leaves_the_batch_and_floor_alive_until_cleanup() {
    for exit in [false, true] {
        let mut state = player(4.0 / 3.0, false);
        let base = state.body_count();
        state.preview_event(ClockEventKind::Falling);
        let duck = state.duck_visit.as_ref().unwrap();
        let position = if exit {
            duck.render_position(Vec2::new(
                duck.width + duck.radius * 3.0,
                duck.layout.floor_y + duck.radius,
            ))
        } else {
            Vec2::new(0.0, duck.layout.bounds_min.y - duck.radius * 3.0)
        };
        world_mut(&mut state).set_pose(CHARACTER, position, 0.0, true);
        tick(&mut state, &[]);
        assert_eq!(
            state.player_duck_state().unwrap().duck.outcome,
            Some(if exit {
                ClockDuckOutcome::Exited
            } else {
                ClockDuckOutcome::Fell
            })
        );
        assert!(world(&state).motion(CHARACTER).is_none());
        ticks(&mut state, 30);
        assert!(state.duck_visit.is_none());
        assert!(!batch(&state).is_empty());
        assert_eq!(state.floor_mode(), ClockFloorMode::EventOwned);
        ticks(&mut state, FALLING_TICKS as usize - 31);
        assert_eq!(state.body_count(), base - 1);
        assert!(batch(&state).is_empty());
        ticks(&mut state, REFORMING_TICKS as usize);
        assert_eq!(state.event_kind(), None);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}
