//! Frozen presentation experiments must work without devices and preserve data.
use std::{collections::BTreeMap, fs, process::Command};

fn run(detail: bool, republish: bool) -> Vec<BTreeMap<String, String>> {
    let directory = tempfile::tempdir().unwrap();
    let settings = directory.path().join("settings.toml");
    // Intentionally malformed: the diagnostic must not even load or recover it.
    fs::write(&settings, "not valid toml {{").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-client"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("SLINT_BACKEND", "nonexistent-backend")
        .env(
            "SPACEWARS_CONTROL_SOCKET",
            directory.path().join("no-socket"),
        )
        .arg("--config-dir")
        .arg(directory.path())
        .args([
            "--benchmark-presentation",
            "--benchmark-width",
            "128",
            "--benchmark-height",
            "96",
            "--presentation-frames",
            "2",
            "--presentation-repeats",
            "2",
        ]);
    if detail {
        command.arg("--presentation-detail");
    }
    if republish {
        command.arg("--presentation-republish");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(settings).unwrap(), "not valid toml {{");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    let columns: Vec<_> = lines.next().unwrap().split(',').collect();
    let rows: Vec<BTreeMap<_, _>> = lines
        .map(|line| {
            let values: Vec<_> = line.split(',').collect();
            assert_eq!(values.len(), columns.len());
            columns
                .iter()
                .zip(values)
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 16);
    for pair in rows.chunks_exact(2) {
        assert_eq!(pair[0]["fixture"], pair[1]["fixture"]);
        assert_eq!(pair[0]["checksum"], pair[1]["checksum"]);
        assert_ne!(pair[0]["ui"], pair[1]["ui"]);
        for row in pair {
            assert_eq!(row["frames"], "2");
            assert_eq!(row["width"], "128");
            assert_eq!(row["height"], "96");
            assert_eq!(
                row["render_calls_per_frame"],
                if detail { "1.000" } else { "0.000" }
            );
            assert_eq!(row["detail"], detail.to_string());
            assert_eq!(
                row["publication"],
                if republish { "replace" } else { "retained" }
            );
            for (key, value) in row {
                if key.ends_with("_ms") || key.ends_with("_per_frame") {
                    let value: f64 = value.parse().unwrap();
                    assert!(value.is_finite() && value >= 0.0);
                }
            }
        }
    }
    rows
}

#[test]
fn frozen_bare_and_full_ui_match_without_settings_or_display() {
    let plain = run(false, false);
    let measured = run(true, false);
    let published = run(true, true);
    for ((plain, measured), published) in plain.iter().zip(&measured).zip(&published) {
        assert_eq!(plain["checksum"], measured["checksum"]);
        assert_eq!(plain["checksum"], published["checksum"]);
    }
}

#[test]
fn invalid_probe_limits_are_rejected_before_opening_a_display() {
    for args in [
        vec!["--presentation-frames", "0"],
        vec!["--presentation-repeats", "21"],
        vec!["--benchmark-width", "4096"],
        vec!["--scenario", "clock"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_engine-client"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .arg("--benchmark-presentation")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}
