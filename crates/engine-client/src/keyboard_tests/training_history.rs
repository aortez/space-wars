use super::*;
use engine_agent::{
    genome_library,
    training_job::{JobSpec, JobState, Recipe, TrainingManager},
};

fn wait(window: &MainWindow, predicate: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "job={} history={}",
            window.get_training_snapshot(),
            window.get_history_snapshot()
        );
        std::thread::sleep(Duration::from_millis(2));
        slint::platform::update_timers_and_animations();
    }
}
fn activate(w: &MainWindow, suffix: &str) {
    assert!(
        ui_activation::activate(w, &format!("training.history.{suffix}")),
        "{suffix}"
    );
    wait(w, || !w.get_history_busy());
}
fn capture(w: &MainWindow, name: &str) {
    let pixels = w.window().take_snapshot().unwrap();
    if let Some(root) = std::env::var_os("SPACEWARS_HISTORY_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        let file = std::fs::File::create(root.join(format!(
            "{name}-{}x{}.png",
            pixels.width(),
            pixels.height()
        )))
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

#[test]
fn history_browses_run_a_and_trials_its_winner_while_b_keeps_its_results() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let w = MainWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(800.0, 480.0));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("training");
    let first = TrainingManager::new(root.clone()).unwrap();
    let first_control = first.control();
    wait(&w, || first_control.snapshot().state == JobState::Ready);
    first_control
        .start(JobSpec {
            recipe: Recipe::StationaryArrival,
            seed: 21,
            population: 4,
            generations: 1,
            initial_genomes: Some(vec![
                genome_library::catalog(&root.join("genomes"))
                    .unwrap()
                    .entries[0]
                    .genome
                    .clone(),
            ]),
        })
        .unwrap();
    wait(&w, || first_control.snapshot().state == JobState::Completed);
    let a = first_control.snapshot();
    drop(first);
    let training = training_controls::install(&w, root.clone()).unwrap();
    let browser = genome_controls::install(&w, root.clone()).unwrap();
    install_ui_navigation(&w);
    install_keyboard_navigation(&w, Rc::new(RefCell::new(input::ClientInput::default())));
    w.set_launcher_visible(true);
    w.show().unwrap();
    wait(&w, || w.get_training_status_code() == "completed");
    assert!(ui_activation::activate(&w, "launcher.training"));
    assert!(ui_activation::activate(&w, "training.new"));
    w.set_training_population(8);
    w.set_training_generations(4);
    w.set_training_seed(21);
    training_controls::publish_setup(&w);
    assert!(ui_activation::activate(&w, "training.start"));
    wait(&w, || w.get_training_can_pause());
    let b_before: serde_json::Value = serde_json::from_str(&w.get_training_snapshot()).unwrap();
    w.set_training_focus(9);
    key(&w, Key::Return);
    wait(&w, || !w.get_history_busy());
    assert!(w.get_training_history());
    while w.get_history_id().as_str() != a.run_id.as_deref().unwrap() {
        activate(&w, "next");
    }
    // Recipe and seed match, so the active run needs its own exact identity.
    assert_eq!(
        w.get_training_run_id().as_str(),
        b_before["run_id"].as_str().unwrap()
    );
    assert!(
        w.get_history_current_label()
            .contains(w.get_training_run_id().as_str())
    );
    assert!(!w.get_history_is_current());
    let current = training_controls::inventory(&w)
        .into_iter()
        .find(|c| c.id == "training.snapshot")
        .unwrap();
    assert!(!current.enabled);
    let current: serde_json::Value =
        serde_json::from_str(current.value.as_deref().unwrap()).unwrap();
    assert_eq!(current["run_id"], b_before["run_id"]);
    assert!(!ui_activation::activate(&w, "training.pause"));
    assert!(!ui_activation::activate(&w, "training.stop"));
    assert!(!ui_activation::activate(&w, "training.start"));
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        w.window()
            .set_size(slint::LogicalSize::new(width as f32, height as f32));
        capture(&w, "history-run");
        w.set_history_focus(2);
        handle_ui_action(&w, spacewars_control::UiAction::Right);
        capture(&w, "history-build");
        handle_ui_action(&w, spacewars_control::UiAction::Left);
        activate(&w, "validation");
        activate(&w, "detail.next");
        capture(&w, "history-validation");
        activate(&w, "detail.next");
        capture(&w, "history-case");
        activate(&w, "runs");
    }
    // Last detail is the last completed generation's exact winner.
    activate(&w, "detail.previous");
    assert!(w.get_history_can_open());
    activate(&w, "pin");
    capture(&w, "history-comparison");
    activate(&w, "unpin");
    let id = w.get_history_id();
    activate(&w, "refresh");
    assert_eq!(w.get_history_id(), id);
    assert!(w.get_history_title().contains("winner"));
    // Touch the Open footer at each supported layout's last size (portrait).
    let width = w.window().size().width as f32;
    let height = w.window().size().height as f32;
    let pw = (width - 32.0).min(760.0);
    let ph = (height - 24.0).min(520.0);
    w.window().take_snapshot().unwrap();
    click(
        &w,
        (width - pw) / 2.0 + 24.0 + (pw - 78.0) / 8.0,
        (height - ph) / 2.0 + ph - 56.0,
    );
    wait(&w, || !w.get_genomes_busy());
    assert!(w.get_genomes_visible());
    assert_eq!(w.get_genomes_id().as_str(), a.best_id.as_deref().unwrap());
    assert!(ui_activation::activate(&w, "genomes.try"));
    wait(&w, || !w.get_genomes_busy());
    assert!(!w.get_genomes_receipt().is_empty());
    assert!(ui_activation::activate(&w, "genomes.back"));
    assert!(ui_activation::activate(&w, "launcher.training"));
    activate(&w, "open");
    activate(&w, "trials");
    capture(&w, "history-practice");
    assert!(w.get_history_body().starts_with("Practice"));
    activate(&w, "current");
    wait(&w, || w.get_training_status_code() == "completed");
    let b: serde_json::Value = serde_json::from_str(&w.get_training_snapshot()).unwrap();
    assert_eq!(b["run_id"], b_before["run_id"]);
    assert_ne!(b["run_id"], a.run_id.clone().unwrap());
    activate(&w, "open");
    activate(&w, "runs");
    assert_eq!(w.get_history_id().as_str(), a.run_id.as_deref().unwrap());
    activate(&w, "detail.previous");
    activate(&w, "pin");
    activate(&w, "next");
    activate(&w, "detail.previous");
    assert_eq!(w.get_history_id().as_str(), b["run_id"].as_str().unwrap());
    assert!(w.get_history_is_current());
    assert!(w.get_history_comparison().starts_with("Same protocol"));
    activate(&w, "validation");
    activate(&w, "detail.next");
    assert!(
        w.get_history_comparison()
            .contains("Different result types")
    );
    activate(&w, "unpin");
    activate(&w, "current");
    drop(browser);
    drop(training);
    // Same seed/config, no history or trials: identical selection and validation.
    let control_manager = TrainingManager::new(dir.path().join("control")).unwrap();
    let control = control_manager.control();
    wait(&w, || control.snapshot().state == JobState::Ready);
    control
        .start(JobSpec {
            recipe: Recipe::StationaryArrival,
            seed: 21,
            population: 8,
            generations: 4,
            initial_genomes: None,
        })
        .unwrap();
    wait(&w, || control.snapshot().state == JobState::Completed);
    let reference: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&control.snapshot()).unwrap()).unwrap();
    for field in [
        "best_id",
        "history",
        "validation",
        "evaluated_candidates",
        "completed_generations",
    ] {
        assert_eq!(b[field], reference[field], "{field}");
    }
    let _restored = training_controls::install(&w, root.clone()).unwrap();
    wait(&w, || w.get_training_status_code() == "completed");
    assert_eq!(
        engine_agent::training_history::catalog(&root, false)
            .unwrap()
            .keys
            .len(),
        2
    );
    assert_eq!(
        engine_agent::training_history::catalog(&root, true)
            .unwrap()
            .keys
            .len(),
        1
    );
}

