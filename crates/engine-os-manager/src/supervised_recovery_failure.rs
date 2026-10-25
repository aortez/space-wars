//! One consumed SIGKILL of the exact early trial recovery observer.
//! Never opens a watchdog, restarts a unit, reboots or changes recovery state.
use crate::supervised_kiosk_hang::{self as hang, Expectation, Pinned, TrialState};
use crate::{Error, Result, linux};
use engine_ota::Sha256;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

mod service;
pub const MAX_INPUT_BYTES: u64 = hang::MAX_INPUT_BYTES;
const LATEST_MS: u64 = 45_000;
const PLAN_LIFETIME_MS: u64 = 10_000;
const OBSERVATION_LATEST_MS: u64 = 300_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    unit: service::Unit,
    start_ticks: u64,
    boot_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Case {
    RecoveryServiceFailure,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Action {
    KillRecoveryObserver,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    schema: u32,
    case: Case,
    action: Action,
    expectation: Expectation,
    selected_ms: u64,
    instance: Instance,
    trial: TrialState,
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
fn root() -> Result<()> {
    require(
        unsafe { libc::geteuid() } == 0,
        "root-owner access required",
    )
}

impl Plan {
    pub fn fingerprint(&self) -> Result<Sha256> {
        Ok(Sha256::digest(&serde_json::to_vec(&serde_json::to_value(
            self,
        )?)?))
    }
    fn validate(&self, selected: Sha256) -> Result<()> {
        require(
            self.schema == 1
                && self.fingerprint()? == selected
                && self.instance.boot_id == self.expectation.trial_boot_id()
                && self.instance.start_ticks > 0
                && self.trial.watchdog_timeout_seconds == 210
                && self.trial.watchdog_armed_ms <= self.selected_ms
                && self.selected_ms < LATEST_MS,
            "recovery failure plan differs from the selected early trial",
        )?;
        self.instance.unit.running()
    }
    fn time(&self, now: u64) -> Result<()> {
        require(
            now < LATEST_MS
                && now
                    .checked_sub(self.selected_ms)
                    .is_some_and(|age| age < PLAN_LIFETIME_MS),
            "recovery failure selection or early injection window expired",
        )
    }
    fn live(&self) -> Result<()> {
        self.expectation.check_early_trial()?;
        require(
            self.expectation.uncommitted_trial_state(LATEST_MS)? == self.trial,
            "recovery journal or early watchdog changed",
        )?;
        require(
            service::instance(self.expectation.image())? == self.instance,
            "selected recovery service instance changed",
        )
    }
}

pub fn inspect(expectation: Expectation) -> Result<Plan> {
    root()?;
    expectation.check_early_trial()?;
    let trial = expectation.uncommitted_trial_state(LATEST_MS)?;
    let instance = service::instance(expectation.image())?;
    let plan = Plan {
        schema: 1,
        case: Case::RecoveryServiceFailure,
        action: Action::KillRecoveryObserver,
        expectation,
        selected_ms: now_ms()?,
        instance,
        trial,
    };
    plan.validate(plan.fingerprint()?)?;
    plan.live()?;
    plan.time(now_ms()?)?;
    Ok(plan)
}

fn one_kill(
    consume: impl FnOnce() -> Result<()>,
    recheck: impl FnOnce() -> Result<u64>,
    signal: impl FnOnce() -> Result<()>,
    exited: impl FnOnce() -> Result<u64>,
) -> Result<(u64, u64)> {
    consume()?;
    let requested = recheck()?;
    signal()?;
    let observed = exited()?;
    require(
        observed >= requested && observed - requested < 5_000,
        "recovery exit observation exceeded its bound",
    )?;
    Ok((requested, observed))
}

fn failed(plan: &Plan, latest: u64) -> Result<service::Unit> {
    require(
        plan.expectation.uncommitted_trial_state(latest)? == plan.trial,
        "recovery journal or early watchdog changed after the fault",
    )?;
    let unit = service::read()?;
    unit.failed(&plan.instance.unit)?;
    require(
        service::read()? == unit,
        "failed recovery unit changed during observation",
    )?;
    Ok(unit)
}

pub fn kill(plan: Plan, selected: Sha256) -> Result<Value> {
    root()?;
    plan.validate(selected)?;
    plan.time(now_ms()?)?;
    plan.live()?;
    let pinned = Pinned::open(plan.instance.unit.pid)?;
    let directory = hang::fault_directory()?;
    let transaction = plan.expectation.transaction();
    let (signal_requested_ms, exit_observed_ms) = one_kill(
        || {
            hang::create_record(
                &directory,
                &format!("{transaction}.json"),
                &json!({
                    "schema":1, "case":"recovery-service-failure", "consumed":true,
                    "signal_delivery":"not-yet-established", "plan_sha256":selected, "plan":plan
                }),
            )
        },
        || {
            plan.live()?;
            let now = now_ms()?;
            plan.time(now)?;
            Ok(now)
        },
        || pinned.signal(libc::SIGKILL),
        || {
            pinned.wait_exited()?;
            now_ms()
        },
    )?;
    // PID 1 may still be collecting the service's workers. Observe only; never
    // send another signal, reset-failed, stop/start the unit or extend a timer.
    let deadline = Instant::now() + Duration::from_secs(6);
    let unit = loop {
        let unit = service::read()?;
        require(
            Instant::now() < deadline && now_ms()? < LATEST_MS,
            "recovery failed-state observation exceeded its bound",
        )?;
        if unit.failed(&plan.instance.unit).is_ok() {
            break unit;
        }
        unit.settling(&plan.instance.unit)?;
        std::thread::sleep(Duration::from_millis(50));
    };
    require(
        failed(&plan, LATEST_MS)? == unit,
        "recovery failed result changed",
    )?;
    let failed_observed_ms = now_ms()?;
    require(
        failed_observed_ms >= exit_observed_ms && failed_observed_ms < LATEST_MS,
        "failed-state confirmation missed the early window",
    )?;
    let result = json!({"schema":1,"case":"recovery-service-failure","action":"kill-recovery-observer",
        "signal":"SIGKILL","transaction":transaction,"boot_id":plan.expectation.trial_boot_id(),
        "plan_sha256":selected,"instance_before":plan.instance,"trial":plan.trial,
        "signal_requested_ms":signal_requested_ms,"exit_observed_ms":exit_observed_ms,
        "failed_observed_ms":failed_observed_ms,"pinned_exit_observed":true,"unit_after":unit,
        "automatic_recovery_qualified":false,
        "next":"Observe the unchanged failed unit and automatic fallback; never replay this consumed fault"});
    hang::create_record(&directory, &format!("{transaction}.result.json"), &result)?;
    Ok(result)
}

/// Read-only follow-up on the existing trial connection, including after 180 s.
/// Its wider bound cannot admit a signal or refresh the early watchdog.
pub fn observe(plan: Plan, selected: Sha256) -> Result<Value> {
    root()?;
    plan.validate(selected)?;
    let unit = failed(&plan, OBSERVATION_LATEST_MS)?;
    let uptime_ms = now_ms()?;
    require(
        uptime_ms < OBSERVATION_LATEST_MS,
        "failed recovery observation window expired",
    )?;
    Ok(
        json!({"schema":1,"case":"recovery-service-failure","action":"observe-failed-recovery",
        "transaction":plan.expectation.transaction(),"boot_id":plan.expectation.trial_boot_id(),
        "plan_sha256":selected,"trial":plan.trial,"unit":unit,"uptime_ms":uptime_ms}),
    )
}

#[cfg(test)]
#[path = "supervised_recovery_failure_tests.rs"]
mod tests;
