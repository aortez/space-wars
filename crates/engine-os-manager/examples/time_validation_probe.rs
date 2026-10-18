//! Read live provider evidence and validate it in memory without changing the
//! clock, installed service, persistent time record or OTA trust state.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_os_manager::linux;
    use engine_os_manager::time::{TimeReason, TimeRecord, TimeTracker};
    if std::env::args_os().len() != 1 {
        return Err("usage: time_validation_probe (no arguments)".into());
    }
    let boot_id = linux::boot_id()?;
    let observation = linux::observe_sync().map_err(std::io::Error::other)?;
    let mut tracker = TimeTracker::new(
        boot_id.clone(),
        TimeRecord::default(),
        TimeReason::WaitingForNetworkSync,
        observation.now,
        None,
    );
    let accepted = tracker.observe(&observation);
    let duplicate_accepted = tracker.observe(&observation);
    let result = serde_json::json!({
        "schema": 1,
        "boot_id": boot_id,
        "observation": observation,
        "accepted": accepted,
        "duplicate_accepted": duplicate_accepted,
        "in_memory_status": tracker.status(observation.now),
        "persistent_time_state_changed": false,
        "installed_service_replaced": false,
        "ota_trust_provisioned": false,
        "scope": "Read-only live time-validation probe; not production service or signed-install qualification"
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    if !accepted || duplicate_accepted {
        return Err("live provider evidence did not establish one usable clock observation".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("time validation probe requires Linux");
    std::process::exit(1);
}
