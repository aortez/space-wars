//! Fixed-source private data/configuration recovery archive, never a restore.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_backup_data OPERATOR_SELECTED_BUNDLE_SHA256 (prepared fixed package; kiosk and device-state writers must be stopped)".into());
    };
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    let report = engine_ota::bootstrap::backup_installed_data(expected, |progress| {
        if let Ok(line) = serde_json::to_string(&progress) {
            eprintln!("{line}");
        }
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap preservation requires Linux");
    std::process::exit(1);
}
