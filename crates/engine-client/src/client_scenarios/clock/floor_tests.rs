use super::*;
use engine_common::{ClockEventKind, ClockEventProfile, ClockFloorMode};

/// Face recovery must remain pixel-exact while the drain intentionally closes.
/// The floor begins 84% down the fixed-height camera; exclude its antialias edge.
pub(super) fn pixels_above_floor(pixels: &slint::SharedPixelBuffer<slint::Rgb8Pixel>) -> &[u8] {
    let rows = (pixels.height() as usize * 84 / 100).saturating_sub(1);
    &pixels.as_bytes()[..rows * pixels.width() as usize * 3]
}

#[test]
fn managed_floor_is_visible_in_renderer_inputs_and_reported_in_clock_state() {
    for viewport in [
        Viewport::new(800.0, 480.0),
        Viewport::new(1024.0, 768.0),
        Viewport::new(480.0, 800.0),
    ] {
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        };
        let mut state = ClockScenario::init(config, 0);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(12, 34, 56).unwrap(),
            )],
            Duration::ZERO,
        );
        let mut scenario = ClockClientScenario {
            state,
            last_emitted_reading: Cell::new(None),
            benchmark: None,
        };
        let mut renderer = crate::raster::RasterRenderer::new();
        for (event, mode) in [
            (None, ClockFloorMode::Closed),
            (Some(ClockEventKind::Rain), ClockFloorMode::DrainOpen),
            (Some(ClockEventKind::ColorCycle), ClockFloorMode::Closed),
            (Some(ClockEventKind::Falling), ClockFloorMode::DrainOpen),
            (Some(ClockEventKind::Marquee), ClockFloorMode::Closed),
            (Some(ClockEventKind::Meltdown), ClockFloorMode::DrainOpen),
        ] {
            if let Some(event) = event {
                scenario.preview_clock_event(event);
            }
            assert_eq!(scenario.clock_state().unwrap().floor, mode);
            let frames = scenario.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(
                frames,
                scenario.render_frames(RenderBackend::Vector, viewport)
            );
            let presentation = crate::render::scene_presentation_from_frames_with_layout(
                &frames,
                viewport,
                scenario.frame_layout(),
            );
            assert!(!presentation.main_primitives.is_empty());
            let pixels = renderer
                .image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                )
                .to_rgb8()
                .unwrap();
            // At event tick zero there is no material in the lower floor strip.
            // The center must match a solid bank only when the drain is closed.
            let row = pixels.height() as usize * 92 / 100;
            let width = pixels.width() as usize;
            let bank = pixels.as_slice()[row * width + width / 10];
            let center = pixels.as_slice()[row * width + width / 2];
            assert_eq!(
                center == bank,
                mode == ClockFloorMode::Closed,
                "{event:?}, {viewport:?}"
            );
        }
        scenario.preview_clock_event(ClockEventKind::Duck);
        assert_eq!(
            scenario.clock_state().unwrap().floor,
            ClockFloorMode::EventOwned
        );
    }
}
