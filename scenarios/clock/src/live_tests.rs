use super::*;

fn ready() -> ClockState {
    let mut state = ClockScenario::init(ClockConfig::default(), 4242);
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

#[test]
fn settings_actions_round_trip_all_values_and_reject_malformed_payloads() {
    for time_format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
        for event_profile in [
            ClockEventProfile::Off,
            ClockEventProfile::Calm,
            ClockEventProfile::Demo,
        ] {
            for bits in 0..8 {
                let settings = ClockSettings {
                    time_format,
                    event_profile,
                    events: ClockEvents {
                        falling: bits & 1 != 0,
                        color_cycle: bits & 2 != 0,
                        meltdown: bits & 4 != 0,
                    },
                };
                assert_eq!(
                    ClockAction::decode(&ClockAction::configure(settings)),
                    Some(ClockAction::Configure(settings))
                );
            }
        }
    }
    for payload in [
        vec![],
        vec![1, 0],
        vec![2, 0, 24, 1, 3],
        vec![1, 0, 13, 1, 3],
        vec![1, 0, 24, 3, 3],
        vec![1, 0, 24, 1, 8],
        vec![1, 0, 24, 1, 3, 0],
    ] {
        assert_eq!(
            ClockAction::decode(&Action::scenario(CLOCK_ACTION_CONFIGURE, payload)),
            None
        );
    }
    for kind in ClockEventKind::ALL {
        assert_eq!(
            ClockAction::decode(&ClockAction::preview_event(kind)),
            Some(ClockAction::PreviewEvent(kind))
        );
    }
    for payload in [vec![1, 0], vec![1, 0, 3], vec![2, 0, 0], vec![1, 0, 0, 0]] {
        assert_eq!(
            ClockAction::decode(&Action::scenario(CLOCK_ACTION_PREVIEW_EVENT, payload)),
            None
        );
    }
}

#[test]
fn live_settings_preserve_falling_physics_and_reform_to_the_new_format() {
    let mut state = ready();
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Falling)],
        Duration::ZERO,
    );
    ticks(&mut state, 30);
    let segments = state.segments().to_vec();
    let counts = (state.body_count(), state.collider_count());
    let settings = ClockSettings {
        time_format: ClockTimeFormat::TwelveHour,
        event_profile: ClockEventProfile::Off,
        events: ClockEvents {
            falling: false,
            color_cycle: false,
            meltdown: false,
        },
    };
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert_eq!(state.settings(), settings);
    assert_eq!(state.segments(), segments);
    assert_eq!((state.body_count(), state.collider_count()), counts);
    assert_eq!(state.phase_tick(), 30);
    assert_eq!(state.simulation_tick(), 30);
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.display().digits, [Some(1), Some(1), Some(5), Some(8)]);
    ticks(
        &mut state,
        FALLING_TICKS + REFORMING_TICKS + COOLDOWN_TICKS - 30,
    );
    assert_eq!(state.lifecycle(), EventLifecycle::Idle);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    assert_eq!(state.next_event_tick(), None);
    assert!(
        state
            .segments()
            .iter()
            .all(|segment| segment.representation == SegmentRepresentation::Anchored)
    );
    ticks(&mut state, 6000);
    assert_eq!(state.event_id(), 1);
}

#[test]
fn live_cadence_changes_reschedule_only_when_needed_and_preserve_cooldowns() {
    let mut state = ready();
    let deadline = state.next_event_tick();
    let mut settings = state.settings();
    settings.time_format = ClockTimeFormat::TwelveHour;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert_eq!(state.next_event_tick(), deadline);
    settings.event_profile = ClockEventProfile::Off;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert_eq!(state.next_event_tick(), None);
    ticks(&mut state, 600);
    settings.event_profile = ClockEventProfile::Demo;
    settings.events.falling = false;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    let deadline = state.next_event_tick().unwrap();
    assert!((960..=1200).contains(&deadline));
    ticks(&mut state, deadline - 600);
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    ticks(&mut state, COLOR_CYCLE_TICKS);
    let ready_at = state.event_ready_at_tick(ClockEventKind::ColorCycle);
    settings.event_profile = ClockEventProfile::Calm;
    ClockScenario::step(
        &mut state,
        &[ClockAction::configure(settings)],
        Duration::ZERO,
    );
    assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
    assert_eq!(
        state.event_ready_at_tick(ClockEventKind::ColorCycle),
        ready_at
    );
    ticks(&mut state, COOLDOWN_TICKS);
    assert!(state.next_event_tick().unwrap() >= state.simulation_tick() + 45 * 60);
}

#[test]
fn repeated_previews_replace_every_phase_without_leaking_resources_or_resetting_ids() {
    for interrupted_tick in [30, FALLING_TICKS + 30, FALLING_TICKS + REFORMING_TICKS + 30] {
        let mut state = ready();
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Falling)],
            Duration::ZERO,
        );
        ticks(&mut state, interrupted_tick);
        for id in 2..=12 {
            let kind = if id % 2 == 0 {
                ClockEventKind::ColorCycle
            } else {
                ClockEventKind::Falling
            };
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(kind)],
                Duration::ZERO,
            );
            assert_eq!(state.event_id(), id);
            assert_eq!(state.event_kind(), Some(kind));
            assert_eq!(state.simulation_tick(), interrupted_tick);
            assert_eq!(state.phase_tick(), 0);
            if kind == ClockEventKind::ColorCycle {
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                assert!(
                    state
                        .segments()
                        .iter()
                        .all(|segment| segment.representation == SegmentRepresentation::Anchored)
                );
            } else {
                assert!(state.body_count() <= 32 && state.collider_count() <= 100);
            }
        }
    }
}

#[test]
fn preview_before_time_sync_is_a_noop() {
    let mut state = ClockScenario::init(ClockConfig::default(), 0);
    ClockScenario::step(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Falling)],
        Duration::ZERO,
    );
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.body_count(), 0);
}
