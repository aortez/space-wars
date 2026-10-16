//! Explicit root-operator phase; does not enable a trial or reboot.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [package, raw, data, originals] = arguments.as_slice() else {
        return Err("usage: bootstrap_replace_legacy BUNDLE_SHA256 RAW_COPIES_SHA256 DATA_RECEIPT_SHA256 ORIGINALS_RECEIPT_SHA256 (independently verified exports; runtime account and legacy helpers stopped)".into());
    };
    let parse =
        |value: &std::ffi::OsStr| -> Result<engine_ota::Sha256, Box<dyn std::error::Error>> {
            Ok(value
                .to_str()
                .ok_or("invalid digest")?
                .to_owned()
                .try_into()?)
        };
    let result = engine_ota::bootstrap::replace_installed_legacy(
        parse(package)?,
        parse(raw)?,
        parse(data)?,
        parse(originals)?,
        |event| {
            if let Ok(line) = serde_json::to_string(&event) {
                eprintln!("{line}");
            }
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap replacement requires Linux");
    std::process::exit(1);
}
