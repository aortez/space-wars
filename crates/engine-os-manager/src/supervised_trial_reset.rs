//! One owner-requested normal reboot of an exact early, uncommitted trial.
//! Shared trial guards and fault intents exclude an old boot or a second fault.
//! No root writer, selector, watchdog, recovery-history or approval mutation.
use crate::kiosk_health::{self, KioskExpectation, KioskHealth};
use crate::supervised_kiosk_hang::{self as hang, Expectation};
use crate::{Error, Result, linux};
use engine_ota::Sha256;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

pub const MAX_INPUT_BYTES: u64 = hang::MAX_INPUT_BYTES;
const LATEST_RESET_MS: u64 = 45_000;
const PLAN_LIFETIME_MS: u64 = 10_000;
const BUS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Case {
    TrialReset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Action {
    NormalReboot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    schema: u32,
    case: Case,
    action: Action,
    expectation: Expectation,
    selected_ms: u64,
    health: KioskHealth,
}

impl Plan {
    pub fn fingerprint(&self) -> Result<Sha256> {
        Ok(Sha256::digest(&serde_json::to_vec(&serde_json::to_value(
            self,
        )?)?))
    }

    fn check_time(&self, now: u64) -> Result<()> {
        require(
            self.schema == 1
                && now < LATEST_RESET_MS
                && now
                    .checked_sub(self.selected_ms)
                    .is_some_and(|age| age < PLAN_LIFETIME_MS)
                && self.health.uptime_ms <= self.selected_ms
                && self.selected_ms - self.health.uptime_ms < 5_000
                && self.health.boot_id == self.expectation.trial_boot_id()
                && self.health.build_id == self.expectation.image().build_id,
            "reset plan expired or health belongs to another boot/image",
        )
    }

    fn recheck(&self) -> Result<()> {
        self.expectation.check_early_trial()?;
        kiosk_health::qualification_instance(
            &self.health,
            &KioskExpectation::from_image(self.expectation.image())?,
        )?;
        self.check_time(now_ms()?)
    }
}

fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}

fn now_ms() -> Result<u64> {
    Ok(linux::clock_reading()?.boottime_us / 1000)
}

pub fn inspect(expectation: Expectation) -> Result<Plan> {
    // The shared inspector verifies root authority, immutable boot evidence,
    // observing transaction, early watchdog, recovery and current kiosk health.
    let observed = hang::inspect(expectation.clone())?;
    let plan = Plan {
        schema: 1,
        case: Case::TrialReset,
        action: Action::NormalReboot,
        expectation,
        selected_ms: now_ms()?,
        health: observed.health().clone(),
    };
    plan.check_time(now_ms()?)?;
    Ok(plan)
}

trait RebootBus {
    async fn parameter(&self) -> Result<String>;
    async fn reboot(&self) -> Result<String>;
}

struct NativeBus<'a> {
    properties: zbus::Proxy<'a>,
    manager: zbus::Proxy<'a>,
}

impl RebootBus for NativeBus<'_> {
    async fn parameter(&self) -> Result<String> {
        let value: zbus::zvariant::OwnedValue = self
            .properties
            .call(
                "Get",
                &("org.freedesktop.login1.Manager", "RebootParameter"),
            )
            .await
            .map_err(bus_error)?;
        value.try_into().map_err(bus_error)
    }

    async fn reboot(&self) -> Result<String> {
        let job: zbus::zvariant::OwnedObjectPath = self
            .manager
            .call("StartUnit", &("reboot.target", "replace-irreversibly"))
            .await
            .map_err(bus_error)?;
        Ok(job.to_string())
    }
}

fn bus_error(error: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("trial reset bus request: {error}"))
}

async fn request_once<T>(
    bus: &impl RebootBus,
    consume: impl FnOnce() -> Result<()>,
    recheck: impl Fn() -> Result<()>,
    deadline: impl FnOnce() -> Result<()>,
    queued: impl FnOnce(&str) -> Result<T>,
) -> Result<T> {
    require(
        bus.parameter().await?.is_empty(),
        "reboot parameter occupied; left unchanged",
    )?;
    recheck()?;
    consume()?;
    recheck()?;
    require(
        bus.parameter().await?.is_empty(),
        "reboot parameter changed; left unchanged",
    )?;
    // Keep the empty-parameter check after slow native/storage checks. Only a
    // cheap clock check follows it, so a slow reply cannot extend the window.
    deadline()?;
    let job = bus.reboot().await?;
    // A lost reply cannot retract a queued reboot. The consumed intent remains,
    // including if this post-request receipt cannot be written before shutdown.
    queued(&job)
}

pub fn reset(plan: Plan, selected: Sha256) -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0,
        "root-owner access required",
    )?;
    require(
        plan.fingerprint()? == selected,
        "reset plan differs from owner selection",
    )?;
    plan.check_time(now_ms()?)?;
    plan.recheck()?;
    let directory = hang::fault_directory()?;
    let transaction = plan.expectation.transaction();
    let outcome = futures_lite::future::block_on(async {
        let connection = futures_lite::future::or(
            async {
                zbus::connection::Builder::system()
                    .map_err(bus_error)?
                    .method_timeout(BUS_TIMEOUT)
                    .build()
                    .await
                    .map_err(bus_error)
            },
            async {
                async_io::Timer::after(BUS_TIMEOUT).await;
                Err(bus_error("connection deadline expired"))
            },
        )
        .await?;
        let bus = NativeBus {
            properties: zbus::Proxy::new(
                &connection,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.DBus.Properties",
            )
            .await
            .map_err(bus_error)?,
            manager: zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
            )
            .await
            .map_err(bus_error)?,
        };
        request_once(
            &bus,
            || {
                hang::create_record(
                    &directory,
                    &format!("{transaction}.json"),
                    &json!({
                        "schema": 1, "case": "trial-reset", "consumed": true,
                        "reboot_request": "not-yet-established", "plan_sha256": selected,
                        "plan": plan,
                    }),
                )
            },
            || plan.recheck(),
            || plan.check_time(now_ms()?),
            |job| {
                let receipt = json!({
                    "schema": 1, "case": "trial-reset", "action": "normal-reboot",
                    "plan_sha256": selected, "transaction": transaction,
                    "boot_id": plan.expectation.trial_boot_id(), "uptime_ms": now_ms()?,
                    "job": job, "reboot_queued": true, "automatic_recovery_qualified": false,
                    "next": "Observe the next boot and journal; never replay an uncertain reset",
                });
                hang::create_record(&directory, &format!("{transaction}.result.json"), &receipt)?;
                Ok(receipt)
            },
        )
        .await
    });
    outcome.map_err(|error| {
        Error::Invalid(format!(
            "{error}; retain fault intent and observe; never replay an uncertain reboot request"
        ))
    })
}

#[cfg(test)]
#[path = "supervised_trial_reset_tests.rs"]
mod tests;
