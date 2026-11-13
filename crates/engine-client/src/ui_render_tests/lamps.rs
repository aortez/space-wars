//! Fixed-reading captures of the corner lamps in the device-sized views,
//! drawn through the production raster adapter and exported through the
//! vector one: the lamps at rest, a ball rolled across the floor that the
//! left lamp follows and hands to the right one, the right lamp presenting
//! the minutes, and an outage's sparks and dark corner. Each ends on exactly
//! the resting face. Then the Lamps control on the live Clock page and in the
//! launcher's Clock settings. A sample always runs; the stills, the controls
//! and every second frame of the 800x480 film are written out when
//! `SPACEWARS_LAMPS_ARTIFACTS` names a directory, for the docs' pictures.
use super::*;
use crate::{
    render::{FrameLayout, Viewport},
    thruster_visual_tests::{raster, svg, write_png},
};
use engine_common::{
    ClockEventKind, ClockEventProfile, ClockLampActivity, ClockLampBrainState, ClockLampSide,
    ClockSettings, RenderPrimitive, Scenario,
};
use scenario_clock::{
    ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario, ClockState,
};
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

/// What one capture runs: a ball rolled across the floor, a present of the
/// minutes, or an outage.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Scene {
    Ball,
    Present,
    Outage,
}

fn resting(lamp: &ClockLampBrainState) -> bool {
    lamp.state == ClockLampActivity::Rest && lamp.narrow_milli == 0 && lamp.brightness_milli == 400
}

/// Starts `scene`: rolls a ball from the left, or has the right lamp
/// present the minutes or go out.
fn start(state: &mut ClockState, scene: Scene) {
    match scene {
        Scene::Ball => {
            let ball = state.rolling_arena_ball(3.0, 15.0);
            state
                .admit_arena_fixture(&[ball])
                .expect("the arena takes the ball");
        }
        Scene::Present => state.preview_lamp(ClockLampActivity::Present, ClockLampSide::Right),
        Scene::Outage => state.preview_lamp(ClockLampActivity::Out, ClockLampSide::Right),
    }
}

fn has_sparks(frame: &engine_common::RenderFrame) -> bool {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == 11)
        .any(|layer| {
            layer
                .primitives
                .iter()
                .any(|primitive| matches!(primitive, RenderPrimitive::Line(_)))
        })
}

