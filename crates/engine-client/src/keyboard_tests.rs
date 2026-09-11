//! Exercise real Slint key dispatch without a Winit adapter, as on LinuxKMS.
use super::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, PlatformError, WindowAdapter, WindowEvent};
use std::cell::Cell;

struct TestPlatform;
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
    }
}

fn key(window: &MainWindow, text: impl Into<SharedString>) {
    slint::platform::update_timers_and_animations();
    let text = text.into();
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn scenario_confirmation_focuses_play_without_changing_the_selection() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    install_ui_navigation(&window);
    install_keyboard_navigation(
        &window,
        Rc::new(RefCell::new(input::ClientInput::default())),
    );
    window.set_launcher_scenarios(ModelRc::new(VecModel::from(vec![
        "spacewars".into(),
        "clock".into(),
        "pizza".into(),
    ])));
    window.set_launcher_scenario("spacewars".into());
    window.set_launcher_seed_text("12345".into());
    window.set_launcher_visible(true);
    let starts = Rc::new(Cell::new(0));
    let started = Rc::clone(&starts);
    window.on_launcher_start_game(move || started.set(started.get() + 1));
    window.show().unwrap();

    key(&window, Key::RightArrow);
    assert_eq!(window.get_launcher_scenario(), "clock");
    key(&window, Key::LeftArrow);
    assert_eq!(window.get_launcher_scenario(), "spacewars");
    click(&window, 725.0, 125.0);
    assert_eq!(window.get_launcher_scenario(), "clock");
    click(&window, 75.0, 125.0);
    assert_eq!(window.get_launcher_scenario(), "spacewars");
    assert_eq!(window.get_launcher_focus_index(), 0);

    // Exercise both launcher layouts and the shared controller action as well
    // as real Slint keyboard dispatch. Confirmation must not start or browse.
    for scenario in ["spacewars", "clock"] {
        for keyboard in [true, false] {
            window.set_launcher_scenario(scenario.into());
            window.set_launcher_focus_index(0);
            let before = starts.get();
            if keyboard {
                key(&window, Key::Return);
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyPressRepeated {
                        text: Key::Return.into(),
                    });
            } else {
                window.invoke_ui_action(UiAction::Confirm.code());
            }
            assert_eq!(window.get_launcher_focus_index(), 1);
            assert_eq!(starts.get(), before);
            assert_eq!(window.get_launcher_scenario(), scenario);
            assert_eq!(window.get_launcher_seed_text(), "12345");

            if keyboard {
                key(&window, Key::Return);
            } else {
                window.invoke_ui_action(UiAction::Confirm.code());
            }
            assert_eq!(starts.get(), before + 1);

            window.set_launcher_focus_index(0);
            window.invoke_ui_action(UiAction::Start.code());
            assert_eq!(starts.get(), before + 2);
            assert_eq!(window.get_launcher_scenario(), scenario);
        }
    }
}

