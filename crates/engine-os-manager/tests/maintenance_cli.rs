#![cfg(target_os = "linux")]

#[test]
fn runtime_identity_cannot_invoke_root_maintenance() {
    // CI and developer tests run as an ordinary user. The separate namespace
    // command fixture covers the root entry point without exposing host disks.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    for args in [
        vec!["initialize-access"],
        vec!["maintenance", "import-root-keys"],
        vec!["maintenance", "fast", "--check"],
        vec!["maintenance", "renderer", "gpu"],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_engine-os-manager"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("root-owned service"));
        assert!(result.stdout.is_empty());
    }
}
