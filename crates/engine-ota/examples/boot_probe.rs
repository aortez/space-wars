//! Read-only live boot evidence; never opens a block device or watchdog.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        serde_json::to_string_pretty(&engine_ota::boot_linux::inspect_boot()?)?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("boot evidence requires Linux");
    std::process::exit(1);
}
