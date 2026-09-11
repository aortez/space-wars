//! Real menu trees must repaint correctly as panels are created and removed.
use crate::MainWindow;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, Image, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
type MenuCase = (&'static str, fn(&MainWindow));

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
    ui.set_controller_disconnected_visible(false);
    ui.set_scenario_error_text("".into());
    ui.set_touch_test_visible(false);
    ui.set_sound_visible(false);
    ui.set_device_info_visible(false);
    ui.set_performance_overlay_enabled(false);
    ui.set_performance_overlay_text("".into());
    ui.set_launcher_busy(false);
    ui.set_spacewars_ui_visible(false);
    ui.set_launcher_scenario("clock".into());
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
        ("launcher-settings", |ui| {
            ui.set_launcher_visible(true);
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
        ("launcher-info", |ui| {
            ui.set_launcher_visible(true);
            ui.set_sound_visible(true);
            ui.set_device_info_visible(true);
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
    for (width, height) in [(800, 480), (480, 800)] {
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
