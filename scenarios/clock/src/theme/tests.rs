use super::*;
use crate::{ClockAction, ClockConfig, ClockDate, ClockScenario};
use engine_common::Scenario;
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
    assert_eq!(
        Appearance::of(ClockTheme::Normal).decoration,
        Decoration::None
    );
    let christmas = Appearance::of(ClockTheme::Christmas);
    assert_eq!(christmas.decoration, Decoration::StringLights);
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
