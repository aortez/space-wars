use super::*;

fn run(
    name: &'static str,
    settings: engine_common::Settings,
    test: impl FnOnce(&mut FunctionalHarness),
) {
    let mut settings = settings;
    settings.launch.renderer = engine_common::RendererSetting::Raster;
    settings.launch.raster_scale = 2.0;
    settings.video.width = 800;
    settings.video.height = 480;
    let mut h =
        FunctionalHarness::spawn_configured(name, "winit-software", 4242, Some(settings)).unwrap();
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| test(&mut h))) {
        h.failure = Some(panic_message(payload.as_ref()));
        drop(h);
        resume_unwind(payload);
    }
}

fn wait_screen(h: &mut FunctionalHarness, screen: UiScreen) -> UiState {
    h.wait_for(
        UiStatePredicate {
            screen: Some(screen),
            ..Default::default()
        },
        TRANSITION_TIMEOUT,
    )
}

fn status(h: &FunctionalHarness) -> String {
    h.client.request("status\n").unwrap()
}

fn saved(h: &FunctionalHarness) -> engine_common::Settings {
    toml::from_str(&fs::read_to_string(h.run_path().join("config/settings.toml")).unwrap()).unwrap()
}

fn assert_stays(h: &mut FunctionalHarness, screen: UiScreen, duration: Duration) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        assert_eq!(h.state().screen, screen);
        thread::sleep(Duration::from_millis(100));
    }
}

fn preferences(h: &mut FunctionalHarness, root: &UiState) -> UiState {
    let app = h.activate_guarded("launcher.sound", root);
    h.activate_guarded("settings.autostart", &app)
}

