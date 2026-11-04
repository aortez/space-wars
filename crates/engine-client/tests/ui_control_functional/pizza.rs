use super::*;
use spacewars_control::{InputButton, InputPressRequest};

const GRAVITY: &str = "pause.pizza.gravity";
const LIMIT: &str = "launcher.settings.pizza.live-ball-limit.next";
const AUTOMATIC: &str = "launcher.settings.pizza.automatic-events.next";

fn status(h: &mut FunctionalHarness) -> Value {
    let text = h
        .client
        .request_before("status\n", request_deadline())
        .unwrap();
    let json = text
        .lines()
        .find_map(|line| line.strip_prefix("pizza="))
        .expect("live Pizza status");
    serde_json::from_str(json).unwrap()
}
fn screen(h: &mut FunctionalHarness, screen: UiScreen) -> UiState {
    h.wait_for(
        UiStatePredicate {
            screen: Some(screen),
            ..Default::default()
        },
        TRANSITION_TIMEOUT,
    )
}
fn pause(h: &mut FunctionalHarness) -> UiState {
    let game = h.state();
    h.pause_guarded(&game);
    screen(h, UiScreen::PauseMain)
}
fn controller(h: &mut FunctionalHarness, state: &UiState, button: InputButton) -> UiState {
    h.client
        .input_press_before(&InputPressRequest::new(state, button), request_deadline())
        .unwrap()
        .state
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn pizza_event_controls_settings_and_lifecycle_smoke() {
    let mut settings = engine_common::Settings::default();
    settings.pizza.desired_balls = 0;
    settings.audio.master_volume = 0.05;
    settings.video.width = 800;
    settings.video.height = 480;
    run_functional_test_with_settings(
        "pizza-event-controls",
        "winit-software",
        7,
        Some(settings),
        |h| {
            let page = h.wait_until_ready();
            let page = h.activate_until_scenario("pizza", page);
            let page = h.activate_guarded("launcher.settings", &page);
            assert_eq!(control_value(&page, AUTOMATIC), Some("On"));
            let page = h.activate_guarded(AUTOMATIC, &page);
            assert_eq!(control_value(&page, AUTOMATIC), Some("Off"));
            assert_eq!(control_value(&page, LIMIT), Some("500"));
            let page = h.activate_guarded(LIMIT, &page);
            let page = h.activate_guarded(LIMIT, &page);
            let page = h.activate_guarded(LIMIT, &page);
            assert_eq!(control_value(&page, LIMIT), Some("5000"));
            assert_eq!(
                control_value(&page, "launcher.settings.pizza.desired-balls.next"),
                Some("0")
            );
            h.capture_screenshot("capacity-5000.png");
            h.activate_guarded("launcher.settings.start", &page);
            let game = screen(h, UiScreen::Gameplay);
            let revision = game.scenario_revision;
            let page = pause(h);
            assert!(control_enabled(&page, GRAVITY));
            let idle = status(h);
            assert_eq!(idle["population"]["limit"], 5000);
            assert_eq!(idle["event"]["phase"], "idle");
            assert!(idle["event"]["automatic_wait_ticks"].is_null());
            h.capture_screenshot("gravity-ready.png");
            // D-pad Up wraps from Resume to the extra row, A uses the same action.
            let page = controller(h, &page, InputButton::Up);
            assert_eq!(selected_control(&page), GRAVITY);
            controller(h, &page, InputButton::South);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            let active = status(h);
            assert_eq!(page.scenario_revision, revision);
            assert_eq!(active["event"]["phase"], "active");
            assert_eq!(active["event"]["trigger"], "manual");
            assert_eq!(
                active["ball_count"], 0,
                "menu activation leaked to playfield"
            );
            assert!(!control_enabled(&page, GRAVITY));
            let rejected =
                h.expect_activate_failure(GRAVITY, Some(page.screen), Some(page.revision));
            assert_eq!(rejected.code, ControlFailureCode::ControlDisabled);
            let controls = h.activate_guarded("pause.controls", &page);
            for _ in 0..5 {
                assert_eq!(status(h), active);
            }
            let page = h.activate_guarded("pause.controls.back", &controls);
            h.capture_screenshot("gravity-active-paused.png");
            h.activate_guarded("pause.resume", &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            let continued = status(h);
            assert_eq!(
                continued["event"]["direction"],
                active["event"]["direction"]
            );
            assert!(continued["tick"].as_u64().unwrap() >= active["tick"].as_u64().unwrap());
            h.activate_guarded("pause.restart", &page);
            let game = screen(h, UiScreen::Gameplay);
            assert_ne!(game.scenario_revision, revision);
            assert_eq!(status(h)["event"]["phase"], "idle");
            let page = pause(h);
            h.activate_guarded("pause.benchmark", &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            assert!(!control_ids(&page).contains(&GRAVITY));
            assert_eq!(status(h)["event"]["phase"], "disabled");
            h.activate_guarded("pause.restart", &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            h.activate_guarded(GRAVITY, &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            h.activate_guarded("pause.return-to-launcher", &page);
            let page = screen(h, UiScreen::LauncherMain);
            let text = h
                .client
                .request_before("status\n", request_deadline())
                .unwrap();
            assert!(!text.lines().any(|line| line.starts_with("pizza=")));
            let page = h.activate_until_scenario("rover-lab", page);
            h.activate_guarded("launcher.start", &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            assert!(!control_ids(&page).contains(&GRAVITY));
            h.activate_guarded("pause.return-to-launcher", &page);
            let page = screen(h, UiScreen::LauncherMain);
            let page = h.activate_until_scenario("pizza", page);
            h.activate_guarded("launcher.start", &page);
            screen(h, UiScreen::Gameplay);
            let idle = status(h);
            assert_eq!(idle["population"]["limit"], 5000);
            assert_eq!(idle["event"]["phase"], "idle");
            assert!(idle["event"]["pending_manual"].is_null());
            assert!(idle["event"]["automatic_wait_ticks"].is_null());
            let page = pause(h);
            h.activate_guarded("pause.return-to-launcher", &page);
            let page = screen(h, UiScreen::LauncherMain);
            h.activate_guarded("launcher.quit", &page);
            let deadline = Instant::now() + TRANSITION_TIMEOUT;
            while h.child.0.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(POLL_INTERVAL);
            }
            let path = h.run_path().join("config/settings.toml");
            let saved: engine_common::Settings =
                toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            assert!(!saved.pizza.automatic_events);
            assert_eq!(saved.pizza.live_ball_limit, 5000);
            assert_eq!(saved.pizza.desired_balls, 0);
            assert_eq!(saved.audio.master_volume, 0.05);
            let log = File::create(h.run_path().join("restarted-client.log")).unwrap();
            h.child = OwnedChild(
                Command::new(env!("CARGO_BIN_EXE_engine-client"))
                    .arg("--config-dir")
                    .arg(h.run_path().join("config"))
                    .env("SPACEWARS_CONTROL_SOCKET", &h._socket_path.0)
                    .env("SLINT_BACKEND", "winit-software")
                    .env_remove("WAYLAND_DISPLAY")
                    .env(
                        "DBUS_SYSTEM_BUS_ADDRESS",
                        "unix:path=/nonexistent-pizza-test-bus",
                    )
                    .stdin(Stdio::null())
                    .stderr(Stdio::from(log.try_clone().unwrap()))
                    .stdout(Stdio::from(log))
                    .spawn()
                    .unwrap(),
            );
            let page = h.wait_until_ready();
            let page = h.activate_until_scenario("pizza", page);
            let page = h.activate_guarded("launcher.settings", &page);
            assert_eq!(control_value(&page, AUTOMATIC), Some("Off"));
            assert_eq!(control_value(&page, LIMIT), Some("5000"));
            let page = h.activate_guarded(AUTOMATIC, &page);
            h.activate_guarded("launcher.settings.start", &page);
            screen(h, UiScreen::Gameplay);
            assert!(status(h)["event"]["automatic_wait_ticks"].is_u64());
        },
    );
}

#[test]
#[ignore = "requires an explicit display; full UI suite includes real event completion"]
fn pizza_event_completes_without_queuing_busy_requests() {
    let mut settings = engine_common::Settings::default();
    settings.pizza.automatic_events = false;
    settings.video.width = 1024;
    settings.video.height = 768;
    run_functional_test_with_settings(
        "pizza-event-completion",
        "winit-femtovg",
        7,
        Some(settings),
        |h| {
            let page = h.wait_until_ready();
            let page = h.activate_until_scenario("pizza", page);
            h.activate_guarded("launcher.start", &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            h.activate_guarded(GRAVITY, &page);
            screen(h, UiScreen::Gameplay);
            let page = pause(h);
            assert_eq!(status(h)["event"]["phase"], "active");
            h.expect_activate_failure(GRAVITY, Some(page.screen), Some(page.revision));
            h.capture_screenshot("gravity-active-paused-gpu.png");
            h.activate_guarded("pause.resume", &page);
            screen(h, UiScreen::Gameplay);
            let deadline = Instant::now() + Duration::from_secs(45);
            loop {
                let current = status(h);
                if current["event"]["phase"] == "idle" {
                    break;
                }
                assert!(Instant::now() < deadline, "event did not finish: {current}");
                if current["event"]["elapsed_ticks"].as_u64().unwrap() >= 120
                    && !h.run_path().join("gravity-running-gpu.png").exists()
                {
                    h.capture_screenshot("gravity-running-gpu.png");
                }
                thread::sleep(Duration::from_millis(100));
            }
            let page = pause(h);
            assert!(control_enabled(&page, GRAVITY));
            let idle = status(h);
            assert!(idle["event"]["pending_manual"].is_null());
            assert!(idle["event"]["automatic_wait_ticks"].is_null());
            h.capture_screenshot("gravity-finished-gpu.png");
        },
    );
}
