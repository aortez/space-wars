use super::capture::{render, save};
use super::*;
use engine_common::{ClockEventProfile, ClockLampMode, ClockThemeChoice, ClockTimeFormat};

const VIEWPORTS: [Viewport; 3] = [
    Viewport::new(800.0, 480.0),
    Viewport::new(1024.0, 768.0),
    Viewport::new(480.0, 800.0),
];

/// The idle face on Christmas Eve at 21:34, with its date, in `theme`, under
/// Steady lamps.
fn face(
    viewport: Viewport,
    theme: ClockThemeChoice,
    time_format: ClockTimeFormat,
) -> ClockClientScenario {
    let config = ClockConfig {
        lamps: ClockLampMode::Steady,
        aspect_ratio: viewport.aspect_ratio(),
        event_profile: ClockEventProfile::Off,
        show_date: true,
        theme,
        time_format,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 7);
    let reading = ClockReading::new(21, 34, 0)
        .unwrap()
        .with_date(ClockDate::new(2026, 12, 24).unwrap());
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    ClockClientScenario {
        state,
        time: super::TimeSource::Host,
        last_emitted: Cell::new(None),
        benchmark: None,
    }
}

/// Pixels near `color`, each channel within `slack` of it, 0 to 255.
fn near(pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>, color: [u8; 3], slack: u8) -> usize {
    pixels
        .as_slice()
        .iter()
        .filter(|p| {
            [p.r, p.g, p.b]
                .iter()
                .zip(color)
                .all(|(&have, want)| have.abs_diff(want) <= slack)
        })
        .count()
}

#[test]
fn christmas_dresses_the_face_and_lights_its_bulbs_in_the_rasters_pixels() {
    let day = ClockTimeFormat::TwentyFourHour;
    for viewport in VIEWPORTS {
        let size = format!("{}x{}", viewport.width, viewport.height);
        let (normal_frames, normal) =
            render(&face(viewport, ClockThemeChoice::Normal, day), viewport);
        let (christmas_frames, christmas) =
            render(&face(viewport, ClockThemeChoice::Christmas, day), viewport);
        save(&normal, &format!("theme-{size}-normal-raster.png"));
        save(&christmas, &format!("theme-{size}-christmas-raster.png"));
        assert_ne!(normal_frames, christmas_frames, "{size}");
        // The cyan digits turn warm white.
        let cyan = [84, 240, 232];
        let warm = [255, 232, 194];
        assert!(near(&normal, cyan, 24) > 500, "{size}");
        assert_eq!(near(&christmas, cyan, 24), 0, "{size}");
        assert!(near(&christmas, warm, 24) > 500, "{size}");
        assert_eq!(near(&normal, warm, 24), 0, "{size}");
        // A bulb of every color lights up, and Normal has none.
        for bulb in [[255, 51, 41], [255, 184, 56], [56, 224, 87], [71, 128, 255]] {
            assert!(near(&christmas, bulb, 24) > 100, "{size} {bulb:?}");
            assert_eq!(near(&normal, bulb, 24), 0, "{size} {bulb:?}");
        }
        // The PM mark turns holly red.
        let (_, twelve) = render(
            &face(
                viewport,
                ClockThemeChoice::Christmas,
                ClockTimeFormat::TwelveHour,
            ),
            viewport,
        );
        save(&twelve, &format!("theme-{size}-christmas-12h-raster.png"));
        let holly = [235, 107, 102];
        assert!(near(&twelve, holly, 16) > 20, "{size}");
        assert!(
            near(&christmas, holly, 16) < near(&twelve, holly, 16),
            "{size}"
        );
    }
}

/// Auto's nights go by the host's local time, as the reading takes it, so one
/// instant can be a December evening in one zone and the afternoon before in
/// the next. A zone that changes takes the theme of the reading it gives,
/// either way, with nothing made up in between.
#[test]
fn autos_nights_follow_the_hosts_local_time_in_every_zone() {
    use chrono::{FixedOffset, TimeZone, Utc};
    use engine_common::ClockTheme::{Christmas, Normal};
    let config = ClockConfig {
        theme: ClockThemeChoice::Auto,
        event_profile: ClockEventProfile::Off,
        ..ClockConfig::default()
    };
    let mut state = ClockScenario::init(config, 7);
    let mut wears = |utc: chrono::DateTime<Utc>, hours: i32| {
        let zone = FixedOffset::east_opt(hours * 3600).unwrap();
        let reading = clock_reading_from_local(utc.with_timezone(&zone));
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        state.theme()
    };
    let first = Utc.with_ymd_and_hms(2026, 12, 2, 2, 0, 0).unwrap();
    // 17:00 on December 1.
    assert_eq!(wears(first, -9), Normal);
    // 18:00 on December 1, the first evening.
    assert_eq!(wears(first, -8), Christmas);
    // 02:00 on December 2, still December 1's night.
    assert_eq!(wears(first, 0), Christmas);
    // 07:00 on December 2, the morning after.
    assert_eq!(wears(first, 5), Normal);
    // The zone set back to 17:00 on December 1.
    assert_eq!(wears(first, -9), Normal);
    let last = Utc.with_ymd_and_hms(2027, 1, 1, 14, 0, 0).unwrap();
    // 05:00 on January 1, still December 31's night.
    assert_eq!(wears(last, -9), Christmas);
    // 06:00 on January 1, when the last night ends.
    assert_eq!(wears(last, -8), Normal);
    // 04:00 on January 2, a January night.
    assert_eq!(wears(last, 14), Normal);
    // The zone set back to 05:00 on January 1.
    assert_eq!(wears(last, -9), Christmas);
}