fn wait_saved(h: &mut FunctionalHarness) -> UiState {
    let deadline = Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let state = h.state();
        if control_value(&state, "autostart.save-status") == Some("saved") {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "auto-start settings failed to save"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn back(h: &mut FunctionalHarness, settings: &UiState) -> UiState {
    h.activate_guarded("autostart.back", settings);
    let app = h.wait_sound_save("saved");
    assert_eq!(app.screen, UiScreen::LauncherSound);
    h.activate_guarded("sound.back", &app)
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn autostart_clock_is_sticky_preserves_preferences_and_yields_without_click_through() {
    let mut settings = engine_common::Settings::default();
    settings.audio.master_volume = 0.05;
    settings.audio.muted = true;
    run("autostart-clock", settings, |h| {
        let root = h.wait_until_ready();
        let before = saved(h);
        let mut p = preferences(h, &root);
        assert_eq!(control_value(&p, "autostart.activity.next"), Some("Off"));
        h.activate_guarded("autostart.activity.next", &p);
        p = wait_saved(h);
        for _ in 0..2 {
            h.activate_guarded("autostart.delay.previous", &p);
            p = wait_saved(h);
        }
        assert_eq!(control_value(&p, "autostart.delay.next"), Some("5"));
        h.capture_screenshot("autostart-settings.png");
        assert_stays(h, UiScreen::LauncherAutostart, Duration::from_secs(6));
        p = h.state();
        let root = back(h, &p);
        // Countdown text is telemetry, so observation must not invalidate controls.
        assert_stays(h, UiScreen::LauncherMain, Duration::from_secs(3));
        assert_eq!(h.state().revision, root.revision);
        h.press_guarded(UiAction::Down, &root);
        assert_stays(h, UiScreen::LauncherMain, Duration::from_secs(3));
        let game = wait_screen(h, UiScreen::Gameplay);
        assert_eq!(game.active_scenario.as_deref(), Some("clock"));
        h.capture_screenshot("automatic-clock.png");
        h.pause_guarded(&game);
        let pause = wait_screen(h, UiScreen::PauseMain);
        assert_stays(h, UiScreen::PauseMain, Duration::from_secs(6));
        let root = h.activate_guarded("pause.return-to-launcher", &pause);
        assert_eq!(root.screen, UiScreen::LauncherMain);
        assert!(saved(h).autostart.enabled);
        assert_eq!(saved(h).spacewars, before.spacewars);
        assert_eq!(saved(h).launch, before.launch);
        assert_eq!(saved(h).last_scenario, before.last_scenario);
        assert_eq!(saved(h).audio, before.audio);
        // Exit gracefully, then relaunch the same preferences in a new process.
        h.activate_guarded("launcher.quit", &root);
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        while h.child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            thread::sleep(POLL_INTERVAL);
        }
        let log = File::create(h.run_path().join("restarted-client.log")).unwrap();
        h.child = OwnedChild(
            Command::new(env!("CARGO_BIN_EXE_engine-client"))
                .arg("--config-dir")
                .arg(h.run_path().join("config"))
                .env("SPACEWARS_CONTROL_SOCKET", &h._socket_path.0)
                .env("SLINT_BACKEND", "winit-software")
                .env_remove("WAYLAND_DISPLAY")
                .stdin(Stdio::null())
                .stdout(Stdio::from(log.try_clone().unwrap()))
                .stderr(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        h.wait_until_ready();
        let game = wait_screen(h, UiScreen::Gameplay);
        assert_eq!(game.active_scenario.as_deref(), Some("clock"));
        h.pause_guarded(&game);
        let pause = wait_screen(h, UiScreen::PauseMain);
        let root = h.activate_guarded("pause.return-to-launcher", &pause);
        let mut p = preferences(h, &root);
        h.activate_guarded("autostart.activity.previous", &p);
        p = wait_saved(h);
        assert_eq!(control_value(&p, "autostart.activity.next"), Some("Off"));
        back(h, &p);
        assert_stays(h, UiScreen::LauncherMain, Duration::from_secs(6));
        assert!(!saved(h).autostart.enabled);
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn autostart_bot_matches_use_match_results_repeat_fresh_worlds_and_preserve_humans() {
    let mut settings = engine_common::Settings::default();
    settings.audio.muted = true;
    settings.autostart.enabled = true;
    settings.autostart.activity = "spacewars-bots".into();
    settings.autostart.delay_seconds = 5;
    settings.spacewars_match.time_limit_seconds = 3;
    run("autostart-bots", settings, |h| {
        h.wait_until_ready();
        let before = saved(h);
        let first = wait_screen(h, UiScreen::Gameplay);
        h.pause_guarded(&first);
        wait_screen(h, UiScreen::PauseMain);
        let paused = status(h);
        assert!(paused.contains("match_player_1=rule_bot"));
        assert!(paused.contains("match_player_2=rule_bot"));
        let remaining = paused
            .lines()
            .find(|line| line.starts_with("match_remaining_seconds="))
            .unwrap()
            .to_owned();
        assert_stays(h, UiScreen::PauseMain, Duration::from_secs(4));
        assert!(status(h).contains(&remaining));
        // Host resume is an explicit control operation, without simulated player input.
        let pause = h.state();
        h.activate_guarded("pause.resume", &pause);
        let mut instances = BTreeSet::new();
        let mut worlds = BTreeSet::new();
        for round in 0..3 {
            let result = wait_screen(h, UiScreen::GameOver);
            assert!(instances.insert(result.scenario_revision.unwrap()));
            let diagnostics = status(h);
            assert!(diagnostics.contains("match_finish_reason=Some(TimeLimit)"));
            h.capture_screenshot(&format!("result-{round}.png"));
            let value = control_value(&result, "game-over.world-seed").unwrap();
            assert!(worlds.insert(value.to_owned()));
            assert_eq!(saved(h).spacewars, before.spacewars);
            assert_eq!(saved(h).launch, before.launch);
            assert_eq!(saved(h).spacewars_match, before.spacewars_match);
            if round < 2 {
                // Wait for the next running instance before looking for its result.
                wait_screen(h, UiScreen::Gameplay);
            } else {
                assert!(diagnostics.contains("autostart_repeats=2"));
                let root = h.press_guarded(UiAction::Start, &result);
                assert_eq!(root.screen, UiScreen::LauncherMain);
                assert_stays(h, UiScreen::LauncherMain, Duration::from_secs(2));
            }
        }
        let root = h.state();
        let p = preferences(h, &root);
        h.activate_guarded("autostart.activity.next", &p);
        let p = wait_saved(h);
        back(h, &p);
        assert_stays(h, UiScreen::LauncherMain, Duration::from_secs(6));
        assert!(!saved(h).autostart.enabled);
    });
}
