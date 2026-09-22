use super::*;

const VISUAL: [ClockEventKind; 3] = [
    ClockEventKind::ColorCycle,
    ClockEventKind::Marquee,
    ClockEventKind::DigitSlide,
];

fn off_player(seed: u64) -> ClockState {
    let mut state = ready(800.0 / 480.0, seed);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    state.configure(settings);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state
}

#[test]
fn visual_start_replace_finish_are_physically_transparent_during_movement_and_jumps() {
    for seed in 0..6 {
        for event in VISUAL {
            let mut composed = off_player(seed);
            let mut baseline = off_player(seed);
            // Short alternating movement stays on the entrance runway. Both
            // worlds receive identical setpoints, through real movement/Rapier.
            for t in 0..900 {
                if t == 8 || t == 47 {
                    composed.preview_event(event);
                }
                let axis = if t % 48 < 24 { 350 } else { -350 };
                let jump = t % 120 == 8;
                let a = input(&composed, axis, jump);
                let b = input(&baseline, axis, jump);
                tick(&mut composed, &[a]);
                tick(&mut baseline, &[b]);
                assert_eq!(
                    composed.player_duck_state(),
                    baseline.player_duck_state(),
                    "seed={seed} event={event:?} tick={t}"
                );
                assert!(composed.player_duck_session().is_some());
                assert_eq!(composed.floor_mode(), ClockFloorMode::EventOwned);
                assert_eq!(composed.body_count(), baseline.body_count());
                assert_eq!(composed.collider_count(), baseline.collider_count());
                assert!(composed.body_count() <= 9);
            }
            assert_eq!(
                composed.event_kind(),
                None,
                "visual event finishes without ending visit"
            );
            assert!(composed.player_duck_state().unwrap().duck.jumps > 1);
        }
    }
}

#[test]
fn next_event_cycles_compatible_events_and_blocked_previews_do_not_evict_anything() {
    let mut state = off_player(42);
    let mut settings = state.settings();
    settings.events.falling = true;
    settings.events.meltdown = true;
    settings.events.rain = true;
    state.configure(settings);
    let session = state.player_duck_state();
    for _ in 0..3 {
        for event in ClockEventKind::ALL
            .into_iter()
            .filter(|k| *k != ClockEventKind::Duck)
        {
            state.next_event();
            assert_eq!(state.event_kind(), Some(event));
            assert_eq!(state.player_duck_state(), session);
        }
    }
    state.finish_event();
    ticks(&mut state, COOLDOWN_TICKS as usize);
    assert!(state.can_trigger_event());
    let id = state.event_id();
    for event in ClockEventKind::ALL {
        if !state.event_blocked_by_player(event) {
            continue;
        }
        assert!(state.event_blocked_by_player(event));
        state.trigger_event(event);
        assert_eq!(state.event_id(), id);
        assert_eq!(state.event_kind(), None);
    }
    state.preview_event(ClockEventKind::Marquee);
    let id = state.event_id();
    let duck = state.player_duck_state();
    for event in ClockEventKind::ALL {
        if !state.event_blocked_by_player(event) {
            continue;
        }
        state.preview_event(event);
        assert_eq!(state.event_kind(), Some(ClockEventKind::Marquee));
        assert_eq!(state.event_id(), id);
        assert_eq!(state.player_duck_state(), duck);
        assert_eq!(
            state.event_notice.unwrap().0,
            "Dismiss your duck to preview this event"
        );
    }
}

#[test]
fn finishing_a_visit_does_not_finish_or_rewind_the_concurrent_event() {
    for event in VISUAL {
        let mut state = off_player(42);
        state.preview_event(event);
        ticks(&mut state, 2);
        let id = state.event_id();
        let phase = state.phase_tick();
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        assert!(state.player_duck_session().is_none());
        assert_eq!(state.event_id(), id);
        assert_eq!(state.event_kind(), Some(event));
        assert_eq!(state.phase_tick(), phase + 30);
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        ticks(&mut state, 800);
        assert_eq!(state.event_kind(), None);
    }
}

#[test]
fn filtered_schedule_has_no_backlog_and_reconfigures_without_changing_saved_enablement() {
    let mut state = off_player(42);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events.color_cycle = false;
    settings.events.marquee = false;
    settings.events.digit_slide = false;
    state.configure(settings);
    assert!(state.automatic_events_suspended());
    ticks(&mut state, 6000);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.next_event_tick(), None);
    state.next_event();
    assert_eq!(
        state.event_notice.unwrap().0,
        "Dismiss your duck for the enabled events"
    );
    assert_eq!(state.event_id(), 0);
    assert!(state.player_duck_session().is_some());
    settings.events.color_cycle = true;
    state.configure(settings);
    assert!(!state.automatic_events_suspended());
    let wait = state.next_event_tick().unwrap() - state.simulation_tick();
    assert!((360..=600).contains(&wait));
    ticks(&mut state, wait as usize);
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    assert_eq!(state.settings(), settings);
    settings.events.color_cycle = false;
    state.configure(settings);
    ticks(&mut state, 1000);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.next_event_tick(), None);
    let id = state.event_id();
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    let wait = state.next_event_tick().unwrap() - state.simulation_tick();
    assert!((360..=600).contains(&wait));
    assert_eq!(state.event_id(), id, "no deferred-event burst");
    ticks(&mut state, wait as usize);
    assert!(EVENT_CATALOG[state.event_kind().unwrap() as usize].uses_floor());
    assert_eq!(state.settings(), settings);
}

#[test]
fn pause_and_resize_cover_both_lifetimes() {
    let mut state = off_player(42);
    state.preview_event(ClockEventKind::Marquee);
    ticks(&mut state, 90);
    let duck = state.player_duck_state();
    let event = state.marquee_state();
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.player_duck_state(), duck);
    assert_eq!(state.marquee_state(), event);
    assert_eq!(ClockScenario::render_frame(&state), frame);
    state.set_aspect_ratio(0.6);
    assert!(state.player_duck_session().is_none());
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
}
