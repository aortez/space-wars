//! The `--:--` face for a time nobody can vouch for (#257).
use super::*;
use crate::digit_surfaces::Glyph;
use engine_common::{ClockEventProfile, Scenario};
use std::time::Duration;

const TICK: Duration = Duration::from_nanos(16_666_667);

fn events(enabled: impl Fn(ClockEventKind) -> bool) -> ClockEvents {
    ClockEvents {
        falling: enabled(ClockEventKind::Falling),
        color_cycle: enabled(ClockEventKind::ColorCycle),
        meltdown: enabled(ClockEventKind::Meltdown),
        duck: enabled(ClockEventKind::Duck),
        marquee: enabled(ClockEventKind::Marquee),
        digit_slide: enabled(ClockEventKind::DigitSlide),
        rain: enabled(ClockEventKind::Rain),
        crow: enabled(ClockEventKind::Crow),
        explosion: enabled(ClockEventKind::Explosion),
        aquarium: enabled(ClockEventKind::Aquarium),
        cell_swap: enabled(ClockEventKind::CellSwap),
        grass: enabled(ClockEventKind::Grass),
        slot_machine: enabled(ClockEventKind::SlotMachine),
        spider: enabled(ClockEventKind::Spider),
        coins: enabled(ClockEventKind::Coins),
        tanks: enabled(ClockEventKind::Tanks),
    }
}

/// A clock whose frames change only when something happens in it.
fn clock(config: ClockConfig, seed: u64) -> ClockState {
    ClockScenario::init(
        ClockConfig {
            lamps: engine_common::ClockLampMode::Steady,
            ..config
        },
        seed,
    )
}

fn quiet() -> ClockConfig {
    ClockConfig {
        event_profile: ClockEventProfile::Off,
        events: events(|_| false),
        ..ClockConfig::default()
    }
}

fn act(state: &mut ClockState, action: Action) {
    ClockScenario::step(state, &[action], Duration::ZERO);
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], TICK);
    }
}

/// Christmas Eve, so the date line, AM/PM and the Christmas theme all have
/// something to show.
fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second)
        .unwrap()
        .with_date(ClockDate::new(2026, 12, 24).unwrap())
}

fn dash(font: ClockFont) -> fonts::CellMask {
    fonts::glyph(font, Some(DASH))
}

/// The face is `--:--`, and nothing of a time is left in the clock: no
/// reading, no date, no AM/PM, and no lit segment resting outside the dashes.
fn assert_no_time(state: &ClockState, context: &str) {
    let display = state.display();
    assert!(display.is_unknown(), "{context}: {display:?}");
    assert!(display.colon_lit, "{context}: the colon stays lit");
    assert_eq!(display.meridiem, None, "{context}");
    assert_eq!(state.reading(), None, "{context}");
    assert_eq!(state.date_label(), None, "{context}");
    let dash = dash(display.font);
    for segment in state.segments() {
        if segment.lit && segment.representation == SegmentRepresentation::Anchored {
            assert!(
                segment.cells().all(|cell| dash.contains(cell)),
                "{context}: {:?} rests lit outside the dash",
                segment.id
            );
        }
    }
}

/// What every slot shows at rest.
fn resting(state: &ClockState) -> [fonts::CellMask; DIGIT_SLOT_COUNT] {
    std::array::from_fn(|slot| Glyph::anchored(state.segments(), slot).mask)
}

#[test]
fn the_unknown_time_action_round_trips_and_carries_nothing() {
    let action = ClockAction::set_time_unknown();
    assert_eq!(
        ClockAction::decode(&action),
        Some(ClockAction::SetTimeUnknown)
    );
    let Action::Scenario { kind, payload } = action else {
        panic!("a scenario action")
    };
    assert_eq!(kind, CLOCK_ACTION_SET_TIME_UNKNOWN);
    for payload in [
        vec![],
        payload[..1].to_vec(),
        [payload.clone(), vec![0]].concat(),
    ] {
        assert_eq!(ClockAction::decode(&Action::scenario(kind, payload)), None);
    }
    let mut older = payload;
    older[0] -= 1;
    assert_eq!(ClockAction::decode(&Action::scenario(kind, older)), None);
}

