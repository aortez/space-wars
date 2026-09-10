use super::*;

fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second).unwrap()
}

fn tick(state: &mut ClockState, reading: Option<ClockReading>) {
    let actions: Vec<_> = reading.into_iter().map(ClockAction::set_reading).collect();
    ClockScenario::step(state, &actions, Duration::from_secs_f64(1.0 / 60.0));
}

fn ready(reading: ClockReading, format: ClockTimeFormat) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            time_format: format,
            ..ClockConfig::default()
        },
        23,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    state
}

#[test]
fn minute_rollovers_slide_only_changed_slots_with_no_physics() {
    for (from, to, format, changed) in [
        (
            reading(12, 34, 59),
            reading(12, 35, 0),
            ClockTimeFormat::TwentyFourHour,
            [false, false, false, true],
        ),
        (
            reading(12, 59, 58),
            reading(13, 0, 1),
            ClockTimeFormat::TwentyFourHour,
            [false, true, true, true],
        ),
        (
            reading(23, 59, 59),
            reading(0, 0, 0),
            ClockTimeFormat::TwentyFourHour,
            [true; 4],
        ),
        (
            reading(12, 59, 59),
            reading(13, 0, 0),
            ClockTimeFormat::TwelveHour,
            [true; 4],
        ),
        (
            reading(11, 59, 59),
            reading(12, 0, 0),
            ClockTimeFormat::TwelveHour,
            [false, true, true, true],
        ),
    ] {
        let mut state = ready(from, format);
        let before = state.display();
        let deadline = state.next_event_tick();
        tick(&mut state, Some(to));
        let slide = state.digit_slide_state().unwrap();
        assert_eq!(slide.from_digits, before.digits);
        assert_eq!(slide.to_digits, state.display().digits);
        assert_eq!(slide.changed_slots, changed);
        assert!(!slide.preview);
        assert_eq!(state.event_phase(), Some(EventPhase::Sliding));
        assert_eq!(state.reading(), Some(to));
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
        for _ in 1..DIGIT_SLIDE_TICKS {
            tick(&mut state, None);
        }
        assert!(state.digit_slide_state().is_none());
        assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
        assert_eq!(
            ClockScenario::render_frame(&state),
            ClockScenario::render_frame(&ready(to, format))
        );
        for _ in 0..COOLDOWN_TICKS {
            tick(&mut state, None);
        }
        assert_eq!(state.lifecycle(), EventLifecycle::Idle);
        assert_eq!(state.next_event_tick(), deadline);
    }
}

#[test]
fn initial_sync_seconds_duplicates_corrections_and_paused_sync_do_not_trigger() {
    let from = reading(12, 34, 58);
    for to in [
        from,
        reading(12, 34, 59),
        reading(12, 36, 0),
        reading(11, 34, 59),
        reading(12, 35, 10),
    ] {
        let mut state = ready(from, ClockTimeFormat::TwentyFourHour);
        tick(&mut state, Some(to));
        assert_eq!(state.event_id(), 0);
        assert_eq!(state.reading(), Some(to));
    }
    let mut state = ClockScenario::init(ClockConfig::default(), 2);
    tick(&mut state, Some(from));
    assert_eq!(state.event_id(), 0);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading(12, 35, 0))],
        Duration::ZERO,
    );
    assert_eq!(state.event_id(), 0);
}

#[test]
fn off_disabled_and_busy_events_skip_transitions_without_a_backlog() {
    let from = reading(12, 34, 59);
    let to = reading(12, 35, 0);
    for profile in [ClockEventProfile::Off, ClockEventProfile::Calm] {
        let mut state = ready(from, ClockTimeFormat::TwentyFourHour);
        let mut settings = state.settings();
        settings.event_profile = profile;
        settings.events.digit_slide = profile == ClockEventProfile::Off;
        ClockScenario::step(
            &mut state,
            &[ClockAction::configure(settings)],
            Duration::ZERO,
        );
        tick(&mut state, Some(to));
        assert_eq!(state.event_id(), 0);
    }
    for kind in ClockEventKind::ALL
        .into_iter()
        .filter(|kind| *kind != ClockEventKind::DigitSlide)
    {
        let mut state = ready(from, ClockTimeFormat::TwentyFourHour);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        tick(&mut state, Some(to));
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!(state.event_id(), 1);
        for _ in 1..EVENT_CATALOG[kind as usize].duration_ticks + COOLDOWN_TICKS {
            tick(&mut state, None);
        }
        assert_eq!(state.event_kind(), None);
        assert_eq!(state.event_id(), 1);
        assert_eq!(state.display(), digits::snapshot(to, state.time_format()));
    }
}

