use super::*;
use engine_agent::{brain_assembly, genome_library};
use spacewars_control::UiAction;

fn footer(window: &MainWindow, index: usize, count: usize) {
    window.window().take_snapshot().unwrap();
    let size = window.window().size();
    let width = size.width as f32;
    let height = size.height as f32;
    let page_width = (width - 32.0).min(760.0);
    let page_height = (height - 24.0).min(520.0);
    let body = page_width - 48.0;
    let x = (width - page_width) / 2.0
        + 24.0
        + index as f32 * (body + 10.0) / count as f32
        + (body - (count - 1) as f32 * 10.0) / (2 * count) as f32;
    let y = (height - page_height) / 2.0 + page_height - 56.0;
    click(window, x, y);
}

fn capture(window: &MainWindow, name: &str) {
    let pixels = window.window().take_snapshot().unwrap();
    if let Some(path) = std::env::var_os("SPACEWARS_ASSEMBLY_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let file = std::fs::File::create(path.join(format!(
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
fn assemblies_save_copy_trial_link_and_reload_while_training_keeps_its_job() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, 480.0));
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("training");
    let _training = training_controls::install(&window, root.clone()).unwrap();
    let browser = genome_controls::install(&window, root.clone()).unwrap();
    install_ui_navigation(&window);
    install_keyboard_navigation(
        &window,
        Rc::new(RefCell::new(input::ClientInput::default())),
    );
    window.set_launcher_visible(true);
    window.show().unwrap();
    pump_worker_until(&window, || window.get_training_status_code() == "ready");
    assert!(ui_activation::activate(&window, "launcher.training"));
    window.set_training_population(32);
    window.set_training_generations(64);
    training_controls::publish_setup(&window);
    assert!(ui_activation::activate(&window, "training.start"));
    pump_worker_until(&window, || window.get_training_can_pause());
    let before: serde_json::Value = serde_json::from_str(&window.get_training_snapshot()).unwrap();
    key(&window, Key::Escape);
    assert!(ui_activation::activate(&window, "launcher.genomes"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(!window.get_genomes_brains());
    let original_component = window.get_genomes_id().to_string();
    // Keyboard Save, then the shared gamepad/keyboard menu path selects Copy.
    window.set_genomes_focus(5);
    key(&window, Key::Return);
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(window.get_genomes_brains());
    let original = window.get_genomes_id().to_string();
    assert_ne!(original, original_component);
    assert_eq!(window.get_genomes_dependency().as_str(), original_component);
    assert!(!ui_activation::activate(&window, "genomes.seed"));
    let original_path = root.join("assemblies").join(format!("{original}.json"));
    let original_bytes = std::fs::read(&original_path).unwrap();
    assert!(ui_activation::activate(&window, "genomes.try"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(window.get_genomes_result().starts_with("Arrived"));

    let mut copied = String::new();
    let mut replacement = String::new();
    for (index, (width, height)) in [(800, 480), (1024, 768), (480, 800)]
        .into_iter()
        .enumerate()
    {
        window
            .window()
            .set_size(slint::LogicalSize::new(width as f32, height as f32));
        slint::platform::update_timers_and_animations();
        capture(&window, "brains");
        let selected = window.get_genomes_id();
        let dependency = window.get_genomes_dependency();
        footer(&window, 2, 5); // Part links the exact dependency.
        pump_worker_until(&window, || !window.get_genomes_busy());
        assert!(!window.get_genomes_brains());
        assert_eq!(window.get_genomes_id(), dependency);
        capture(&window, "components");
        footer(&window, 2, 5); // Save deduplicates this already saved composition.
        pump_worker_until(&window, || !window.get_genomes_busy());
        assert!(window.get_genomes_brains());
        assert_eq!(window.get_genomes_id(), selected);

        window.set_genomes_focus(4);
        handle_ui_action(&window, UiAction::Confirm);
        assert!(window.get_genomes_copying());
        assert!(!ui_activation::activate(&window, "genomes.copy.save"));
        handle_ui_action(&window, UiAction::Right);
        assert_ne!(window.get_genomes_id(), dependency);
        assert!(window.get_genomes_saveable());
        capture(&window, "copy");
        if index == 0 {
            replacement = window.get_genomes_id().to_string();
            footer(&window, 0, 2);
            assert!(window.get_genomes_busy());
            key(&window, Key::Escape); // Saving continues after leaving the view.
            assert!(!window.get_genomes_visible());
            pump_worker_until(&window, || !window.get_genomes_busy());
            assert!(ui_activation::activate(&window, "launcher.genomes"));
            pump_worker_until(&window, || !window.get_genomes_busy());
            copied = window.get_genomes_id().to_string();
            assert_ne!(copied, original);
            assert_eq!(window.get_genomes_dependency().as_str(), replacement);
        } else {
            footer(&window, 1, 2); // Cancel preserves selection and both saved brains.
            assert_eq!(window.get_genomes_id().as_str(), copied);
        }
        assert!(!window.get_genomes_copying());
        assert!(window.get_genomes_brains());
        assert_eq!(window.get_genomes_count(), 2);
        footer(&window, 3, 5);
        pump_worker_until(&window, || !window.get_genomes_busy());
        assert_eq!(window.get_genomes_id().as_str(), copied);
        footer(&window, 4, 5);
        assert!(!window.get_genomes_visible());
        assert!(ui_activation::activate(&window, "launcher.genomes"));
        pump_worker_until(&window, || !window.get_genomes_busy());
    }
    assert_eq!(std::fs::read(&original_path).unwrap(), original_bytes);
    assert!(ui_activation::activate(&window, "genomes.practice.next"));
    assert!(window.get_genomes_moving());
    assert!(ui_activation::activate(&window, "genomes.try"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(window.get_genomes_result().starts_with("Arrived"));
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(window.get_genomes_receipt().as_str()).unwrap())
            .unwrap();
    assert_eq!(receipt["assembly"]["id"], copied);
    assert_eq!(receipt["episode"]["brain"]["controller"], replacement);
    assert_eq!(receipt["episode"]["case"]["name"], "moving_right");
    let after: serde_json::Value = serde_json::from_str(&window.get_training_snapshot()).unwrap();
    assert_eq!(after["run_id"], before["run_id"]);
    assert_eq!(after["spec"], before["spec"]);
    assert!(window.get_training_active());
    assert!(after["evaluated_candidates"].as_u64() > before["evaluated_candidates"].as_u64());
    assert!(ui_activation::activate(&window, "genomes.back"));
    assert!(ui_activation::activate(&window, "launcher.training"));
    assert!(ui_activation::activate(&window, "training.stop"));
    pump_worker_until(&window, || window.get_training_status_code() == "stopped");
    key(&window, Key::Escape);

    drop(browser);
    let _restored = genome_controls::install(&window, root.clone()).unwrap();
    assert!(ui_activation::activate(&window, "launcher.genomes"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(ui_activation::activate(&window, "genomes.brains"));
    assert_eq!(window.get_genomes_count(), 2);
    let mut ids = Vec::new();
    for _ in 0..2 {
        ids.push(window.get_genomes_id().to_string());
        assert!(ui_activation::activate(&window, "genomes.next"));
    }
    assert!(ids.contains(&original) && ids.contains(&copied));
}

#[test]
fn browser_rechecks_dependencies_before_trials_and_clears_unavailable_selection() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut entry = genome_library::catalog(&root.join("genomes"))
        .unwrap()
        .entries
        .remove(0);
    entry.genome.layers[0].biases[0] += 0.125;
    entry = genome_library::Entry::new(entry.genome, entry.origin).unwrap();
    genome_library::save(&root.join("genomes"), &entry).unwrap();
    let assembly = brain_assembly::create(root, &entry.id, "Dependency test").unwrap();
    let _browser = genome_controls::install(&window, root.into()).unwrap();
    install_ui_navigation(&window);
    window.set_launcher_visible(true);
    window.show().unwrap();
    assert!(ui_activation::activate(&window, "launcher.genomes"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(ui_activation::activate(&window, "genomes.brains"));
    assert!(window.get_genomes_compatible());
    let path = root.join("genomes").join(format!("{}.json", entry.id));
    std::fs::remove_file(path).unwrap();
    assert!(ui_activation::activate(&window, "genomes.try"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(
        window
            .get_genomes_error()
            .contains("Missing guidance genome")
    );
    assert!(window.get_genomes_receipt().is_empty());
    assert!(!root.join("assembly-trials").exists());
    assert!(ui_activation::activate(&window, "genomes.refresh"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert_eq!(window.get_genomes_id().as_str(), assembly.id);
    assert!(!window.get_genomes_compatible());
    assert!(!ui_activation::activate(&window, "genomes.try"));
    genome_library::save(&root.join("genomes"), &entry).unwrap();
    assert!(ui_activation::activate(&window, "genomes.refresh"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert!(window.get_genomes_compatible());
    assert!(
        !window
            .get_genomes_result()
            .contains("Missing guidance genome")
    );
    std::fs::write(
        root.join("assemblies")
            .join(format!("{}.json", assembly.id)),
        b"corrupt",
    )
    .unwrap();
    assert!(ui_activation::activate(&window, "genomes.refresh"));
    pump_worker_until(&window, || !window.get_genomes_busy());
    assert_eq!(window.get_genomes_count(), 0);
    assert!(window.get_genomes_id().is_empty());
    assert!(!window.get_genomes_compatible());
    assert!(!window.get_genomes_warning().is_empty());
}