#[test]
fn every_face_shows_its_own_dashes_and_no_date_or_am_pm_in_either_format() {
    for font in ClockFont::ALL {
        for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
            let config = ClockConfig {
                fonts: ClockFontSettings {
                    selected: font,
                    ..ClockFontSettings::default()
                },
                time_format: format,
                show_date: true,
                theme: ClockThemeChoice::Auto,
                ..quiet()
            };
            let context = format!("{font:?} {format:?}");
            // A kiosk that boots without a trustworthy time never shows one.
            let mut booted = clock(config, 1);
            act(&mut booted, ClockAction::set_time_unknown());
            assert!(booted.time_unknown());
            assert_eq!(booted.active_font(), font, "{context}");
            assert_no_time(&booted, &context);
            assert_eq!(resting(&booted), [dash(font); 4], "{context}");
            assert_eq!(booted.theme(), ClockTheme::Normal, "{context}");

            // A clock that loses its time loses all of it at once.
            let mut lost = clock(config, 1);
            act(&mut lost, ClockAction::set_reading(reading(21, 34, 5)));
            assert!(lost.date_label().is_some(), "{context}");
            assert_eq!(lost.theme(), ClockTheme::Christmas, "{context}");
            act(&mut lost, ClockAction::set_time_unknown());
            assert_no_time(&lost, &context);
            assert_eq!(resting(&lost), [dash(font); 4], "{context}");
            assert_eq!(lost.theme(), ClockTheme::Normal, "{context}");
            assert_eq!(
                ClockScenario::render_frame(&lost),
                ClockScenario::render_frame(&booted),
                "{context}: the same `--:--` however the time went"
            );
        }
    }
}

#[test]
fn the_dashes_hold_still_with_a_steady_colon() {
    let mut state = clock(quiet(), 2);
    act(&mut state, ClockAction::set_time_unknown());
    let still = ClockScenario::render_frame(&state);
    for _ in 0..3 {
        ticks(&mut state, 61);
        assert_no_time(&state, "a second later");
        assert_eq!(ClockScenario::render_frame(&state), still);
    }
}

#[test]
fn a_new_font_reaches_the_dashes_without_a_reading() {
    let mut state = clock(quiet(), 3);
    act(&mut state, ClockAction::set_time_unknown());
    assert_eq!(state.active_font(), ClockFont::Classic);
    let mut settings = state.settings();
    settings.fonts.selected = ClockFont::Serif;
    act(&mut state, ClockAction::configure(settings));
    assert_eq!(state.active_font(), ClockFont::Serif);
    assert_no_time(&state, "after the font changed");
    assert_eq!(resting(&state), [dash(ClockFont::Serif); 4]);
}

#[test]
fn the_time_returns_at_its_latest_reading_without_a_slide() {
    for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
        let config = ClockConfig {
            event_profile: ClockEventProfile::Demo,
            events: events(|kind| kind == ClockEventKind::DigitSlide),
            time_format: format,
            show_date: true,
            ..ClockConfig::default()
        };
        let mut state = clock(config, 4);
        act(&mut state, ClockAction::set_reading(reading(9, 59, 58)));
        act(&mut state, ClockAction::set_time_unknown());
        ticks(&mut state, 5);
        // One second on from the last reading, and a minute changed, but
        // nothing slides out of the dashes: the face snaps to the truth.
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(10, 0, 0))],
            TICK,
        );
        assert!(!state.time_unknown());
        assert_eq!(state.event_kind(), None);
        let mut fresh = clock(config, 4);
        act(&mut fresh, ClockAction::set_reading(reading(10, 0, 0)));
        assert_eq!(state.display(), fresh.display());
        assert_eq!(state.segments(), fresh.segments());
        assert_eq!(state.date_label(), fresh.date_label());
        assert!(state.date_label().is_some());
    }
}

