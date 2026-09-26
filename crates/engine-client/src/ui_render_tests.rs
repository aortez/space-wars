//! Real menu trees must repaint correctly as panels are created and removed.
use crate::MainWindow;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, Image, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
type MenuCase = (&'static str, fn(&MainWindow));

#[test]
fn clock_crow_renders_visible_perches_hops_and_shared_events_on_device_layouts() {
    use crate::render::{FrameLayout, Viewport};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use scenario_clock::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use std::time::Duration;
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let windows = windows.borrow();
    let output = std::env::var_os("SPACEWARS_CROW_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows[0].set_size(PhysicalSize::new(width, height));
        let viewport = Viewport::new(width as f32, height as f32);
        for name in ["entering", "perched", "hopping", "rain", "duck", "leaving"] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: ClockEventProfile::Off,
                    rain_amount: engine_common::ClockRainAmount::Heavy,
                    show_date: true,
                    ..Default::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(12, 34, 0)
                        .unwrap()
                        .with_date(ClockDate::new(2026, 9, 25).unwrap()),
                )],
                Duration::ZERO,
            );
            let baseline = crate::thruster_visual_tests::raster(
                &ClockScenario::render_frame(&state),
                viewport,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Crow)],
                Duration::ZERO,
            );
            let ticks = if name == "entering" { 65 } else { 105 };
            for _ in 0..ticks {
                ClockScenario::step(&mut state, &[], Duration::from_millis(16));
            }
            if name == "hopping" {
                for _ in 0..400 {
                    if state
                        .crow_state()
                        .is_some_and(|c| c.phase.as_str() == "hopping" && c.phase_tick >= 12)
                    {
                        break;
                    }
                    ClockScenario::step(&mut state, &[], Duration::from_millis(16));
                }
                assert_eq!(state.crow_state().unwrap().phase.as_str(), "hopping");
            } else if matches!(name, "rain" | "duck" | "leaving") {
                let action = match name {
                    "rain" => ClockAction::preview_event(ClockEventKind::Rain),
                    "duck" => ClockAction::toggle_player_duck(1),
                    _ => ClockAction::preview_event(ClockEventKind::Falling),
                };
                ClockScenario::step(&mut state, &[action], Duration::ZERO);
                for _ in 0..if name == "leaving" { 20 } else { 180 } {
                    ClockScenario::step(&mut state, &[], Duration::from_millis(16));
                }
            } else {
                assert_eq!(state.crow_state().unwrap().phase.as_str(), name);
            }
            assert!(state.crow_state().is_some());
            let frame = ClockScenario::render_frame(&state);
            let pixels = crate::thruster_visual_tests::raster(&frame, viewport);
            if matches!(name, "entering" | "perched" | "hopping") {
                let changed = pixels
                    .as_slice()
                    .iter()
                    .zip(baseline.as_slice())
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    changed > 80,
                    "{width}x{height} {name}: crow must reach visible pixels, got {changed}"
                );
            }
            let vector = crate::thruster_visual_tests::svg(&frame, viewport);
            assert!(!vector.contains("NaN") && !vector.contains("inf"));
            reset_panels(&ui);
            let overlay = crate::render::raster_text_overlay(
                std::slice::from_ref(&frame),
                viewport,
                FrameLayout::EqualHorizontal,
            );
            ui.set_raster_visible(true);
            ui.set_raster_frame(Image::from_rgb8(pixels));
            ui.set_primitives(Rc::new(slint::VecModel::from(overlay)).into());
            let mut screenshot = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            windows[0].request_redraw();
            windows[0].draw_if_needed(|renderer| {
                renderer.render(screenshot.make_mut_slice(), width as usize);
            });
            if let Some(output) = &output {
                let stem = format!("crow-{width}x{height}-{name}");
                crate::thruster_visual_tests::write_png(
                    &output.join(format!("{stem}.png")),
                    &screenshot,
                );
                std::fs::write(output.join(format!("{stem}.svg")), vector).unwrap();
            }
        }
    }
}

