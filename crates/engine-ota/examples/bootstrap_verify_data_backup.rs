//! Read-only independent-export check. No archive extraction or live mutation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [path, digest] = arguments.as_slice() else {
        return Err("usage: bootstrap_verify_data_backup EXPORTED_ACTIVE_DIRECTORY DEVICE_REPORTED_RECEIPT_SHA256".into());
    };
    let expected = digest
        .to_str()
        .ok_or("invalid digest")?
        .to_owned()
        .try_into()?;
    let checked =
        engine_ota::bootstrap::verify_data_backup_export(std::path::Path::new(path), expected)?;
    println!("{}", serde_json::to_string_pretty(&checked)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap export verification requires Linux");
    std::process::exit(1);
}