#[test]
fn saying_the_time_is_unknown_again_changes_nothing() {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        events: events(|_| false),
        rain_amount: engine_common::ClockRainAmount::Heavy,
        ..ClockConfig::default()
    };
    let mut once = clock(config, 5);
    let mut repeated = clock(config, 5);
    for state in [&mut once, &mut repeated] {
        act(state, ClockAction::set_reading(reading(21, 34, 5)));
        act(state, ClockAction::preview_event(ClockEventKind::Rain));
        ticks(state, 40);
        act(state, ClockAction::set_time_unknown());
    }
    let event = once.event_id();
    for tick in 0..240 {
        ClockScenario::step(&mut once, &[], TICK);
        ClockScenario::step(&mut repeated, &[ClockAction::set_time_unknown()], TICK);
        assert_eq!(
            ClockScenario::render_frame(&once),
            ClockScenario::render_frame(&repeated),
            "tick {tick}"
        );
    }
    assert_eq!(repeated.event_id(), event);
    assert_eq!(repeated.event_kind(), Some(ClockEventKind::Rain));
}

/// The events that move, reshape or replace the digits, which wait while the
/// time is unknown.
fn on_the_digits(kind: ClockEventKind) -> bool {
    EVENT_CATALOG[kind as usize].effect.waits_for_the_time()
}

/// Every event enabled, under `profile`.
fn all_enabled(profile: ClockEventProfile) -> ClockConfig {
    ClockConfig {
        event_profile: profile,
        events: events(|_| true),
        ..ClockConfig::default()
    }
}

/// Whether `kind` is under way, as a timed event or as a visit.
fn running(state: &ClockState, kind: ClockEventKind) -> bool {
    match kind {
        ClockEventKind::Duck => state.has_duck_visit(),
        ClockEventKind::Crow => state.crow_state().is_some(),
        ClockEventKind::Spider => state.spider_state().is_some(),
        _ => state.event_kind() == Some(kind),
    }
}

fn notice(state: &ClockState) -> Option<&'static str> {
    state.event_notice.map(|(notice, _)| notice)
}

#[test]
fn the_events_on_the_digits_are_the_ones_that_move_or_replace_them() {
    let waiting: Vec<_> = ClockEventKind::ALL
        .into_iter()
        .filter(|kind| on_the_digits(*kind))
        .collect();
    assert_eq!(
        waiting,
        [
            ClockEventKind::Falling,
            ClockEventKind::Meltdown,
            ClockEventKind::Marquee,
            ClockEventKind::DigitSlide,
            ClockEventKind::Explosion,
            ClockEventKind::CellSwap,
            ClockEventKind::SlotMachine,
            ClockEventKind::Coins,
        ]
    );
}

#[test]
fn digit_events_wait_while_the_time_is_unknown_and_a_fresh_wait_follows_it() {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Demo,
        events: events(on_the_digits),
        ..ClockConfig::default()
    };
    let mut state = clock(config, 6);
    act(&mut state, ClockAction::set_time_unknown());
    // A minute of Demo, whose events come every 6 to 10 seconds, passes
    // without one, and without a word on the face. Nothing waits in line.
    for _ in 0..60 * 60 {
        ticks(&mut state, 1);
        assert_eq!(state.event_notice, None);
    }
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.lifecycle(), EventLifecycle::Idle);
    assert_eq!(state.next_event_tick(), None);

    // Asking for one says why nothing happens, and so does Next.
    let mut requests: Vec<_> = ClockEventKind::ALL
        .into_iter()
        .filter(|kind| on_the_digits(*kind))
        .flat_map(|kind| {
            [
                ClockAction::preview_event(kind),
                ClockAction::trigger_event(kind),
            ]
        })
        .collect();
    requests.push(ClockAction::next_event());
    for request in requests {
        state.event_notice = None;
        act(&mut state, request.clone());
        assert_eq!(state.event_id(), 0, "{request:?}");
        assert_no_time(&state, &format!("{request:?}"));
        assert_eq!(
            notice(&state),
            Some(ClockState::TIME_UNKNOWN_NOTICE),
            "{request:?}"
        );
    }

    // The time comes back. No event that waited fires; a fresh Demo wait of
    // 6 to 10 seconds starts instead.
    let back = state.simulation_tick();
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    let next = state
        .next_event_tick()
        .expect("Demo schedules a next event");
    assert!(
        (back + 6 * 60..=back + 10 * 60).contains(&next),
        "{next} after {back}"
    );
    ticks(&mut state, next - back - 1);
    assert_eq!(state.event_id(), 0);
    ticks(&mut state, 1);
    assert_eq!(state.event_id(), 1);
}

