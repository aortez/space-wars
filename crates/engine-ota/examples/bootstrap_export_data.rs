//! Stream private recovery data to the operator without writing a device copy.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_export_data OPERATOR_SELECTED_BUNDLE_SHA256 > PRIVATE_ARCHIVE.tar.partial (prepared fixed package; kiosk and device-state writers must be stopped; progress and completion report go to stderr)".into());
    };
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    let report = engine_ota::bootstrap::export_installed_data(
        expected,
        std::io::stdout().lock(),
        |bytes| eprintln!("{{\"stage\":\"exporting\",\"bytes\":{bytes}}}"),
    )?;
    eprintln!("{}", serde_json::to_string(&report)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap preservation requires Linux");
    std::process::exit(1);
}