#[test]
fn automatic_activity_consumes_keyboard_and_touch_before_exposing_the_launcher() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    let directory = tempfile::tempdir().unwrap();
    let settings = Arc::new(RwLock::new(Settings::default()));
    let writer =
        settings_writer::SettingsWriter::new(directory.path().join("settings.toml")).unwrap();
    let catalog = Rc::new(RefCell::new(nes_roms::NesRomCatalog::new(directory.path())));
    let (input, _) = input::new_shared_input();
    let launcher = launcher::Launcher::new(
        &window,
        Rc::new(RefCell::new(None)),
        host::new_scenario_controls(),
        Rc::clone(&input),
        Arc::clone(&settings),
        Rc::clone(&catalog),
        writer.clone(),
    );
    install_ui_navigation(&window);
    install_keyboard_navigation(&window, input);
    let _timer = autostart::install(&window, launcher, settings, writer, catalog, true);
    let exits = Rc::new(Cell::new(0));
    let exited = Rc::clone(&exits);
    let weak = window.as_weak();
    window.on_ingame_return_launcher(move || {
        exited.set(exited.get() + 1);
        let window = weak.upgrade().unwrap();
        window.set_launcher_visible(true);
        window.set_launcher_focus_index(1);
    });
    let starts = Rc::new(Cell::new(0));
    let started = Rc::clone(&starts);
    window.on_launcher_start_game(move || started.set(started.get() + 1));
    window.show().unwrap();
    window.set_launcher_visible(false);
    window.set_autostart_running(true);
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Return.into(),
    });
    assert!(window.get_launcher_visible());
    assert_eq!(exits.get(), 1);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated {
            text: Key::Return.into(),
        });
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Return.into(),
    });
    assert_eq!(starts.get(), 0, "exit input must not start a manual game");
    key(&window, Key::Return);
    assert_eq!(starts.get(), 1, "fresh input remains usable");

    // A release delivered outside the window must not leave consumed input stuck.
    window.set_launcher_visible(false);
    window.set_autostart_running(true);
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Return.into(),
    });
    window.global::<UserActivity>().set_pointer_held(true);
    window.global::<UserActivity>().invoke_focus_lost();
    assert!(!window.global::<UserActivity>().get_pointer_held());
    key(&window, Key::Return);
    assert_eq!(starts.get(), 2, "fresh input works after focus loss");

    window.set_launcher_visible(false);
    window.set_autostart_running(true);
    click(&window, 400.0, 280.0);
    assert_eq!(exits.get(), 3);
    assert!(!window.get_autostart_running());
    assert_eq!(
        starts.get(),
        2,
        "pointer release must not click the newly exposed menu"
    );

    // The new App Settings list must reveal Auto-start below Device Info,
    // even when a short window cannot show all rows at once.
    window.set_sound_visible(true);
    window.set_sound_focus_index(3);
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 360.0));
    key(&window, Key::DownArrow);
    assert_eq!(window.get_sound_focus_index(), 4);
    window.window().take_snapshot().unwrap();
    click(&window, 400.0, 190.0);
    assert!(window.get_autostart_settings_visible());
    assert_eq!(window.get_autostart_focus_index(), 0);
    key(&window, Key::DownArrow);
    key(&window, Key::RightArrow);
    assert_eq!(window.get_autostart_delay(), "60");
    // The child's Back action stays below its rows and returns one level.
    click(&window, 600.0, 288.0);
    assert!(!window.get_autostart_settings_visible());
    assert!(window.get_sound_visible());
    assert_eq!(window.get_sound_focus_index(), 4);
    assert_eq!(exits.get(), 3);
    window.window().take_snapshot().unwrap();
    click(&window, 400.0, 288.0);
    assert!(!window.get_sound_visible());
}

