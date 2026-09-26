use super::*;

fn reading(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second)
        .unwrap()
        .with_date(ClockDate::new(year, month, day).unwrap())
}

#[test]
fn gregorian_validation_and_weekdays_include_century_leap_rules() {
    for (year, month, day) in [
        (0, 1, 1),
        (10000, 1, 1),
        (2026, 0, 1),
        (2026, 13, 1),
        (2026, 1, 0),
        (2026, 4, 31),
        (2026, 2, 29),
        (1900, 2, 29),
        (2100, 2, 29),
    ] {
        assert_eq!(ClockDate::new(year, month, day), None);
    }
    for (year, month, day, label) in [
        (1, 1, 1, "MONDAY · JANUARY 1"),
        (2000, 1, 1, "SATURDAY · JANUARY 1"),
        (2000, 2, 29, "TUESDAY · FEBRUARY 29"),
        (2024, 2, 29, "THURSDAY · FEBRUARY 29"),
        (2025, 1, 1, "WEDNESDAY · JANUARY 1"),
        (2026, 9, 25, "FRIDAY · SEPTEMBER 25"),
        (9999, 12, 31, "FRIDAY · DECEMBER 31"),
    ] {
        assert_eq!(ClockDate::new(year, month, day).unwrap().label(), label);
    }
}

#[test]
fn date_and_time_actions_are_atomic_validated_and_observable() {
    let value = reading(2024, 2, 29, 23, 59, 59);
    let action = ClockAction::set_reading(value);
    assert_eq!(
        ClockAction::decode(&action),
        Some(ClockAction::SetReading(value))
    );
    let Action::Scenario { kind, payload } = action else {
        panic!()
    };
    for length in [0, 1, 2, 3, 4, 6, 7, 8, 10] {
        let mut malformed = payload.clone();
        malformed.resize(length, 0);
        assert_eq!(
            ClockAction::decode(&Action::scenario(kind, malformed)),
            None
        );
    }
    for (offset, invalid) in [(0, 6), (2, 24), (3, 60), (4, 60), (7, 13), (8, 30)] {
        let mut malformed = payload.clone();
        malformed[offset] = invalid;
        assert_eq!(
            ClockAction::decode(&Action::scenario(kind, malformed)),
            None
        );
    }
    let mut state = ClockScenario::init(
        ClockConfig {
            show_date: true,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(value)],
        Duration::ZERO,
    );
    assert_eq!(state.date_label(), Some("THURSDAY · FEBRUARY 29"));
    let snapshot = ClockScenario::observe(&state);
    assert_eq!(
        snapshot.payload,
        vec![2, 0, 1, 23, 59, 59, 24, 1, 232, 7, 2, 29]
    );
    ClockScenario::step(&mut state, &[], Duration::from_secs(86400));
    assert_eq!(ClockScenario::observe(&state).payload, snapshot.payload); // no scenario wall clock
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(0, 0, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(state.date_label(), None); // time-only sources never leave a stale date
}

#[test]
fn midnight_month_year_and_leap_day_rollovers_use_latest_supplied_date() {
    for (from, to) in [
        ((2026, 1, 31), (2026, 2, 1)),
        ((2026, 2, 28), (2026, 3, 1)),
        ((2024, 2, 28), (2024, 2, 29)),
        ((2024, 2, 29), (2024, 3, 1)),
        ((2026, 12, 31), (2027, 1, 1)),
    ] {
        let mut state = ClockScenario::init(ClockConfig::default(), 42);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(
                from.0, from.1, from.2, 23, 59, 59,
            ))],
            Duration::ZERO,
        );
        let value = reading(to.0, to.1, to.2, 0, 0, 0);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(value)],
            Duration::from_millis(16),
        );
        assert_eq!(state.reading(), Some(value));
        assert_eq!(
            state.date_label(),
            Some(value.date().unwrap().label().as_str())
        );
        assert_eq!(state.event_kind(), Some(ClockEventKind::DigitSlide));
    }
    // A date correction is not a near-contiguous midnight, even if its HH:MM:SS is.
    for to in [(2026, 1, 1), (2026, 1, 3), (1, 1, 1), (9999, 12, 31)] {
        let mut state = ClockScenario::init(ClockConfig::default(), 42);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(2026, 1, 1, 23, 59, 59))],
            Duration::ZERO,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(to.0, to.1, to.2, 0, 0, 0))],
            Duration::from_millis(16),
        );
        assert_eq!(state.event_kind(), None);
    }
}