/// The time is lost a tick before a Demo deadline that falls between the
/// schedule's once-a-second checks. The digit events leave the schedule at
/// once, so none starts and no notice shows.
#[test]
fn losing_the_time_just_before_a_deadline_starts_nothing_on_the_digits() {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Off,
        events: events(on_the_digits),
        ..ClockConfig::default()
    };
    let mut state = clock(config, 6);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    ticks(&mut state, 30);
    let demo = ClockSettings {
        event_profile: ClockEventProfile::Demo,
        ..state.settings()
    };
    act(&mut state, ClockAction::configure(demo));
    let due = state.next_event_tick().expect("Demo schedules an event");
    assert_ne!(due % u64::from(FIXED_HZ), 0);
    let wait = due - state.simulation_tick() - 1;
    ticks(&mut state, wait);
    act(&mut state, ClockAction::set_time_unknown());
    for _ in 0..2 * FIXED_HZ {
        ticks(&mut state, 1);
        assert_eq!(state.event_notice, None);
        assert_eq!(state.event_id(), 0);
    }
}

fn visitor(kind: ClockEventKind) -> bool {
    matches!(
        kind,
        ClockEventKind::Duck | ClockEventKind::Crow | ClockEventKind::Spider
    )
}

/// Loses the time as every timed event starts and at three points into it,
/// and follows the event to its end: the face never rests on the old time
/// again, the event cleans up as it would for any new reading, and the clock
/// ends up exactly like one that never knew the time.
#[test]
fn losing_the_time_mid_event_ends_every_event_on_the_dashes() {
    for definition in EVENT_CATALOG.into_iter().filter(|d| !visitor(d.kind)) {
        let kind = definition.kind;
        for fraction in [0.0, 0.05, 0.5, 0.9] {
            let config = ClockConfig {
                time_format: ClockTimeFormat::TwelveHour,
                show_date: true,
                ..quiet()
            };
            let mut state = clock(config, 7);
            act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
            act(&mut state, ClockAction::trigger_event(kind));
            assert_eq!(state.event_kind(), Some(kind));
            ticks(
                &mut state,
                (definition.duration_ticks as f32 * fraction) as u64,
            );
            if state.event_kind() != Some(kind) {
                continue;
            }
            let context = format!("{kind:?} lost at {fraction}");
            let phase = state.event_phase();
            act(&mut state, ClockAction::set_time_unknown());
            let mut remaining = definition.duration_ticks + 60 * 60;
            while state.event_kind().is_some() {
                assert_no_time(&state, &format!("{context} ({phase:?})"));
                assert!(remaining > 0, "{context}: the event never ended");
                remaining -= 1;
                ticks(&mut state, 1);
            }
            assert_eq!(state.lifecycle(), EventLifecycle::Cooldown, "{context}");
            assert_eq!(
                (state.body_count(), state.collider_count()),
                (0, 0),
                "{context}"
            );
            let mut never = clock(config, 7);
            act(&mut never, ClockAction::set_time_unknown());
            assert_no_time(&state, &context);
            assert_eq!(state.segments(), never.segments(), "{context}");
            assert_eq!(
                ClockScenario::render_frame(&state),
                ClockScenario::render_frame(&never),
                "{context}"
            );
            let cells = 4 * dash(ClockFont::Classic).cells().count();
            assert_eq!(state.digit_surfaces().len(), cells, "{context}");
            assert_eq!(state.surface_accounting().live, cells, "{context}");
        }
    }
}

