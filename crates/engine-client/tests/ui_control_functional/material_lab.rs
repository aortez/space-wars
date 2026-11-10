use super::*;
use scenario_material_lab::YIELD_CHECKPOINT_STEP;
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
    // Leave a neutral frame between presses; status diagnostics are cached.
    thread::sleep(Duration::from_millis(150));
}
fn choose(h: &mut FunctionalHarness, s: &mut UiState, current: &mut u64, target: u64) {
    let state = snapshot(h, |_| true);
    let count = state["control_count"].as_u64().unwrap();
    assert!(target < count);
    while *current != target {
        let down = (target + count - *current) % count;
        let up = (*current + count - target) % count;
        let (button, next) = if down <= up {
            (InputButton::Down, (*current + 1) % count)
        } else {
            (InputButton::Up, (*current + count - 1) % count)
        };
        press(h, s, button);
        *current = next;
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
    json!({"case":v["case"],"steps":v["steps"],"summary":v["summary"],"remaining":v["remaining"],"events":v["events"],"impacts":v["impacts"],"separations":v["separations"],"weight":v["weight"],"gravity_impulse":v["gravity_impulse"],"load_changes":v["load_changes"],"pair":v["pair"]})
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
        let mut control = initial["control"].as_u64().unwrap();
        for _ in 0..3 {
            choose(h, &mut s, &mut control, 3);
        }
        snapshot(h, |v| v["case"] == "Load, release & return");
        h.capture_screenshot("material-initial-vector.png");
        choose(h, &mut s, &mut control, 0);
        snapshot(h, |v| v["steps"].as_u64().unwrap() > 80);
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
        let yielded = snapshot(h, |v| {
            v["steps"] == YIELD_CHECKPOINT_STEP && v["paused"] == true
        });
        h.capture_screenshot("material-yielded-vector.png");
        choose(h, &mut s, &mut control, 6);
        let released = snapshot(h, |v| v["weight"] == 0.0);
        assert_eq!(released["steps"], yielded["steps"]);
        assert_eq!(released["summary"], yielded["summary"]);
        assert_eq!(released["load_changes"].as_array().unwrap().len(), 1);
        choose(h, &mut s, &mut control, 4);
        let saved = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Saved")
        });
        assert_eq!(saved["paused"], true);
        assert!(saved["summary"]["damage_work"].as_f64().unwrap() > 0.0);
        assert!(h.run_path().join("config/material-lab.bin").is_file());
        h.capture_screenshot("material-saved-vector.png");
        choose(h, &mut s, &mut control, 1);
        let next = snapshot(h, |v| {
            v["steps"].as_u64() == Some(saved["steps"].as_u64().unwrap() + 1)
        });
        choose(h, &mut s, &mut control, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&saved), physics(&loaded));
        choose(h, &mut s, &mut control, 1);
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
        control = snapshot(h, |_| true)["control"].as_u64().unwrap();
        choose(h, &mut s, &mut control, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&saved), physics(&loaded));
        h.capture_screenshot("material-loaded-raster.png");
        choose(h, &mut s, &mut control, 0);
        let separated = snapshot(h, |v| v["paused"] == true && v["separations"] == 1);
        assert!(separated["stopped"].is_null());
        h.capture_screenshot("material-separated-raster.png");
        choose(h, &mut s, &mut control, 7);
        let light = snapshot(h, |v| v["weight"] == 1000.0);
        assert_eq!(light["summary"], separated["summary"]);
        assert_eq!(light["steps"], separated["steps"]);
        assert_eq!(light["load_changes"].as_array().unwrap().len(), 2);
        choose(h, &mut s, &mut control, 0);
        let end = snapshot(h, |v| v["stopped"].is_string());
        assert!(end["stopped"].as_str().unwrap().starts_with("Run complete"));
        assert_eq!(end["storage_status"], end["stopped"]);
        assert!(end["impacts"].as_u64().unwrap() >= 2);
        assert!(end["separations"].as_u64().unwrap() >= 1);
        assert_eq!(
            end["summary"]["damage_work"],
            separated["summary"]["damage_work"]
        );
        h.capture_screenshot("material-complete-raster.png");
        // Malformed input never replaces the visible, valid physical state.
        fs::write(h.run_path().join("config/material-lab.bin"), b"corrupt").unwrap();
        choose(h, &mut s, &mut control, 5);
        let refused = snapshot(h, |v| {
            v["storage_status"]
                .as_str()
                .unwrap()
                .starts_with("Load failed")
        });
        assert_eq!(physics(&end), physics(&refused));
    });
}
fn body(v: &Value, index: usize, field: &str) -> f64 {
    v["pair"]["bodies"][index][field].as_f64().unwrap()
}
#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn material_lab_free_pairs_step_save_load_and_both_renderers() {
    run_functional_test_with_seed("material-lab-pairs", "winit-femtovg", 42, |h| {
        let ready = h.wait_until_ready();
        let mut s = launch(h, ready, "vector");
        let mut control = snapshot(h, |v| v["paused"] == true)["control"]
            .as_u64()
            .unwrap();
        for _ in 0..4 {
            choose(h, &mut s, &mut control, 3);
        }
        let initial = snapshot(h, |v| v["case"] == "Wood strikes wood");
        assert_eq!(initial["steps"], 0);
        assert_eq!(initial["phase"], "Approach");
        assert_eq!(body(&initial, 0, "velocity"), 6.0);
        assert_eq!(body(&initial, 1, "velocity"), 0.0);
        for index in 0..2 {
            assert_eq!(body(&initial, index, "remaining"), 1200.0);
        }
        h.capture_screenshot("pair-wood-wood-initial-vector.png");
        // Run until the wood yields, then pause with the control still selected.
        choose(h, &mut s, &mut control, 0);
        snapshot(h, |v| body(v, 0, "damage_work") > 0.0);
        press(h, &mut s, InputButton::South);
        let paused = snapshot(h, |v| v["paused"] == true);
        assert!(paused["stopped"].is_null());
        choose(h, &mut s, &mut control, 4);
        let saved = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Saved")
        });
        assert_eq!(physics(&saved), physics(&paused));
        h.capture_screenshot("pair-wood-wood-saved-vector.png");
        // A free pair refuses external loads without touching the run.
        choose(h, &mut s, &mut control, 8);
        let refused = snapshot(h, |v| {
            v["storage_status"]
                .as_str()
                .unwrap()
                .starts_with("A free pair")
        });
        assert_eq!(physics(&refused), physics(&saved));
        choose(h, &mut s, &mut control, 1);
        let next = snapshot(h, |v| {
            v["steps"].as_u64() == Some(saved["steps"].as_u64().unwrap() + 1)
        });
        choose(h, &mut s, &mut control, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&loaded), physics(&saved));
        choose(h, &mut s, &mut control, 1);
        assert_eq!(
            physics(&snapshot(h, |v| v["steps"] == next["steps"])),
            physics(&next)
        );
        choose(h, &mut s, &mut control, 0);
        let end = snapshot(h, |v| v["stopped"].is_string());
        assert!(end["stopped"].as_str().unwrap().starts_with("Run complete"));
        assert_eq!(
            (&end["impacts"], &end["separations"]),
            (&json!(1), &json!(1))
        );
        // The striker is thrown back, the struck body driven away, and equal
        // wood layers keep equal damage.
        assert!(body(&end, 0, "velocity") < 0.0 && body(&end, 1, "velocity") > 0.0);
        assert!(body(&end, 0, "damage_work") > 0.0);
        assert!((body(&end, 0, "damage_work") - body(&end, 1, "damage_work")).abs() < 1e-6);
        assert!(body(&end, 1, "remaining") < 1200.0);
        h.capture_screenshot("pair-wood-wood-complete-vector.png");
        // The slot survives leaving the lab and changing the renderer.
        h.pause_guarded(&s);
        let host_pause = h.wait_for(
            UiStatePredicate {
                screen: Some(UiScreen::PauseMain),
                ..Default::default()
            },
            TRANSITION_TIMEOUT,
        );
        h.activate_guarded("pause.return-to-launcher", &host_pause);
        let ready = h.wait_until_ready();
        s = launch(h, ready, "raster");
        control = snapshot(h, |_| true)["control"].as_u64().unwrap();
        choose(h, &mut s, &mut control, 5);
        let loaded = snapshot(h, |v| {
            v["storage_status"].as_str().unwrap().starts_with("Loaded")
        });
        assert_eq!(physics(&loaded), physics(&saved));
        h.capture_screenshot("pair-wood-wood-loaded-raster.png");
        choose(h, &mut s, &mut control, 3);
        snapshot(h, |v| v["case"] == "Wood meets armor" && v["steps"] == 0);
        choose(h, &mut s, &mut control, 0);
        let end = snapshot(h, |v| v["stopped"].is_string());
        assert!(end["stopped"].as_str().unwrap().starts_with("Run complete"));
        // Both bodies reverse; only the wood yields and spends capacity.
        assert!(body(&end, 0, "velocity") < 0.0 && body(&end, 1, "velocity") > 0.0);
        assert!(body(&end, 0, "permanent_set") > 0.0);
        assert!(body(&end, 0, "remaining") < body(&end, 0, "capacity"));
        assert_eq!(body(&end, 1, "damage_work"), 0.0);
        assert_eq!(body(&end, 1, "remaining"), body(&end, 1, "capacity"));
        h.capture_screenshot("pair-wood-armor-complete-raster.png");
    });
}
