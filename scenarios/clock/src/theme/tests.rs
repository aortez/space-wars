use super::*;
use crate::{ClockAction, ClockConfig, ClockDate, ClockScenario, ClockState};
use engine_common::{Action, ClockEventKind, ClockEventProfile, Scenario};
use std::time::Duration;

fn at(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> ClockReading {
    ClockReading::new(hour, minute, second)
        .unwrap()
        .with_date(ClockDate::new(year, month, day).unwrap())
}

fn auto(reading: ClockReading) -> ClockTheme {
    resolve(ClockThemeChoice::Auto, Some(reading))
}

#[test]
fn auto_wears_christmas_from_six_on_each_december_evening_until_six_the_next_morning() {
    use ClockTheme::{Christmas, Normal};
    for (reading, theme) in [
        // November's last night is not December's.
        (at(2026, 11, 30, 18, 0, 0), Normal),
        (at(2026, 11, 30, 23, 59, 59), Normal),
        (at(2026, 12, 1, 0, 0, 0), Normal),
        (at(2026, 12, 1, 5, 59, 59), Normal),
        (at(2026, 12, 1, 6, 0, 0), Normal),
        // December's first evening starts the first night.
        (at(2026, 12, 1, 17, 59, 59), Normal),
        (at(2026, 12, 1, 18, 0, 0), Christmas),
        (at(2026, 12, 1, 23, 59, 59), Christmas),
        (at(2026, 12, 2, 0, 0, 0), Christmas),
        (at(2026, 12, 2, 5, 59, 59), Christmas),
        (at(2026, 12, 2, 6, 0, 0), Normal),
        (at(2026, 12, 25, 12, 0, 0), Normal),
        (at(2026, 12, 25, 21, 34, 0), Christmas),
        // The last night runs into the new year's first morning.
        (at(2026, 12, 31, 18, 0, 0), Christmas),
        (at(2027, 1, 1, 0, 0, 0), Christmas),
        (at(2027, 1, 1, 5, 59, 59), Christmas),
        (at(2027, 1, 1, 6, 0, 0), Normal),
        (at(2027, 1, 1, 18, 0, 0), Normal),
        // Other months' nights are ordinary.
        (at(2026, 10, 31, 21, 0, 0), Normal),
        (at(2026, 7, 4, 22, 0, 0), Normal),
        (at(2024, 3, 1, 2, 0, 0), Normal),
        // A night before the first civil day has no evening to belong to.
        (at(1, 1, 1, 3, 0, 0), Normal),
    ] {
        assert_eq!(auto(reading), theme, "{reading:?}");
    }
}

#[test]
fn auto_wears_the_ordinary_look_without_a_dated_reading() {
    assert_eq!(resolve(ClockThemeChoice::Auto, None), ClockTheme::Normal);
    let undated = ClockReading::new(21, 0, 0).unwrap();
    assert_eq!(auto(undated), ClockTheme::Normal);
}

#[test]
fn a_chosen_theme_holds_at_every_date_and_time_and_without_one() {
    let readings = [
        None,
        Some(ClockReading::new(21, 0, 0).unwrap()),
        Some(at(2026, 7, 4, 12, 0, 0)),
        Some(at(2026, 12, 24, 21, 0, 0)),
    ];
    for reading in readings {
        assert_eq!(
            resolve(ClockThemeChoice::Normal, reading),
            ClockTheme::Normal
        );
        assert_eq!(
            resolve(ClockThemeChoice::Christmas, reading),
            ClockTheme::Christmas
        );
    }
}

#[test]
fn the_clock_wears_the_theme_of_its_latest_reading_however_the_time_moved() {
    let config = ClockConfig {
        theme: ClockThemeChoice::Auto,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 1);
    // A clock's readings can repeat an hour, skip one, or jump either way when
    // the time is corrected. Each reading decides on its own, so nothing is
    // made up for the nights in between.
    for (reading, theme) in [
        (at(2026, 12, 24, 17, 59, 59), ClockTheme::Normal),
        (at(2026, 12, 24, 18, 0, 0), ClockTheme::Christmas),
        (at(2026, 12, 24, 18, 0, 0), ClockTheme::Christmas),
        (at(2026, 11, 2, 1, 30, 0), ClockTheme::Normal),
        (at(2026, 11, 2, 1, 30, 0), ClockTheme::Normal),
        (at(2027, 1, 1, 3, 0, 0), ClockTheme::Christmas),
        (at(2026, 12, 20, 10, 0, 0), ClockTheme::Normal),
        (at(2027, 3, 14, 3, 0, 0), ClockTheme::Normal),
    ] {
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        assert_eq!(state.theme(), theme, "{reading:?}");
        let palette = Appearance::of(theme).digits;
        assert_eq!(state.palette(), palette, "the idle face wears it");
    }
}

#[test]
fn normal_is_the_clocks_own_look_and_christmas_keeps_the_rooms_colors() {
    assert_eq!(
        Appearance::of(ClockTheme::Normal).digits,
        DigitPalette::default()
    );
    assert_eq!(Appearance::of(ClockTheme::Normal).lights, 0.0);
    let christmas = Appearance::of(ClockTheme::Christmas);
    assert_eq!(christmas.lights, 1.0);
    // Warm white: red at full, blue lowest, and every channel bright.
    let fill = christmas.digits.fill;
    assert!(
        fill.r >= fill.g && fill.g > fill.b && fill.b > 0.6,
        "{fill:?}"
    );
    // The dark cells are no brighter than the ordinary face's.
    let luma = |c: RenderColor| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let normal = DigitPalette::default().unlit;
    assert!(luma(christmas.digits.unlit) <= luma(normal));
}

#[test]
fn themes_and_choices_keep_stable_ids() {
    let id = |value: serde_json::Value| value.as_str().unwrap().to_owned();
    let themes: Vec<_> = ClockTheme::ALL
        .iter()
        .map(|theme| id(serde_json::to_value(theme).unwrap()))
        .collect();
    assert_eq!(themes, ["normal", "christmas"]);
    let choices: Vec<_> = ClockThemeChoice::ALL
        .iter()
        .map(|choice| id(serde_json::to_value(choice).unwrap()))
        .collect();
    assert_eq!(choices, ["auto", "normal", "christmas"]);
    assert_eq!(ClockThemeChoice::default(), ClockThemeChoice::Auto);
    for (index, choice) in ClockThemeChoice::ALL.into_iter().enumerate() {
        assert_eq!(choice as usize, index);
    }
}

#[test]
fn a_new_theme_fades_in_over_half_a_second() {
    let mut look = Look::settled(ClockTheme::Normal);
    look.head_for(ClockTheme::Christmas);
    assert_eq!(
        look.drawn(),
        Appearance::NORMAL,
        "nothing jumps at the change"
    );
    let mut lights = Vec::new();
    for _ in 0..FADE_TICKS {
        look.advance();
        lights.push(look.drawn().lights);
    }
    // Half a second at sixty ticks a second, easing in and out about its middle.
    assert_eq!(FADE_TICKS, 30);
    assert!(
        lights.windows(2).all(|pair| pair[0] < pair[1]),
        "{lights:?}"
    );
    assert_eq!(lights[14], 0.5);
    assert_eq!(look.drawn(), Appearance::CHRISTMAS, "it ends on the theme");
    look.advance();
    assert_eq!(look.drawn(), Appearance::CHRISTMAS);
}

#[test]
fn the_theme_a_look_heads_for_already_leaves_its_fade_alone() {
    let mut look = Look::settled(ClockTheme::Normal);
    look.head_for(ClockTheme::Christmas);
    for _ in 0..10 {
        look.advance();
    }
    let fading = look;
    look.head_for(ClockTheme::Christmas);
    assert_eq!(look, fading);
    let mut settled = Look::settled(ClockTheme::Christmas);
    settled.head_for(ClockTheme::Christmas);
    assert_eq!(settled, Look::settled(ClockTheme::Christmas));
}

#[test]
fn a_change_in_the_middle_of_a_fade_turns_back_from_what_is_drawn() {
    let mut look = Look::settled(ClockTheme::Normal);
    look.head_for(ClockTheme::Christmas);
    for _ in 0..10 {
        look.advance();
    }
    let drawn = look.drawn();
    assert!(drawn.lights > 0.0 && drawn.lights < 1.0, "{drawn:?}");
    look.head_for(ClockTheme::Normal);
    assert_eq!(look.drawn(), drawn, "nothing jumps at the turn");
    let mut last = drawn.lights;
    for tick in 0..FADE_TICKS {
        look.advance();
        let lights = look.drawn().lights;
        assert!(lights < last, "tick {tick}: {last} to {lights}");
        last = lights;
    }
    assert_eq!(look.drawn(), Appearance::NORMAL);
}

const TICK: Duration = Duration::from_nanos(16_666_667);

/// A quiet Clock at `reading` that has run a tick, so a new theme fades in
/// rather than taking hold at once.
fn running(choice: ClockThemeChoice, reading: ClockReading) -> ClockState {
    let config = ClockConfig {
        theme: choice,
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 1);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    ClockScenario::step(&mut state, &[], TICK);
    state
}

fn steps(state: &mut ClockState, ticks: u32) {
    for _ in 0..ticks {
        ClockScenario::step(state, &[], TICK);
    }
}

#[test]
fn set_theme_carries_each_choice_and_refuses_anything_else() {
    for choice in ClockThemeChoice::ALL {
        assert_eq!(
            ClockAction::decode(&ClockAction::set_theme(choice)),
            Some(ClockAction::SetTheme(choice))
        );
    }
    let Action::Scenario { kind, payload } = ClockAction::set_theme(ClockThemeChoice::Christmas)
    else {
        unreachable!()
    };
    let version = &payload[..2];
    for payload in [
        version.to_vec(),
        [version, &[3]].concat(),
        [version, &[2, 0]].concat(),
        [&[0, 0], &payload[2..]].concat(),
    ] {
        assert_eq!(
            ClockAction::decode(&Action::scenario(kind, payload.clone())),
            None,
            "{payload:?}"
        );
    }
}

#[test]
fn a_clock_shows_its_first_theme_at_once() {
    // Starting the Clock, or coming back to it, shows the theme from the
    // first frame: there is no look on show yet to fade from.
    let christmas = ClockScenario::init(
        ClockConfig {
            theme: ClockThemeChoice::Christmas,
            ..ClockConfig::default()
        },
        1,
    );
    assert_eq!(christmas.appearance(), Appearance::CHRISTMAS);
    let mut auto = ClockScenario::init(
        ClockConfig {
            theme: ClockThemeChoice::Auto,
            ..ClockConfig::default()
        },
        1,
    );
    assert_eq!(auto.appearance(), Appearance::NORMAL, "no date yet");
    ClockScenario::step(
        &mut auto,
        &[ClockAction::set_reading(at(2026, 12, 24, 21, 0, 0))],
        Duration::ZERO,
    );
    assert_eq!(auto.appearance(), Appearance::CHRISTMAS);
}

#[test]
fn a_new_theme_fades_in_on_the_scenes_own_time() {
    let mut state = running(ClockThemeChoice::Auto, at(2026, 12, 24, 17, 59, 59));
    assert_eq!(state.appearance(), Appearance::NORMAL);
    // Evening falls, and the look sets off toward Christmas.
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(at(2026, 12, 24, 18, 0, 0))],
        TICK,
    );
    let first = state.appearance();
    assert!(first.lights > 0.0 && first.lights < 0.01, "{first:?}");
    // A pause steps without time. The readings that come meanwhile neither
    // move the fade nor start it again, and nor does a resize.
    for second in 1..=5 {
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(at(2026, 12, 24, 18, 0, second))],
            Duration::ZERO,
        );
        assert_eq!(state.appearance(), first, "paused {second}");
    }
    state.set_aspect_ratio(480.0 / 800.0);
    assert_eq!(state.appearance(), first, "resized");
    steps(&mut state, FADE_TICKS - 2);
    assert_ne!(state.appearance(), Appearance::CHRISTMAS);
    steps(&mut state, 1);
    assert_eq!(state.appearance(), Appearance::CHRISTMAS);
    assert_eq!(state.palette(), Appearance::CHRISTMAS.digits);
}