#[test]
fn lamps_render_rest_track_present_and_outage_and_come_back_at_target_sizes() {
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
        // Profile Off: no event comes along, and the lamps still act.
        let config = ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            show_date: true,
            ..Default::default()
        };
        let reading = ClockReading::new(12, 39, 50)
            .unwrap()
            .with_date(ClockDate::new(2026, 11, 10).unwrap());
        // At this seed the left lamp is eager.
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
        let primitives = |frame: &engine_common::RenderFrame| {
            frame
                .layers
                .iter()
                .map(|layer| layer.primitives.len())
                .sum::<usize>()
        };
        traces.push(serde_json::json!({
            "width": width,
            "height": height,
            "seed": seed,
            "still": "steady",
            "lamps": state.lamps_state(),
            "primitives": primitives(&frame),
        }));
        let mut films = 0;
        for scene in [Scene::Ball, Scene::Present, Scene::Outage] {
            // Half of outages spark; a few more tries find one that does.
            let tries = if scene == Scene::Outage { 6 } else { 1 };
            let mut sparked = false;
            for _ in 0..tries {
                start(&mut state, scene);
                let (mut narrowest, mut dark, mut tick) = ([0u32; 2], 0, 0);
                loop {
                    let lamps = state.lamps_state();
                    let sides = [lamps.left, lamps.right];
                    // Done once both rest and the ball has left the view.
                    if tick > 0 && sides.iter().all(resting) && state.arena_fixtures().is_empty() {
                        break;
                    }
                    for (lamp, side) in sides.iter().enumerate() {
                        let narrow = match scene {
                            Scene::Present => 850,
                            _ => 1000,
                        };
                        narrowest[lamp] = if side.narrow_milli == narrow {
                            narrowest[lamp] + 1
                        } else {
                            0
                        };
                    }
                    dark = if sides[1].state == ClockLampActivity::Out
                        && sides[1].brightness_milli == 0
                    {
                        dark + 1
                    } else {
                        0
                    };
                    let still = match scene {
                        Scene::Ball if narrowest[0] == 20 => Some("track"),
                        Scene::Present if narrowest[1] == 20 => Some("present"),
                        Scene::Outage if dark == 9 => Some("sparks"),
                        Scene::Outage if dark == 40 && sparked => Some("outage"),
                        _ => None,
                    };
                    let film = width == 800 && output.is_some() && tick % 2 == 0;
                    if still.is_some() || film || tick % 60 == 0 {
                        let frame = ClockScenario::render_frame(&state);
                        // Only an outage that sparks makes the stills.
                        let still = match still {
                            Some("sparks") if !has_sparks(&frame) => None,
                            Some("sparks") => {
                                sparked = true;
                                still
                            }
                            other => other,
                        };
                        let pixels = draw(&ui, window, &frame, viewport);
                        if tick % 60 == 0 {
                            let vector = svg(&frame, viewport);
                            assert!(!vector.contains("NaN") && !vector.contains("inf"));
                        }
                        if let Some(name) = still {
                            assert!(
                                pixels.as_bytes() != steady.as_bytes(),
                                "{width}x{height}: {name} draws the resting lamps"
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
                                "lamps": lamps,
                                "primitives": primitives(&frame),
                            }));
                        }
                        if film && let Some(path) = &output {
                            write_png(&path.join(format!("film/frame-{films:04}.png")), &pixels);
                            films += 1;
                        }
                    }
                    ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                    tick += 1;
                    assert!(tick < 20 * 60, "{width}x{height}: {scene:?} never ended");
                }
                // Each leaves nothing behind: the lamps rest again.
                let pixels = draw(&ui, window, &ClockScenario::render_frame(&state), viewport);
                assert!(
                    pixels.as_bytes() == steady.as_bytes(),
                    "{width}x{height}: the lamps are not at rest after {scene:?}"
                );
                if scene != Scene::Outage || sparked {
                    break;
                }
            }
            if scene == Scene::Outage {
                assert!(
                    sparked,
                    "{width}x{height}: no outage sparked in {tries} tries"
                );
            }
        }
        // The ball handed over: the right lamp followed it too.
        assert_eq!(state.lamps_state().handoffs, 1, "{width}x{height}");
        // The Lamps control, selected, on the live Clock page and in the
        // launcher's Clock settings, beside the profile's longest value.
        for live in [false, true] {
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            crate::apply_scenario_metadata(&ui, "clock");
            crate::clock_controls::publish_settings(
                &ui,
                ClockSettings {
                    event_profile: ClockEventProfile::Demo,
                    ..ClockSettings::default()
                },
            );
            assert_eq!(ui.get_launcher_clock_lamps(), "Lively");
            assert_eq!(ui.get_launcher_clock_event_profile(), "Demo");
            ui.set_clock_event_labels(
                Rc::new(slint::VecModel::from(
                    ClockEventKind::ALL
                        .iter()
                        .map(|kind| kind.label().into())
                        .collect::<Vec<_>>(),
                ))
                .into(),
            );
            if live {
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                ui.set_ingame_clock_focus_index(24);
            } else {
                ui.set_launcher_visible(true);
                ui.set_launcher_settings_visible(true);
                ui.set_launcher_settings_focus_index(23);
            }
            let pixels = capture(window, viewport);
            if let Some(path) = &output {
                let page = if live { "live" } else { "launcher" };
                write_png(
                    &path.join(format!("lamps-{width}x{height}-{page}-controls.png")),
                    &pixels,
                );
            }
        }
    }
    assert_eq!(traces.len(), 15, "a still was missed: {traces:?}");
    if let Some(path) = &output {
        std::fs::write(
            path.join("captures.json"),
            serde_json::to_vec_pretty(&traces).unwrap(),
        )
        .unwrap();
    }
}
