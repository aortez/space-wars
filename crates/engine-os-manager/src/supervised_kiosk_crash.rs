//! One consumed, bounded series of crashes of an exact uncommitted trial.
//! The shipped Restart=on-failure policy stays active. Each replacement process
//! is freshly authenticated and pinned; no service, selector or watchdog writes.
use crate::kiosk_health::{self, KioskExpectation, KioskHealth, KioskObserver};
use crate::supervised_kiosk_hang::{self as hang, Expectation, Pinned};
use crate::{Error, Result, linux};
use engine_ota::Sha256;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::File;
use std::time::Duration;

pub const MAX_INPUT_BYTES: u64 = hang::MAX_INPUT_BYTES;
const FIRST_LATEST_MS: u64 = 45_000;
const PLAN_LIFETIME_MS: u64 = 10_000;
const INTERVAL_MS: u64 = 30_000;
const MAX_GAP_MS: u64 = 45_000;
const LAST_AT_LEAST_MS: u64 = 125_000;
const LATEST_MS: u64 = 170_000;
const MAX_CRASHES: u32 = 6;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Policy {
    max_crashes: u32,
    interval_ms: u64,
    max_gap_ms: u64,
    last_at_least_ms: u64,
    latest_ms: u64,
}
impl Policy {
    fn fixed() -> Self {
        Self {
            max_crashes: MAX_CRASHES,
            interval_ms: INTERVAL_MS,
            max_gap_ms: MAX_GAP_MS,
            last_at_least_ms: LAST_AT_LEAST_MS,
            latest_ms: LATEST_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Case {
    KioskCrash,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Action {
    BoundedSigkillSeries,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    schema: u32,
    case: Case,
    action: Action,
    expectation: Expectation,
    selected_ms: u64,
    health: KioskHealth,
    policy: Policy,
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

impl Plan {
    pub fn fingerprint(&self) -> Result<Sha256> {
        Ok(Sha256::digest(&serde_json::to_vec(&serde_json::to_value(
            self,
        )?)?))
    }
    fn initial_time(&self, now: u64) -> Result<()> {
        require(
            self.schema == 1
                && self.policy == Policy::fixed()
                && now < FIRST_LATEST_MS
                && now
                    .checked_sub(self.selected_ms)
                    .is_some_and(|age| age < PLAN_LIFETIME_MS)
                && self.health.uptime_ms <= self.selected_ms
                && self.selected_ms - self.health.uptime_ms < 5_000
                && self.health.boot_id == self.expectation.trial_boot_id()
                && self.health.build_id == self.expectation.image().build_id,
            "crash plan expired, policy changed or health belongs to another boot/image",
        )
    }
}

pub fn inspect(expectation: Expectation) -> Result<Plan> {
    let observed = hang::inspect(expectation.clone())?;
    let plan = Plan {
        schema: 1,
        case: Case::KioskCrash,
        action: Action::BoundedSigkillSeries,
        expectation,
        selected_ms: now_ms()?,
        health: observed.health().clone(),
        policy: Policy::fixed(),
    };
    plan.initial_time(now_ms()?)?;
    Ok(plan)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    step: u32,
    health_before: KioskHealth,
    signal_requested_ms: u64,
    exit_observed_ms: u64,
}

#[derive(Clone, Copy)]
struct Window {
    step: u32,
    earliest_ms: u64,
    latest_ms: u64,
}
impl Window {
    fn check(&self, now: u64) -> Result<()> {
        require(
            self.step > 0
                && self.step <= MAX_CRASHES
                && now >= self.earliest_ms
                && now < self.latest_ms
                && now < LATEST_MS,
            "crash series count, spacing or absolute deadline exceeded",
        )
    }
}

fn same_process(a: &KioskHealth, b: &KioskHealth) -> bool {
    a.process_id == b.process_id
        && a.start_ticks == b.start_ticks
        && a.invocation_id == b.invocation_id
}

trait Series {
    fn wait_until(&mut self, uptime_ms: u64) -> Result<()>;
    fn kill_once(&mut self, window: Window, previous: &[Receipt]) -> Result<Receipt>;
}

// Any failure ends this consumed series. Never retry an observation or signal.
fn run_series(series: &mut impl Series) -> Result<Vec<Receipt>> {
    let mut receipts: Vec<Receipt> = Vec::new();
    for step in 1..=MAX_CRASHES {
        let window = match receipts.last() {
            None => Window {
                step,
                earliest_ms: 0,
                latest_ms: FIRST_LATEST_MS,
            },
            Some(last) => Window {
                step,
                earliest_ms: last.exit_observed_ms + INTERVAL_MS,
                latest_ms: (last.signal_requested_ms + MAX_GAP_MS).min(LATEST_MS),
            },
        };
        series.wait_until(window.earliest_ms)?;
        let receipt = series.kill_once(window, &receipts)?;
        window.check(receipt.signal_requested_ms)?;
        require(
            receipt.step == step
                && receipt.exit_observed_ms >= receipt.signal_requested_ms
                && receipt.exit_observed_ms - receipt.signal_requested_ms < 5_000
                && receipts
                    .iter()
                    .all(|r| !same_process(&r.health_before, &receipt.health_before)),
            "crash receipt is stale, repeated or lacks bounded exit observation",
        )?;
        let finished = receipt.signal_requested_ms >= LAST_AT_LEAST_MS;
        receipts.push(receipt);
        if finished {
            return Ok(receipts);
        }
    }
    Err(Error::Invalid(
        "crash series exhausted before its final window".into(),
    ))
}

fn one_crash(
    consume: impl FnOnce() -> Result<()>,
    recheck: impl FnOnce() -> Result<u64>,
    signal: impl FnOnce() -> Result<()>,
    exited: impl FnOnce() -> Result<u64>,
) -> Result<(u64, u64)> {
    consume()?;
    let requested = recheck()?;
    signal()?;
    let observed = exited()?;
    Ok((requested, observed))
}

struct Native<'a> {
    plan: &'a Plan,
    selected: Sha256,
    directory: File,
}
impl Series for Native<'_> {
    fn wait_until(&mut self, uptime_ms: u64) -> Result<()> {
        loop {
            let now = now_ms()?;
            if now >= uptime_ms {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis((uptime_ms - now).min(250)));
        }
    }

    fn kill_once(&mut self, window: Window, previous: &[Receipt]) -> Result<Receipt> {
        window.check(now_ms()?)?;
        self.plan.expectation.check_observing_trial()?;
        let expected = KioskExpectation::from_image(self.plan.expectation.image())?;
        let health = if window.step == 1 {
            self.plan.health.clone()
        } else {
            KioskObserver::default().observe(&expected)?
        };
        require(
            health.boot_id == self.plan.expectation.trial_boot_id()
                && health.build_id == self.plan.expectation.image().build_id
                && previous
                    .iter()
                    .all(|r| !same_process(&r.health_before, &health)),
            "replacement kiosk belongs to another boot/image or was already signalled",
        )?;
        let pinned = Pinned::open(health.process_id)?;
        kiosk_health::qualification_instance(&health, &expected)?;
        let transaction = self.plan.expectation.transaction();
        let step_name = format!("{transaction}.crash-{}", window.step);
        let (signal_requested_ms, exit_observed_ms) = one_crash(
            || {
                hang::create_record(
                    &self.directory,
                    &format!("{step_name}.json"),
                    &json!({
                        "schema":1, "case":"kiosk-crash", "transaction":transaction,
                        "boot_id":self.plan.expectation.trial_boot_id(), "plan_sha256":self.selected,
                        "step":window.step, "signal":"SIGKILL", "signal_delivery":"not-yet-established",
                        "health_before":health
                    }),
                )
            },
            || {
                self.plan.expectation.check_observing_trial()?;
                kiosk_health::qualification_instance(&health, &expected)?;
                // Only cheap clock checks follow the slow journal/image/process
                // checks. A stall cannot extend the initial or inter-crash bound.
                let now = now_ms()?;
                if window.step == 1 {
                    self.plan.initial_time(now)?;
                } else {
                    require(
                        now.checked_sub(health.uptime_ms)
                            .is_some_and(|age| age < 5_000),
                        "replacement health became stale",
                    )?;
                }
                window.check(now)?;
                Ok(now)
            },
            || pinned.signal(libc::SIGKILL),
            || {
                pinned.wait_exited()?;
                now_ms()
            },
        )?;
        let receipt = Receipt {
            step: window.step,
            health_before: health,
            signal_requested_ms,
            exit_observed_ms,
        };
        hang::create_record(
            &self.directory,
            &format!("{step_name}.result.json"),
            &json!({
                "schema":1, "case":"kiosk-crash", "transaction":transaction,
                "boot_id":self.plan.expectation.trial_boot_id(), "plan_sha256":self.selected,
                "signal":"SIGKILL", "pinned_exit_observed":true, "receipt":receipt
            }),
        )?;
        Ok(receipt)
    }
}

pub fn crash(plan: Plan, selected: Sha256) -> Result<Value> {
    require(
        unsafe { libc::geteuid() } == 0,
        "root-owner access required",
    )?;
    require(
        plan.fingerprint()? == selected,
        "crash plan differs from owner selection",
    )?;
    plan.initial_time(now_ms()?)?;
    plan.expectation.check_early_trial()?;
    kiosk_health::qualification_instance(
        &plan.health,
        &KioskExpectation::from_image(plan.expectation.image())?,
    )?;
    let directory = hang::fault_directory()?;
    let transaction = plan.expectation.transaction();
    // Hang, reset and crash share the same exclusive transaction intent. Even
    // partial consumption prevents another series after a lost SSH response.
    hang::create_record(
        &directory,
        &format!("{transaction}.json"),
        &json!({
            "schema":1, "case":"kiosk-crash", "consumed":true,
            "signal_delivery":"not-yet-established", "plan_sha256":selected, "plan":plan
        }),
    )?;
    let mut native = Native {
        plan: &plan,
        selected,
        directory,
    };
    let receipts = run_series(&mut native)?;
    let result = json!({
        "schema":1, "case":"kiosk-crash", "action":"bounded-sigkill-series", "signal":"SIGKILL",
        "transaction":transaction, "boot_id":plan.expectation.trial_boot_id(), "plan_sha256":selected,
        "series_completed":true, "policy":plan.policy, "receipts":receipts,
        "automatic_recovery_qualified":false,
        "next":"Observe automatic fallback; never resume or replay this consumed series"
    });
    hang::create_record(
        &native.directory,
        &format!("{transaction}.result.json"),
        &result,
    )?;
    Ok(result)
}

#[cfg(test)]
#[path = "supervised_kiosk_crash_tests.rs"]
mod tests;