#[test]
fn auto_wears_normal_while_the_time_is_unknown_and_a_chosen_theme_holds() {
    let evening = at(2026, 12, 24, 21, 0, 0);
    let mut auto = running(ClockThemeChoice::Auto, evening);
    let mut christmas = running(ClockThemeChoice::Christmas, evening);
    assert_eq!(auto.appearance(), Appearance::CHRISTMAS);
    for state in [&mut auto, &mut christmas] {
        ClockScenario::step(state, &[ClockAction::set_time_unknown()], TICK);
        steps(state, FADE_TICKS - 1);
    }
    // Auto has no date to go by, so the `--:--` face wears Normal, and no
    // reading or date is made up for it. A chosen theme needs no time.
    assert_eq!(auto.theme(), ClockTheme::Normal);
    assert_eq!(auto.appearance(), Appearance::NORMAL);
    assert_eq!(auto.reading, None);
    assert_eq!(auto.date_label(), None);
    assert_eq!(christmas.appearance(), Appearance::CHRISTMAS);
    assert_eq!(christmas.display(), auto.display());
    // The time comes back, and Auto finds its night again.
    for state in [&mut auto, &mut christmas] {
        ClockScenario::step(
            state,
            &[ClockAction::set_reading(at(2026, 12, 24, 21, 0, 5))],
            TICK,
        );
        steps(state, FADE_TICKS - 1);
    }
    assert_eq!(auto.appearance(), Appearance::CHRISTMAS);
    assert_eq!(christmas.appearance(), Appearance::CHRISTMAS);
}