#[test]
fn backend_neutral_keyboard_reaches_clock_settings_and_does_not_repeat_shortcuts() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    let input = Rc::new(RefCell::new(input::ClientInput::default()));
    install_ui_navigation(&window);
    install_keyboard_navigation(&window, Rc::clone(&input));
    window.set_launcher_visible(true);
    window.show().unwrap();
    key(&window, Key::DownArrow);
    assert_eq!(window.get_launcher_focus_index(), 1);
    let starts = Rc::new(Cell::new(0));
    let started = Rc::clone(&starts);
    window.on_launcher_start_game(move || started.set(started.get() + 1));
    key(&window, Key::Return);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated {
            text: Key::Return.into(),
        });
    assert_eq!(starts.get(), 1);

    window.set_launcher_scenario("clock".into());
    window.set_launcher_visible(false);
    key(&window, "p");
    assert!(input.borrow_mut().take_pause_requested());
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated { text: "p".into() });
    assert!(!input.borrow_mut().take_pause_requested());
    key(&window, Key::Escape);
    assert!(input.borrow_mut().take_back_requested());
    key(&window, Key::F1);
    assert!(input.borrow_mut().take_controls_requested());

    window.set_ingame_menu_visible(true);
    key(&window, Key::DownArrow);
    key(&window, Key::DownArrow);
    assert_eq!(window.get_ingame_menu_focus_index(), 4);
    key(&window, Key::Return);
    assert!(window.get_ingame_clock_visible());
    let adjusted = Rc::new(Cell::new(None));
    let adjustment = Rc::clone(&adjusted);
    window.on_ingame_clock_adjust(move |index, delta| adjustment.set(Some((index, delta))));
    key(&window, Key::RightArrow);
    assert_eq!(adjusted.get(), Some((0, 1)));
    adjusted.set(None);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated {
            text: Key::RightArrow.into(),
        });
    assert_eq!(adjusted.get(), None);
    key(&window, Key::DownArrow);
    key(&window, Key::DownArrow);
    key(&window, Key::RightArrow);
    assert_eq!(window.get_ingame_clock_focus_index(), 3);
    key(&window, Key::Return);
    assert_eq!(adjusted.get(), Some((3, 1)));
    let resumes = Rc::new(Cell::new(0));
    let resumed = Rc::clone(&resumes);
    window.on_ingame_resume(move || resumed.set(resumed.get() + 1));
    key(&window, Key::Escape);
    assert!(!window.get_ingame_clock_visible());
    assert_eq!(resumes.get(), 0);
    key(&window, Key::Escape);
    assert_eq!(resumes.get(), 1);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated {
            text: Key::Escape.into(),
        });
    assert_eq!(resumes.get(), 1);
    key(&window, "R");
    assert!(input.borrow_mut().take_reset_requested());
    key(&window, "q");
    assert!(input.borrow_mut().take_return_launcher_requested());

    // Full-screen keyboard focus must not steal pointer hits from sibling UI.
    window.set_ingame_menu_visible(false);
    let opens = Rc::new(Cell::new(0));
    let opened = Rc::clone(&opens);
    window.on_ingame_clock_open(move || opened.set(opened.get() + 1));
    click(&window, 710.0, 34.0);
    assert_eq!(opens.get(), 1);
    window.set_ingame_menu_visible(true);
    window.set_ingame_clock_visible(true);
    // Pi-sized page: left-hand event switch and time-format row are hittable.
    click(&window, 175.0, 222.0);
    assert_eq!(adjusted.get(), Some((2, 1)));
    click(&window, 400.0, 222.0);
    assert_eq!(adjusted.get(), Some((7, 1)));
    click(&window, 510.0, 222.0);
    assert_eq!(adjusted.get(), Some((8, 1)));
    click(&window, 625.0, 222.0);
    assert_eq!(adjusted.get(), Some((11, 1)));
    adjusted.set(None);
    click(&window, 649.0, 110.0);
    assert_eq!(adjusted.get(), Some((0, 1)));
    click(&window, 175.0, 334.0);
    assert_eq!(adjusted.get(), Some((9, 1)));
    click(&window, 649.0, 334.0);
    assert_eq!(adjusted.get(), Some((10, 1)));
    // The extra launcher row must not overlap Back/Start at 800×480.
    window.set_ingame_menu_visible(false);
    window.set_launcher_visible(true);
    window.set_launcher_settings_visible(true);
    assert!(window.get_launcher_clock_digit_slide_enabled());
    click(&window, 728.0, 168.0);
    assert!(!window.get_launcher_clock_digit_slide_enabled());
    assert_eq!(window.get_launcher_settings_focus_index(), 3);
    key(&window, Key::DownArrow);
    assert_eq!(window.get_launcher_settings_focus_index(), 4);
    assert!(window.get_launcher_clock_meltdown_enabled());
    click(&window, 372.0, 324.0);
    assert!(!window.get_launcher_clock_meltdown_enabled());
    assert!(window.get_launcher_settings_visible());
    assert_eq!(window.get_launcher_settings_focus_index(), 7);
    assert!(window.get_launcher_clock_duck_enabled());
    click(&window, 728.0, 324.0);
    assert!(!window.get_launcher_clock_duck_enabled());
    assert_eq!(window.get_launcher_settings_focus_index(), 8);
    click(&window, 372.0, 376.0);
    assert!(!window.get_launcher_clock_marquee_enabled());
    assert_eq!(window.get_launcher_settings_focus_index(), 9);
    click(&window, 728.0, 376.0);
    assert_eq!(window.get_launcher_clock_marquee_preset(), "Clock spin");
    assert_eq!(window.get_launcher_settings_focus_index(), 10);
    click(&window, 100.0, 428.0);
    assert!(!window.get_launcher_settings_visible());
}

fn click(window: &MainWindow, x: f32, y: f32) {
    slint::platform::update_timers_and_animations();
    let position = slint::LogicalPosition::new(x, y);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
}

