//! Real menu trees must repaint correctly as panels are created and removed.
use crate::MainWindow;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, Image, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
type MenuCase = (&'static str, fn(&MainWindow));

#[test]
fn scoreboard_renders_and_buttons_work_on_device_layouts() {
    use crate::scoreboard::{RoundResult, Scoreboard};
    use scenario_spacewars::{PlayerId, surface_sortie::match_rules::MatchOutcome};
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::cell::Cell;
    use std::time::Duration;

    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let clicked = Rc::new(Cell::new(None));
    let next = clicked.clone();
    ui.on_ingame_restart(move || next.set(Some(0)));
    let next = clicked.clone();
    ui.on_ingame_new_match(move || next.set(Some(1)));
    let next = clicked.clone();
    ui.on_ingame_return_launcher(move || next.set(Some(2)));
    let output = std::env::var_os("SPACEWARS_SCOREBOARD_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows.borrow()[0].set_size(PhysicalSize::new(width, height));
        reset_panels(&ui);
        ui.set_launcher_scenario("spacewars".into());
        ui.set_launcher_seed_text(u64::MAX.to_string().into());
        ui.set_game_over_visible(true);
        let mut scores = Scoreboard::default();
        let mut result = RoundResult {
            outcome: MatchOutcome::Winner(PlayerId::PLAYER_1),
            elapsed: Duration::from_secs(135),
            planets: [2, 1],
            controllers: ["Human".into(), "Planner bot".into()],
        };
        for revision in 1..=12 {
            scores.record(revision, &result);
        }
        for (name, outcome, message) in [
            (
                "winner",
                MatchOutcome::Winner(PlayerId::PLAYER_2),
                "Player 2 wins / time limit / more planets owned",
            ),
            ("draw", MatchOutcome::Draw, "Draw / both pilots lost"),
        ] {
            result.outcome = outcome;
            result.planets = [0, 2];
            let revision = if name == "winner" { 13 } else { 14 };
            scores.record(revision, &result);
            scores.publish(&ui, &result);
            ui.set_spacewars_message_text(message.into());
            slint::platform::update_timers_and_animations();
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            windows.borrow()[0].request_redraw();
            windows.borrow()[0].draw_if_needed(|renderer| {
                renderer.render(pixels.make_mut_slice(), width as usize);
            });
            if let Some(path) = &output {
                crate::thruster_visual_tests::write_png(
                    &path.join(format!("{name}-{width}x{height}.png")),
                    &pixels,
                );
            }
            let panel_width = (width as f32 - 32.0).min(640.0);
            for button in 0..3 {
                clicked.set(None);
                let x = (width as f32 - panel_width) / 2.0
                    + 24.0
                    + button as f32 * (panel_width - 38.0) / 3.0
                    + (panel_width - 68.0) / 6.0;
                let y = height as f32 / 2.0 - 212.0 + 364.0;
                for event in [
                    WindowEvent::PointerPressed {
                        position: slint::LogicalPosition::new(x, y),
                        button: PointerEventButton::Left,
                    },
                    WindowEvent::PointerReleased {
                        position: slint::LogicalPosition::new(x, y),
                        button: PointerEventButton::Left,
                    },
                ] {
                    ui.window().dispatch_event(event);
                }
                assert_eq!(
                    clicked.get(),
                    Some(button),
                    "{name} {width}x{height} button {button}"
                );
            }
        }
    }
}

#[test]
fn controller_setup_renders_on_picade_and_hyperpixel_layouts() {
    use crate::controller_controls::{self, Device};
    use crate::controller_profile::RawState;
    use crate::input::GamepadSeatInput;
    use crate::settings_writer::SettingsWriter;
    use engine_common::{ControllerControl, Settings};
    use std::sync::{Arc, RwLock};
    use std::time::Instant;

    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = controller_controls::install(
        &ui,
        Arc::new(RwLock::new(Settings::default())),
        SettingsWriter::new(directory.path().join("settings.toml")).unwrap(),
        crate::input::new_shared_input().1,
    );
    for (id, name) in [(0, "Space-Wars Picade"), (1, "Xbox 360 USB Controller")] {
        state.borrow_mut().connect(Device {
            id,
            name: name.into(),
            key: format!("gilrs-v1:linux:{id}"),
            seat: Some(id),
        });
    }
    ui.set_launcher_visible(true);
    ui.set_sound_visible(true);
    let output = std::env::var_os("SPACEWARS_CONTROLLER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows.borrow()[0].set_size(PhysicalSize::new(width, height));
        // ReusedBuffer repaints only dirty regions. Retain the other pixels
        // across small updates such as changing a player label or selection.
        let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
        ui.invoke_controllers_open();
        for stage in [
            "devices",
            "device",
            "assigned",
            "device-bottom",
            "capture",
            "review",
            "trial",
            "tester",
            "settings",
        ] {
            match stage {
                "device" => ui.invoke_controllers_command("controllers.device.0".into()),
                "assigned" => ui.invoke_controllers_command("controllers.assign-p2".into()),
                "device-bottom" => ui.set_controllers_focus_index(5),
                "capture" => ui.invoke_controllers_command("controllers.remap".into()),
                "review" => {
                    for (code, _) in ControllerControl::ALL.iter().enumerate() {
                        let mut state = state.borrow_mut();
                        state.observe(
                            0,
                            &RawState::default(),
                            &GamepadSeatInput::default(),
                            None,
                            Instant::now(),
                        );
                        state.observe(
                            0,
                            &RawState {
                                buttons: vec![(code as u32, 1.0)],
                                ..Default::default()
                            },
                            &GamepadSeatInput::default(),
                            Some(true),
                            Instant::now(),
                        );
                        state.observe(
                            0,
                            &RawState::default(),
                            &GamepadSeatInput::default(),
                            Some(false),
                            Instant::now(),
                        );
                    }
                    state.borrow_mut().tick(&ui, Instant::now());
                }
                "trial" => ui.invoke_controllers_command("controllers.try".into()),
                "tester" => {
                    ui.invoke_controllers_command("controllers.back".into());
                    ui.invoke_controllers_command("controllers.test".into());
                    state.borrow_mut().observe(
                        0,
                        &RawState::default(),
                        &GamepadSeatInput::default(),
                        None,
                        Instant::now(),
                    );
                    state.borrow_mut().observe(
                        0,
                        &RawState {
                            buttons: vec![(304, 1.0), (308, 1.0)],
                            axes: vec![(0, -1.0)],
                            ..Default::default()
                        },
                        &GamepadSeatInput {
                            south: true,
                            west: true,
                            left_stick_x: -1.0,
                            ..Default::default()
                        },
                        Some(true),
                        Instant::now(),
                    );
                    state
                        .borrow_mut()
                        .tick(&ui, Instant::now() + std::time::Duration::from_millis(101));
                }
                "settings" => {
                    ui.set_controllers_visible(false);
                    ui.set_sound_focus_index(7);
                }
                _ => {}
            }
            // The real event loop runs deferred property-change handlers,
            // including scrolling a newly focused controller row into view.
            slint::platform::update_timers_and_animations();
            windows.borrow()[0].request_redraw();
            windows.borrow()[0].draw_if_needed(|renderer| {
                renderer.render(pixels.make_mut_slice(), width as usize);
            });
            assert!(
                pixels
                    .as_slice()
                    .iter()
                    .filter(|pixel| pixel.r > 150 && pixel.g > 150 && pixel.b > 150)
                    .count()
                    > 300,
                "missing menu text for {stage}"
            );
            if stage == "device-bottom" {
                let selected_pixels = pixels
                    .as_slice()
                    .chunks(width as usize)
                    .skip(height as usize / 2)
                    .flatten()
                    .filter(|pixel| {
                        pixel.r < 70 && pixel.g > 60 && pixel.b > 90 && pixel.b > pixel.g
                    })
                    .count();
                assert!(
                    selected_pixels > 1000,
                    "focused Back row did not scroll into view at {width}x{height}"
                );
            }
            if let Some(output) = &output {
                crate::thruster_visual_tests::write_png(
                    &output.join(format!("controllers-{width}x{height}-{stage}.png")),
                    &pixels,
                );
            }
        }
    }
}

#[test]
fn clock_explosion_captures_warning_burst_reformation_and_controls_on_device_layouts() {
    use crate::render::{FrameLayout, Viewport};
    use engine_common::{
        ClockEventKind, ClockEventProfile, ClockFont, ClockFontSettings, ClockTimeFormat, Scenario,
    };
    use scenario_clock::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use std::time::Duration;
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let windows = windows.borrow();
    let output = std::env::var_os("SPACEWARS_EXPLOSION_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows[0].set_size(PhysicalSize::new(width, height));
        let viewport = Viewport::new(width as f32, height as f32);
        for (name, elapsed) in [
            ("warning", 18),
            ("burst", 54),
            ("bounce", 170),
            ("return-start", 246),
            ("return-early", 270),
            ("reforming", 288),
            ("return-late", 324),
            ("recovered", 336),
            ("retargeted", 306),
            ("retargeted-recovered", 336),
            ("serif-reforming", 288),
            ("serif-retargeted", 306),
            ("serif-retargeted-recovered", 336),
            ("duck", 90),
            ("controls", 18),
        ] {
            let font = if name.starts_with("serif-") {
                ClockFont::Serif
            } else {
                ClockFont::Classic
            };
            let mut state = ClockScenario::init(
                ClockConfig {
                    fonts: ClockFontSettings {
                        selected: font,
                        ..Default::default()
                    },
                    aspect_ratio: viewport.aspect_ratio(),
                    event_profile: ClockEventProfile::Off,
                    time_format: ClockTimeFormat::TwelveHour,
                    show_date: true,
                    ..Default::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(12, 58, 0)
                        .unwrap()
                        .with_date(ClockDate::new(2026, 9, 26).unwrap()),
                )],
                Duration::ZERO,
            );
            let baseline = crate::thruster_visual_tests::raster(
                &ClockScenario::render_frame(&state),
                viewport,
            );
            if name == "duck" {
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::toggle_player_duck(1)],
                    Duration::ZERO,
                );
                for _ in 0..90 {
                    ClockScenario::step(&mut state, &[], Duration::from_millis(16));
                }
            }
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Explosion)],
                Duration::ZERO,
            );
            for tick in 0..elapsed {
                if name.contains("retargeted") && tick == 270 {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::set_reading(
                            ClockReading::new(0, 11, 0)
                                .unwrap()
                                .with_date(ClockDate::new(2026, 9, 27).unwrap()),
                        )],
                        Duration::ZERO,
                    );
                }
                ClockScenario::step(&mut state, &[], Duration::from_millis(16));
            }
            let frame = ClockScenario::render_frame(&state);
            let pixels = crate::thruster_visual_tests::raster(&frame, viewport);
            let changed = pixels
                .as_slice()
                .iter()
                .zip(baseline.as_slice())
                .filter(|(a, b)| a != b)
                .count();
            if name == "recovered" {
                assert_eq!(changed, 0);
            } else {
                assert!(changed > 100);
            }
            if name.ends_with("retargeted-recovered") {
                let mut clean = ClockScenario::init(
                    ClockConfig {
                        fonts: ClockFontSettings {
                            selected: font,
                            ..Default::default()
                        },
                        aspect_ratio: viewport.aspect_ratio(),
                        event_profile: ClockEventProfile::Off,
                        time_format: ClockTimeFormat::TwelveHour,
                        show_date: true,
                        ..Default::default()
                    },
                    42,
                );
                ClockScenario::step(
                    &mut clean,
                    &[ClockAction::set_reading(state.reading().unwrap())],
                    Duration::ZERO,
                );
                assert_eq!(frame, ClockScenario::render_frame(&clean));
            }
            let vector = crate::thruster_visual_tests::svg(&frame, viewport);
            assert!(!vector.contains("NaN") && !vector.contains("inf"));
            reset_panels(&ui);
            ui.set_raster_visible(true);
            ui.set_raster_frame(Image::from_rgb8(pixels));
            ui.set_primitives(
                Rc::new(slint::VecModel::from(crate::render::raster_text_overlay(
                    std::slice::from_ref(&frame),
                    viewport,
                    FrameLayout::EqualHorizontal,
                )))
                .into(),
            );
            if name == "controls" {
                ui.set_launcher_scenario("clock".into());
                ui.set_ingame_menu_visible(true);
                ui.set_ingame_clock_visible(true);
                ui.set_ingame_clock_focus_index(15);
                ui.set_clock_event_labels(
                    Rc::new(slint::VecModel::from(
                        ClockEventKind::ALL
                            .iter()
                            .map(|kind| kind.label().into())
                            .collect::<Vec<_>>(),
                    ))
                    .into(),
                );
                ui.set_clock_preview_index(ClockEventKind::Explosion as i32);
            }
            let mut screenshot = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            windows[0].request_redraw();
            windows[0].draw_if_needed(|renderer| {
                renderer.render(screenshot.make_mut_slice(), width as usize);
            });
            if let Some(output) = &output {
                let stem = format!("explosion-{width}x{height}-{name}");
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

#[test]
fn clock_fonts_picker_and_faces_render_and_select_across_device_layouts() {
    use crate::render::Viewport;
    use engine_common::{ClockEventProfile, ClockFont, ClockFontSettings, Scenario};
    use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario};
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(TestPlatform(windows.clone()))).unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    crate::clock_fonts::install(&ui, crate::host::new_scenario_controls());
    let output = std::env::var_os("SPACEWARS_FONT_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        windows.borrow()[0].set_size(PhysicalSize::new(width, height));
        reset_panels(&ui);
        ui.set_launcher_visible(true);
        ui.set_launcher_settings_visible(true);
        crate::clock_fonts::publish(&ui, ClockFontSettings::default());
        assert!(crate::ui_activation::activate(
            &ui,
            "launcher.settings.clock.fonts"
        ));
        assert!(ui.get_clock_fonts_visible());
        assert!(crate::ui_activation::activate(
            &ui,
            "clock.fonts.select.serif"
        ));
        assert_eq!(
            crate::clock_fonts::settings(&ui).unwrap().selected,
            ClockFont::Serif
        );
        assert!(crate::ui_activation::activate(
            &ui,
            "clock.fonts.pool.classic"
        ));
        assert!(
            !crate::clock_fonts::settings(&ui)
                .unwrap()
                .pool
                .contains(ClockFont::Classic)
        );
        assert!(crate::ui_activation::activate(&ui, "clock.fonts.rotate"));
        assert!(crate::clock_fonts::settings(&ui).unwrap().rotate);
        let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
        windows.borrow()[0].request_redraw();
        windows.borrow()[0].draw_if_needed(|renderer| {
            renderer.render(pixels.make_mut_slice(), width as usize);
        });
        if let Some(path) = &output {
            crate::thruster_visual_tests::write_png(
                &path.join(format!("picker-{width}x{height}.png")),
                &pixels,
            );
        }
        assert!(crate::ui_activation::activate(&ui, "clock.fonts.back"));
        assert!(!ui.get_clock_fonts_visible());
        let mut faces = Vec::new();
        for font in ClockFont::ALL {
            let mut state = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: width as f32 / height as f32,
                    fonts: ClockFontSettings {
                        selected: font,
                        ..Default::default()
                    },
                    event_profile: ClockEventProfile::Off,
                    ..Default::default()
                },
                42,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(23, 58, 0).unwrap(),
                )],
                std::time::Duration::ZERO,
            );
            let frame = ClockScenario::render_frame(&state);
            let viewport = Viewport::new(width as f32, height as f32);
            let pixels = crate::thruster_visual_tests::raster(&frame, viewport);
            assert!(
                faces
                    .iter()
                    .all(|old: &SharedPixelBuffer<Rgb8Pixel>| old.as_slice() != pixels.as_slice())
            );
            let svg = crate::thruster_visual_tests::svg(&frame, viewport);
            assert!(!svg.contains("NaN") && !svg.contains("inf"));
            if let Some(path) = &output {
                crate::thruster_visual_tests::write_png(
                    &path.join(format!(
                        "{}-{width}x{height}.png",
                        font.label().to_ascii_lowercase()
                    )),
                    &pixels,
                );
            }
            faces.push(pixels);
        }
    }
}

fn reset_panels(ui: &MainWindow) {
    ui.set_launcher_visible(false);
    ui.set_launcher_settings_visible(false);
    ui.set_launcher_controls_visible(false);
    ui.set_ingame_menu_visible(false);
    ui.set_ingame_controls_visible(false);
    ui.set_ingame_clock_visible(false);
    ui.set_clock_fonts_visible(false);
    ui.set_game_over_visible(false);
    ui.set_scoreboard_visible(false);
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
    ui.set_controllers_visible(false);
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