#[test]
fn rapid_changes_of_theme_never_jump_and_settle_on_the_last() {
    let mut state = running(ClockThemeChoice::Normal, at(2026, 7, 4, 12, 0, 0));
    let mut last = state.appearance().lights;
    // The steepest a fade gets is half again its average pace.
    let most = 1.5 / FADE_TICKS as f32;
    for tick in 0..40 {
        let choice = [ClockThemeChoice::Christmas, ClockThemeChoice::Normal][tick % 2];
        ClockScenario::step(&mut state, &[ClockAction::set_theme(choice)], TICK);
        let lights = state.appearance().lights;
        assert!(
            (lights - last).abs() <= most,
            "tick {tick}: {last} to {lights}"
        );
        last = lights;
    }
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_theme(ClockThemeChoice::Christmas)],
        TICK,
    );
    for tick in 1..FADE_TICKS {
        let lights = state.appearance().lights;
        assert!(lights < 1.0, "tick {tick}: {lights}");
        steps(&mut state, 1);
        let next = state.appearance().lights;
        assert!(next > lights && next - lights <= most, "tick {tick}");
    }
    assert_eq!(state.appearance(), Appearance::CHRISTMAS);
}

/// What a theme leaves alone, as the Clock reports it: the event running,
/// its ID and age, the scheduler's deadline and cooldowns, the visitors, the
/// arena and its bodies, the digits' surfaces, the lamps and a player's seat.
fn world(state: &ClockState) -> String {
    let mut arena = state.arena_state();
    // How long a step took on this machine is no part of the world.
    if let Some(step) = arena.last_step.as_mut() {
        step.wall_us = 0;
        step.collision_detection_us = 0;
        step.solver_us = 0;
    }
    format!(
        "{:?}",
        (
            (
                state.simulation_tick(),
                state.lifecycle(),
                state.event_id(),
                state.event_kind(),
                state.event_phase(),
                state.phase_tick(),
                state.next_event_tick(),
            ),
            ClockEventKind::ALL.map(|kind| state.event_ready_at_tick(kind)),
            (
                state.body_count(),
                state.collider_count(),
                arena,
                state.digit_surface_state(),
            ),
            (
                state.meltdown_state(),
                state.duck_state(),
                state.marquee_state(),
                state.crow_state(),
                state.explosion_state(),
                state.tanks_state(),
                state.coins_state(),
                state.spider_state(),
            ),
            (
                state.lamps_state(),
                state.player_duck_session(),
                state.display(),
                state.segments(),
            ),
        )
    )
}

