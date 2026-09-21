use super::*;
use engine_common::{ClockDuckOutcome, ClockFloorMode};
use events::duck::planner::Surface;

mod composition;

fn ready(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Demo,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 59, 59).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn tick(state: &mut ClockState, actions: &[Action]) {
    ClockScenario::step(state, actions, Duration::from_nanos(16_666_667));
}

fn ticks(state: &mut ClockState, count: usize) {
    for _ in 0..count {
        tick(state, &[]);
    }
}

fn toggle(state: &mut ClockState, player: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(player)],
        Duration::ZERO,
    );
}

fn input(state: &ClockState, move_milli: i16, jump: bool) -> Action {
    let (session_id, player) = state.player_duck_session().unwrap();
    ClockAction::player_duck_input(ClockDuckInput {
        session_id,
        player,
        move_milli,
        jump,
    })
}

#[test]
fn player_actions_are_versioned_bounded_and_reject_stale_or_other_owner_input() {
    let mut state = ready(800.0 / 480.0, 42);
    toggle(&mut state, 2);
    let action = input(&state, -750, true);
    let decoded = ClockAction::decode(&action).unwrap();
    assert!(matches!(
        decoded,
        ClockAction::PlayerDuckInput(ClockDuckInput {
            player: 2,
            move_milli: -750,
            jump: true,
            ..
        })
    ));
    let Action::Scenario { payload, .. } = action else {
        unreachable!()
    };
    for len in 0..payload.len() {
        assert!(
            ClockAction::decode(&Action::scenario(
                CLOCK_ACTION_PLAYER_DUCK_INPUT,
                payload[..len].to_vec()
            ))
            .is_none()
        );
    }
    for (index, value) in [(0, 0), (2, 0), (2, 3), (13, 2)] {
        let mut bad = payload.clone();
        bad[index] = value;
        assert!(
            ClockAction::decode(&Action::scenario(CLOCK_ACTION_PLAYER_DUCK_INPUT, bad)).is_none()
        );
    }
    let mut invalid_axis = payload;
    invalid_axis[11..13].copy_from_slice(&1001_i16.to_le_bytes());
    assert!(
        ClockAction::decode(&Action::scenario(
            CLOCK_ACTION_PLAYER_DUCK_INPUT,
            invalid_axis
        ))
        .is_none()
    );
    for player in [0, 3] {
        assert!(ClockAction::decode(&ClockAction::toggle_player_duck(player)).is_none());
    }
    let original = state.player_duck_session().unwrap();
    toggle(&mut state, 1);
    assert_eq!(state.player_duck_session(), Some(original));
    let stale = ClockAction::player_duck_input(ClockDuckInput {
        session_id: original.0 + 1,
        player: 2,
        move_milli: 1000,
        jump: true,
    });
    let foreign = ClockAction::player_duck_input(ClockDuckInput {
        session_id: original.0,
        player: 1,
        move_milli: 1000,
        jump: true,
    });
    tick(&mut state, &[stale, foreign]);
    assert_eq!(state.player_duck_state().unwrap().move_milli, 0);
    toggle(&mut state, 2);
    ticks(&mut state, 30);
    toggle(&mut state, 2);
    assert_ne!(state.player_duck_session().unwrap().0, original.0);
}

