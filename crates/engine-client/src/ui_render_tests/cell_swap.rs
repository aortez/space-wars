//! Fixed-reading captures through both production scene adapters and the real UI.
use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::write_png,
};
use engine_common::{
    ClockCellSwapPattern, ClockEventKind, ClockEventProfile, ClockFont, ClockFontSettings,
    ClockSettings, ClockTimeFormat, Scenario,
};
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
    vector: bool,
) -> SharedPixelBuffer<Rgb8Pixel> {
    reset_panels(ui);
    let frames = std::slice::from_ref(frame);
    ui.set_raster_visible(true);
    ui.set_raster_frame(if vector {
        // MinimalSoftwareWindow does not draw Path items. Rasterize an export
        // of the production vector adapter's paths through Slint's SVG loader;
        // text still uses the same native overlay as the application.
        let svg = crate::thruster_visual_tests::svg(frame, viewport);
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        Image::load_from_svg_data(svg.as_bytes()).unwrap()
    } else {
        Image::from_rgb8(crate::thruster_visual_tests::raster(frame, viewport))
    });
    let rows = crate::render::raster_text_overlay(frames, viewport, FrameLayout::EqualHorizontal);
    ui.set_primitives(Rc::new(slint::VecModel::from(rows)).into());
    capture(window, viewport)
}

fn capture(window: &MinimalSoftwareWindow, viewport: Viewport) -> SharedPixelBuffer<Rgb8Pixel> {
    let mut pixels = SharedPixelBuffer::new(viewport.width as u32, viewport.height as u32);
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), viewport.width as usize);
    });
    pixels
}

#[test]
fn cell_swap_renders_motion_rollover_and_controls_at_target_sizes() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_CELL_SWAP_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut traces = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let windows = windows.borrow();
        let window = &windows[0];
        window.set_size(PhysicalSize::new(width, height));
        for (font, pattern) in [
            (ClockFont::Classic, ClockCellSwapPattern::Shuffle),
            (ClockFont::Matrix, ClockCellSwapPattern::MirrorHorizontal),
            (ClockFont::Sans, ClockCellSwapPattern::MirrorVertical),
            (ClockFont::Serif, ClockCellSwapPattern::Shuffle),
        ] {
            let config = ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                time_format: ClockTimeFormat::TwelveHour,
                show_date: true,
                fonts: ClockFontSettings {
                    selected: font,
                    ..Default::default()
                },
                ..Default::default()
            };
            let date = ClockDate::new(2026, 10, 17).unwrap();
            let from = ClockReading::new(12, 59, 58).unwrap().with_date(date);
            let to = ClockReading::new(13, 0, 0).unwrap().with_date(date);
            let (seed, mut state) = (0..128)
                .find_map(|seed| {
                    let mut state = ClockScenario::init(config, seed);
                    ClockScenario::step(
                        &mut state,
                        &[
                            ClockAction::set_reading(from),
                            ClockAction::preview_event(ClockEventKind::CellSwap),
                        ],
                        Duration::ZERO,
                    );
                    (state.cell_swap_state().unwrap().pattern == pattern).then_some((seed, state))
                })
                .unwrap();
            let mut reference = ClockScenario::init(config, seed);
            ClockScenario::step(
                &mut reference,
                &[ClockAction::set_reading(from)],
                Duration::ZERO,
            );
            let initial_frame = ClockScenario::render_frame(&reference);
            let initial = [
                draw(&ui, window, &initial_frame, viewport, false),
                draw(&ui, window, &initial_frame, viewport, true),
            ];
            ClockScenario::step(
                &mut reference,
                &[ClockAction::set_reading(to)],
                Duration::ZERO,
            );
            let final_frame = ClockScenario::render_frame(&reference);
            let expected = [
                draw(&ui, window, &final_frame, viewport, false),
                draw(&ui, window, &final_frame, viewport, true),
            ];
            for tick in 0..=75 {
                if tick == 30 {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::set_reading(to)],
                        Duration::ZERO,
                    );
                }
                let frame = ClockScenario::render_frame(&state);
                if [0, 20, 40, 60, 67, 75].contains(&tick) {
                    for (index, backend) in ["raster", "vector"].into_iter().enumerate() {
                        let pixels = draw(&ui, window, &frame, viewport, index == 1);
                        if tick == 0 {
                            assert!(
                                pixels.as_bytes() == initial[index].as_bytes(),
                                "initial {font:?} {backend}"
                            );
                        }
                        if tick == 75 {
                            assert!(
                                pixels.as_bytes() == expected[index].as_bytes(),
                                "final {font:?} {backend}"
                            );
                        }
                        if tick == 20 {
                            assert!(
                                pixels.as_bytes() != initial[index].as_bytes(),
                                "motion {font:?} {backend}"
                            );
                        }
                        if let Some(path) = &output {
                            write_png(
                                &path.join(format!(
                                    "cell-swap-{width}x{height}-{}-{backend}-{tick:02}.png",
                                    font.label().to_lowercase()
                                )),
                                &pixels,
                            );
                        }
                    }
                    traces.push(serde_json::json!({"width":width,"height":height,"font":font,"seed":seed,"tick":tick,"swap":state.cell_swap_state(),"phase":state.event_phase().map(|p|p.as_str()),"primitives":frame.layers.iter().map(|l|l.primitives.len()).sum::<usize>()}));
                }
                // One real-time sequence for reviewing paths and the rollover.
                if width == 800
                    && font == ClockFont::Classic
                    && (tick % 2 == 0 || tick == 75)
                    && let Some(path) = &output
                {
                    let pixels = draw(&ui, window, &frame, viewport, false);
                    write_png(&path.join(format!("playback-{tick:02}.png")), &pixels);
                }
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            assert!(state.cell_swap_state().is_none());
        }
        // Screens include the actual menu tree, not a reconstructed mockup.
        for live in [false, true] {
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            crate::apply_scenario_metadata(&ui, "clock");
            crate::clock_controls::publish_settings(
                &ui,
                ClockSettings {
                    time_format: ClockTimeFormat::TwelveHour,
                    event_profile: ClockEventProfile::Off,
                    show_date: true,
                    fonts: ClockFontSettings {
                        selected: ClockFont::Serif,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            ui.set_clock_event_labels(
                Rc::new(slint::VecModel::from(
                    ClockEventKind::ALL
                        .iter()
                        .map(|k| k.label().into())
                        .collect::<Vec<_>>(),
                ))
                .into(),
            );
            ui.set_clock_preview_index(ClockEventKind::CellSwap as i32);
            if live {
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                ui.set_ingame_clock_focus_index(18);
            } else {
                ui.set_launcher_visible(true);
                ui.set_launcher_settings_visible(true);
                ui.set_launcher_settings_focus_index(17);
            }
            let pixels = capture(window, viewport);
            if let Some(path) = &output {
                write_png(
                    &path.join(format!(
                        "cell-swap-{width}x{height}-{}-controls.png",
                        if live { "live" } else { "launcher" }
                    )),
                    &pixels,
                );
            }
        }
    }
    if let Some(path) = &output {
        std::fs::write(
            path.join("captures.json"),
            serde_json::to_vec_pretty(&traces).unwrap(),
        )
        .unwrap();
    }
}
