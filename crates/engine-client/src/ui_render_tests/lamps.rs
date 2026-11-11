//! Fixed-reading captures of the corner lamps through both production scene
//! adapters: the steady lamps, a spotlight resting on the digit nearest its
//! lamp and partway through its sweep, and an outage's dark corner, in the
//! device-sized views. Every mood ends on exactly the steady face. A sample
//! always runs; the stills and every second frame of the 800x480 film are
//! written out when `SPACEWARS_LAMPS_ARTIFACTS` names a directory, for the
//! docs' pictures.
use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventProfile, ClockLampMood, ClockLampSide, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
use std::time::Duration;

// Every capture owns a fresh pixel buffer, so request full repainting.
struct CapturePlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

fn draw(
    ui: &MainWindow,
    window: &MinimalSoftwareWindow,
    frame: &engine_common::RenderFrame,
    viewport: Viewport,
) -> SharedPixelBuffer<Rgb8Pixel> {
    reset_panels(ui);
    let frames = std::slice::from_ref(frame);
    ui.set_raster_visible(true);
    ui.set_raster_frame(Image::from_rgb8(raster(frame, viewport)));
    let rows = crate::render::raster_text_overlay(frames, viewport, FrameLayout::EqualHorizontal);
    ui.set_primitives(Rc::new(slint::VecModel::from(rows)).into());
    let mut pixels = SharedPixelBuffer::new(viewport.width as u32, viewport.height as u32);
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), viewport.width as usize);
    });
    pixels
}

#[test]
fn lamps_render_steady_spotlight_and_outage_and_come_back_at_target_sizes() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_LAMPS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path.join("film")).unwrap();
    }
    let mut traces = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let windows = windows.borrow();
        let window = &windows[0];
        window.set_size(PhysicalSize::new(width, height));
        // Profile Off: no event comes along, and a preview still runs.
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            show_date: true,
            ..Default::default()
        };
        let reading = ClockReading::new(12, 39, 50)
            .unwrap()
            .with_date(ClockDate::new(2026, 11, 10).unwrap());
        let seed = 7;
        let mut state = ClockScenario::init(config, seed);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        let frame = ClockScenario::render_frame(&state);
        let steady = draw(&ui, window, &frame, viewport);
        let vector = svg(&frame, viewport);
        assert!(!vector.contains("NaN") && !vector.contains("inf"));
        if let Some(path) = &output {
            write_png(
                &path.join(format!("lamps-{width}x{height}-steady.png")),
                &steady,
            );
        }
        let mut films = 0;
        for (mood, lamp) in [
            (ClockLampMood::Spotlight, ClockLampSide::Left),
            (ClockLampMood::Outage, ClockLampSide::Right),
            (ClockLampMood::Flicker, ClockLampSide::Left),
        ] {
            state.preview_lamp_mood(mood, lamp);
            let mut dark = 0;
            let mut ticks = 0;
            while let Some(current) = state.lamps_state().mood {
                assert_eq!((current.kind, current.lamp), (mood, lamp));
                assert!(!current.yielding, "{width}x{height}: {mood:?} gave way");
                dark = if current.brightness_milli == 0 {
                    dark + 1
                } else {
                    0
                };
                let still = match (mood, current.tick) {
                    (ClockLampMood::Spotlight, 72) => Some("spotlight"),
                    (ClockLampMood::Spotlight, 186) => Some("sweep"),
                    (ClockLampMood::Outage, _) if dark == 40 => Some("outage"),
                    _ => None,
                };
                let film = width == 800 && output.is_some() && current.tick % 2 == 0;
                if still.is_some() || film || current.tick % 60 == 0 {
                    let frame = ClockScenario::render_frame(&state);
                    let pixels = draw(&ui, window, &frame, viewport);
                    if current.tick % 60 == 0 {
                        let vector = svg(&frame, viewport);
                        assert!(!vector.contains("NaN") && !vector.contains("inf"));
                    }
                    if let Some(name) = still {
                        assert!(
                            pixels.as_bytes() != steady.as_bytes(),
                            "{width}x{height}: {name} draws the steady lamps"
                        );
                        if let Some(path) = &output {
                            write_png(
                                &path.join(format!("lamps-{width}x{height}-{name}.png")),
                                &pixels,
                            );
                        }
                        traces.push(serde_json::json!({
                            "width": width,
                            "height": height,
                            "seed": seed,
                            "still": name,
                            "lamps": state.lamps_state(),
                            "primitives": frame.layers.iter().map(|layer| layer.primitives.len()).sum::<usize>(),
                        }));
                    }
                    if film && let Some(path) = &output {
                        write_png(&path.join(format!("film/frame-{films:04}.png")), &pixels);
                        films += 1;
                    }
                }
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                ticks += 1;
                assert!(ticks < 20 * 60, "{width}x{height}: {mood:?} never ended");
            }
            // A mood leaves nothing behind: the lamps are steady again.
            let pixels = draw(&ui, window, &ClockScenario::render_frame(&state), viewport);
            assert!(
                pixels.as_bytes() == steady.as_bytes(),
                "{width}x{height}: the lamps are not steady after {mood:?}"
            );
        }
    }
    assert_eq!(traces.len(), 9, "a still was missed: {traces:?}");
    if let Some(path) = &output {
        std::fs::write(
            path.join("captures.json"),
            serde_json::to_vec_pretty(&traces).unwrap(),
        )
        .unwrap();
    }
}