#[test]
fn recreated_launcher_buttons_do_not_retain_pressed_state_or_lose_shortcuts() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    let input = Rc::new(RefCell::new(input::ClientInput::default()));
    install_ui_navigation(&window);
    install_keyboard_navigation(&window, Rc::clone(&input));
    let starts = Rc::new(Cell::new(0));
    let started = Rc::clone(&starts);
    window.on_launcher_start_game(move || started.set(started.get() + 1));
    window.show().unwrap();

    for cycle in 0..3 {
        window.set_launcher_visible(true);
        window.set_launcher_focus_index(0);
        slint::platform::update_timers_and_animations();
        let position = slint::LogicalPosition::new(220.0, 278.0);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
        // Remove the menu while its Start button has a pointer grab. A later
        // release must not activate the old button or latch the recreated one.
        window.set_launcher_visible(false);
        slint::platform::update_timers_and_animations();
        // This headless adapter has no render loop. Commit the conditional
        // tree removal before testing input against the newly displayed tree.
        window.window().take_snapshot().unwrap();
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            });
        assert_eq!(starts.get(), cycle * 2);

        window.set_launcher_visible(true);
        click(&window, 220.0, 278.0);
        assert_eq!(starts.get(), cycle * 2 + 1);
        window.set_launcher_focus_index(0);
        key(&window, Key::DownArrow);
        assert_eq!(window.get_launcher_focus_index(), 1);
        key(&window, Key::Return);
        assert_eq!(starts.get(), cycle * 2 + 2);
        window.set_launcher_visible(false);
        key(&window, "p");
        assert!(input.borrow_mut().take_pause_requested());
    }
}

#[test]
fn sound_keyboard_touch_and_menu_actions_share_persistent_controls() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let settings = Arc::new(RwLock::new(Settings::default()));
    let writer = settings_writer::SettingsWriter::new(path.clone()).unwrap();
    let _status = settings_writer::install_status(&window, writer.clone());
    let input = Rc::new(RefCell::new(input::ClientInput::default()));
    install_ui_navigation(&window);
    install_keyboard_navigation(&window, input);
    sound_controls::install(
        &window,
        host::new_scenario_controls(),
        Arc::clone(&settings),
        writer.clone(),
    );
    device_info::install(&window, directory.path().to_path_buf()).unwrap();
    let resumes = Rc::new(Cell::new(0));
    let resumed = Rc::clone(&resumes);
    window.on_ingame_resume(move || resumed.set(resumed.get() + 1));
    window.set_launcher_visible(true);
    window.show().unwrap();

    key(&window, Key::DownArrow);
    key(&window, Key::DownArrow);
    key(&window, Key::RightArrow);
    key(&window, Key::RightArrow);
    assert_eq!(window.get_launcher_focus_index(), 5);
    key(&window, Key::Return);
    assert!(window.get_sound_visible());
    assert_eq!(window.get_sound_volume_percent(), 25);
    key(&window, Key::RightArrow);
    assert_eq!(window.get_sound_volume_percent(), 30);
    // Touch hits the same shared callbacks; no platform-specific key injection.
    click(&window, 710.0, 120.0);
    assert_eq!(window.get_sound_volume_percent(), 35);
    click(&window, 400.0, 182.0);
    assert!(window.get_sound_muted());
    click(&window, 400.0, 244.0);
    assert!(window.get_performance_overlay_enabled());
    key(&window, Key::DownArrow);
    assert_eq!(window.get_sound_focus_index(), 3);
    let audio_before_info = settings.read().unwrap().audio;
    key(&window, Key::Return);
    assert!(window.get_device_info_visible());
    pump_until(|| window.get_device_info_ready());
    // The minimal non-Winit test platform has no draw loop. Realize the new
    // scroll layout before using its bounds, just as a displayed frame does.
    window.window().take_snapshot().unwrap();
    pump_until(|| window.get_device_info_scroll_limit() > 0.0);
    key(&window, Key::DownArrow);
    assert!(window.get_device_info_scroll_offset() < 0.0);
    // Touch Back belongs to Info, not the covered launcher or App Settings.
    click(&window, 250.0, 410.0);
    assert!(!window.get_device_info_visible());
    assert!(window.get_sound_visible());
    assert_eq!(window.get_sound_focus_index(), 3);
    assert_eq!(settings.read().unwrap().audio, audio_before_info);
    assert_eq!(resumes.get(), 0);
    // A shorter window forces the settings body to scroll. Its focused Device
    // Info row is revealed after resizing and remains a real touch target.
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 360.0));
    window.window().take_snapshot().unwrap();
    click(&window, 400.0, 190.0);
    assert!(window.get_device_info_visible());
    key(&window, Key::Escape);
    assert!(window.get_sound_visible());
    assert_eq!(window.get_sound_focus_index(), 3);
    window.window().take_snapshot().unwrap();
    // Back remains below the scroll area, even when the last row is selected.
    click(&window, 400.0, 288.0);
    assert!(!window.get_sound_visible());
    assert_eq!(resumes.get(), 0);
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    window.invoke_sound_open();
    window.set_sound_focus_index(3);
    key(&window, Key::UpArrow);
    assert_eq!(window.get_sound_focus_index(), 2);
    key(&window, Key::LeftArrow);
    assert!(!window.get_performance_overlay_enabled());
    key(&window, Key::RightArrow);
    assert!(window.get_performance_overlay_enabled());
    key(&window, Key::Escape);
    assert!(!window.get_sound_visible());
    assert_eq!(resumes.get(), 0);

    window.set_launcher_visible(false);
    window.set_launcher_scenario("falling".into());
    window.set_ingame_menu_visible(true);
    key(&window, Key::DownArrow);
    key(&window, Key::DownArrow);
    assert_eq!(window.get_ingame_menu_focus_index(), 4);
    key(&window, Key::Return);
    assert!(window.get_sound_visible());
    assert_eq!(window.get_sound_volume_percent(), 35);
    assert!(window.get_sound_muted());
    assert!(window.get_performance_overlay_enabled());
    let snapshot = settings.read().unwrap().clone();
    writer.save_blocking(snapshot).unwrap();
    pump_until(|| !window.get_settings_save_pending());

    if let Some(path) = std::env::var_os("SPACEWARS_TEST_SOUND_SCREENSHOT") {
        let snapshot = window.window().take_snapshot().unwrap();
        let mut encoder = png::Encoder::new(
            std::fs::File::create(path).unwrap(),
            snapshot.width(),
            snapshot.height(),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(snapshot.as_bytes())
            .unwrap();
    }

    // A failed save must not prevent adjustment, backing out, or retrying.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    window.invoke_ui_action(UiAction::Right.code());
    pump_until(|| !window.get_settings_save_pending());
    assert_eq!(settings.read().unwrap().audio.master_volume, 0.40);
    assert!(!window.get_settings_save_error().is_empty());
    std::fs::remove_dir(&path).unwrap();
    window.set_sound_focus_index(6);
    window.invoke_ui_action(UiAction::Confirm.code());
    pump_until(|| !window.get_settings_save_pending());
    assert!(window.get_settings_save_error().is_empty());
    let saved = settings::load_settings(&path).unwrap().settings;
    assert_eq!(saved.audio.master_volume, 0.40);
    assert!(saved.audio.muted);
    assert!(saved.video.show_fps);

    // Confirm repeats must never toggle mute repeatedly; Start resumes only in-game.
    window.set_sound_focus_index(1);
    key(&window, Key::Return);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressRepeated {
            text: Key::Return.into(),
        });
    assert!(!window.get_sound_muted());
    key(&window, Key::Escape);
    assert_eq!(resumes.get(), 0);
    window.invoke_sound_open();
    window.invoke_ui_action(UiAction::Start.code());
    assert_eq!(resumes.get(), 1);
    assert!(!window.get_sound_visible());
    let snapshot = settings.read().unwrap().clone();
    writer.save_blocking(snapshot).unwrap();
}