#[test]
fn changes_of_theme_leave_the_world_as_it_would_have_been() {
    let config = ClockConfig {
        theme: ClockThemeChoice::Normal,
        event_profile: ClockEventProfile::Demo,
        show_date: true,
        ..ClockConfig::default()
    };
    let mut plain = ClockScenario::init(config, 7);
    let mut themed = ClockScenario::init(config, 7);
    // The twins see the same evening, a player joining and leaving, and the
    // same pauses. Only the themed one changes its theme, outright and by
    // Auto, which picks the Christmas it already wears, in quick succession,
    // and both during this seed's Meltdowns, Explosion and Slot Machine and
    // between them.
    let changes = [
        (90, ClockThemeChoice::Christmas),
        (600, ClockThemeChoice::Auto),
        (700, ClockThemeChoice::Normal),
        (1200, ClockThemeChoice::Christmas),
        (1600, ClockThemeChoice::Normal),
        (1605, ClockThemeChoice::Christmas),
        (1612, ClockThemeChoice::Auto),
        (2200, ClockThemeChoice::Normal),
        (2500, ClockThemeChoice::Christmas),
        (3400, ClockThemeChoice::Normal),
        (3500, ClockThemeChoice::Auto),
        (4400, ClockThemeChoice::Normal),
    ];
    let mut kinds = Vec::new();
    let mut seated = 0;
    let mut during = Vec::new();
    for tick in 0..4500u32 {
        let second = tick / 60;
        let mut actions = Vec::new();
        if tick % 60 == 0 {
            let reading = at(2026, 12, 24, 21, (second / 60) as u8, (second % 60) as u8);
            actions.push(ClockAction::set_reading(reading));
        }
        if tick == 1000 || tick == 1900 {
            actions.push(ClockAction::toggle_player_duck(1));
        }
        let mut themed_actions = actions.clone();
        if let Some(&(_, choice)) = changes.iter().find(|(at, _)| *at == tick) {
            themed_actions.push(ClockAction::set_theme(choice));
            during.push(plain.event_kind());
        }
        // Every so often the host pauses for a few polls.
        let dt = if tick % 500 < 3 { Duration::ZERO } else { TICK };
        ClockScenario::step(&mut plain, &actions, dt);
        ClockScenario::step(&mut themed, &themed_actions, dt);
        assert_eq!(world(&plain), world(&themed), "tick {tick}");
        if let Some(kind) = plain.event_kind().filter(|kind| !kinds.contains(kind)) {
            kinds.push(kind);
        }
        seated += u32::from(plain.player_duck_session().is_some());
    }
    assert!(kinds.len() >= 3, "{kinds:?}");
    assert!(seated > 300, "a player sat for {seated} ticks");
    let events = during.iter().flatten().count();
    assert!(events >= 6 && during.len() - events >= 3, "{during:?}");
    // Back on Normal, the two draw the same frame.
    assert_eq!(themed.appearance(), Appearance::NORMAL);
    assert_eq!(
        crate::render::render_frame(&plain),
        crate::render::render_frame(&themed)
    );
}