#[test]
fn clock_calendar_date_renders_in_band_through_native_text_overlay() {
    use crate::render::{FrameLayout, Viewport};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use scenario_clock::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use std::time::Duration;
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let windows = windows.borrow();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows[0].set_size(PhysicalSize::new(width, height));
        let viewport = Viewport::new(width as f32, height as f32);
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: viewport.aspect_ratio(),
                show_date: true,
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            42,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 58, 0)
                    .unwrap()
                    .with_date(ClockDate::new(2026, 9, 30).unwrap()),
            )],
            Duration::ZERO,
        );
        for (name, event) in [
            ("idle", None),
            ("rain", Some(ClockEventKind::Rain)),
            ("marquee", Some(ClockEventKind::Marquee)),
        ] {
            if let Some(event) = event {
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::preview_event(event)],
                    Duration::ZERO,
                );
                for _ in 0..120 {
                    ClockScenario::step(&mut state, &[], Duration::from_millis(16));
                }
            }
            reset_panels(&ui);
            ui.set_launcher_scenario("clock".into());
            let frame = ClockScenario::render_frame(&state);
            let frames = [frame];
            let overlay =
                crate::render::raster_text_overlay(&frames, viewport, FrameLayout::EqualHorizontal);
            let vector = crate::render::scene_presentation_from_frames_with_layout(
                &frames,
                viewport,
                FrameLayout::EqualHorizontal,
            );
            let text = overlay
                .iter()
                .find(|p| p.text == "WEDNESDAY · SEPTEMBER 30")
                .unwrap();
            assert!(vector.main_primitives.iter().any(|p| p == text));
            let pixels = crate::thruster_visual_tests::raster(&frames[0], viewport);
            ui.set_raster_visible(true);
            ui.set_raster_frame(Image::from_rgb8(pixels));
            ui.set_primitives(Rc::new(slint::VecModel::from(overlay)).into());
            let mut screenshot = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            windows[0].request_redraw();
            windows[0].draw_if_needed(|renderer| {
                renderer.render(screenshot.make_mut_slice(), width as usize);
            });
            let label: Vec<_> = screenshot
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    *i / (width as usize) < height as usize * 7 / 100
                        && p.r > 80
                        && p.g > 120
                        && p.b > 120
                })
                .map(|(i, _)| (i % width as usize, i / width as usize))
                .collect();
            assert!(label.len() > 100, "date text must reach real Slint pixels");
            assert!(
                label
                    .iter()
                    .all(|&(x, y)| x > 3 && x + 3 < width as usize && y > 1)
            );
            if let Some(directory) = std::env::var_os("SPACEWARS_CALENDAR_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                crate::thruster_visual_tests::write_png(
                    &directory.join(format!("calendar-{width}x{height}-{name}.png")),
                    &screenshot,
                );
            }
        }
    }
}

