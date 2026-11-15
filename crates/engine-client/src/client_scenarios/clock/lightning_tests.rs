use super::capture::{render, save};
use super::*;
use engine_common::{ClockEventKind, ClockEventProfile, ClockLampMode, ClockRainAmount};

const TICK: Duration = Duration::from_nanos(16_666_667);

/// The same heavy shower twice, with lightning and without, under Steady
/// lamps, which never change a frame on their own.
fn showers(viewport: Viewport, seed: u64) -> [ClockClientScenario; 2] {
    [true, false].map(|lightning| {
        let config = ClockConfig {
            lamps: ClockLampMode::Steady,
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            rain_amount: ClockRainAmount::Heavy,
            lightning,
            ..ClockConfig::default()
        };
        let mut state = ClockScenario::init(config, seed);
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
                ClockAction::preview_event(ClockEventKind::Rain),
            ],
            Duration::ZERO,
        );
        ClockClientScenario {
            state,
            last_emitted_reading: Cell::new(None),
            benchmark: None,
        }
    })
}

/// The mean of every pixel's channels, 0 to 255.
fn brightness(pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>) -> f64 {
    let sum: u64 = pixels
        .as_slice()
        .iter()
        .map(|p| u64::from(p.r) + u64::from(p.g) + u64::from(p.b))
        .sum();
    sum as f64 / (pixels.as_slice().len() * 3) as f64
}

/// Pixels as white as the bolt's core: the face's cyan and the lamps' warm
/// bulbs are not.
fn bolt_pixels(pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>) -> usize {
    pixels
        .as_slice()
        .iter()
        .filter(|p| p.r >= 215 && p.g >= 215 && p.b >= 230)
        .count()
}

#[test]
fn a_strike_flashes_the_room_and_draws_its_bolt_then_leaves_no_trace() {
    for viewport in [
        Viewport::new(800.0, 480.0),
        Viewport::new(1024.0, 768.0),
        Viewport::new(480.0, 800.0),
    ] {
        let size = format!("{}x{}", viewport.width, viewport.height);
        let [mut stormy, mut calm] = showers(viewport, 7);
        let mut tick = 0;
        // Until the first strike the shower is the same with lightning as
        // without it.
        while stormy.state.rain_state().unwrap().strikes == 0 {
            tick += 1;
            assert!(tick < 1200, "a heavy shower strikes: {size}");
            stormy.step(&[], TICK);
            calm.step(&[], TICK);
            if tick % 120 == 0 {
                assert_eq!(render(&stormy, viewport).0, render(&calm, viewport).0);
            }
        }
        let rain = stormy.state.rain_state().unwrap();
        assert_eq!(
            rain.flash_milli, 1000,
            "a strike opens at full flash: {size}"
        );
        let (_, lit) = render(&stormy, viewport);
        let (_, dark) = render(&calm, viewport);
        assert!(
            brightness(&lit) > brightness(&dark) + 20.0,
            "the flash lights the room: {} against {}, {size}",
            brightness(&lit),
            brightness(&dark)
        );
        assert!(
            bolt_pixels(&lit) > bolt_pixels(&dark) + 50,
            "the bolt shows: {} against {}, {size}",
            bolt_pixels(&lit),
            bolt_pixels(&dark)
        );
        save(&dark, &format!("lightning-{size}-before.png"));
        // Every frame of the strike, saved for review.
        for age in 0..48 {
            if age > 0 {
                stormy.step(&[], TICK);
                calm.step(&[], TICK);
            }
            save(
                &render(&stormy, viewport).1,
                &format!("lightning-{size}-{age:02}.png"),
            );
        }
        // Once its light and sparks are gone, a strike leaves nothing behind.
        assert_eq!(stormy.state.rain_state().unwrap().flash_milli, 0);
        assert_eq!(render(&stormy, viewport).0, render(&calm, viewport).0);
    }
}