#[test]
fn date_toggle_round_trips_without_advancing_events_or_creating_physics() {
    let mut state = ClockScenario::init(ClockConfig::default(), 42);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading(2026, 9, 25, 12, 0, 0))],
        Duration::ZERO,
    );
    let deadline = state.next_event_tick();
    for show_date in [true, false, true] {
        let settings = ClockSettings {
            show_date,
            ..state.settings()
        };
        let action = ClockAction::configure(settings);
        assert_eq!(
            ClockAction::decode(&action),
            Some(ClockAction::Configure(settings))
        );
        ClockScenario::step(&mut state, &[action], Duration::ZERO);
        assert_eq!(state.settings().show_date, show_date);
        assert_eq!(state.simulation_tick(), 0);
        assert_eq!(state.next_event_tick(), deadline);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        let frame = ClockScenario::render_frame(&state);
        let labels = frame.layers.iter().flat_map(|l| &l.primitives).filter(|p| matches!(p, engine_common::RenderPrimitive::Text(t) if t.text == "FRIDAY · SEPTEMBER 25")).count();
        assert_eq!(labels, usize::from(show_date));
    }
}

#[test]
fn calendar_changes_preserve_every_event_and_yield_to_interaction_notices() {
    use engine_common::RenderPrimitive;
    let old = reading(2026, 9, 25, 23, 59, 59);
    let new = reading(2026, 9, 26, 0, 0, 0);
    for kind in ClockEventKind::ALL {
        let mut state = ClockScenario::init(
            ClockConfig {
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(old),
                ClockAction::preview_event(kind),
            ],
            Duration::ZERO,
        );
        for _ in 0..30 {
            ClockScenario::step(&mut state, &[], Duration::from_millis(16));
        }
        let before = (
            state.simulation_tick(),
            state.event_id(),
            state.phase_tick(),
            state.body_count(),
            state.collider_count(),
            state.segments().to_vec(),
        );
        let settings = ClockSettings {
            show_date: true,
            ..state.settings()
        };
        ClockScenario::step(
            &mut state,
            &[ClockAction::configure(settings)],
            Duration::ZERO,
        );
        assert_eq!(
            before,
            (
                state.simulation_tick(),
                state.event_id(),
                state.phase_tick(),
                state.body_count(),
                state.collider_count(),
                state.segments().to_vec()
            )
        );
        ClockScenario::step(&mut state, &[ClockAction::set_reading(new)], Duration::ZERO);
        assert_eq!(state.date_label(), Some("SATURDAY · SEPTEMBER 26"));
        assert_eq!(state.simulation_tick(), before.0);
        assert_eq!(state.event_id(), before.1);
        state.set_aspect_ratio(0.6);
        assert_eq!(state.date_label(), Some("SATURDAY · SEPTEMBER 26"));
        state.event_notice = Some(("EVENT NOTICE", 60));
        let frame = ClockScenario::render_frame(&state);
        assert!(
            !frame
                .layers
                .iter()
                .flat_map(|l| &l.primitives)
                .any(|p| matches!(p, RenderPrimitive::Text(t) if t.text.contains("SEPTEMBER")))
        );
    }
    let mut state = ClockScenario::init(
        ClockConfig {
            show_date: true,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(old),
            ClockAction::toggle_player_duck(1),
        ],
        Duration::ZERO,
    );
    assert!(state.player_duck_session().is_some());
    let frame = ClockScenario::render_frame(&state);
    assert!(
        !frame
            .layers
            .iter()
            .flat_map(|l| &l.primitives)
            .any(|p| matches!(p, RenderPrimitive::Text(t) if t.text.contains("SEPTEMBER")))
    );
}
