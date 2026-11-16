//! The Clock on a host whose time nobody vouches for (#257).
use super::capture::{render, save};
use super::*;
use engine_common::{ClockEventProfile, ClockFont, ClockLampMode, ClockTimeFormat};

const VIEWPORTS: [Viewport; 3] = [
    Viewport::new(800.0, 480.0),
    Viewport::new(1024.0, 768.0),
    Viewport::new(480.0, 800.0),
];

/// Normal's lit digits and its AM/PM mark.
const CYAN: [u8; 3] = [84, 240, 232];
const LABEL: [u8; 3] = [133, 184, 196];

fn settings(font: ClockFont, time_format: ClockTimeFormat) -> Settings {
    let mut settings = Settings::default();
    settings.clock.fonts.selected = font;
    settings.clock.time_format = time_format;
    settings.clock.show_date = true;
    settings.clock.event_profile = ClockEventProfile::Off;
    settings.clock.lamps = ClockLampMode::Steady;
    settings
}

/// Where the pixels near `color` are, each channel within `slack` of it, as
/// their count and bounding box `[left, top, right, bottom]`.
fn near(
    pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>,
    color: [u8; 3],
    slack: u8,
) -> (usize, [u32; 4]) {
    let width = pixels.width();
    let mut count = 0;
    let mut bounds = [u32::MAX, u32::MAX, 0, 0];
    for (index, p) in pixels.as_slice().iter().enumerate() {
        if [p.r, p.g, p.b]
            .iter()
            .zip(color)
            .all(|(&have, want)| have.abs_diff(want) <= slack)
        {
            let (x, y) = (index as u32 % width, index as u32 / width);
            count += 1;
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
    }
    (count, bounds)
}

#[test]
fn a_clock_without_a_trusted_time_starts_on_dashes_and_reports_no_time() {
    let settings = settings(ClockFont::Classic, ClockTimeFormat::TwelveHour);
    let mut scenario = live(
        17,
        &settings,
        Viewport::new(800.0, 480.0),
        TimeSource::Unknown,
    );
    assert!(scenario.state.time_unknown());
    assert_eq!(scenario.state.reading(), None);
    let report = scenario.clock_state().unwrap();
    assert!(report.time_unknown);
    assert_eq!(
        (report.reading, report.date, report.date_label),
        (None, None, None)
    );
    assert_eq!(report.display_digits, [None; 4]);
    // Saying it again says nothing new.
    assert!(scenario.actions_for_now().is_empty());

    // Once the time is vouched for, the next tick carries the host's reading.
    scenario.time = TimeSource::Host;
    let actions = scenario.actions_for_now();
    assert!(matches!(
        actions.as_slice(),
        [action] if matches!(ClockAction::decode(action), Some(ClockAction::SetReading(_)))
    ));
    ClockScenario::step(&mut scenario.state, &actions, Duration::ZERO);
    let report = scenario.clock_state().unwrap();
    assert!(!report.time_unknown);
    assert!(report.reading.is_some() && report.date_label.is_some());
    assert!(report.display_digits.iter().any(Option::is_some));

    // And when it no longer is, the next tick takes it away again.
    scenario.time = TimeSource::Unknown;
    assert_eq!(
        scenario.actions_for_now(),
        vec![ClockAction::set_time_unknown()]
    );
    assert!(scenario.actions_for_now().is_empty());
}

#[test]
fn every_face_draws_its_dashes_in_the_rasters_pixels() {
    for viewport in VIEWPORTS {
        let size = format!("{}x{}", viewport.width, viewport.height);
        for font in ClockFont::ALL {
            let context = format!("{size} {font:?}");
            let twelve = settings(font, ClockTimeFormat::TwelveHour);
            let unknown = live(7, &twelve, viewport, TimeSource::Unknown);
            let (_, dashes) = render(&unknown, viewport);
            save(
                &dashes,
                &format!("unknown-time-{size}-{}.png", font.label().to_lowercase()),
            );
            let mut known = live(7, &twelve, viewport, TimeSource::Host);
            // 10:08 PM lights every bar of the 0 and the 8.
            let reading = ClockReading::new(22, 8, 0).unwrap();
            ClockScenario::step(
                &mut known.state,
                &[ClockAction::set_reading(reading)],
                Duration::ZERO,
            );
            let (_, digits) = render(&known, viewport);

            // The lit face is one thin band across the middle of the digits,
            // where the time's face stands nine cells tall.
            let (lit, [left, top, right, bottom]) = near(&dashes, CYAN, 24);
            let (digit_lit, [_, digit_top, _, digit_bottom]) = near(&digits, CYAN, 24);
            assert!(lit > 50, "{context}: {lit} lit pixels");
            assert!(digit_lit > 2 * lit, "{context}: {digit_lit} against {lit}");
            let (band, face) = (bottom - top, digit_bottom - digit_top);
            assert!(
                2 * band < face,
                "{context}: a band {band} high in a face {face} high"
            );
            assert!(
                (top + bottom).abs_diff(digit_top + digit_bottom) < face / 4,
                "{context}: the band is off the middle of the digits"
            );
            assert!(5 * band < right - left, "{context}: not a band");
            // No AM/PM, even in the 12-hour format.
            assert!(near(&digits, LABEL, 12).0 > 20, "{context}");
            assert_eq!(near(&dashes, LABEL, 12).0, 0, "{context}");
        }
    }
}
