//! One-use owner SIGSTOP for a precisely selected, early, uncommitted trial.
//! No writer, boot-selector, watchdog, reboot, approval or recovery-lock access.
use crate::kiosk_health::{self, KioskExpectation, KioskHealth, KioskObserver};
use crate::{Error, Result, linux};
use engine_ota::boot_linux::{BootEvidence, inspect_boot};
use engine_ota::recovery::{BootBinding, Phase, RecoveryRecord, SlotImage};
use engine_ota::{ImageIdentity, Sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, Instant};

pub const MAX_INPUT_BYTES: u64 = 32 * 1024;
const LATEST_STOP_MS: u64 = 45_000;
const PLAN_LIFETIME_MS: u64 = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    schema: u32,
    binding: BootBinding,
    kernel_build_id: String,
    image: ImageIdentity,
    transaction: Sha256,
    origin_boot_id: String,
    trial_boot_id: String,
    previous_good: SlotImage,
    candidate: SlotImage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    expectation: Expectation,
    selected_ms: u64,
    health: KioskHealth,
}
impl Plan {
    pub(crate) fn health(&self) -> &KioskHealth {
        &self.health
    }

    pub fn fingerprint(&self) -> Result<Sha256> {
        // Canonical object ordering also permits transport by the Python runner.
        Ok(Sha256::digest(&serde_json::to_vec(&serde_json::to_value(
            self,
        )?)?))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: RecoveryRecord,
    checksum: Sha256,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Early {
    schema: u32,
    boot_id: String,
    trial_id: Sha256,
    armed_boottime_ms: u64,
    timeout_seconds: u32,
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn require(value: bool, message: &str) -> Result<()> {
    if !value {
        return Err(invalid(message));
    }
    Ok(())
}
fn root() -> Result<()> {
    require(
        unsafe { libc::geteuid() } == 0,
        "root-owner access required",
    )
}
fn now_ms() -> Result<u64> {
    Ok(linux::clock_reading()?.boottime_us / 1000)
}

fn recovery_running() -> Result<()> {
    futures_lite::future::block_on(futures_lite::future::or(
        async {
            let connection = zbus::Connection::system()
                .await
                .map_err(|e| invalid(&e.to_string()))?;
            let properties = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1/unit/spacewars_2drecovery_2eservice",
                "org.freedesktop.DBus.Properties",
            )
            .await
            .map_err(|e| invalid(&e.to_string()))?;
            let active: zbus::zvariant::OwnedValue = properties
                .call("Get", &("org.freedesktop.systemd1.Unit", "ActiveState"))
                .await
                .map_err(|e| invalid(&e.to_string()))?;
            let pid: zbus::zvariant::OwnedValue = properties
                .call("Get", &("org.freedesktop.systemd1.Service", "MainPID"))
                .await
                .map_err(|e| invalid(&e.to_string()))?;
            require(
                String::try_from(active).ok().as_deref() == Some("activating")
                    && u32::try_from(pid).ok().is_some_and(|pid| pid > 1),
                "production recovery service is not running its trial observer",
            )
        },
        async {
            async_io::Timer::after(Duration::from_secs(1)).await;
            Err(invalid("recovery service observation timed out"))
        },
    ))
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

impl Expectation {
    pub(crate) fn transaction(&self) -> Sha256 {
        self.transaction
    }

    pub(crate) fn trial_boot_id(&self) -> &str {
        &self.trial_boot_id
    }

    pub(crate) fn image(&self) -> &ImageIdentity {
        &self.image
    }

    pub(crate) fn check_early_trial(&self) -> Result<()> {
        guard(self, LATEST_STOP_MS)
    }

    pub(crate) fn check_observing_trial(&self) -> Result<()> {
        guard(self, engine_ota::recovery::TRIAL_DEADLINE_MS)
    }

    // Read-only evidence for the intentionally failed recovery-observer case.
    // Signal admission still uses check_early_trial with the running service.
    pub(crate) fn uncommitted_trial_state(&self, latest_ms: u64) -> Result<TrialState> {
        trial_state(self, latest_ms)
    }

    fn validate(&self) -> Result<()> {
        self.image.validate()?;
        require(
            self.schema == 1
                && !self.image.source.dirty
                && uuid(&self.origin_boot_id)
                && uuid(&self.trial_boot_id)
                && self.origin_boot_id != self.trial_boot_id
                && self.kernel_build_id.len() == 40
                && self
                    .kernel_build_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && self.candidate.slot == self.previous_good.slot.other()
                && self.candidate.generation > self.previous_good.generation
                && self.candidate.payload != self.previous_good.payload
                && self.candidate.build_id == self.image.build_id
                && self.candidate.data == self.image.data
                && self.binding.boot_identity == self.image.boot_identity
                && self.image.recovery_protocol == 2
                && [
                    self.transaction,
                    self.candidate.payload,
                    self.candidate.expanded,
                    self.binding.device_id,
                    self.binding.layout_id,
                    self.binding.hardware_profile,
                ]
                .iter()
                .all(|v| v.as_bytes() != &[0; 32]),
            "invalid or unbound hang-trial expectation",
        )
    }

    fn matches(
        &self,
        boot: &BootEvidence,
        journal: &Envelope,
        early: &Early,
        latest_ms: u64,
    ) -> Result<()> {
        self.validate()?;
        let b = &boot.observation;
        let record = &journal.record;
        record.validate().map_err(engine_ota::Error::from)?;
        require(
            record.fingerprint() == journal.checksum,
            "recovery checksum mismatch",
        )?;
        // Pure transition validation checks the journal's private boot binding.
        // Never acquire its lock or apply the returned plan: recovery owns both.
        record.reconcile(b).map_err(engine_ota::Error::from)?;
        let tx = record
            .transaction()
            .ok_or_else(|| invalid("missing trial transaction"))?;
        require(
            b.boot_id == self.trial_boot_id
                && b.boot_id != self.origin_boot_id
                && b.binding == self.binding
                && boot.kernel_build_id == self.kernel_build_id
                && boot.image == self.image
                && b.recovery_protocol == 2
                && b.tryboot
                && b.trial_id == Some(self.transaction)
                && b.actual_slot == Some(self.candidate.slot)
                && b.default_slot == Some(self.previous_good.slot)
                && b.actual_build_id.as_deref() == Some(self.image.build_id.as_str())
                && b.uptime_ms < latest_ms
                && record.good() == &self.previous_good
                && tx.id == self.transaction
                && tx.phase == Phase::Observing
                && tx.origin_boot_id == self.origin_boot_id
                && tx.trial_boot_id.as_deref() == Some(self.trial_boot_id.as_str())
                && tx.candidate == self.candidate
                && tx.previous_good == self.previous_good
                && tx.health.is_none()
                && tx.failure.is_none(),
            "not the selected early uncommitted candidate trial",
        )?;
        require(
            early.schema == 1
                && early.boot_id == self.trial_boot_id
                && early.trial_id == self.transaction
                && early.timeout_seconds == 210
                && early.armed_boottime_ms <= b.uptime_ms,
            "early watchdog receipt does not match trial",
        )
    }
}

// Fixed, descriptor-relative paths. A root-controlled replacement or unsafe
// file is rejected, never followed. Observation does not take recovery locks.
fn open_at(parent: &File, name: &str, flags: i32, mode: u32) -> Result<File> {
    let name = CString::new(name).map_err(|_| invalid("invalid fixed filename"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn directory(parent: &File, name: &str, owner: u32, private: bool) -> Result<File> {
    let file = open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let m = file.metadata()?;
    require(
        m.is_dir() && m.uid() == owner && m.mode() & if private { 0o077 } else { 0o022 } == 0,
        "unsafe evidence directory",
    )?;
    Ok(file)
}
fn system_directory() -> Result<File> {
    let root = File::open("/")?;
    let data = directory(&root, "data", 0, false)?;
    let system = directory(&data, "spacewars-system", 0, true)?;
    require(
        data.metadata()?.dev() == system.metadata()?.dev(),
        "system data crosses filesystems",
    )?;
    Ok(system)
}
fn read_at(parent: &File, name: &str, limit: u64, owner: u32) -> Result<Vec<u8>> {
    let pinned = open_at(parent, name, libc::O_PATH, 0)?;
    let before = pinned.metadata()?;
    require(
        before.is_file()
            && before.uid() == owner
            && before.mode() & 0o077 == 0
            && before.nlink() == 1
            && before.dev() == parent.metadata()?.dev()
            && before.len() <= limit,
        "unsafe or oversized evidence file",
    )?;
    let file = File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?;
    let mut bytes = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    require(
        bytes.len() as u64 == before.len()
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec(),
        "evidence changed during read",
    )?;
    Ok(bytes)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrialState {
    pub recovery: Sha256,
    pub watchdog_armed_ms: u64,
    pub watchdog_timeout_seconds: u32,
}

fn trial_state(expected: &Expectation, latest_ms: u64) -> Result<TrialState> {
    let boot = inspect_boot()?;
    let system = system_directory()?;
    let recovery = directory(&system, "recovery", 0, true)?;
    // Recovery runs as root with Group=spacewars. Private mode and owner UID
    // are authority; the file's group grants no access through mode 0600.
    let journal: Envelope =
        serde_json::from_slice(&read_at(&recovery, "state.json", 128 * 1024, 0)?)?;
    require(
        boot.observation.tryboot && boot.observation.boot_id == expected.trial_boot_id,
        "not the selected early uncommitted candidate trial",
    )?;
    let root = File::open("/")?;
    let run = directory(&root, "run", 0, false)?;
    let evidence = directory(&run, "spacewars-recovery", 0, true)?;
    let early = serde_json::from_slice(&read_at(&evidence, "early-watchdog.json", 1024, 0)?)?;
    match open_at(&evidence, "completed-watchdog.json", libc::O_PATH, 0) {
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            return Err(invalid(
                "watchdog completion already exists or cannot be inspected",
            ));
        }
    }
    expected.matches(&boot, &journal, &early, latest_ms)?;
    require(now_ms()? < latest_ms, "trial observation window expired")?;
    Ok(TrialState {
        recovery: journal.checksum,
        watchdog_armed_ms: early.armed_boottime_ms,
        watchdog_timeout_seconds: early.timeout_seconds,
    })
}

fn guard(expected: &Expectation, latest_ms: u64) -> Result<()> {
    trial_state(expected, latest_ms)?;
    recovery_running()?;
    require(
        now_ms()? < latest_ms,
        "injection window expired during observation",
    )
}

fn plan_time(plan: &Plan, now: u64) -> Result<()> {
    require(
        now < LATEST_STOP_MS
            && now
                .checked_sub(plan.selected_ms)
                .is_some_and(|age| age < PLAN_LIFETIME_MS)
            && plan.health.uptime_ms <= plan.selected_ms
            && plan.selected_ms - plan.health.uptime_ms < 5_000
            && plan.health.boot_id == plan.expectation.trial_boot_id
            && plan.health.build_id == plan.expectation.image.build_id,
        "hang plan expired or health belongs to another boot/image",
    )
}

pub fn inspect(expectation: Expectation) -> Result<Plan> {
    root()?;
    guard(&expectation, LATEST_STOP_MS)?;
    let expected = KioskExpectation::from_image(&expectation.image)?;
    let health = KioskObserver::default().observe(&expected)?;
    guard(&expectation, LATEST_STOP_MS)?;
    let plan = Plan {
        expectation,
        selected_ms: now_ms()?,
        health,
    };
    plan_time(&plan, now_ms()?)?;
    Ok(plan)
}

pub(crate) struct Pinned(File);
impl Pinned {
    pub(crate) fn wait_exited(&self) -> Result<()> {
        let mut descriptor = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // A pidfd becomes readable only when its pinned process exits. Never
        // look up the numeric PID again after delivering the signal.
        let result = unsafe { libc::poll(&mut descriptor, 1, 2000) };
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        require(
            result == 1 && descriptor.revents & libc::POLLIN != 0,
            "pinned kiosk exit was not observed within two seconds",
        )
    }

    pub(crate) fn open(pid: u32) -> Result<Self> {
        require(pid > 1 && pid <= i32::MAX as u32, "invalid kiosk PID")?;
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as i32, 0) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(unsafe { File::from_raw_fd(fd as i32) }))
    }
    pub(crate) fn signal(&self, signal: i32) -> Result<()> {
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.0.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

pub(crate) fn create_record(parent: &File, name: &str, value: &Value) -> Result<()> {
    let mut file = open_at(
        parent,
        name,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    parent.sync_all()?;
    Ok(())
}
pub(crate) fn fault_directory() -> Result<File> {
    let system = system_directory()?;
    let name = CString::new("recovery-faults").unwrap();
    if unsafe { libc::mkdirat(system.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error.into());
        }
    }
    let faults = directory(&system, "recovery-faults", 0, true)?;
    require(
        system.metadata()?.dev() == faults.metadata()?.dev(),
        "fault records cross filesystems",
    )?;
    system.sync_all()?;
    Ok(faults)
}

// Consumption must be durable before a signal. Errors after consumption never
// grant a replay. If a late postcheck fails, resume only the same pinned process.
fn one_stop(
    consume: impl FnOnce() -> Result<()>,
    recheck: impl FnOnce() -> Result<()>,
    signal: impl Fn(i32) -> Result<()>,
    postcheck: impl FnOnce() -> Result<()>,
) -> Result<()> {
    consume()?;
    recheck()?;
    signal(libc::SIGSTOP)?;
    if let Err(error) = postcheck() {
        signal(libc::SIGCONT).map_err(|resume| invalid(&format!(
            "postcheck failed ({error}); pinned-process resume also failed ({resume}); observe, do not replay"
        )))?;
        return Err(error);
    }
    Ok(())
}
fn stopped(health: &KioskHealth) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        require(
            kiosk_health::process(health.process_id, 1000)? == health.start_ticks,
            "selected process disappeared or changed",
        )?;
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", health.process_id))?;
        if stat
            .rsplit_once(')')
            .and_then(|(_, v)| v.split_whitespace().next())
            == Some("T")
        {
            return Ok(());
        }
        require(
            Instant::now() < deadline,
            "selected process did not enter group stop",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn stop(plan: Plan, selected: Sha256) -> Result<Value> {
    root()?;
    require(
        plan.fingerprint()? == selected,
        "plan digest differs from owner selection",
    )?;
    plan_time(&plan, now_ms()?)?;
    guard(&plan.expectation, LATEST_STOP_MS)?;
    let expected = KioskExpectation::from_image(&plan.expectation.image)?;
    let pinned = Pinned::open(plan.health.process_id)?;
    kiosk_health::qualification_instance(&plan.health, &expected)?;
    let directory = fault_directory()?;
    let name = format!("{}.json", plan.expectation.transaction);
    one_stop(
        || {
            create_record(
                &directory,
                &name,
                &json!({
                    "schema": 1, "case": "kiosk-hang", "consumed": true,
                    "signal_delivery": "not-yet-established",
                    "plan_sha256": selected, "plan": plan
                }),
            )
        },
        || {
            kiosk_health::qualification_instance(&plan.health, &expected)?;
            guard(&plan.expectation, LATEST_STOP_MS)?;
            plan_time(&plan, now_ms()?)
        },
        |signal| pinned.signal(signal),
        || {
            stopped(&plan.health)?;
            kiosk_health::qualification_instance(&plan.health, &expected)?;
            guard(&plan.expectation, LATEST_STOP_MS)?;
            plan_time(&plan, now_ms()?)
        },
    )?;
    let result = json!({
        "schema": 1, "case": "kiosk-hang", "signal": "SIGSTOP",
        "plan_sha256": selected, "transaction": plan.expectation.transaction,
        "boot_id": plan.expectation.trial_boot_id, "uptime_ms": now_ms()?,
        "health_before": plan.health, "stopped_process_observed": true,
        "automatic_recovery_qualified": false,
        "next": "Observe automatic fallback; never replay an uncertain stop"
    });
    create_record(
        &directory,
        &format!("{}.result.json", plan.expectation.transaction),
        &result,
    )?;
    Ok(result)
}

#[cfg(test)]
#[path = "supervised_kiosk_hang_tests.rs"]
mod tests;
