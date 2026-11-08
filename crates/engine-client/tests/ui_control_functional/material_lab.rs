use super::*;
use spacewars_control::{InputButton, InputPressRequest};
fn press(h: &mut FunctionalHarness, s: &mut UiState, b: InputButton) {
    *s = h
        .client
        .input_press_before(
            &InputPressRequest::new(s, b),
            Instant::now() + Duration::from_secs(4),
        )
        .unwrap()
        .state;
    thread::sleep(Duration::from_millis(150));
}
fn choose(h: &mut FunctionalHarness, s: &mut UiState, target: u64) {
    let mut current = snapshot(h, |_| true)["control"].as_u64().unwrap();
    while current != target {
        press(h, s, InputButton::Down);
        current = (current + 1) % 6;
    }
    press(h, s, InputButton::South);
}
fn snapshot(h: &mut FunctionalHarness, predicate: impl Fn(&Value) -> bool) -> Value {
    let until = Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let status = h
            .client
            .request_before("status\n", Instant::now() + Duration::from_secs(2))
            .unwrap();
        if let Some(line) = status.lines().find_map(|l| l.strip_prefix("material_lab=")) {
            let value: Value = serde_json::from_str(line).unwrap();
            if predicate(&value) {
                return value;
            }
        }
        assert!(Instant::now() < until, "material state: {status}");
        thread::sleep(Duration::from_millis(30));
    }
}
fn physics(v: &Value) -> Value {
    json!({"case":v["case"],"steps":v["steps"],"summary":v["summary"],"remaining":v["remaining"],"events":v["events"],"impacts":v["impacts"],"separations":v["separations"]})
}
fn launch(h: &mut FunctionalHarness, mut s: UiState, renderer: &str) -> UiState {
    s = h.activate_until_scenario("material-lab", s);
    s = h.activate_guarded("launcher.settings", &s);
    if control_value(&s, "launcher.settings.renderer.next") != Some(renderer) {
        s = h.activate_guarded("launcher.settings.renderer.next", &s);
    }
    h.activate_guarded("launcher.settings.start", &s);
    h.wait_for(
        UiStatePredicate {
            screen: Some(UiScreen::Gameplay),
            scenario: Some("material-lab".into()),
            ..Default::default()
        },
        TRANSITION_TIMEOUT,
    )
}
#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_lab_pause_save_load_and_both_renderers() {
    run_functional_test_with_seed("material-lab", "winit-femtovg", 42, |h| {
        let ready = h.wait_until_ready();
        let mut s = launch(h, ready, "vector");
        let initial = snapshot(h, |v| v["paused"] == true);
        assert_eq!(initial["steps"], 0);
        h.capture_screenshot("material-initial-vector.png");
        choose(h, &mut s, 0);
        snapshot(h, |v| v["steps"].as_u64().unwrap() > 210);
        h.pause_guarded(&s);
        let paused = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        let before = snapshot(h, |_| true);
        thread::sleep(Duration::from_millis(350));
        assert_eq!(physics(&snapshot(h, |_| true)), physics(&before));
        h.activate_guarded("pause.resume", &paused);
        s = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::Gameplay),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        choose(h, &mut s, 4);
        let saved = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Saved")
        });
        assert_eq!(saved["paused"], true);
        assert!(saved["summary"]["damage_work"].as_f64().unwrap() > 0.0);
        assert!(h.run_path().join("config/material-lab.bin").is_file());
        h.capture_screenshot("material-saved-vector.png");
        choose(h, &mut s, 1);
        let next = snapshot(h, |v| {
            v["steps"].as_u64() == Some(saved["steps"].as_u64().unwrap() + 1)
        });
        choose(h, &mut s, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&saved), physics(&loaded));
        choose(h, &mut s, 1);
        assert_eq!(
            physics(&snapshot(h, |v| v["steps"] == next["steps"])),
            physics(&next)
        );
        h.pause_guarded(&s);
        let paused = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        h.activate_guarded("pause.return-to-launcher", &paused);
        let ready = h.wait_until_ready();
        // Quit and restart the process using the same configuration and saved slot.
        h.activate_guarded("launcher.quit", &ready);
        let until = Instant::now() + TRANSITION_TIMEOUT;
        while h.child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until);
            thread::sleep(POLL_INTERVAL);
        }
        let log = File::create(h.run_path().join("restarted-client.log")).unwrap();
        h.child = OwnedChild(
            Command::new(env!("CARGO_BIN_EXE_engine-client"))
                .arg("--ignore-physical-controllers")
                .arg("--config-dir")
                .arg(h.run_path().join("config"))
                .env("SPACEWARS_CONTROL_SOCKET", &h._socket_path.0)
                .env("SLINT_BACKEND", "winit-femtovg")
                .env_remove("WAYLAND_DISPLAY")
                .stdin(Stdio::null())
                .stderr(Stdio::from(log.try_clone().unwrap()))
                .stdout(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let ready = h.wait_until_ready();
        s = launch(h, ready, "raster");
        choose(h, &mut s, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&saved), physics(&loaded));
        h.capture_screenshot("material-loaded-raster.png");
        choose(h, &mut s, 0);
        let end = snapshot(h, |v| v["stopped"].is_string());
        assert!(end["stopped"].as_str().unwrap().starts_with("Run complete"));
        assert!(end["impacts"].as_u64().unwrap() >= 2);
        assert!(end["separations"].as_u64().unwrap() >= 1);
        h.capture_screenshot("material-complete-raster.png");
        // Malformed input never replaces the visible, valid physical state.
        fs::write(h.run_path().join("config/material-lab.bin"), b"corrupt").unwrap();
        choose(h, &mut s, 5);
        let refused = snapshot(h, |v| {
            v["storage_status"]
                .as_str()
                .unwrap()
                .starts_with("Load failed")
        });
        assert_eq!(physics(&end), physics(&refused));
    });
}
