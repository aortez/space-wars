use super::*;
use spacewars_control::{InputButton, InputPressRequest};

fn press(h: &mut FunctionalHarness, s: &mut UiState, b: InputButton) {
    let request = InputPressRequest::new(s, b);
    *s = h
        .client
        .input_press_before(&request, Instant::now() + Duration::from_secs(4))
        .unwrap()
        .state;
    h.record_state(&format!("input press {}", b.as_str()), s);
    thread::sleep(Duration::from_millis(150));
}
fn choose(h: &mut FunctionalHarness, s: &mut UiState, current: &mut usize, target: usize) {
    while *current != target {
        press(h, s, InputButton::Down);
        *current = (*current + 1) % 14;
    }
    press(h, s, InputButton::South);
}
fn snapshot(h: &mut FunctionalHarness, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + TRANSITION_TIMEOUT;
    loop {
        let status = h
            .client
            .request_before("status\n", Instant::now() + Duration::from_secs(2))
            .unwrap();
        if let Some(line) = status.lines().find_map(|l| l.strip_prefix("garden_lab=")) {
            let v: Value = serde_json::from_str(line).unwrap();
            if predicate(&v) {
                h.history
                    .push(json!({"command":"garden snapshot","snapshot":v}));
                return v;
            }
        }
        assert!(
            Instant::now() < deadline,
            "garden state did not satisfy outcome: {status}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}
fn tissues(v: &Value) -> &Vec<Value> {
    v["tissues"].as_array().unwrap()
}
fn tick(v: &Value) -> u64 {
    v["tick"].as_u64().unwrap()
}
fn fresh(h: &mut FunctionalHarness, s: &mut UiState, c: &mut usize) -> Value {
    choose(h, s, c, 12);
    *c = 0;
    choose(h, s, c, 0);
    snapshot(h, |v| {
        v["paused"] == true
            && tick(v) <= 2
            && v["availability"] == json!([1000, 1000])
            && v["intensity"] == 1000
    })
}
fn until(h: &mut FunctionalHarness, s: &mut UiState, c: &mut usize, target: u64) -> Value {
    let v = snapshot(h, |v| v["paused"] == true);
    assert!(tick(&v) < target, "fixture exceeded target: {v}");
    choose(h, s, c, 1);
    for _ in tick(&v) + 1..target {
        press(h, s, InputButton::South);
    }
    snapshot(h, |v| v["paused"] == true && tick(v) == target)
}
fn capture(h: &mut FunctionalHarness, renderer: &str, name: &str, v: &Value) -> PathBuf {
    fs::write(
        h.run_path().join(format!("garden-{renderer}-{name}.json")),
        serde_json::to_vec_pretty(v).unwrap(),
    )
    .unwrap();
    h.capture_screenshot(&format!("garden-{renderer}-{name}.png"))
}
fn color_pixels(path: &Path, matches: impl Fn(&[u8]) -> bool) -> usize {
    let mut decoder = png::Decoder::new(File::open(path).unwrap());
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unexpected {other:?}"),
    };
    // Crop the diagnostic rows and sun; include shoots above the soil line.
    pixels[(frame.height as usize * 2 / 5) * frame.width as usize * channels..frame.buffer_size()]
        .chunks_exact(channels)
        .filter(|p| matches(p))
        .count()
}
fn green(path: &Path) -> usize {
    color_pixels(path, |p| p[1] > 180 && p[0] < 110 && p[2] < 130)
}
fn assert_connected_shade(v: &Value) {
    let shaded = tissues(v)
        .iter()
        .find(|t| {
            t["role"] == "Shoot"
                && t["shade"].as_u64().unwrap() > 0
                && t["health"].as_u64().unwrap() > 0
        })
        .expect("no stressed living shoot");
    let mut component = BTreeSet::from([shaded["id"].as_u64().unwrap()]);
    loop {
        let n = component.len();
        for edge in v["connections"].as_array().unwrap() {
            let a = edge[0].as_u64().unwrap();
            let b = edge[1].as_u64().unwrap();
            if tissues(v)
                .iter()
                .any(|t| (t["id"] == a || t["id"] == b) && t["health"] == 0)
            {
                continue;
            }
            if component.contains(&a) || component.contains(&b) {
                component.insert(a);
                component.insert(b);
            }
        }
        if n == component.len() {
            break;
        }
    }
    assert!(
        tissues(v)
            .iter()
            .any(|t| component.contains(&t["id"].as_u64().unwrap())
                && t["role"] == "Shoot"
                && t["shade"] == 0
                && t["health"] == 100),
        "shade must remain connected to healthy sunlit grass: {v}"
    );
}
#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn garden_lab_controls_lifecycle_and_both_renderers() {
    run_functional_test_with_backend("garden-lab", "winit-femtovg", |h| {
        let ready = h.wait_until_ready();
        let mut s = h.activate_until_scenario("garden-lab", ready);
        for renderer in ["vector", "raster"] {
            s = h.activate_guarded("launcher.settings", &s);
            if control_value(&s, "launcher.settings.renderer.next") != Some(renderer) {
                s = h.activate_guarded("launcher.settings.renderer.next", &s);
            }
            assert_eq!(
                control_value(&s, "launcher.settings.renderer.next"),
                Some(renderer)
            );
            s = h.activate_guarded("launcher.settings.back", &s);
            h.capture_screenshot(&format!("garden-{renderer}-launcher.png"));
            h.activate_guarded("launcher.start", &s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("garden-lab".into()),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let mut c = 0;
            choose(h, &mut s, &mut c, 0);
            let paused = snapshot(h, |v| v["paused"] == true);
            thread::sleep(Duration::from_millis(1200));
            assert_eq!(
                paused,
                snapshot(h, |v| v["paused"] == true),
                "paused physiology changed"
            );
            let stepped = until(h, &mut s, &mut c, tick(&paused) + 1);
            assert_eq!(tick(&stepped), tick(&paused) + 1);
            capture(h, renderer, "pause-step", &stepped);

            fresh(h, &mut s, &mut c);
            let sun = until(h, &mut s, &mut c, 10);
            assert!(
                tissues(&sun)
                    .iter()
                    .filter(|t| t["role"] == "Shoot")
                    .count()
                    >= 2
            );
            assert!(tissues(&sun).iter().any(|t| t["role"] == "Runner"));
            let image = capture(h, renderer, "growth-connectivity", &sun);
            assert!(green(&image) > 20, "no living grass in candidate capture");
            assert!(
                color_pixels(&image, |p| p[0] > 160 && p[1] > 190 && p[2] > 230) > 20,
                "underground tissue invisible"
            );
            assert!(
                color_pixels(&image, |p| p[0] < 110 && p[1] > 160 && p[2] > 230) > 20,
                "connections invisible"
            );
            choose(h, &mut s, &mut c, 13);
            let selected = snapshot(h, |v| v["selected_tissue"]["id"] == tissues(&sun)[1]["id"]);
            assert_eq!(selected["selected_tissue"], tissues(&selected)[1]);
            assert_ne!(selected["selected_tissue"]["id"], tissues(&sun)[0]["id"]);
            let image = capture(h, renderer, "selected-tissue", &selected);
            assert_selected_target(&image, &selected["selected_tissue"]);
            choose(h, &mut s, &mut c, 10);
            let hidden = snapshot(h, |v| v["inspect"] == false);
            let image = capture(h, renderer, "inspection-off", &hidden);
            assert_eq!(
                color_pixels(&image, |p| p[0] < 110 && p[1] > 160 && p[2] > 230),
                0
            );
            choose(h, &mut s, &mut c, 10);
            choose(h, &mut s, &mut c, 7);
            let stress = until(h, &mut s, &mut c, 14);
            assert_connected_shade(&stress);
            let image = capture(h, renderer, "shade-stress", &stress);
            assert!(
                color_pixels(&image, |p| p[0] > 220
                    && p[1] > 130
                    && p[1] < 190
                    && p[2] < 70)
                    > 20,
                "stressed tissue invisible"
            );
            choose(h, &mut s, &mut c, 8);
            let moved = snapshot(h, |v| v["shade_x"] == 10 && v["shade"] == true);
            capture(h, renderer, "shade-moved", &moved);
            choose(h, &mut s, &mut c, 9);
            snapshot(h, |v| v["shade_x"] == 11 && v["shade"] == true);
            let stressed_ids: Vec<_> = tissues(&stress)
                .iter()
                .filter(|t| t["shade"].as_u64().unwrap() > 0)
                .map(|t| t["id"].clone())
                .collect();
            choose(h, &mut s, &mut c, 7);
            let recovery = until(h, &mut s, &mut c, 18);
            assert!(
                tissues(&recovery)
                    .iter()
                    .any(|t| stressed_ids.contains(&t["id"])
                        && t["shade"] == 0
                        && t["health"] == 100)
            );
            capture(h, renderer, "shade-recovery", &recovery);
            choose(h, &mut s, &mut c, 7);
            let dead = until(h, &mut s, &mut c, 30);
            assert!(
                tissues(&dead)
                    .iter()
                    .any(|t| t["role"] == "Shoot" && t["health"] == 0)
            );
            let image = capture(h, renderer, "shade-death", &dead);
            assert!(
                color_pixels(&image, |p| p[0] > 115
                    && p[0] < 145
                    && p[1] > 90
                    && p[1] < 115
                    && p[2] > 90
                    && p[2] < 115)
                    > 20,
                "dead tissue invisible"
            );

            fresh(h, &mut s, &mut c);
            let before = until(h, &mut s, &mut c, 10);
            choose(h, &mut s, &mut c, 11);
            let cut = snapshot(h, |v| {
                v["paused"] == true && tissues(v).iter().all(|t| t["role"] != "Shoot")
            });
            assert!(
                tissues(&cut).iter().all(|t| tissues(&before).contains(t)),
                "cut changed survivors"
            );
            assert!(
                cut["loss"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_u64().unwrap() > 0)
            );
            let image = capture(h, renderer, "cut", &cut);
            assert_eq!(green(&image), 0);
            let regrowth = until(h, &mut s, &mut c, 11);
            assert!(
                regrowth["growth"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_u64().unwrap() > 0)
            );
            assert!(tissues(&regrowth).iter().any(|t| t["role"] == "Shoot"
                && !tissues(&before).iter().any(|old| old["id"] == t["id"])));
            for resource in 0..3 {
                let stored = |v: &Value| {
                    tissues(v)
                        .iter()
                        .map(|t| t["stores"][resource].as_u64().unwrap())
                        .sum::<u64>()
                };
                assert_eq!(
                    stored(&cut) + regrowth["intake"][resource].as_u64().unwrap(),
                    stored(&regrowth)
                        + regrowth["maintenance"][resource].as_u64().unwrap()
                        + regrowth["growth"][resource].as_u64().unwrap()
                        + regrowth["loss"][resource].as_u64().unwrap()
                );
            }
            let image = capture(h, renderer, "regrowth", &regrowth);
            assert!(green(&image) > 20);

            for (name, control, resource) in [
                ("dim-room", 4, 2),
                ("nutrient-shortage", 5, 0),
                ("water-shortage", 6, 1),
            ] {
                fresh(h, &mut s, &mut c);
                choose(h, &mut s, &mut c, control);
                snapshot(h, |v| {
                    if control == 4 {
                        v["intensity"] == 50
                    } else {
                        v["availability"][resource] == 0 && v["availability"][1 - resource] == 1000
                    }
                });
                let shortage = until(h, &mut s, &mut c, 10);
                assert!(
                    tissues(&shortage)
                        .iter()
                        .any(|t| t["health"].as_u64().unwrap() < 100)
                );
                assert!(shortage["unmet"][resource].as_u64().unwrap() > 0);
                capture(h, renderer, &format!("{name}-stress"), &shortage);
                let dead = until(h, &mut s, &mut c, 40);
                assert!(tissues(&dead).iter().all(|t| t["health"] == 0));
                let image = capture(h, renderer, name, &dead);
                assert_eq!(green(&image), 0, "shortage secretly sustained grass");
            }
            fresh(h, &mut s, &mut c);
            choose(h, &mut s, &mut c, 2);
            snapshot(h, |v| v["speed"] == 8);
            choose(h, &mut s, &mut c, 3);
            snapshot(h, |v| v["speed"] == 4);
            choose(h, &mut s, &mut c, 6);
            choose(h, &mut s, &mut c, 7);
            until(h, &mut s, &mut c, 10);
            snapshot(h, |v| v["availability"][1] == 0 && v["shade"] == true);
            let instance = s.scenario_revision;
            h.pause_guarded(&s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            h.activate_guarded("pause.restart", &s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            assert_ne!(instance, s.scenario_revision);
            snapshot(h, |v| {
                tick(v) < 8
                    && v["speed"] == 4
                    && v["shade"] == false
                    && v["availability"] == json!([1000, 1000])
            });
            h.pause_guarded(&s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            h.activate_guarded("pause.return-to-launcher", &s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::LauncherMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
        }
    });
}

// Match the marker to the selected cell, not just any magenta pixels in the HUD.
fn assert_selected_target(path: &Path, tissue: &Value) {
    let mut decoder = png::Decoder::new(File::open(path).unwrap());
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unexpected {other:?}"),
    };
    let scale = frame.height as f64 / 22.0;
    let cx = frame.width as f64 / 2.0 + (tissue["cell"][0].as_f64().unwrap() + 0.5 - 8.0) * scale;
    let cy = frame.height as f64 / 2.0 - (tissue["cell"][1].as_f64().unwrap() + 0.5 - 4.0) * scale;
    let magenta: Vec<_> = pixels[..frame.buffer_size()]
        .chunks_exact(channels)
        .enumerate()
        .filter(|(_, p)| p[0] > 220 && p[1] < 100 && p[2] > 220)
        .map(|(i, _)| {
            (
                (i % frame.width as usize) as f64,
                (i / frame.width as usize) as f64,
            )
        })
        .collect();
    assert!(magenta.len() > 20, "selected tissue marker invisible");
    assert!(
        magenta.iter().all(|(x, y)| (x - cx).abs() < 0.6 * scale
            && *y > cy - 1.1 * scale
            && *y < cy + 0.6 * scale),
        "marker does not identify selected tissue cell: {tissue}"
    );
}