#[test]
fn player_visit_outlives_automatic_visual_events_and_keeps_live_time() {
    let mut state = ready(800.0 / 480.0, 42);
    let settings = state.settings();
    toggle(&mut state, 1);
    let session = state.player_duck_session();
    let mut seen = Vec::new();
    for _ in 0..3 * DUCK_TICKS {
        tick(&mut state, &[]);
        if let Some(kind) = state.event_kind() {
            assert!(!EVENT_CATALOG[kind as usize].uses_floor());
            seen.push(kind);
        }
    }
    assert_eq!(
        state.player_duck_session(),
        session,
        "no timed-event eviction"
    );
    assert!(seen.contains(&ClockEventKind::ColorCycle));
    assert!(seen.contains(&ClockEventKind::Marquee));
    assert!(!state.automatic_events_suspended());
    assert!(state.body_count() <= 9);
    assert!(state.player_duck_state().unwrap().duck.navigation.is_none());
    // Isolate the minute transition from any due periodic deadline.
    let mut slide_only = settings;
    slide_only.events.color_cycle = false;
    slide_only.events.marquee = false;
    state.configure(slide_only);
    state.finish_event();
    ticks(&mut state, COOLDOWN_TICKS as usize);
    let reading = ClockReading::new(13, 0, 0).unwrap();
    tick(&mut state, &[ClockAction::set_reading(reading)]);
    assert_eq!(
        state.display(),
        digits::snapshot(reading, settings.time_format)
    );
    assert_eq!(state.event_kind(), Some(ClockEventKind::DigitSlide));
    state.configure(settings);
    let before = state.player_duck_state();
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.player_duck_state(), before);
    assert_eq!(ClockScenario::render_frame(&state), frame);
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    assert!(state.player_duck_session().is_none());
    assert!(!state.automatic_events_suspended());
    assert_eq!(state.body_count(), 0);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    ticks(&mut state, COOLDOWN_TICKS as usize);
    assert!(state.next_event_tick().is_some());
    for _ in 0..601 {
        tick(&mut state, &[]);
        if state.event_kind().is_some() {
            break;
        }
    }
    assert!(state.event_kind().is_some(), "automatic schedule resumes");
    assert_eq!(state.settings(), settings);
}

#[test]
fn disabled_events_and_off_profile_do_not_disable_player_visits_or_change_preferences() {
    let mut state = ready(800.0 / 480.0, 42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    settings.events = ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: false,
    };
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    assert!(state.player_duck_state().is_some());
    let session = state.player_duck_session();
    ClockScenario::step(&mut state, &[ClockAction::next_event()], Duration::ZERO);
    assert_eq!(
        state.player_duck_session(),
        session,
        "nothing enabled means no replacement"
    );
    assert_eq!(state.settings(), settings);
    toggle(&mut state, 1);
    ticks(&mut state, 1000);
    assert_eq!(state.player_duck_session(), None);
    assert_eq!(state.next_event_tick(), None);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.settings(), settings);
}

#[test]
fn player_jump_needs_a_fresh_grounded_press_and_never_auto_hops_or_buffers() {
    let mut state = ready(800.0 / 480.0, 42);
    toggle(&mut state, 1);
    let held = input(&state, 0, true);
    tick(&mut state, std::slice::from_ref(&held));
    ticks(&mut state, 90);
    assert_eq!(
        state.player_duck_state().unwrap().duck.jumps,
        0,
        "opening press is not buffered"
    );
    let release = input(&state, 0, false);
    tick(&mut state, std::slice::from_ref(&release));
    tick(&mut state, std::slice::from_ref(&held));
    assert_eq!(state.player_duck_state().unwrap().duck.jumps, 1);
    ticks(&mut state, 5);
    tick(&mut state, &[release]);
    tick(&mut state, std::slice::from_ref(&held));
    for _ in 0..240 {
        tick(&mut state, std::slice::from_ref(&held));
    }
    let duck = state.player_duck_state().unwrap().duck;
    assert!(duck.grounded);
    assert_eq!(
        duck.jumps, 1,
        "airborne re-press/held landing does not jump"
    );
    let release = input(&state, 0, false);
    tick(&mut state, &[release]);
    tick(&mut state, &[held]);
    assert_eq!(state.player_duck_state().unwrap().duck.jumps, 2);
}

