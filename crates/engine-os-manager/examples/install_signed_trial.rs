//! Separately built owner tool. inspect selects cached signed/staged bytes;
//! install consumes that selection, writes the inactive root and requests reboot.
#[cfg(target_os = "linux")]
mod linux {
    use engine_os_manager::{
        linux,
        time::{TimeReason, TimeRecord, TimeTracker},
    };
    use engine_ota::{Channel, Error, Result, Sha256, UsableTime, supervised_install};
    use std::io::Read;

    pub async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Check before reading stdin or opening device state, including inspect.
        if unsafe { libc::geteuid() } != 0 {
            return Err(
                "signed trial requires separately authenticated root operator access".into(),
            );
        }
        let args: Vec<_> = std::env::args().skip(1).collect();
        let selected = match args.as_slice() {
            [operation, digest] if operation == "install" => {
                let digest = Sha256::try_from(digest.clone()).map_err(std::io::Error::other)?;
                let mut bytes = Vec::new();
                std::io::stdin().take(supervised_install::MAX_SELECTION_BYTES + 1).read_to_end(&mut bytes)?;
                Some(supervised_install::Selection::parse(&bytes, digest)?)
            }
            [operation, channel] | [operation, channel, _] if operation == "inspect"
                && matches!(channel.as_str(), "release" | "nightly") => None,
            _ => return Err("usage: install_signed_trial inspect release|nightly [retry-payload-sha256] | install SELECTION_SHA256 < selection.json".into()),
        };
        let observation = linux::observe_sync().map_err(std::io::Error::other)?;
        let mut tracker = TimeTracker::new(
            linux::boot_id()?,
            TimeRecord::default(),
            TimeReason::WaitingForNetworkSync,
            observation.now,
            None,
        );
        if !tracker.observe(&observation) {
            return Err("a current native successful NTP receipt is required".into());
        }
        let time = || -> Result<UsableTime> {
            let now = linux::clock_reading().map_err(|e| Error::Boot(e.to_string()))?;
            tracker.tick(now);
            let status = tracker.status(now);
            if !status.usable {
                return Err(Error::WaitingForTime);
            }
            UsableTime::from_unix_microseconds(status.utc_us.ok_or(Error::WaitingForTime)?)
        };
        if let Some(selection) = selected {
            let id = supervised_install::install(
                selection,
                time,
                |progress| {
                    eprintln!(
                        "{}",
                        serde_json::to_string(&progress).expect("progress serializes")
                    )
                },
                engine_os_manager::supervised_install::request_reboot,
            )
            .await?;
            println!(
                "{}",
                serde_json::json!({"transaction": id, "reboot_requested": true,
                "automatic_recovery_qualification_granted": false})
            );
        } else {
            let channel = if args[1] == "release" {
                Channel::Release
            } else {
                Channel::Nightly
            };
            let retry = args
                .get(2)
                .map(|p| Sha256::try_from(p.clone()))
                .transpose()
                .map_err(std::io::Error::other)?;
            let selection = supervised_install::inspect(channel, retry, time).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                "selection_sha256": selection.fingerprint(), "selection": selection,
                "scope": "Explicit supervised signed trial; install writes the inactive root and restarts"}))?
            );
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run().await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("signed trial installation requires Linux");
    std::process::exit(1);
}
