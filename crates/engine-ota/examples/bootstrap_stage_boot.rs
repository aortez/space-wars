//! Supervised root-only boot staging; does not request a trial or reboot.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [package, raw, data, originals, written, connection] = arguments.as_slice() else {
        return Err("usage: bootstrap_stage_boot BUNDLE_SHA256 RAW_COPIES_SHA256 DATA_RECEIPT_SHA256 ORIGINALS_RECEIPT_SHA256 ROOT_INSTALL_SHA256 'CLIENT_IP CLIENT_PORT SERVER_IP SERVER_PORT' (root operator; independent recovery exports required)".into());
    };
    let result = engine_ota::bootstrap::stage_bootstrap_boot(
        package.clone().try_into()?,
        raw.clone().try_into()?,
        data.clone().try_into()?,
        originals.clone().try_into()?,
        written.clone().try_into()?,
        engine_ota::bootstrap::SshCheckConnection::parse(connection)?,
        |event| {
            if let Ok(json) = serde_json::to_string(&event) {
                eprintln!("{json}");
            }
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap boot staging requires Linux");
    std::process::exit(1);
}
