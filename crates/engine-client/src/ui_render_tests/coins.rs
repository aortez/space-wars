//! Fixed-reading captures of the Coins event through the production raster
//! adapter and the real UI: the lit cells turning about their axes, the
//! cascade, the pile, the magpie's first trip and the return in the
//! device-sized views, and the pause and launcher pages that hold the
//! switch. A sample always runs; the stills and every third frame of the
//! 800x480 film are written out when `SPACEWARS_COINS_ARTIFACTS` names a
//! directory, for the docs' pictures.
use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{ClockEventKind, ClockEventProfile, ClockSettings, Scenario};
use scenario_clock::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
use std::{collections::BTreeSet, time::Duration};

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
fn coins_render_the_turn_cascade_pile_return_and_controls_at_target_sizes() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(CapturePlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let output = std::env::var_os("SPACEWARS_COINS_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path.join("film")).unwrap();
    }
    let mut traces = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        let viewport = Viewport::new(width as f32, height as f32);
        let windows = windows.borrow();
        let window = &windows[0];
        window.set_size(PhysicalSize::new(width, height));
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            show_date: true,
            ..Default::default()
        };
        let reading = ClockReading::new(12, 39, 50)
            .unwrap()
            .with_date(ClockDate::new(2026, 11, 7).unwrap());
        let seed = 7;
        // The ordinary face with this reading, which the event must give back.
        let mut reference = ClockScenario::init(config, seed);
        ClockScenario::step(
            &mut reference,
            &[ClockAction::set_reading(reading)],
            Duration::ZERO,
        );
        let face = draw(
            &ui,
            window,
            &ClockScenario::render_frame(&reference),
            viewport,
        );
        let mut state = ClockScenario::init(config, seed);
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(reading),
                ClockAction::preview_event(ClockEventKind::Coins),
            ],
            Duration::ZERO,
        );
        assert_eq!(state.event_kind(), Some(ClockEventKind::Coins));
        let coins = state.coins_state().unwrap();
        assert!(
            coins.cells > 0 && coins.cells <= coins.max_cells,
            "{coins:?}"
        );
        let mut phases = BTreeSet::new();
        let mut shot = BTreeSet::new();
        let mut films = 0;
        let mut recovered_at = None;
        // The whole event and a second of the recovered face.
        for tick in 0u64.. {
            let phase = state.event_phase().map(|phase| phase.as_str());
            if let Some(phase) = phase {
                phases.insert(phase);
            } else if recovered_at.is_none() {
                recovered_at = Some(tick);
            }
            if recovered_at.is_some_and(|start| tick == start + 60) {
                let pixels = draw(&ui, window, &ClockScenario::render_frame(&state), viewport);
                assert!(
                    pixels.as_bytes() == face.as_bytes(),
                    "{width}x{height}: the recovered face differs"
                );
                break;
            }
            assert!(tick < 40 * 60, "{width}x{height}: the event never ended");
            // The magpie's stills are its first trip: just landed by the
            // pile, the coin in its beak with its head still down, and on
            // its way out with it.
            let magpie = state.coins_state().and_then(|coins| coins.magpie);
            let first_trip = |phase: &str, at: u64| {
                magpie.filter(|bird| {
                    bird.phase.as_str() == phase && bird.phase_tick == at && bird.trips == 0
                })
            };
            let still = match (phase, state.phase_tick()) {
                (Some("minting"), 18) => Some("minting"),
                (Some("cascading"), 30) => Some("cascading"),
                (Some("cascading"), 230) => Some("piled"),
                (Some("gathering"), _) if first_trip("landed", 2).is_some() => Some("landed"),
                (Some("gathering"), _) if first_trip("taking", 26).is_some() => Some("taking"),
                (Some("gathering"), _) if first_trip("leaving", 30).is_some() => Some("carrying"),
                (Some("reforming"), 45) => Some("reforming"),
                _ => None,
            };
            // The bird lands again after a hop, and its still is the first
            // landing: each still is taken once.
            let still = still.filter(|name| shot.insert(*name));
            let film = width == 800 && output.is_some() && tick % 3 == 0;
            if still.is_some() || film || tick % 60 == 0 {
                let frame = ClockScenario::render_frame(&state);
                let pixels = draw(&ui, window, &frame, viewport);
                if tick % 60 == 0 {
                    let vector = svg(&frame, viewport);
                    assert!(!vector.contains("NaN") && !vector.contains("inf"));
                }
                if let Some(name) = still {
                    assert!(
                        pixels.as_bytes() != face.as_bytes(),
                        "{width}x{height}: {name} draws the ordinary face"
                    );
                    if let Some(path) = &output {
                        write_png(
                            &path.join(format!("coins-{width}x{height}-{name}.png")),
                            &pixels,
                        );
                    }
                    traces.push(serde_json::json!({
                        "width": width,
                        "height": height,
                        "seed": seed,
                        "tick": tick,
                        "still": name,
                        "coins": state.coins_state(),
                        "bodies": state.body_count(),
                        "primitives": frame.layers.iter().map(|layer| layer.primitives.len()).sum::<usize>(),
                    }));
                }
                if film {
                    if let Some(path) = &output {
                        write_png(&path.join(format!("film/frame-{films:04}.png")), &pixels);
                    }
                    films += 1;
                }
            }
            ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        for phase in ["minting", "cascading", "gathering", "reforming"] {
            assert!(
                phases.contains(phase),
                "{width}x{height}: no {phase}: {phases:?}"
            );
        }
        assert_eq!(traces.len() % 7, 0, "{width}x{height}: a still was missed");
        // Screens include the actual menu tree, not a reconstructed mockup.
        for live in [false, true] {
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            crate::apply_scenario_metadata(&ui, "clock");
            crate::clock_controls::publish_settings(
                &ui,
                ClockSettings {
                    show_date: true,
                    ..Default::default()
                },
            );
            ui.set_clock_event_labels(
                Rc::new(slint::VecModel::from(
                    ClockEventKind::ALL
                        .iter()
                        .map(|kind| kind.label().into())
                        .collect::<Vec<_>>(),
                ))
                .into(),
            );
            ui.set_clock_preview_index(ClockEventKind::Coins as i32);
            if live {
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                ui.set_ingame_clock_focus_index(22);
            } else {
                ui.set_launcher_visible(true);
                ui.set_launcher_settings_visible(true);
                ui.set_launcher_settings_focus_index(21);
            }
            let pixels = capture(window, viewport);
            if let Some(path) = &output {
                write_png(
                    &path.join(format!(
                        "coins-{width}x{height}-{}-controls.png",
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