#[test]
fn keyboard_return_to_launcher_releases_input_before_invoking_ui_callback() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    window.set_launcher_visible(false);
    let input = Rc::new(RefCell::new(input::ClientInput::default()));
    let controls = host::new_scenario_controls();
    install_keyboard_navigation(&window, Rc::clone(&input));
    let returned = Rc::new(Cell::new(false));
    let return_flag = Rc::clone(&returned);
    let callback_input = Rc::clone(&input);
    window.on_ingame_return_launcher(move || {
        // The normal launcher callback clears the shared input too.
        callback_input.borrow_mut().clear();
        return_flag.set(true);
    });
    window.show().unwrap();
    let timer = host::start_scenario_loop(
        &window,
        "clock",
        4242,
        host::ScenarioLoopOptions {
            renderer: host::RenderBackend::Raster,
            raster_scale: 2.0,
            input: Some(Rc::clone(&input)),
            controls: Some(Rc::clone(&controls)),
            ..Default::default()
        },
    )
    .unwrap();
    key(&window, "p");
    pump_until(|| {
        controls
            .borrow()
            .clock_state()
            .is_some_and(|state| state.paused)
    });
    key(&window, "q");
    pump_until(|| returned.get());
    timer.stop();
}

fn pump_until(predicate: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "Slint host transition timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
        slint::platform::update_timers_and_animations();
    }
}
