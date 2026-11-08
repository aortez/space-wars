//! Read-only verification of independently exported original migration files.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [path, digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_verify_legacy EXPORTED_ACTIVE_DIRECTORY DEVICE_REPORTED_RECEIPT_SHA256".into());
    };
    // The verifiers read the exported active tree, not a direct archive.
    let exported = std::path::Path::new(path);
    if !exported.join("binding.json").is_file() || !exported.join("legacy-root").is_dir() {
        return Err(format!(
            "{} is not an exported active directory: expected binding.json and legacy-root/ (the preserved original files); export the device's active tree, not a direct archive",
            exported.display()
        )
        .into());
    }
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    let result = engine_ota::bootstrap::verify_legacy_export(std::path::Path::new(path), expected)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap export verification requires Linux");
    std::process::exit(1);
}