#[test]
fn retained_text_updates_match_replaced_models_and_full_repaints() {
    use crate::{PrimitiveKind, ScenePrimitive, host::update_raster_text_overlay};
    use slint::{Color, ModelRc, VecModel};
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let uis = [MainWindow::new().unwrap(), MainWindow::new().unwrap()];
    for ui in &uis {
        ui.show().unwrap();
    }
    let windows = windows.borrow();
    for (width, height) in [(320, 180), (180, 320)] {
        for window in windows.iter() {
            window.set_size(PhysicalSize::new(width, height));
        }
        let mut pixels = [
            SharedPixelBuffer::<Rgb8Pixel>::new(width, height),
            SharedPixelBuffer::<Rgb8Pixel>::new(width, height),
        ];
        let first = ScenePrimitive {
            kind: PrimitiveKind::Text,
            width: width as f32 / 2.0,
            height: height as f32,
            text: "Walking back to ship".into(),
            text_x: 10.0,
            text_y: 30.0,
            color: Color::from_rgb_u8(255, 230, 150).into(),
            font_size: 16.0,
            ..Default::default()
        };
        let mut rows = vec![
            first.clone(),
            ScenePrimitive {
                x: width as f32 / 2.0,
                text: "Other pilot".into(),
                ..first.clone()
            },
        ];
        for step in 0..10 {
            match step {
                1 => {} // Identical publication must preserve existing pixels.
                2 => rows[0].text = "Board".into(),
                3 => {
                    rows[0].text_x = -12.0;
                    rows[0].text_y = 80.0;
                }
                4 => {
                    rows[0].font_size = 24.0;
                    rows[0].color = Color::from_argb_u8(120, 20, 255, 100).into();
                }
                5 => rows.push(ScenePrimitive {
                    text_y: 120.0,
                    text: "Rebuilt".into(),
                    ..first.clone()
                }),
                6 => {
                    rows.remove(0);
                }
                7 => rows.clear(),
                8 => rows.push(first.clone()),
                9 => rows[0].width = 30.0,
                _ => {}
            }
            update_raster_text_overlay(&uis[0], rows.clone());
            uis[1].set_primitives(ModelRc::new(VecModel::from(rows.clone())));
            for index in 0..2 {
                windows[index].request_redraw();
                assert!(windows[index].draw_if_needed(|renderer| {
                    renderer.render(pixels[index].make_mut_slice(), width as usize);
                }));
            }
            assert!(
                pixels[0].as_bytes() == pixels[1].as_bytes(),
                "text step {step}, {width}×{height}: stale pixels or clipping changed"
            );
        }
    }
}

impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let mut windows = self.0.borrow_mut();
        let mode = if windows.is_empty() {
            RepaintBufferType::ReusedBuffer
        } else {
            RepaintBufferType::NewBuffer
        };
        let window = MinimalSoftwareWindow::new(mode);
        windows.push(window.clone());
        Ok(window)
    }
}

fn reset_panels(ui: &MainWindow) {
    ui.set_launcher_visible(false);
    ui.set_launcher_settings_visible(false);
    ui.set_launcher_controls_visible(false);
    ui.set_ingame_menu_visible(false);
    ui.set_ingame_controls_visible(false);
    ui.set_ingame_clock_visible(false);
    ui.set_game_over_visible(false);
    ui.set_autostart_settings_visible(false);
    ui.set_autostart_running(false);
    ui.set_autostart_caption("".into());
    ui.set_controller_disconnected_visible(false);
    ui.set_scenario_error_text("".into());
    ui.set_touch_test_visible(false);
    ui.set_sound_visible(false);
    ui.set_settings_save_error("".into());
    ui.set_settings_save_pending(false);
    ui.set_sound_focus_index(0);
    ui.set_device_info_visible(false);
    ui.set_performance_overlay_enabled(false);
    ui.set_performance_overlay_text("".into());
    ui.set_launcher_busy(false);
    ui.set_spacewars_ui_visible(false);
    ui.set_launcher_scenario("clock".into());
    ui.set_launcher_scenario_title("Clock".into());
    ui.set_launcher_focus_index(0);
}

