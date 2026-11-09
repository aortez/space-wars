use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::write_png,
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
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
fn tanks_render_both_adapters_and_controls_without_covering_the_reading() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_TANKS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let windows = windows.borrow();
        let window = &windows[0];
        window.set_size(PhysicalSize::new(width, height));
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(12, 39, 50).unwrap(),
            )],
            Duration::ZERO,
        );
        let reference = [false, true].map(|vector| {
            draw(
                &ui,
                window,
                &ClockScenario::render_frame(&state),
                viewport,
                vector,
            )
        });
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Tanks)],
            Duration::ZERO,
        );
        let mut saw_fight = false;
        for tick in 0..=84 * 60 {
            let phase = state.event_phase().map(|p| p.as_str());
            let capture_tick = if phase == Some("beam-down") { 60 } else { 30 };
            if state.phase_tick() == capture_tick || phase.is_none() {
                let frame = ClockScenario::render_frame(&state);
                for (index, vector) in [false, true].into_iter().enumerate() {
                    let pixels = draw(&ui, window, &frame, viewport, vector);
                    for (actual, reference) in
                        pixels.as_slice().iter().zip(reference[index].as_slice())
                    {
                        if reference.g > 160 && reference.b > 160 && reference.r < 150 {
                            assert_eq!(
                                actual, reference,
                                "{width}x{height}: lit digits remain visible in {phase:?}"
                            );
                        }
                    }
                    if phase == Some("fighting") {
                        saw_fight = true;
                        assert!(
                            pixels.as_bytes() != reference[index].as_bytes(),
                            "{width}x{height}: invisible battle in adapter {index}"
                        );
                    }
                    if phase.is_none() {
                        assert!(
                            pixels.as_bytes() == reference[index].as_bytes(),
                            "{width}x{height}: cleanup differs in adapter {index}"
                        );
                    }
                    if let Some(path) = &output {
                        write_png(
                            &path.join(format!(
                                "tanks-{width}x{height}-{}-{}.png",
                                phase.unwrap_or("finished"),
                                if vector { "vector" } else { "raster" }
                            )),
                            &pixels,
                        );
                    }
                }
            }
            if phase.is_none() {
                break;
            }
            assert!(tick < 84 * 60, "bounded departure");
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        assert!(saw_fight);
        for live in [false, true] {
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            crate::apply_scenario_metadata(&ui, "clock");
            crate::clock_controls::publish_settings(&ui, ClockSettings::default());
            ui.set_clock_event_labels(
                Rc::new(slint::VecModel::from(
                    ClockEventKind::ALL
                        .iter()
                        .map(|kind| kind.label().into())
                        .collect::<Vec<_>>(),
                ))
                .into(),
            );
            ui.set_clock_preview_index(ClockEventKind::Tanks as i32);
            if live {
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                ui.set_ingame_clock_focus_index(23);
            } else {
                ui.set_launcher_visible(true);
                ui.set_launcher_settings_visible(true);
                ui.set_launcher_settings_focus_index(22);
            }
            let pixels = capture(window, viewport);
            if let Some(path) = &output {
                write_png(
                    &path.join(format!(
                        "tanks-{width}x{height}-{}-controls.png",
                        if live { "live" } else { "launcher" }
                    )),
                    &pixels,
                );
            }
        }
    }
}