#[test]
fn history_suspends_hidden_replays_and_retains_playback_on_return() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let w = MainWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(800.0, 480.0));
    let dir = tempfile::tempdir().unwrap();
    let _training = training_controls::install(&w, dir.path().into()).unwrap();
    install_ui_navigation(&w);
    w.set_launcher_visible(true);
    w.show().unwrap();
    wait(&w, || w.get_training_status_code() == "ready");
    assert!(ui_activation::activate(&w, "launcher.training"));
    w.set_training_population(4);
    w.set_training_generations(1);
    training_controls::publish_setup(&w);
    assert!(ui_activation::activate(&w, "training.start"));
    wait(&w, || w.get_training_status_code() == "completed");

    // Hide the preview before its first timer tick: no replay should start.
    assert!(ui_activation::activate(&w, "training.preview.best"));
    activate(&w, "open");
    let until = std::time::Instant::now() + Duration::from_millis(300);
    wait(&w, || std::time::Instant::now() >= until);
    assert!(w.get_training_preview_id().is_empty());

    activate(&w, "current");
    wait(&w, || w.get_training_preview_tick() >= 4);
    assert!(ui_activation::activate(&w, "training.preview.follow"));
    assert!(!w.get_training_preview_follow());
    activate(&w, "open");
    let frozen = w.get_training_preview_tick();
    let selected = w.get_training_preview_id();
    let until = std::time::Instant::now() + Duration::from_millis(300);
    wait(&w, || std::time::Instant::now() >= until);
    assert_eq!(w.get_training_preview_tick(), frozen);
    assert_eq!(w.get_training_preview_id(), selected);
    assert!(!ui_activation::activate(&w, "training.preview.best"));

    activate(&w, "current");
    wait(&w, || w.get_training_preview_tick() > frozen);
    assert_eq!(w.get_training_preview_id(), selected);
    assert_eq!(w.get_training_preview_mode(), 2);
    assert!(!w.get_training_preview_follow());
}