#[test]
fn pause_duplicates_new_targets_and_format_changes_preserve_or_cancel_as_intended() {
    let mut state = ready(reading(12, 34, 59), ClockTimeFormat::TwentyFourHour);
    tick(&mut state, Some(reading(12, 35, 0)));
    let slide = state.digit_slide_state();
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert_eq!(state.digit_slide_state(), slide);
        assert_eq!(ClockScenario::render_frame(&state), frame);
    }
    tick(&mut state, Some(reading(12, 35, 0)));
    tick(&mut state, Some(reading(12, 35, 1)));
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.phase_tick(), 3);
    assert!(!state.display().colon_lit);
    tick(&mut state, Some(reading(15, 45, 0)));
    assert!(state.digit_slide_state().is_none());
    assert_eq!(state.event_id(), 1);
    assert_eq!(
        ClockScenario::render_frame(&state),
        ClockScenario::render_frame(&ready(reading(15, 45, 0), state.time_format()))
    );

    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
        Duration::ZERO,
    );
    let mut settings = state.settings();
    settings.events.digit_slide = false;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert!(state.digit_slide_state().is_some()); // enablement is for future events
    settings.time_format = ClockTimeFormat::TwelveHour;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert!(state.digit_slide_state().is_none());
    assert_eq!(state.display().digits, [None, Some(3), Some(4), Some(5)]);
}

#[test]
fn previews_replacements_resize_and_repeats_are_bounded_and_deterministic() {
    for aspect in [0.25, 0.75, 800.0 / 480.0, 4.0] {
        let mut state = ready(reading(8, 8, 0), ClockTimeFormat::TwentyFourHour);
        state.set_aspect_ratio(aspect);
        for _ in 0..8 {
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
                Duration::ZERO,
            );
            let slide = state.digit_slide_state().unwrap();
            assert!(slide.preview);
            assert_eq!(slide.from_digits, slide.to_digits);
            assert_eq!(slide.changed_slots, [true; 4]);
            for _ in 0..DIGIT_SLIDE_TICKS {
                let frame = ClockScenario::render_frame(&state);
                assert_eq!(frame, ClockScenario::render_frame(&state));
                assert!(
                    frame
                        .layers
                        .iter()
                        .map(|l| l.primitives.len())
                        .sum::<usize>()
                        < 300
                );
                let bounds = frame.camera.world_bounds(aspect);
                for primitive in frame.layers.iter().flat_map(|l| &l.primitives) {
                    if let engine_common::RenderPrimitive::Polygon(polygon) = primitive {
                        assert!(polygon.points.iter().all(|p| p.x.is_finite()
                            && p.y.is_finite()
                            && p.x >= bounds.min.x
                            && p.x <= bounds.max.x
                            && p.y >= bounds.min.y
                            && p.y <= bounds.max.y));
                    }
                }
                tick(&mut state, None);
            }
            assert!(state.digit_slide_state().is_none());
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        }
        for kind in ClockEventKind::ALL {
            ClockScenario::step(
                &mut state,
                &[
                    ClockAction::preview_event(kind),
                    ClockAction::preview_event(ClockEventKind::DigitSlide),
                ],
                Duration::ZERO,
            );
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert!(
                state.meltdown_state().is_none()
                    && state.duck_state().is_none()
                    && state.marquee_state().is_none()
            );
        }
        state.set_aspect_ratio(if aspect == 4.0 { 1.0 } else { 4.0 });
        assert!(state.digit_slide_state().is_none());
    }
}
