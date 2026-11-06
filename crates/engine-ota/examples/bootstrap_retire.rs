//! Move a provisioned unit's production records into a private retirement on
//! /data before a supervised boot-set upgrade. Never installs, provisions or
//! reboots; repeating it finishes an interrupted retirement or reports it.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [good] = arguments.as_slice() else {
        return Err(
            "usage: bootstrap_retire GOOD_BUILD_ID (the confirmed image whose records retire)"
                .into(),
        );
    };
    let good = good.to_str().ok_or("invalid build ID")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&engine_ota::bootstrap::retire_production_records(good)?)?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("production retirement requires Linux");
    std::process::exit(1);
}
