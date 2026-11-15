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
        last_emitted_reading: Cell::new(None),
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
fn christmas_dresses_the_face_and_hangs_its_lights_the_same_in_both_render_paths() {
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