#[test]
fn corrupt_history_and_missing_exact_links_do_not_select_another_genome() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let w = MainWindow::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let _training = training_controls::install(&w, dir.path().into()).unwrap();
    let _browser = genome_controls::install(&w, dir.path().into()).unwrap();
    install_ui_navigation(&w);
    w.set_launcher_visible(true);
    w.show().unwrap();
    wait(&w, || w.get_training_status_code() == "ready");
    std::fs::write(dir.path().join("run-1.json"), b"broken").unwrap();
    assert!(ui_activation::activate(&w, "launcher.training"));
    w.set_training_focus(6);
    handle_ui_action(&w, spacewars_control::UiAction::Right);
    assert_eq!(w.get_training_focus(), 8); // Skip the absent current-job button.
    handle_ui_action(&w, spacewars_control::UiAction::Confirm);
    wait(&w, || !w.get_history_busy());
    assert!(w.get_history_error());
    assert!(!w.get_history_can_open());
    let selected = w.get_history_id();
    std::fs::remove_file(dir.path().join("run-1.json")).unwrap();
    activate(&w, "refresh");
    assert_eq!(w.get_history_id(), selected);
    assert!(w.get_history_error());
    activate(&w, "current");
    assert!(w.get_training_can_start());
    assert!(w.invoke_genomes_select(false, "0".repeat(64).into()));
    wait(&w, || !w.get_genomes_busy());
    assert_eq!(w.get_genomes_count(), 0);
    assert!(!w.get_genomes_compatible());
    assert!(!ui_activation::activate(&w, "genomes.try"));
    assert!(!w.get_genomes_error().is_empty());
}
