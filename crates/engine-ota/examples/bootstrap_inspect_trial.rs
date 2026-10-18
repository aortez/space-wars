//! Read the existing trial journal under the operator/service mount namespace.
//! Takes the install lock; never advances the journal, touches a watchdog or reboots.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("usage: bootstrap_inspect_trial (no arguments)".into());
    }
    let lock = engine_ota::install_lock::InstallLock::acquire()?;
    let store =
        engine_ota::bootstrap::TrialStore::open(&lock)?.ok_or("no active bootstrap trial")?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "phase": store.phase(),
            "trial_id": store.trial_id(),
            "failure": store.failure(),
            "journal_mutated": false,
            "watchdog_opened": false,
            "reboot_requested": false,
        }))?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap inspection requires Linux");
    std::process::exit(1);
}
