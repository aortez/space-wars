#![cfg(target_os = "linux")]

use std::process::Command;

#[test]
fn installed_command_requires_restart_consent_before_contacting_the_service() {
    let binary = env!("CARGO_BIN_EXE_engine-os-manager");
    let help = Command::new(binary)
        .args(["install", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for operation in ["status", "offer", "confirm", "cancel"] {
        assert!(help.contains(operation));
    }
    let output = Command::new(binary)
        .args([
            "install",
            "confirm",
            "--service",
            &"1".repeat(64),
            "--sequence",
            "1",
            "--candidate",
            &"2".repeat(64),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--restart")
    );
}
