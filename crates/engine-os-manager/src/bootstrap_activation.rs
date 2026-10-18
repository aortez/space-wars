//! Explicit supervised activation, outside the unprivileged device socket.
use engine_ota::bootstrap::{self, BootActivationReport, SshCheckConnection, TrialActivation};
use engine_ota::{Error, Result, Sha256};
use std::time::Duration;

const TRIAL_PARAMETER: &str = "0 tryboot";
const BUS_TIMEOUT: Duration = Duration::from_secs(2);

pub fn activate(
    package: Sha256,
    written_root: Sha256,
    trial: Sha256,
    ssh_connection: &str,
) -> Result<BootActivationReport> {
    // Also enforced by the library install lock, before opening installed paths.
    if unsafe { libc::geteuid() } != 0 {
        return Err(error("bootstrap activation requires a root operator"));
    }
    let connection = SshCheckConnection::parse(ssh_connection)?;
    bootstrap::activate_bootstrap_boot(
        package,
        written_root,
        trial,
        connection,
        |stage| {
            eprintln!(
                "{}",
                serde_json::to_string(&stage).expect("progress serializes")
            )
        },
        |permit| request_trial(permit),
    )
}

pub(crate) fn request_trial(permit: &mut impl RequestPermit) -> Result<()> {
    futures_lite::future::block_on(async {
        let connection = futures_lite::future::or(
            async {
                zbus::connection::Builder::system()
                    .map_err(error)?
                    .method_timeout(BUS_TIMEOUT)
                    .build()
                    .await
                    .map_err(error)
            },
            async {
                async_io::Timer::after(BUS_TIMEOUT).await;
                Err(error("timed out connecting to the system bus"))
            },
        )
        .await?;
        request_on(&connection, permit).await
    })
}

// Crate-private: only native permit adapters reach this handshake.
pub(crate) trait RequestPermit {
    fn begin(&mut self) -> Result<()>;
    fn authorize(&mut self) -> Result<()>;
    fn queued(&mut self) -> Result<()>;
    fn fail(&mut self) -> Result<()>;
}

impl RequestPermit for TrialActivation<'_> {
    fn begin(&mut self) -> Result<()> {
        self.begin()
    }
    fn authorize(&mut self) -> Result<()> {
        self.authorize_reboot()
    }
    fn queued(&mut self) -> Result<()> {
        self.reboot_queued()
    }
    fn fail(&mut self) -> Result<()> {
        self.fail_request()
    }
}

async fn parameter(properties: &zbus::Proxy<'_>) -> Result<String> {
    // An uncached Get is required: logind does not signal every external change.
    let value: zbus::zvariant::OwnedValue = properties
        .call(
            "Get",
            &("org.freedesktop.login1.Manager", "RebootParameter"),
        )
        .await
        .map_err(error)?;
    value.try_into().map_err(error)
}

async fn clear_owned_parameter(
    login: &zbus::Proxy<'_>,
    properties: &zbus::Proxy<'_>,
) -> Result<()> {
    match parameter(properties).await?.as_str() {
        "" => return Ok(()),
        TRIAL_PARAMETER => {}
        _ => return Err(error("another reboot parameter is present; left unchanged")),
    }
    login
        .call::<_, _, ()>("SetRebootParameter", &("",))
        .await
        .map_err(error)?;
    if !parameter(properties).await?.is_empty() {
        return Err(error("bootstrap reboot parameter cleanup did not clear"));
    }
    Ok(())
}

async fn request_on(connection: &zbus::Connection, permit: &mut impl RequestPermit) -> Result<()> {
    let login = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(error)?;
    let properties = zbus::Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.DBus.Properties",
    )
    .await
    .map_err(error)?;
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(error)?;
    if !parameter(&properties).await?.is_empty() {
        return Err(error(
            "bootstrap activation requires an empty reboot parameter; left unchanged",
        ));
    }
    permit.begin()?;
    let mut parameter_attempted = false;
    let outcome = async {
        permit.authorize()?;
        // Recheck after potentially slow storage validation before setting it.
        if !parameter(&properties).await?.is_empty() {
            return Err(error(
                "reboot parameter changed before bootstrap activation",
            ));
        }
        parameter_attempted = true;
        login
            .call::<_, _, ()>("SetRebootParameter", &(TRIAL_PARAMETER,))
            .await
            .map_err(error)?;
        if parameter(&properties).await? != TRIAL_PARAMETER {
            return Err(error("bootstrap reboot parameter did not match"));
        }
        permit.authorize()?;
        if parameter(&properties).await? != TRIAL_PARAMETER {
            return Err(error(
                "reboot parameter changed before bootstrap reboot job",
            ));
        }
        let _: zbus::zvariant::OwnedObjectPath = manager
            .call("StartUnit", &("reboot.target", "replace-irreversibly"))
            .await
            .map_err(error)?;
        permit.queued()
    }
    .await;
    if let Err(cause) = outcome {
        // A timeout cannot cancel a message already delivered to systemd.
        // Durable failure makes a late trial return to the retained default.
        let failure = permit.fail();
        let cleanup = if parameter_attempted {
            clear_owned_parameter(&login, &properties).await
        } else {
            Ok(())
        };
        let mut detail =
            format!("{cause}; bootstrap attempt consumed; operator reconciliation required");
        if let Err(e) = failure {
            detail.push_str(&format!("; failure record: {e}"));
        }
        if let Err(e) = cleanup {
            detail.push_str(&format!("; parameter cleanup: {e}"));
        }
        return Err(error(detail));
    }
    Ok(())
}

fn error(value: impl std::fmt::Display) -> Error {
    Error::Boot(value.to_string())
}

#[cfg(test)]
#[path = "bootstrap_activation_tests.rs"]
mod tests;
