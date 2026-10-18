//! Read-only root-operator verification; supplied SSH context is not authority.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [package, raw, data, originals, connection] = arguments.as_slice() else {
        return Err("usage: bootstrap_check_legacy_access BUNDLE_SHA256 RAW_COPIES_SHA256 DATA_RECEIPT_SHA256 ORIGINALS_RECEIPT_SHA256 'CLIENT_IP CLIENT_PORT SERVER_IP SERVER_PORT' (observed SSH context, not authorization)".into());
    };
    let result = engine_ota::bootstrap::check_installed_legacy_access(
        package.clone().try_into()?,
        raw.clone().try_into()?,
        data.clone().try_into()?,
        originals.clone().try_into()?,
        engine_ota::bootstrap::SshCheckConnection::parse(connection)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap access checks require Linux");
    std::process::exit(1);
}
