//! Unprivileged, read-only inventory; does not install, lock or reboot.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: bootstrap_probe (no arguments)".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&engine_ota::bootstrap::inspect_bootstrap_boot()?)?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap inventory requires Linux");
    std::process::exit(1);
}