#[test]
fn a_reel_that_already_stopped_shows_the_dash_as_soon_as_the_time_is_lost() {
    let mut state = clock(quiet(), 8);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    act(
        &mut state,
        ClockAction::trigger_event(ClockEventKind::SlotMachine),
    );
    let reels = |state: &ClockState| {
        state
            .slot_machine_state()
            .expect("the slot machine is running")
    };
    while !reels(&state).locked_slots[0] {
        ticks(&mut state, 1);
    }
    assert!(!reels(&state).locked_slots[3], "the last reel still turns");
    assert!(reels(&state).target_digits[0].is_some_and(|digit| digit < 10));
    act(&mut state, ClockAction::set_time_unknown());
    while state.event_kind().is_some() {
        assert_eq!(reels(&state).target_digits, [Some(DASH); 4]);
        ticks(&mut state, 1);
    }
    assert_no_time(&state, "after the reels");
}

/// A visit is the room's own business: one already under way when the time is
/// lost carries on and leaves as it would, over the dashes.
#[test]
fn a_visit_carries_on_over_the_dashes_when_the_time_is_lost() {
    let visiting = |state: &ClockState| {
        state.has_duck_visit() || state.crow_state().is_some() || state.spider_state().is_some()
    };
    for definition in EVENT_CATALOG.into_iter().filter(|d| visitor(d.kind)) {
        let kind = definition.kind;
        let mut state = clock(quiet(), 9);
        act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
        act(&mut state, ClockAction::preview_event(kind));
        ticks(&mut state, 120);
        assert!(visiting(&state), "{kind:?} is visiting");
        act(&mut state, ClockAction::set_time_unknown());
        assert!(visiting(&state), "{kind:?} stays");
        let mut remaining = definition.duration_ticks + 60 * 60;
        while visiting(&state) {
            assert_no_time(&state, &format!("{kind:?}"));
            assert!(remaining > 0, "{kind:?} never left");
            remaining -= 1;
            ticks(&mut state, 1);
        }
        assert_no_time(&state, &format!("{kind:?} has left"));
        assert_eq!(resting(&state), [dash(ClockFont::Classic); 4], "{kind:?}");
    }
}

/// Visitors, weather, the aquarium, grass, tanks and colour keep the room
/// alive while the face shows `--:--`, and leave the dashes as they are.
#[test]
fn the_rest_of_the_scene_carries_on_while_the_time_is_unknown() {
    let mut state = clock(all_enabled(ClockEventProfile::Demo), 6);
    act(&mut state, ClockAction::set_time_unknown());
    let mut started = Vec::new();
    for tick in 0..3 * 60 * 60 {
        ticks(&mut state, 1);
        assert_no_time(&state, &format!("tick {tick}"));
        assert_ne!(notice(&state), Some(ClockState::TIME_UNKNOWN_NOTICE));
        if let Some(kind) = state.last_started_event
            && !started.contains(&kind)
        {
            assert!(!on_the_digits(kind), "{kind:?} started");
            started.push(kind);
        }
    }
    assert!(started.len() >= 4, "{started:?}");
    assert!(
        started.iter().any(|kind| matches!(
            kind,
            ClockEventKind::Duck | ClockEventKind::Crow | ClockEventKind::Spider
        )),
        "{started:?}"
    );
}

#[test]
fn visitors_and_scene_events_start_on_request_while_the_time_is_unknown() {
    for kind in ClockEventKind::ALL
        .into_iter()
        .filter(|kind| !on_the_digits(*kind))
    {
        let mut asked = clock(all_enabled(ClockEventProfile::Off), 10);
        act(&mut asked, ClockAction::set_time_unknown());
        act(&mut asked, ClockAction::trigger_event(kind));
        assert!(running(&asked, kind), "{kind:?} on a trigger");

        let mut state = clock(all_enabled(ClockEventProfile::Off), 10);
        act(&mut state, ClockAction::set_time_unknown());
        act(&mut state, ClockAction::preview_event(kind));
        assert!(running(&state, kind), "{kind:?} on a preview");
        assert_eq!(notice(&state).filter(|n| n.contains("clock knows")), None);
        // It runs its course over the dashes.
        let mut remaining = 120 * 60;
        while running(&state, kind) {
            assert_no_time(&state, &format!("{kind:?}"));
            assert!(remaining > 0, "{kind:?} never ended");
            remaining -= 1;
            ticks(&mut state, 1);
        }
        assert_no_time(&state, &format!("after {kind:?}"));
        assert_eq!(resting(&state), [dash(ClockFont::Classic); 4], "{kind:?}");
    }
}

