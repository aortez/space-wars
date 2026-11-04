//! Fixed-source recovery copies, not an installation or boot operation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_backup OPERATOR_SELECTED_BUNDLE_SHA256 (prepared fixed /data/spacewars-bootstrap/package; boot must be read-only)".into());
    };
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    let report = engine_ota::bootstrap::backup_installed_package(expected, |progress| {
        if let Ok(line) = serde_json::to_string(&progress) {
            eprintln!("{line}");
        }
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap backups require Linux");
    std::process::exit(1);
}
