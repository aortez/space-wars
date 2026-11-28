use super::*;

#[test]
fn steering_a_clock_run_updates_the_next_scored_generation_and_preserves_the_replay() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let job = training_controls::install(&window, directory.path().to_owned()).unwrap();
    let wait = |condition: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(180);
        while !condition() {
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                window.get_training_snapshot()
            );
            std::thread::sleep(Duration::from_millis(5));
            slint::platform::update_timers_and_animations();
        }
    };
    let activate = |id| assert!(ui_activation::activate(&window, id), "{id}");
    window
        .window()
        .set_size(slint::LogicalSize::new(800., 480.));
    install_ui_navigation(&window);
    window.set_launcher_visible(true);
    window.show().unwrap();
    wait(&|| window.get_training_can_start());
    activate("launcher.training");
    activate("training.recipe.previous");
    assert_eq!(window.get_training_recipe_label(), "Clock duck · Platforms");
    window.set_training_population(4);
    window.set_training_generations(2);
    activate("training.start");
    wait(&|| window.get_training_can_pause());
    assert!(window.get_training_worker_status().contains("/8 visits"));
    activate("training.duck.steer");
    activate("training.duck.scale.next");
    assert!(window.get_duck_edit_status().contains("generation 2"));
    let proposed: serde_json::Value =
        serde_json::from_str(&window.get_training_snapshot()).unwrap();
    let expected_scale = proposed["pending"][0]["mutation_scale"].as_f64().unwrap() as f32;
    activate("training.duck.steer");
    wait(&|| window.get_training_status_code() == "completed");
    let snapshot: serde_json::Value =
        serde_json::from_str(&window.get_training_snapshot()).unwrap();
    let path = std::path::PathBuf::from(snapshot["run_dir"].as_str().unwrap());
    activate("training.preview.best");
    wait(&|| window.get_duck_best_frame().size().width > 0);
    assert!(window.get_duck_best_details().contains("Tick "));
    assert_eq!(window.get_training_points().row_count(), 2);
    assert_eq!(window.get_duck_terms().row_count(), 5);
    let output = std::env::var_os("SPACEWARS_PREVIEW_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
        activate("training.preview.speed");
        activate("training.preview.speed");
    }
    for (width, height, chart_x, chart_y) in [(800, 480, 739., 84.), (1024, 768, 457., 482.)] {
        window
            .window()
            .set_size(slint::LogicalSize::new(width as f32, height as f32));
        window.window().take_snapshot().unwrap();
        let before = window.get_duck_term();
        click(&window, chart_x, chart_y);
        assert_eq!(
            window.get_duck_term(),
            (before + 1) % 5,
            "Chart term must be touchable at {width}×{height}"
        );
        while window.get_duck_term() != before {
            activate("training.duck.term");
        }
        if let Some(path) = &output {
            let in_play = |details: SharedString| {
                let Some((_, ticks)) = details.rsplit_once("Tick ") else {
                    return false;
                };
                let Some((now, total)) = ticks.split_once(" / ") else {
                    return false;
                };
                now.parse::<u64>().unwrap_or(0)
                    >= (total.parse::<u64>().unwrap_or(u64::MAX) / 2).clamp(1, 150)
            };
            wait(&|| {
                in_play(window.get_duck_best_details())
                    && (width < 920 || in_play(window.get_duck_current_details()))
            });
            let pixels = window.window().take_snapshot().unwrap();
            let file = std::fs::File::create(
                path.join(format!("clock-duck-training-{width}x{height}.png")),
            )
            .unwrap();
            let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels.as_bytes())
                .unwrap();
        }
    }
    drop(job);
    let saved = engine_agent::trainer::Trainer::resume(path.clone()).unwrap();
    assert_eq!(saved.completed(), 2);
    let history = saved.history().unwrap();
    assert_eq!(
        history[0].config.mutation_scale,
        engine_agent::trainer::TrainerConfig::default().mutation_scale
    );
    assert_ne!(history[0].config.mutation_scale, expected_scale);
    assert_eq!(history[1].config.mutation_scale, expected_scale);

    // A viewer can follow a historical winner while the job directory has an owner.
    let archive = engine_agent::trainer::Trainer::read_replay(&path, Some(1)).unwrap();
    let winner = archive.0.best_genome[..8].to_owned();
    let scored = archive.0.best.clone();
    let expected_doors = archive.0.door_exits.unwrap();
    let viewer = MainWindow::new().unwrap();
    viewer
        .window()
        .set_size(slint::LogicalSize::new(1024., 768.));
    viewer.set_launcher_visible(true);
    let viewer_dir = tempfile::tempdir().unwrap();
    let _viewer = training_controls::install_with_replay(
        &viewer,
        viewer_dir.path().to_owned(),
        Some((path, archive.0, archive.1)),
    )
    .unwrap();
    viewer.show().unwrap();
    wait(&|| viewer.get_duck_best_id().contains(&winner));
    assert_eq!(viewer.get_training_status_code(), "watching");
    assert_eq!(viewer.get_training_points().row_count(), 1);
    assert!(!viewer.get_training_can_resume());
    assert!(!viewer.get_duck_can_edit());
    assert!(viewer.get_duck_door_exits().contains(&format!(
        "L {}/{}",
        expected_doors[0][0], expected_doors[0][1]
    )));

    let command = |id| assert!(ui_activation::activate(&viewer, id), "{id}");
    command("training.duck.brain.best");
    command("training.duck.brain.pause");
    command("training.duck.brain.compare");
    command("training.duck.brain.step");
    wait(&|| viewer.get_duck_units().row_count() == 64);
    assert_eq!(viewer.get_duck_best_outputs().row_count(), 4);
    assert_eq!(viewer.get_duck_current_outputs().row_count(), 4);
    assert!(viewer.get_duck_current_id().contains(&winner));
    assert!(viewer.get_duck_current_details().contains("Left door"));
    assert!(viewer.get_duck_best_details().contains("Right door"));
    assert!(viewer.get_duck_polar().size().width > 0);
    let decision_tick = || {
        viewer
            .get_duck_inspect_status()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    let start = decision_tick();
    let recorded = scored.control_at(start);
    let outputs = if scored.case.entrance_right {
        viewer.get_duck_best_outputs()
    } else {
        viewer.get_duck_current_outputs()
    };
    assert_eq!(
        outputs.row_data(1).unwrap().applied,
        f32::from(u8::from(recorded.jump))
    );
    command("training.duck.brain.step");
    wait(&|| decision_tick() == start + u64::from(spacewars_ai::c3::DECISION_TICKS));
    command("training.duck.brain.current");
    assert_eq!(decision_tick(), start + 3);
    command("training.duck.brain.move");
    assert_eq!(viewer.get_duck_action(), 0);
    assert!(viewer.get_duck_signal().row_count() >= 2);
    command("training.duck.brain.close");
    assert!(!viewer.get_duck_compare());
    assert_eq!(viewer.get_duck_inspect(), -1);
}