#[test]
fn next_skips_the_digit_events_while_the_time_is_unknown() {
    let mut state = clock(all_enabled(ClockEventProfile::Off), 12);
    act(&mut state, ClockAction::set_time_unknown());
    let mut shown = Vec::new();
    for _ in 0..8 {
        act(&mut state, ClockAction::next_event());
        let kind = state.last_started_event.expect("Next starts something");
        assert!(!on_the_digits(kind), "{kind:?}");
        assert_ne!(notice(&state), Some(ClockState::TIME_UNKNOWN_NOTICE));
        if !shown.contains(&kind) {
            shown.push(kind);
        }
        ticks(&mut state, 30);
        assert_no_time(&state, &format!("{kind:?}"));
    }
    assert!(shown.len() >= 4, "{shown:?}");
}

/// With only digit events enabled nothing can start, and a refused Next
/// leaves the scene as it was, an open grass ceiling included.
#[test]
fn a_refused_next_leaves_an_open_ceiling_be() {
    let mut state = clock(all_enabled(ClockEventProfile::Off), 13);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    act(
        &mut state,
        ClockAction::preview_event(ClockEventKind::Grass),
    );
    let mut remaining = 30 * 60;
    while state.ceiling.phase != ceiling::Phase::Open {
        assert!(remaining > 0, "the ceiling never opened");
        remaining -= 1;
        ticks(&mut state, 1);
    }
    let settings = ClockSettings {
        events: events(on_the_digits),
        ..state.settings()
    };
    act(&mut state, ClockAction::configure(settings));
    act(&mut state, ClockAction::set_time_unknown());
    let phase = state.ceiling.phase;
    act(&mut state, ClockAction::next_event());
    assert_eq!(notice(&state), Some(ClockState::TIME_UNKNOWN_NOTICE));
    assert_eq!(state.ceiling.phase, phase);
}

/// A player's duck is the room's own business: it joins while the time is
/// unknown, and one that joined before the time was lost can still be sent
/// home.
#[test]
fn a_player_brings_and_dismisses_their_duck_whatever_the_time() {
    let mut state = clock(quiet(), 11);
    act(&mut state, ClockAction::set_time_unknown());
    act(&mut state, ClockAction::toggle_player_duck(1));
    assert!(
        state
            .player_duck_session()
            .is_some_and(|(_, player)| player == 1)
    );
    assert_ne!(notice(&state), Some(ClockState::TIME_UNKNOWN_NOTICE));

    let dismissed = |lose: bool| {
        let mut state = clock(quiet(), 11);
        act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
        act(&mut state, ClockAction::toggle_player_duck(1));
        ticks(&mut state, 120);
        if lose {
            act(&mut state, ClockAction::set_time_unknown());
        }
        act(&mut state, ClockAction::toggle_player_duck(1));
        assert_ne!(notice(&state), Some(ClockState::TIME_UNKNOWN_NOTICE));
        state.player_duck_state().map(|duck| duck.phase)
    };
    assert_eq!(dismissed(false).as_deref(), Some("resetting"));
    assert_eq!(dismissed(true), dismissed(false));
}

#[test]
fn the_lamps_hold_their_light_while_the_time_is_unknown() {
    let mut state = clock(quiet(), 14);
    act(&mut state, ClockAction::set_reading(reading(23, 30, 0)));
    let night = state.lamp_output();
    assert!(night < 0.5, "{night}");
    act(&mut state, ClockAction::set_time_unknown());
    assert_eq!(state.lamp_output(), night);
    ticks(&mut state, 60);
    assert_eq!(state.lamp_output(), night);
    act(&mut state, ClockAction::set_reading(reading(12, 0, 0)));
    assert_eq!(state.lamp_output(), lamps::light::output(1.0));
}