#[test]
fn menu_lifecycles_match_full_repaints_and_preserve_root_state() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let uis = [MainWindow::new().unwrap(), MainWindow::new().unwrap()];
    let mut frame = SharedPixelBuffer::<Rgb8Pixel>::new(32, 24);
    for (index, pixel) in frame.make_mut_slice().iter_mut().enumerate() {
        *pixel = Rgb8Pixel::new((index % 32 * 7) as u8, (index / 32 * 9) as u8, 40);
    }
    for ui in &uis {
        ui.set_raster_visible(true);
        ui.set_raster_frame(Image::from_rgb8(frame.clone()));
        ui.set_launcher_clock_time_format("12-hour".into());
        ui.set_launcher_clock_event_profile("Off".into());
        ui.set_launcher_settings_focus_index(10);
        ui.set_ingame_clock_focus_index(4);
        ui.set_clock_event_labels(Rc::new(slint::VecModel::from(vec!["Falling".into()])).into());
        ui.set_scenario_controls_help("Move with the joystick. A selects. B goes back.".into());
        ui.set_sound_volume_percent(5);
        ui.set_device_info_ready(true);
        ui.set_device_info_rows(slint::ModelRc::new(slint::VecModel::from(vec![
            crate::DeviceInfoRow {
                id: "info.hostname".into(),
                label: "Device · Hostname".into(),
                value: "sw-picade-2".into(),
            },
            crate::DeviceInfoRow {
                id: "info.addresses".into(),
                label: "Network · Local addresses".into(),
                value: "wlan0: 192.168.1.142\nwlan0: fe80::1234:5678:abcd:ef12".into(),
            },
            crate::DeviceInfoRow {
                id: "info.controllers".into(),
                label: "Controllers · Current player assignments".into(),
                value: "Space-Wars Picade · Player 1\nMicrosoft X-Box 360 pad · Player 2".into(),
            },
        ])));
        ui.set_p1_name("Player 1".into());
        ui.set_p1_status("Ship Health: 80%".into());
        ui.set_p1_status_fraction(0.8);
        ui.set_p2_name("Rule Bot".into());
        ui.set_p2_status("Pod Rebuild: 30%".into());
        ui.set_p2_status_fraction(0.3);
        ui.set_spacewars_message_text("Player 1 wins!".into());
        ui.set_controller_disconnected_text("Player 2 controller disconnected".into());
        ui.set_launcher_busy_title("Opening Clock".into());
        ui.set_launcher_busy_detail("Preparing scenario".into());
        ui.show().unwrap();
    }
    let windows = windows.borrow();
    let cases: &[MenuCase] = &[
        ("gameplay", |_| {}),
        ("overlay-enabled", |ui| {
            ui.set_performance_overlay_enabled(true);
            ui.set_performance_overlay_text("FPS 60 | UPS 60".into());
        }),
        ("overlay-paused", |ui| {
            ui.set_performance_overlay_enabled(true);
            ui.set_performance_overlay_text("Paused".into());
            ui.set_ingame_menu_visible(true);
        }),
        ("overlay-disabled", |ui| {
            ui.set_performance_overlay_text("FPS 60 | UPS 60".into());
        }),
        ("launcher", |ui| ui.set_launcher_visible(true)),
        ("launcher-confirmed", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_focus_index(1);
        }),
        ("launcher-countdown", |ui| {
            ui.set_launcher_visible(true);
            ui.set_autostart_caption("Spacewars bots starts in 30 seconds".into());
        }),
        ("launcher-spacewars", |ui| {
            ui.set_launcher_scenario("spacewars".into());
            ui.set_launcher_scenario_title("Space-Wars".into());
            ui.set_launcher_seed_text(u64::MAX.to_string().into());
            ui.set_launcher_p2_controller("rule bot".into());
            ui.set_launcher_visible(true);
        }),
        ("launcher-long-title", |ui| {
            ui.set_launcher_scenario("spacewars-terrain-travel-duel".into());
            ui.set_launcher_scenario_title("Space-Wars Terrain Travel Duel".into());
            ui.set_launcher_visible(true);
        }),
        ("launcher-settings", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_settings_visible(true);
        }),
        ("launcher-match-settings", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_scenario("spacewars".into());
            ui.set_launcher_settings_visible(true);
        }),
        ("launcher-controls", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_controls_visible(true);
        }),
        ("touch-test", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_controls_visible(true);
            ui.set_touch_test_visible(true);
        }),
        ("launcher-sound", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
        }),
        ("launcher-autostart", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_autostart_settings_visible(true);
            ui.set_autostart_choice("Spacewars bots".into());
            ui.set_autostart_start_available(true);
        }),
        ("launcher-sound-autostart", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_sound_focus_index(4);
        }),
        ("launcher-autostart-save-error", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_autostart_settings_visible(true);
            ui.set_settings_save_error("Storage is not writable".into());
        }),
        ("launcher-info", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_device_info_visible(true);
        }),
        ("launcher-sound-save-error", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_settings_save_error("Storage is not writable".into());
            ui.set_sound_focus_index(6);
        }),
        ("busy", |ui| {
            ui.set_launcher_visible(true);
            ui.set_launcher_busy(true);
        }),
        ("pause", |ui| ui.set_ingame_menu_visible(true)),
        ("pause-controls", |ui| {
            ui.set_ingame_menu_visible(true);
            ui.set_ingame_controls_visible(true);
        }),
        ("pause-clock", |ui| {
            ui.set_ingame_menu_visible(true);
            ui.set_ingame_clock_visible(true);
        }),
        ("pause-sound", |ui| {
            ui.set_ingame_menu_visible(true);
            ui.set_sound_visible(true);
        }),
        ("pause-info", |ui| {
            ui.set_ingame_menu_visible(true);
            ui.set_sound_visible(true);
            ui.set_device_info_visible(true);
        }),
        ("pause-autostart", |ui| {
            ui.set_ingame_menu_visible(true);
            ui.set_sound_visible(true);
            ui.set_autostart_settings_visible(true);
        }),
        ("disconnected", |ui| {
            ui.set_controller_disconnected_visible(true)
        }),
        ("error", |ui| {
            ui.set_scenario_error_text("Test error".into())
        }),
        ("spacewars-hud", |ui| {
            ui.set_launcher_scenario("spacewars".into());
            ui.set_spacewars_ui_visible(true);
        }),
        ("game-over", |ui| {
            ui.set_launcher_scenario("spacewars".into());
            ui.set_spacewars_ui_visible(true);
            ui.set_game_over_visible(true);
        }),
        ("falling", |ui| ui.set_launcher_scenario("falling".into())),
        ("gameplay-restored", |_| {}),
    ];
    // Keep both trees alive across transitions and a resize. The first buffer
    // preserves old pixels; the reference repaints the entire window every time.
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for window in windows.iter() {
            window.set_size(PhysicalSize::new(width, height));
        }
        let mut pixels = [
            SharedPixelBuffer::<Rgb8Pixel>::new(width, height),
            SharedPixelBuffer::<Rgb8Pixel>::new(width, height),
        ];
        for (name, configure) in cases {
            for ui in &uis {
                reset_panels(ui);
                configure(ui);
            }
            slint::platform::update_timers_and_animations();
            for index in 0..2 {
                assert!(
                    windows[index].draw_if_needed(|renderer| {
                        renderer.render(pixels[index].make_mut_slice(), width as usize);
                    }),
                    "{name}: changing panels must invalidate the window"
                );
            }
            assert!(
                pixels[0].as_bytes() == pixels[1].as_bytes(),
                "{name} at {width}×{height}: partial repaint left different pixels"
            );
            for ui in &uis {
                assert_eq!(ui.get_launcher_clock_time_format(), "12-hour");
                assert_eq!(ui.get_launcher_clock_event_profile(), "Off");
                assert_eq!(ui.get_launcher_settings_focus_index(), 10);
                assert_eq!(ui.get_ingame_clock_focus_index(), 4);
                assert_eq!(ui.get_sound_volume_percent(), 5);
            }
            // Optional, local before/after visual comparison. No golden hashes
            // in CI: system fonts can differ between machines.
            if let Some(directory) = std::env::var_os("SPACEWARS_MENU_TEST_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                let file =
                    std::fs::File::create(directory.join(format!("{width}-{height}-{name}.png")))
                        .unwrap();
                let mut encoder = png::Encoder::new(file, width, height);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(pixels[0].as_bytes())
                    .unwrap();
            }
        }
    }
}
