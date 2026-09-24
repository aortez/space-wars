use super::*;

fn ready(profile: ClockEventProfile) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn next(state: &mut ClockState) {
    ClockScenario::step(state, &[ClockAction::next_event()], Duration::ZERO);
}

#[test]
fn next_event_action_has_strict_versioned_decoding() {
    assert_eq!(
        ClockAction::decode(&ClockAction::next_event()),
        Some(ClockAction::NextEvent)
    );
    assert_eq!(
        ClockAction::decode(&Action::scenario(CLOCK_ACTION_NEXT_EVENT, vec![])),
        None
    );
    assert_eq!(
        ClockAction::decode(&Action::scenario(CLOCK_ACTION_NEXT_EVENT, vec![0, 0])),
        None
    );
    let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
    payload.push(0);
    assert_eq!(
        ClockAction::decode(&Action::scenario(CLOCK_ACTION_NEXT_EVENT, payload)),
        None
    );
}

#[test]
fn next_cycles_enabled_events_in_all_profiles_without_changing_preferences() {
    for profile in [
        ClockEventProfile::Off,
        ClockEventProfile::Calm,
        ClockEventProfile::Demo,
    ] {
        let mut state = ready(profile);
        let settings = state.settings();
        let enabled: Vec<_> = ClockEventKind::ALL
            .into_iter()
            .filter(|kind| settings.events.enabled(*kind))
            .collect();
        let repeated = enabled.iter().filter(|kind| **kind != ClockEventKind::Duck);
        for kind in enabled.iter().chain(repeated) {
            next(&mut state);
            assert_eq!(state.last_started_event, Some(*kind));
            if *kind == ClockEventKind::Duck {
                assert!(state.duck_state().is_some());
                assert_eq!(state.event_kind(), None);
            } else {
                assert_eq!(state.event_kind(), Some(*kind));
            }
            assert_eq!(state.settings(), settings);
        }
    }
}

#[test]
fn no_enabled_events_is_a_bounded_noop_and_feedback_expires_only_while_running() {
    let mut state = ready(ClockEventProfile::Off);
    state.config.events = ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: false,
    };
    next(&mut state);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.event_notice.unwrap().0, "No events enabled");
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..180 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(ClockScenario::render_frame(&state), frame);
    for _ in 0..120 {
        ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    }
    assert!(state.event_notice.is_none());
}

#[test]
fn replacing_each_live_event_recovers_its_resources_and_keeps_the_latest_time() {
    for kind in ClockEventKind::ALL {
        let mut state = ready(ClockEventProfile::Off);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        for _ in 0..90 {
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let reading = ClockReading::new(13, 42, 15).unwrap();
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        // Select one appearance-only successor to expose any leaked world state.
        let mut settings = state.settings();
        settings.events = ClockEvents {
            falling: false,
            color_cycle: true,
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
        next(&mut state);
        assert_eq!(
            state.event_kind(),
            Some(ClockEventKind::ColorCycle),
            "replacing {kind:?}"
        );
        if kind == ClockEventKind::Duck {
            assert!(
                state.duck_state().is_some(),
                "next event preserves the visit"
            );
            assert_eq!(
                state.body_count(),
                state.duck_scene().unwrap().physics_counts().0
            );
        } else {
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        }
        assert!(state.meltdown_state().is_none());
        assert!(state.rain_state().is_none());
        assert!(
            state
                .segments()
                .iter()
                .all(|segment| segment.representation == SegmentRepresentation::Anchored)
        );
        assert_eq!(state.reading(), Some(reading));
        assert_eq!(state.event_notice.unwrap().0, "Color Cycle");
        assert_eq!(
            state.floor_mode(),
            if kind == ClockEventKind::Duck {
                engine_common::ClockFloorMode::EventOwned
            } else {
                engine_common::ClockFloorMode::Closed
            }
        );
    }
}

#[test]
fn next_after_automatic_or_preview_events_follows_the_last_started_kind() {
    for kind in ClockEventKind::ALL {
        let mut state = ready(ClockEventProfile::Off);
        state.config.events = ClockEvents {
            falling: true,
            color_cycle: true,
            meltdown: true,
            duck: true,
            marquee: true,
            digit_slide: true,
            rain: true,
        };
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(kind)],
            Duration::ZERO,
        );
        next(&mut state);
        assert_eq!(
            state.last_started_event,
            Some(ClockEventKind::ALL[(kind as usize + 1) % ClockEventKind::ALL.len()])
        );
    }
}