#[test]
fn a_reel_stopped_on_the_dash_shows_its_digit_as_soon_as_the_time_returns() {
    let mut state = clock(quiet(), 8);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    act(
        &mut state,
        ClockAction::trigger_event(ClockEventKind::SlotMachine),
    );
    let reels = |state: &ClockState| {
        state
            .slot_machine_state()
            .expect("the slot machine is running")
    };
    while !reels(&state).locked_slots[1] {
        ticks(&mut state, 1);
    }
    assert!(!reels(&state).locked_slots[3], "the last reel still turns");
    act(&mut state, ClockAction::set_time_unknown());
    ticks(&mut state, 3);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 9)));
    let digits = state.display().digits;
    assert!(!digits.contains(&Some(DASH)));
    while state.event_kind().is_some() {
        assert_eq!(reels(&state).target_digits, digits);
        ticks(&mut state, 1);
    }
}

/// Cell Swap and Marquee's clock presets draw the face themselves, so they
/// take the dashes the moment the time is lost, not once they end.
#[test]
fn events_that_draw_their_own_face_take_the_dashes_as_soon_as_the_time_is_lost() {
    let mut state = clock(quiet(), 15);
    act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
    act(
        &mut state,
        ClockAction::trigger_event(ClockEventKind::CellSwap),
    );
    ticks(&mut state, 10);
    act(&mut state, ClockAction::set_time_unknown());
    assert!(state.cell_swap_state().is_some());
    while let Some(swap) = state.cell_swap_state() {
        assert_eq!(swap.to_digits, [Some(DASH); 4]);
        ticks(&mut state, 1);
    }

    for preset in [
        engine_common::ClockMarqueePreset::ClockChase,
        engine_common::ClockMarqueePreset::ClockWave,
        engine_common::ClockMarqueePreset::ClockSpin,
    ] {
        let config = ClockConfig {
            marquee_preset: preset,
            ..quiet()
        };
        let mut state = clock(config, 15);
        act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
        act(
            &mut state,
            ClockAction::trigger_event(ClockEventKind::Marquee),
        );
        ticks(&mut state, 60);
        let shown = state.marquee_state().expect("the marquee runs").cell_count;
        act(&mut state, ClockAction::set_time_unknown());
        let dashes = presentation::content::Content::clock(state.display())
            .cells
            .len();
        assert_ne!(shown, dashes, "{preset:?}");
        while let Some(marquee) = state.marquee_state() {
            assert_eq!(marquee.cell_count, dashes, "{preset:?}");
            ticks(&mut state, 1);
        }
    }
}

/// The time returns while every timed event is in flight, and the event goes
/// on to the new reading: the face never falls back to the dashes, and the
/// clock ends up like one that only ever knew the new time.
#[test]
fn getting_the_time_back_mid_event_ends_every_event_on_the_new_reading() {
    for definition in EVENT_CATALOG.into_iter().filter(|d| !visitor(d.kind)) {
        let kind = definition.kind;
        let config = ClockConfig {
            time_format: ClockTimeFormat::TwelveHour,
            show_date: true,
            ..quiet()
        };
        let mut state = clock(config, 7);
        act(&mut state, ClockAction::set_reading(reading(21, 34, 5)));
        act(&mut state, ClockAction::trigger_event(kind));
        let fifth = definition.duration_ticks / 5;
        ticks(&mut state, fifth);
        act(&mut state, ClockAction::set_time_unknown());
        ticks(&mut state, fifth);
        if state.event_kind() != Some(kind) {
            continue;
        }
        let context = format!("{kind:?} back at {:?}", state.event_phase());
        act(&mut state, ClockAction::set_reading(reading(21, 37, 0)));
        let mut remaining = definition.duration_ticks + 60 * 60;
        while state.event_kind().is_some() {
            assert!(!state.display().is_unknown(), "{context}");
            assert!(remaining > 0, "{context}: the event never ended");
            remaining -= 1;
            ticks(&mut state, 1);
        }
        assert_eq!(
            (state.body_count(), state.collider_count()),
            (0, 0),
            "{context}"
        );
        let mut only = clock(config, 7);
        act(&mut only, ClockAction::set_reading(reading(21, 37, 0)));
        assert_eq!(state.display(), only.display(), "{context}");
        assert_eq!(state.date_label(), only.date_label(), "{context}");
        assert_eq!(state.segments(), only.segments(), "{context}");
        assert_eq!(
            ClockScenario::render_frame(&state),
            ClockScenario::render_frame(&only),
            "{context}"
        );
    }
}
