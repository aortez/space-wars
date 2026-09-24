use super::*;

fn automatic(aspect: f32, seed: u64, elapsed: usize) -> ClockState {
    let mut state = ready(aspect, seed);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    state.configure(settings);
    state.preview_event(ClockEventKind::Duck);
    ticks(&mut state, elapsed);
    state
}

#[test]
fn taking_over_an_automatic_duck_preserves_the_actor_course_and_phase() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for elapsed in [0, 20, 36, 70, 190, 400, 1236] {
            let mut state = automatic(aspect, 42, elapsed);
            let mut before = state.duck_state().expect("automatic duck");
            let facing = before.navigation.as_ref().unwrap().facing_right;
            before.navigation = None;
            let duck = state.duck_scene().unwrap();
            let phase = (duck.phase, duck.phase_tick, duck.tick);
            let surfaces = duck.course.as_ref().unwrap().surfaces.clone();
            let frame = join::visible_polygons(&state);
            let time = state.simulation_tick();
            let event_id = state.event_id();
            let settings = state.settings();
            toggle(&mut state, 2);
            let player = state.player_duck_state().unwrap();
            assert_eq!(player.duck, before, "aspect={aspect}, elapsed={elapsed}");
            assert_eq!(player.facing_right, facing);
            let duck = state.duck_visit.as_ref().unwrap();
            assert_eq!((duck.phase, duck.phase_tick, duck.tick), phase);
            assert_eq!(duck.course.as_ref().unwrap().surfaces, surfaces);
            assert_eq!(join::visible_polygons(&state), frame);
            assert_eq!(state.simulation_tick(), time);
            assert_eq!(state.event_id(), event_id, "not a new automatic event");
            assert_eq!(state.event_kind(), None, "player now owns the visit");
            assert_eq!(state.settings(), settings);
            assert_eq!(player.player, 2);
        }
    }
}

#[test]
fn taken_over_visit_composes_with_events_and_cleans_up_across_dismissal_and_resize() {
    for kind in [
        ClockEventKind::Rain,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        let mut state = automatic(4.0 / 3.0, 42, 70);
        let world = state.duck_scene().unwrap().arena_world() as *const _;
        toggle(&mut state, 1);
        assert_eq!(
            state.duck_visit.as_ref().unwrap().arena_world() as *const _,
            world,
            "the world is moved, not reconstructed"
        );
        let player = state.player_duck_state().unwrap();
        let frame = ClockScenario::render_frame(&state);
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert_eq!(state.player_duck_state(), Some(player.clone()));
        assert_eq!(ClockScenario::render_frame(&state), frame);
        toggle(&mut state, 2);
        assert_eq!(
            state.player_duck_state(),
            Some(player.clone()),
            "other seat cannot dismiss"
        );
        state.preview_event(kind);
        ticks(&mut state, 50);
        assert_eq!(state.player_duck_session(), Some((player.session_id, 1)));
        let id = state.event_id();
        let phase = state.phase_tick();
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        assert!(state.duck_visit.is_none());
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!(state.event_id(), id);
        assert_eq!(state.phase_tick(), phase + 30);
        toggle(&mut state, 2);
        assert_eq!(state.event_id(), id);
        assert_ne!(state.player_duck_session().unwrap().0, player.session_id);
        ticks(&mut state, 60);
        assert!(
            state
                .player_duck_state()
                .unwrap()
                .duck
                .position_milli
                .is_some()
        );
        state.preview_event(ClockEventKind::ColorCycle);
        assert!(state.body_count() <= 9);
        state.set_aspect_ratio(0.6);
        assert!(state.duck_visit.is_none());
        assert_eq!(state.event_kind(), None);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    }
}

#[test]
fn takeover_retires_only_the_automatic_slot_and_dismissal_restores_its_schedule() {
    let mut state = automatic(5.0 / 3.0, 42, 70);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    state.configure(settings);
    toggle(&mut state, 1);
    let visit = state.player_duck_session();
    let mut saw_event = false;
    for _ in 0..DUCK_TICKS * 2 {
        tick(&mut state, &[]);
        assert_eq!(state.player_duck_session(), visit);
        assert_ne!(state.event_kind(), Some(ClockEventKind::Duck));
        saw_event |= state.event_kind().is_some();
    }
    assert!(
        saw_event,
        "takeover must not leave the scheduler stuck active"
    );
    assert_eq!(state.settings(), settings);
    settings.events.color_cycle = false;
    settings.events.marquee = false;
    settings.events.digit_slide = false;
    state.configure(settings);
    state.finish_event();
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!(state.body_count(), 0);
    for _ in 0..2100 {
        tick(&mut state, &[]);
        if state.duck_state().is_some() {
            break;
        }
    }
    assert_eq!(state.event_kind(), None);
    assert!(state.duck_state().is_some());
    assert!(state.player_duck_session().is_none());
    assert_eq!(state.settings(), settings);
}

#[test]
fn an_already_departed_bot_starts_a_fresh_visit_instead_of_reviving_a_missing_body() {
    let mut state = automatic(5.0 / 3.0, 42, 0);
    for _ in 0..DUCK_TICKS {
        if state.duck_visit.as_ref().map(|duck| duck.phase) == Some(EventPhase::Resetting) {
            break;
        }
        tick(&mut state, &[]);
    }
    assert_eq!(
        state.duck_visit.as_ref().map(|duck| duck.phase),
        Some(EventPhase::Resetting)
    );
    assert_eq!(state.body_count(), 0);
    assert!(state.duck_state().unwrap().outcome.is_some());
    toggle(&mut state, 1);
    let duck = state.player_duck_state().unwrap();
    assert_eq!(duck.phase, "opening");
    assert_eq!(duck.duck.outcome, None);
    ticks(&mut state, 90);
    assert!(state.player_duck_state().unwrap().duck.grounded);
}
