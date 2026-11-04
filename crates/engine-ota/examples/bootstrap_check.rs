//! Explicit root/operator package preflight. No install or reboot operation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_check OPERATOR_SELECTED_BUNDLE_SHA256 (fixed /data/spacewars-bootstrap/package)".into());
    };
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&engine_ota::bootstrap::check_installed_package(expected)?)?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap checking requires Linux");
    std::process::exit(1);
}
