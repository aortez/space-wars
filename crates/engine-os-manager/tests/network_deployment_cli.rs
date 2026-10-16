#![cfg(target_os = "linux")]

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn deployment_cli_rejects_bad_stdin_before_contacting_the_service_without_echoing_it() {
    let secret = "a".repeat(70);
    for operation in ["admit", "renew", "release"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_engine-os-manager"))
            .args(["network", "deployment", operation])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(child.stdin.take().unwrap(), "{secret}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("deployment capability line on stdin"));
        assert!(!error.contains(&secret));
    }
}