#[test]
fn screen_relative_movement_back_wall_and_real_exit_work_in_both_directions() {
    let mut directions = [false; 2];
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        for seed in 0..6 {
            let mut state = ready(aspect, seed);
            toggle(&mut state, 1);
            let scene = state.player_duck.as_mut().unwrap();
            let direction = scene.direction;
            directions[usize::from(direction > 0.0)] = true;
            // A flat authored fixture isolates entry, screen mirroring and exit
            // from obstacle planning. Movement still goes through real Rapier.
            scene.course.as_mut().unwrap().surfaces = vec![Surface {
                start: -scene.radius * 8.0,
                end: scene.width + scene.radius * 8.0,
                height: 0.0,
            }];
            ticks(&mut state, 90);
            let initial_x = state
                .player_duck_state()
                .unwrap()
                .duck
                .position_milli
                .unwrap()[0];
            let back = input(&state, (-direction * 1000.0) as i16, false);
            tick(&mut state, &[back]);
            ticks(&mut state, 90);
            let at_wall = state.player_duck_state().unwrap();
            let x = at_wall.duck.position_milli.unwrap()[0];
            assert!((x - initial_x) as f32 * direction < 0.0);
            assert_eq!(
                at_wall.duck.outcome, None,
                "physical rear wall keeps player on screen"
            );
            state.preview_event(ClockEventKind::Marquee);
            let forward = input(&state, (direction * 1000.0) as i16, false);
            tick(&mut state, &[forward]);
            let mut exited = false;
            for _ in 0..600 {
                if let Some(player) = state.player_duck_state() {
                    exited |= player.duck.outcome == Some(ClockDuckOutcome::Exited);
                } else {
                    break;
                }
                tick(&mut state, &[]);
            }
            assert!(exited, "seed={seed} aspect={aspect}");
            assert!(state.player_duck_state().is_none());
            assert_eq!(state.event_kind(), Some(ClockEventKind::Marquee));
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        }
    }
    assert_eq!(directions, [true, true]);
}

#[test]
fn missing_a_gap_falls_and_releases_player_resources() {
    let mut state = ready(800.0 / 480.0, 3);
    toggle(&mut state, 1);
    let scene = state.player_duck.as_mut().unwrap();
    let axis = (scene.direction * 1000.0) as i16;
    scene.course.as_mut().unwrap().surfaces = vec![
        Surface {
            start: -64.0,
            end: scene.width * 0.3,
            height: 0.0,
        },
        Surface {
            start: scene.width * 0.7,
            end: scene.width + 64.0,
            height: 0.0,
        },
    ];
    ticks(&mut state, 90);
    state.preview_event(ClockEventKind::Marquee);
    let forward = input(&state, axis, false);
    tick(&mut state, &[forward]);
    let mut fell = false;
    for _ in 0..300 {
        if let Some(player) = state.player_duck_state() {
            fell |= player.duck.outcome == Some(ClockDuckOutcome::Fell);
        } else {
            break;
        }
        tick(&mut state, &[]);
    }
    assert!(fell);
    assert!(state.player_duck_state().is_none());
    assert_eq!(state.event_kind(), Some(ClockEventKind::Marquee));
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!(state.body_count(), 0);
}

#[test]
fn replacing_events_and_player_visits_resize_and_repeated_cleanup_are_bounded() {
    let mut state = ready(800.0 / 480.0, 42);
    for _ in 0..3 {
        for event in ClockEventKind::ALL {
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(event)],
                Duration::ZERO,
            );
            ticks(&mut state, 90);
            let event_id = state.event_id();
            let active_at_spawn = state.event_kind();
            toggle(&mut state, 1);
            assert_eq!(
                state.event_id(),
                event_id,
                "player is not a scheduler event"
            );
            assert_eq!(
                state.event_kind(),
                active_at_spawn.filter(|kind| !EVENT_CATALOG[*kind as usize].uses_floor())
            );
            assert!(state.rain_state().is_none() && state.meltdown_state().is_none());
            assert_eq!(state.body_count(), 0, "old world dropped before opening");
            ticks(&mut state, 60);
            assert!(state.body_count() <= 9);
            ClockScenario::step(&mut state, &[ClockAction::next_event()], Duration::ZERO);
            assert!(state.player_duck_state().is_some());
            assert_eq!(state.event_id(), event_id + 1);
            assert!(!EVENT_CATALOG[state.event_kind().unwrap() as usize].uses_floor());
            toggle(&mut state, 1);
            ticks(&mut state, 30);
            assert!(state.player_duck_state().is_none());
        }
    }
    toggle(&mut state, 1);
    ticks(&mut state, 60);
    state.set_aspect_ratio(0.6);
    assert!(state.player_duck_state().is_none());
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
}
