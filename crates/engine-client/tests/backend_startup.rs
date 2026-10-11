//! Presentation selection failures must not rewrite rendering preferences.
use std::{fs, process::Command};

#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
#[test]
fn shaping_probe_succeeds_without_a_backend_and_preserves_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let socket = directory.path().join("control.sock");
    let original = "deliberately not valid TOML";
    fs::write(&path, original).unwrap();
    let fonts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/clock/fonts");
    let output = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("SLINT_BACKEND", "nonexistent-backend")
        .env("SPACEWARS_CONTROL_SOCKET", &socket)
        .arg("--config-dir")
        .arg(directory.path())
        .args([
            "--benchmark-presentation",
            "--presentation-gpu-text",
            "--gpu-text-shaping",
            "--gpu-text-detail",
            "--presentation-repeats",
            "1",
            "--gpu-text-font-regular",
        ])
        .arg(fonts.join("DejaVuSans.ttf"))
        .arg("--gpu-text-font-bold")
        .arg(fonts.join("DejaVuSerif.ttf"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("exact_metrics=true"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("femto_word_face_cpu_ms"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!socket.exists());
}

#[test]
fn failed_backend_selection_preserves_settings_and_does_not_start_ipc() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let socket = directory.path().join("control.sock");
    let mut settings = engine_common::Settings::default();
    // This valid TOML needs launch normalization/writeback on successful startup.
    settings.launch.raster_scale = 99.0;
    settings.launch.renderer = engine_common::RendererSetting::Vector;
    let original = toml::to_string(&settings).unwrap();
    fs::write(&path, &original).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("SLINT_BACKEND", "nonexistent-backend")
        .env("SPACEWARS_CONTROL_SOCKET", &socket)
        .arg("--config-dir")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(error.contains("backend"), "{error}");
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!socket.exists());
}

#[test]
fn failed_upload_probe_does_not_touch_settings_or_start_ipc() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let socket = directory.path().join("control.sock");
    // Invalid TOML would be repaired by normal startup, but the standalone probe
    // must never read or rewrite it, even when GPU setup fails.
    let original = "deliberately not valid TOML";
    fs::write(&path, original).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("SPACEWARS_CONTROL_SOCKET", &socket)
        .arg("--config-dir")
        .arg(directory.path())
        .args([
            "--benchmark-presentation",
            "--presentation-upload",
            "--presentation-upload-device",
            "/nonexistent/spacewars-upload-test-render-node",
            "--presentation-frames",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!socket.exists());
}

#[test]
fn failed_geometry_probe_does_not_touch_settings_or_start_ipc() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.toml");
    let socket = directory.path().join("control.sock");
    let original = "deliberately not valid TOML";
    fs::write(&path, original).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("SPACEWARS_CONTROL_SOCKET", &socket)
        .arg("--config-dir")
        .arg(directory.path())
        .args([
            "--benchmark-presentation",
            "--presentation-geometry",
            "--geometry-device",
            "/nonexistent/spacewars-geometry-test-render-node",
            "--presentation-frames",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
    assert!(!socket.exists());
}
