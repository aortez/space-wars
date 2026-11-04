//! Check the actual CLI's startup warning with its deployed default log filter.
#![cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::process::Command;

#[test]
fn missing_and_rejected_service_handoffs_are_visible_without_rust_log() {
    for rejected in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engine-os-manager"));
        command.arg("serve").env_remove("RUST_LOG");
        for name in ["LISTEN_PID", "LISTEN_FDS", "LISTEN_FDNAMES"] {
            command.env_remove(name);
        }
        if rejected {
            command
                .env("LISTEN_PID", "0")
                .env("LISTEN_FDS", "1")
                .env("LISTEN_FDNAMES", "connection");
        }
        // Stop at the existing root check, before any state or worker access.
        if unsafe { libc::geteuid() } == 0 {
            command.uid(65534).gid(65534);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        let expected = if rejected {
            "host network namespace handoff rejected"
        } else {
            "host network namespace handoff missing"
        };
        assert_eq!(stderr.matches(expected).count(), 1, "{stderr}");
        assert!(stderr.contains("idle shutdown disabled"), "{stderr}");
        // A pipe, like the journal, must receive plain text.
        assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
    }
}
