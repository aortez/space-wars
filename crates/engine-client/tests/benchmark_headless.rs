//! Exercise the public binary without any display, input device, or timing gate.
use std::{collections::BTreeMap, fs, process::Command};

fn run(case: &str, renderer: &str, warmup: u64) -> Vec<BTreeMap<String, String>> {
    let run = tempfile::tempdir().unwrap();
    let config = run.path().join("config");
    fs::create_dir(&config).unwrap();
    let saved = "[clock]\ntime_format='12-hour'\nevent_profile='demo'\nmarquee_message='IGNORE THIS'\n[clock.events]\nfalling=false\n";
    fs::write(config.join("settings.toml"), saved).unwrap();
    let report = run.path().join("report.csv");
    let result = Command::new(env!("CARGO_BIN_EXE_engine-client"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .args([
            "--scenario",
            "clock",
            "--benchmark-headless",
            "--seed",
            "7",
            "--benchmark-width",
            "80",
            "--benchmark-height",
            "64",
            "--benchmark-seconds",
            "4",
            "--raster-scale",
            "1.5",
            "--clock-benchmark-case",
            case,
            "--renderer",
            renderer,
            "--benchmark-warmup-seconds",
            &warmup.to_string(),
        ])
        .arg("--config-dir")
        .arg(&config)
        .arg("--benchmark-report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(config.join("settings.toml")).unwrap(),
        saved
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert_eq!(fs::read_to_string(report).unwrap(), output);
    let mut lines = output.lines();
    let header: Vec<_> = lines.next().unwrap().split(',').collect();
    let rows: Vec<_> = lines
        .map(|line| {
            let values: Vec<_> = line.split(',').collect();
            assert_eq!(values.len(), header.len());
            header
                .iter()
                .zip(values)
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 4);
    rows
}

#[test]
fn clock_csv_reports_real_dimensions_resources_and_cpu_only_work() {
    for renderer in ["vector", "raster"] {
        let rows = run("digit-slide", renderer, 0);
        for row in &rows {
            assert_eq!(row["benchmark_version"], "2");
            assert_eq!(row["seed"], "7");
            assert_eq!(row["viewport_width"], "80");
            assert_eq!(
                row["internal_width"],
                if renderer == "raster" { "120" } else { "80" }
            );
            assert_eq!(
                row["internal_height"],
                if renderer == "raster" { "96" } else { "64" }
            );
            assert_eq!(row["clock_case"], "digit-slide");
            assert_eq!(row["frames"], "60");
            assert_eq!(row["updates"], "60");
            assert_eq!(row["max_bodies"], "0");
            assert_eq!(row["max_colliders"], "0");
            assert!(row["max_scene_items"].parse::<usize>().unwrap() > 0);
            for field in [
                "avg_step_ms",
                "avg_render_ms",
                "avg_present_ms",
                "p95_total_ms",
            ] {
                let value = row[field].parse::<f64>().unwrap();
                assert!(value.is_finite() && value >= 0.0);
            }
        }
        assert!(rows.iter().any(|r| r["event_active_frames"] != "0"));
        assert!(rows.iter().any(|r| r["event_active_frames"] == "0"));
    }
}

#[test]
fn warmup_does_not_shift_the_measured_workload() {
    let cold = run("falling", "vector", 0);
    let warm = run("falling", "vector", 2);
    for (a, b) in cold.iter().zip(&warm) {
        for field in [
            "second",
            "frames",
            "scene_items",
            "max_scene_items",
            "max_bodies",
            "max_colliders",
            "event_active_frames",
        ] {
            assert_eq!(a[field], b[field], "{field}");
        }
    }
    assert!(cold.iter().any(|r| r["max_bodies"] != "0"));
}

#[test]
fn invalid_benchmark_arguments_are_rejected_without_a_display() {
    for args in [
        vec!["--clock-benchmark-case", "bogus"],
        vec!["--clock-benchmark-recipe", "bogus"],
        vec!["--benchmark-width", "0"],
        vec!["--benchmark-height", "9999999"],
        vec!["--benchmark-seconds", "0"],
        vec!["--benchmark-warmup-seconds", "121"],
        vec!["--benchmark", "--benchmark-headless"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_engine-client"))
            .args(args)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        // A display initialization failure also exits unsuccessfully. Require
        // Clap's argument-error code so that cannot mask accepted bad options.
        assert_eq!(
            result.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
