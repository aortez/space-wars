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
        *current = (*current + 1) % scenario_garden_lab::CONTROLS.len();
    }
    press(h, s, InputButton::South);
}
fn snapshot(h: &mut FunctionalHarness, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
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
fn mature(v: &Value) -> usize {
    tissues(v)
        .iter()
        .filter(|t| t["role"] == "Shoot" && t["size"] == 3 && t["health"].as_u64().unwrap() > 0)
        .count()
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
fn garden_lab_pause_step_and_renderers_smoke() {
    run_functional_test_with_seed("garden-smoke", "winit-femtovg", 42, |h| {
        let ready = h.wait_until_ready();
        let mut state = h.activate_until_scenario("garden-lab", ready);
        for renderer in ["vector", "raster"] {
            state = h.activate_guarded("launcher.settings", &state);
            if control_value(&state, "launcher.settings.renderer.next") != Some(renderer) {
                state = h.activate_guarded("launcher.settings.renderer.next", &state);
            }
            assert_eq!(
                control_value(&state, "launcher.settings.renderer.next"),
                Some(renderer)
            );
            h.activate_guarded("launcher.settings.start", &state);
            state = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("garden-lab".into()),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let mut control = 0;
            choose(h, &mut state, &mut control, 0);
            let paused = snapshot(h, |v| v["paused"] == true);
            let stepped = until(h, &mut state, &mut control, tick(&paused) + 1);
            assert_eq!(tick(&stepped), tick(&paused) + 1);
            assert!(!tissues(&stepped).is_empty());
            let deadline = Instant::now() + TRANSITION_TIMEOUT;
            loop {
                if green(&capture(h, renderer, "smoke", &stepped)) > 0 {
                    break;
                }
                assert!(Instant::now() < deadline, "garden shoots did not render");
                thread::sleep(POLL_INTERVAL);
            }
            assert_eq!(stepped, snapshot(h, |v| v["paused"] == true));
            let reset = fresh(h, &mut state, &mut control);
            assert!(tick(&reset) <= 2);
            h.pause_guarded(&state);
            let pause = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            h.activate_guarded("pause.return-to-launcher", &pause);
            state = h.wait_until_ready();
        }
    });
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn garden_lab_controls_lifecycle_and_both_renderers() {
    run_functional_test_with_seed("garden-lab", "winit-femtovg", 42, |h| {
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
            let capped = until(h, &mut s, &mut c, 40);
            assert_eq!(tissues(&capped).len(), 32);
            assert!(
                capped["growth_summary"]
                    .as_str()
                    .unwrap()
                    .contains("tissue limit")
            );
            capture(h, renderer, "capped", &capped);
            choose(h, &mut s, &mut c, 14);
            let tuned = snapshot(h, |v| v["tissue_limit"] == 33);
            for field in [
                "biological_state",
                "pending_ns",
                "tick",
                "tissues",
                "connections",
                "intake",
                "maintenance",
                "growth",
                "loss",
            ] {
                assert_eq!(tuned[field], capped[field], "tuning changed {field}");
            }
            let expanded = until(h, &mut s, &mut c, 41);
            assert_eq!(tissues(&expanded).len(), 33);
            assert!(
                expanded["growth"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_u64().unwrap() > 0)
            );
            capture(h, renderer, "expanded", &expanded);
            fresh(h, &mut s, &mut c);
            choose(h, &mut s, &mut c, 17);
            for _ in 0..3 {
                press(h, &mut s, InputButton::South);
            }
            let narrow = snapshot(h, |v| v["spread_radius"] == 0);
            let blocked = until(h, &mut s, &mut c, tick(&narrow) + 1);
            assert_eq!(tissues(&blocked).len(), tissues(&narrow).len());
            assert!(
                blocked["growth_summary"]
                    .as_str()
                    .unwrap()
                    .contains("spread radius")
            );
            choose(h, &mut s, &mut c, 16);
            let wider = snapshot(h, |v| v["spread_radius"] == 1);
            assert_eq!(wider["tissues"], blocked["tissues"]);
            assert_eq!(wider["pending_ns"], blocked["pending_ns"]);
            let spread = until(h, &mut s, &mut c, tick(&wider) + 1);
            assert!(tissues(&spread).len() > tissues(&wider).len());
            assert!(
                spread["growth"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_u64().unwrap() > 0)
            );
            capture(h, renderer, "spread-expanded", &spread);
            fresh(h, &mut s, &mut c);
            for (time, label) in [
                (2, "emergence"),
                (60, "developing-small"),
                (120, "developing-size-two"),
                (186, "first-establishment"),
                (240, "established-before-cut"),
            ] {
                let v = until(h, &mut s, &mut c, time);
                if time >= 186 {
                    assert!(mature(&v) >= 3);
                }
                if time == 60 {
                    assert!(
                        tissues(&v)
                            .iter()
                            .any(|t| t["development"][0].as_u64().unwrap() > 0)
                    );
                }
                capture(h, renderer, label, &v);
            }
            let before = snapshot(h, |v| tick(v) == 240);
            choose(h, &mut s, &mut c, 11);
            let cut = snapshot(h, |v| tissues(v).iter().all(|t| t["role"] != "Shoot"));
            for t in tissues(&cut) {
                let old = tissues(&before)
                    .iter()
                    .find(|b| b["id"] == t["id"])
                    .unwrap();
                for field in [
                    "id",
                    "organism",
                    "health",
                    "size",
                    "development",
                    "invested",
                    "support",
                ] {
                    assert_eq!(t[field], old[field]);
                }
            }
            for i in 0..3 {
                let removed: u64 = tissues(&before)
                    .iter()
                    .filter(|t| t["role"] == "Shoot")
                    .map(|t| t["stores"][i].as_u64().unwrap())
                    .sum();
                assert_eq!(
                    cut["loss"][i].as_u64().unwrap() - before["loss"][i].as_u64().unwrap(),
                    removed
                );
            }
            capture(h, renderer, "immediate-cut-240", &cut);
            for (time, label) in [
                (241, "small-replacement"),
                (300, "replacement-development"),
                (360, "replacement-size-two"),
                (403, "mature-regrowth"),
                (480, "bounded-stop"),
            ] {
                let v = until(h, &mut s, &mut c, time);
                if time >= 403 {
                    assert!(mature(&v) >= 3);
                }
                assert!(
                    tissues(&v)
                        .iter()
                        .filter(|t| t["role"] == "Shoot")
                        .all(|t| !tissues(&before).iter().any(|b| b["id"] == t["id"]))
                );
                capture(h, renderer, label, &v);
            }
            let complete = snapshot(h, |v| tick(v) == 480 && v["run_state"] == "complete");
            choose(h, &mut s, &mut c, 14);
            let tuned_complete = snapshot(h, |v| v["tissue_limit"] == 33);
            assert_eq!(complete["tissues"], tuned_complete["tissues"]);
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
            let dead = until(h, &mut s, &mut c, 190);
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
                tissues(&cut).iter().all(|t| tissues(&before)
                    .iter()
                    .any(|b| b["id"] == t["id"] && b["health"] == t["health"])),
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
                let shortage = until(h, &mut s, &mut c, 80);
                assert!(
                    tissues(&shortage)
                        .iter()
                        .any(|t| t["health"].as_u64().unwrap() < 100)
                );
                assert!(shortage["unmet"][resource].as_u64().unwrap() > 0);
                capture(h, renderer, &format!("{name}-stress"), &shortage);
                let dead = until(h, &mut s, &mut c, 240);
                assert!(tissues(&dead).iter().all(|t| t["health"] == 0));
                let image = capture(h, renderer, name, &dead);
                assert_eq!(green(&image), 0, "shortage secretly sustained grass");
            }
            fresh(h, &mut s, &mut c);
            choose(h, &mut s, &mut c, 2);
            snapshot(h, |v| v["speed"] == 2);
            choose(h, &mut s, &mut c, 3);
            snapshot(h, |v| v["speed"] == 1);
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
                    && v["speed"] == 1
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
            && *y > cy - 2.0 * scale
            && *y < cy + 0.6 * scale),
        "marker does not identify selected tissue cell: {tissue}"
    );
}

fn near(p: &[u8], c: [u8; 3]) -> bool {
    (0..3).all(|i| (i16::from(p[i]) - i16::from(c[i])).abs() < 12)
}
/// Count matching pixels inside a world-space box, using the lab camera's
/// fixed 22-unit view height. Keeps anti-aliased HUD text out of the count.
fn region_pixels(path: &Path, [x0, x1, y0, y1]: [f64; 4], c: [u8; 3]) -> usize {
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
    pixels[..frame.buffer_size()]
        .chunks_exact(channels)
        .enumerate()
        .filter(|(i, p)| {
            let x = ((i % frame.width as usize) as f64 - frame.width as f64 / 2.0) / scale;
            let y = (frame.height as f64 / 2.0 - (i / frame.width as usize) as f64) / scale;
            (x0..=x1).contains(&x) && (y0..=y1).contains(&y) && near(p, c)
        })
        .count()
}
/// The rock's frame at any quarter turn, clear of the bed and the HUD text.
const ROCK_REGION: [f64; 4] = [8.2, 17.0, -6.0, 3.0];
const SLATE: [u8; 3] = [92, 97, 107];
fn moss(v: &Value) -> Vec<&Value> {
    tissues(v).iter().filter(|t| t["role"] == "Moss").collect()
}
fn rock_tissues(v: &Value) -> &Vec<Value> {
    v["moss"]["rock"]["tissues"].as_array().unwrap()
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn garden_lab_moss_surfaces_light_motion_and_support_loss_in_both_renderers() {
    run_functional_test_with_seed("garden-moss", "winit-femtovg", 42, |h| {
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
            fresh(h, &mut s, &mut c);
            choose(h, &mut s, &mut c, 18);
            let placed = snapshot(h, |v| {
                v["moss"]["placed"] == true && v["moss"]["rock"]["tissues"].is_array()
            });
            assert_eq!(moss(&placed).len(), 1);
            assert_eq!(moss(&placed)[0]["cell"], json!([1, 4]));
            assert_eq!(rock_tissues(&placed).len(), 1);

            // The dim room starves grass but pays for moss on dirt and rock.
            choose(h, &mut s, &mut c, 4);
            snapshot(h, |v| v["intensity"] == 50);
            let dim = until(h, &mut s, &mut c, 90);
            let bed = moss(&dim);
            assert!(bed.len() >= 3, "dirt moss did not spread: {dim}");
            for t in &bed {
                assert_eq!(t["health"], 100);
                assert_eq!(t["light"], 50);
                assert_eq!(t["surface"]["eligible"], true);
                assert_eq!(t["contact"], "Eligible");
            }
            assert!(
                tissues(&dim)
                    .iter()
                    .any(|t| t["role"] == "Shoot" && t["shade"].as_u64().unwrap() > 0)
            );
            let living = |v: &Value| {
                rock_tissues(v)
                    .iter()
                    .filter(|t| t["health"].as_u64().unwrap() > 0)
                    .count()
            };
            assert!(living(&dim) >= 5, "rock moss did not spread: {dim}");
            let image = capture(h, renderer, "moss-dim", &dim);
            assert!(color_pixels(&image, |p| near(p, [115, 184, 46])) > 20);
            assert!(region_pixels(&image, ROCK_REGION, SLATE) > 20);

            // Upside down, the old top face sits under its own rock.
            choose(h, &mut s, &mut c, 19);
            choose(h, &mut s, &mut c, 19);
            let turned = snapshot(h, |v| v["moss"]["rock"]["quarter_turns"] == 2);
            let top: Vec<_> = rock_tissues(&turned)
                .iter()
                .filter(|t| t["cell"][1] == 4 && t["health"] == 100)
                .map(|t| t["id"].clone())
                .collect();
            assert!(!top.is_empty());
            let inverted = until(h, &mut s, &mut c, tick(&turned) + 1);
            for t in rock_tissues(&inverted) {
                if top.contains(&t["id"]) {
                    assert_eq!(t["light"], 0, "inverted moss still lit: {t}");
                    assert_eq!(t["contact"], "Eligible");
                    assert!(t["world"][1].as_f64().unwrap() < -1.5);
                }
            }
            capture(h, renderer, "moss-inverted", &inverted);

            // Sustained darkness kills the inverted face beside lit moss, while
            // the same dim room has killed the grass and still feeds dirt moss.
            let config = engine_core::plants::grass::physiology();
            let deadline =
                u64::from(config.grace_ticks) + 100 / u64::from(config.damage_per_tick) + 2;
            let dieback = until(h, &mut s, &mut c, tick(&inverted) + deadline);
            for t in rock_tissues(&dieback) {
                if top.contains(&t["id"]) {
                    assert_eq!(t["health"], 0, "inverted moss survived: {t}");
                }
            }
            assert!(
                rock_tissues(&dieback)
                    .iter()
                    .any(|t| !top.contains(&t["id"]) && t["health"] == 100)
            );
            assert!(
                tissues(&dieback)
                    .iter()
                    .filter(|t| t["role"] != "Moss")
                    .all(|t| t["health"] == 0)
            );
            assert!(moss(&dieback).iter().all(|t| t["health"] == 100));
            let image = capture(h, renderer, "moss-dieback", &dieback);
            assert!(region_pixels(&image, ROCK_REGION, [128, 102, 102]) > 20);

            // Breakage detaches unsupported moss; removal detaches the rest.
            choose(h, &mut s, &mut c, 20);
            let broken = snapshot(h, |v| v["moss"]["rock"]["broken"] == true);
            assert!(rock_tissues(&broken).iter().all(|t| t["cell"][0] != 6));
            capture(h, renderer, "moss-broken", &broken);
            choose(h, &mut s, &mut c, 21);
            let removed = snapshot(h, |v| v["moss"]["rock"]["removed"] == true);
            assert!(rock_tissues(&removed).is_empty());
            let image = capture(h, renderer, "moss-rock-removed", &removed);
            assert_eq!(region_pixels(&image, ROCK_REGION, SLATE), 0);

            // Scraping discards the strip's own shares; only new, paid tissue
            // refills it.
            choose(h, &mut s, &mut c, 22);
            let scraped = snapshot(h, |v| moss(v).iter().all(|t| t["cell"][0] != 2));
            let strip: Vec<_> = moss(&removed)
                .into_iter()
                .filter(|t| t["cell"][0] == 2)
                .collect();
            assert!(!strip.is_empty());
            for resource in 0..3 {
                let lost: u64 = strip
                    .iter()
                    .map(|t| t["stores"][resource].as_u64().unwrap())
                    .sum();
                assert_eq!(
                    scraped["loss"][resource].as_u64().unwrap(),
                    removed["loss"][resource].as_u64().unwrap() + lost
                );
            }
            let old: Vec<_> = moss(&removed).iter().map(|t| t["id"].clone()).collect();
            let regrown = until(h, &mut s, &mut c, tick(&scraped) + 8);
            let refill = moss(&regrown)
                .into_iter()
                .find(|t| t["cell"][0] == 2)
                .expect("scraped strip was not regrown");
            assert!(!old.contains(&refill["id"]));
            capture(h, renderer, "moss-regrowth", &regrown);

            h.pause_guarded(&s);
            s = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            h.activate_guarded("pause.return-to-launcher", &s);
            s = h.wait_until_ready();
        }
    });
}
