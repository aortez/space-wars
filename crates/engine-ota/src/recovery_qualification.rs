//! Opt-in root-operator boot diagnostics. No production recovery transition,
//! qualification flag, image write, watchdog ioctl or update admission bypass.
//! A missing-root selector is intentionally invalid for ordinary OTA. Restore
//! it after fallback before restarting the normal recovery service.
use crate::boot::{SlotRoots, UNPREPARED_TRIAL};
use crate::boot_files::BootFiles;
use crate::boot_linux::{Runtime, check_mount};
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file};
use crate::recovery::Slot;
use crate::{Error, ImageIdentity, RecoveryStore, Result, Sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MAX_PLAN_BYTES: u64 = 64 * 1024;
const DIRECTORY: &str = "spacewars-recovery-tests";
const MISSING_ROOT: &str = "ffffffff-ff";
const SELECTOR: &str = "spacewars-trial.txt";
const PARAMETER: &str = "0 tryboot";
const RECOVERY: &str = "spacewars-system/recovery/state.json";
const COMPLETED: &str = "spacewars-bootstrap/completed/boot-trial/record.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    device: Sha256,
    layout: Sha256,
    image: ImageIdentity,
    kernel: String,
    profile: String,
    slot: Slot,
    root_a: String,
    root_b: String,
}

impl Stamp {
    fn of(runtime: &Runtime) -> Self {
        Self {
            device: runtime.physical.fingerprint(),
            layout: runtime.layout.layout_id,
            image: runtime.image.clone(),
            kernel: runtime.kernel.clone(),
            profile: runtime.profile.clone(),
            slot: runtime.layout.active,
            root_a: runtime.layout.partitions[1].partuuid.clone(),
            root_b: runtime.layout.partitions[2].partuuid.clone(),
        }
    }

    fn roots(&self) -> Result<SlotRoots> {
        SlotRoots::new(self.root_a.clone(), self.root_b.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u32,
    case: String,
    id: Sha256,
    origin_boot_id: String,
    stamp: Stamp,
    default_selector: String,
    original_selector: String,
    fault_selector: String,
    hardware_selector: String,
    hostname: Option<String>,
    recovery_sha256: Sha256,
    completed_bootstrap_sha256: Sha256,
}

impl Plan {
    fn check(&self, files: &BootFiles) -> Result<()> {
        if self.schema != 1
            || self.case != "missing-root"
            || !crate::recovery::boot_id(&self.origin_boot_id)
            || self.original_selector != UNPREPARED_TRIAL
            || self.default_selector
                != files
                    .template()
                    .render(&self.stamp.roots()?, self.stamp.slot, None)
            || self.fault_selector != fault_selector(files, self.id)?
            || self.stamp.root_a == MISSING_ROOT
            || self.stamp.root_b == MISSING_ROOT
        {
            return Err(invalid("invalid missing-root plan"));
        }
        Ok(())
    }

    fn fingerprint(&self) -> Result<Sha256> {
        // JSON Value canonicalizes object key order for workstation transport.
        Ok(Sha256::digest(&serde_json::to_vec(&serde_json::to_value(
            self,
        )?)?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Preparing,
    Prepared,
    Requested,
    Restoring,
    Restored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    plan: Plan,
    phase: Phase,
    request_consumed: bool,
    restored_boot_id: Option<String>,
}

impl State {
    fn validate(&self) -> Result<()> {
        if self.plan.schema != 1
            || self.plan.case != "missing-root"
            || !crate::recovery::boot_id(&self.plan.origin_boot_id)
            || (matches!(self.phase, Phase::Preparing | Phase::Prepared) && self.request_consumed)
            || (self.phase == Phase::Requested && !self.request_consumed)
            || (matches!(self.phase, Phase::Restoring | Phase::Restored)
                != self.restored_boot_id.is_some())
            || self.restored_boot_id.as_ref().is_some_and(|id| {
                !crate::recovery::boot_id(id)
                    || (self.request_consumed && id == &self.plan.origin_boot_id)
            })
        {
            return Err(invalid("inconsistent diagnostic state"));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    state: State,
    checksum: Sha256,
}

struct Store {
    parent: File,
    directory: File,
    owner: u32,
    state: State,
    bytes: Vec<u8>,
}

impl Store {
    fn create(parent: File, owner: u32, plan: Plan) -> Result<Self> {
        // A partial or outstanding attempt blocks another one. Never discard
        // an uncertain record to make a fresh activation allowance.
        for (index, entry) in fs::read_dir(fd_path(&parent))?.enumerate() {
            if index >= 64 {
                return Err(invalid(
                    "archive the diagnostic history before adding more cases",
                ));
            }
            let id: Sha256 = entry?
                .file_name()
                .to_string_lossy()
                .into_owned()
                .try_into()
                .map_err(invalid)?;
            let previous = Self::open(parent.try_clone()?, owner, id)?;
            if previous.state.phase != Phase::Restored || id == plan.id {
                return Err(invalid(
                    "diagnostic ID reused or previous attempt not restored",
                ));
            }
        }
        mkdir(&parent, &plan.id.to_string(), owner, false)?;
        let directory = open_at(
            &parent,
            &plan.id.to_string(),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        let mut store = Self {
            parent,
            directory,
            owner,
            state: State {
                plan,
                phase: Phase::Preparing,
                request_consumed: false,
                restored_boot_id: None,
            },
            bytes: Vec::new(),
        };
        store.persist()?;
        store.parent.sync_all()?;
        Ok(store)
    }

    fn open(parent: File, owner: u32, id: Sha256) -> Result<Self> {
        owned(&parent.metadata()?, true, owner, true)?;
        let directory = open_at(
            &parent,
            &id.to_string(),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        owned(&directory.metadata()?, true, owner, true)?;
        let bytes = private_read(&directory, "record.json", owner, MAX_PLAN_BYTES)?;
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if envelope.state.plan.id != id
            || envelope.checksum != Sha256::digest(&serde_json::to_vec(&envelope.state)?)
        {
            return Err(invalid("diagnostic record checksum or ID differs"));
        }
        envelope.state.validate()?;
        Ok(Self {
            parent,
            directory,
            owner,
            state: envelope.state,
            bytes,
        })
    }

    fn check(&self) -> Result<()> {
        let named = open_at(
            &self.parent,
            &self.state.plan.id.to_string(),
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        if named.metadata()?.dev() != self.directory.metadata()?.dev()
            || named.metadata()?.ino() != self.directory.metadata()?.ino()
            || (!self.bytes.is_empty()
                && private_read(&self.directory, "record.json", self.owner, MAX_PLAN_BYTES)?
                    != self.bytes)
        {
            return Err(invalid("diagnostic directory or record changed"));
        }
        owned(&named.metadata()?, true, self.owner, true)
    }

    fn persist(&mut self) -> Result<()> {
        self.check()?;
        self.state.validate()?;
        let bytes = serde_json::to_vec(&Envelope {
            checksum: Sha256::digest(&serde_json::to_vec(&self.state)?),
            state: self.state.clone(),
        })?;
        if bytes.len() as u64 > MAX_PLAN_BYTES {
            return Err(invalid("diagnostic state exceeds its bound"));
        }
        let mut temp = tempfile::NamedTempFile::new_in(fd_path(&self.directory))?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(fd_path(&self.directory).join("record.json"))
            .map_err(|e| e.error)?;
        self.directory.sync_all()?;
        syncfs(&self.directory)?;
        self.bytes = bytes;
        Ok(())
    }

    fn consume(&mut self) -> Result<()> {
        if self.state.phase != Phase::Prepared || self.state.request_consumed {
            return Err(invalid(
                "diagnostic activation already consumed or not prepared",
            ));
        }
        self.state.phase = Phase::Requested;
        self.state.request_consumed = true;
        self.persist()
    }

    fn begin_restore(&mut self, boot_id: &str) -> Result<()> {
        if self.state.request_consumed && boot_id == self.state.plan.origin_boot_id {
            return Err(invalid(
                "consumed attempt requires a new normal boot; inspect uncertain reboot requests",
            ));
        }
        if self.state.phase != Phase::Restored {
            self.state.phase = Phase::Restoring;
            self.state.restored_boot_id = Some(boot_id.into());
            self.persist()?;
        }
        Ok(())
    }
}

struct Device {
    lock: InstallLock,
    runtime: Runtime,
    data: File,
    files: BootFiles,
}

impl Device {
    fn open() -> Result<Self> {
        let lock = InstallLock::acquire()?;
        crate::bootstrap::require_no_active_bootstrap(&lock)?;
        let runtime = Runtime::inspect()?;
        if runtime.tryboot {
            return Err(invalid("diagnostic requires a normal boot"));
        }
        let root = open_directory(Path::new("/"), 0)?;
        let boot = open_at(&root, "boot", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        check_mount(&boot, &runtime.layout, true)?;
        let files = BootFiles::open(boot, 0, runtime.image.boot_identity)?;
        if files
            .template()
            .running_trial(
                &runtime.roots()?,
                runtime.layout.active,
                false,
                &runtime.command_line,
            )?
            .is_some()
        {
            return Err(invalid("unexpected running trial"));
        }
        let data = crate::recovery_linux::open_data_mount()?;
        // This first diagnostic is deliberately restricted to the unpublished
        // bootstrap baseline. It cannot run beside an enabled OTA installation.
        for path in [
            "/data/spacewars-system/updates",
            "/data/spacewars-system/trust",
            "/data/spacewars-system/staging",
        ] {
            match fs::symlink_metadata(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    return Err(invalid(
                        "OTA trust/staging must be unprovisioned for this diagnostic",
                    ));
                }
            }
        }
        Ok(Self {
            lock,
            runtime,
            data,
            files,
        })
    }

    fn plan(&self, id: Sha256) -> Result<Plan> {
        let evidence = crate::boot_linux::inspect_boot()?;
        let journal = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
        journal.record().maintenance(&evidence.observation)?;
        if journal.record().good().generation != 0
            || journal.record().transaction().is_some()
            || journal.record().retained_good().is_some()
        {
            return Err(invalid(
                "missing-root diagnostic requires the initial bootstrap baseline",
            ));
        }
        let completed = private_read(&self.data, COMPLETED, 0, 512 * 1024)?;
        let proof: Value = serde_json::from_slice(&completed)?;
        if proof.pointer("/record/canonical/finish/phase") != Some(&json!("complete")) {
            return Err(invalid("bootstrap handoff is not complete"));
        }
        let plan = Plan {
            schema: 1,
            case: "missing-root".into(),
            id,
            origin_boot_id: self.runtime.boot_id.clone(),
            stamp: Stamp::of(&self.runtime),
            default_selector: boot_text(&self.files, "cmdline.txt", 8192)?,
            original_selector: boot_text(&self.files, SELECTOR, 8192)?,
            fault_selector: fault_selector(&self.files, id)?,
            hardware_selector: boot_text(&self.files, "spacewars-device.txt", 4096)?,
            hostname: hostname(&self.files)?,
            recovery_sha256: Sha256::digest(&private_read(&self.data, RECOVERY, 0, 128 * 1024)?),
            completed_bootstrap_sha256: Sha256::digest(&completed),
        };
        self.guard(&plan, false)?;
        missing_root()?;
        Ok(plan)
    }

    fn parent(&self, create: bool) -> Result<File> {
        if create {
            mkdir(&self.data, DIRECTORY, 0, true)?;
            self.data.sync_all()?;
        }
        let parent = open_at(&self.data, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&parent.metadata()?, true, 0, true)?;
        Ok(parent)
    }

    fn guard(&self, plan: &Plan, require_origin: bool) -> Result<()> {
        self.lock.check()?;
        let current = Runtime::inspect()?;
        if !self.runtime.same_boot(&current)
            || Stamp::of(&current) != plan.stamp
            || (require_origin && current.boot_id != plan.origin_boot_id)
        {
            return Err(invalid(
                "device, media, image or boot changed from the diagnostic plan",
            ));
        }
        let named_boot = open_directory(Path::new("/boot"), 0)?;
        if named_boot.metadata()?.dev() != self.files.directory().metadata()?.dev()
            || named_boot.metadata()?.ino() != self.files.directory().metadata()?.ino()
        {
            return Err(invalid("boot mount changed"));
        }
        check_mount(&named_boot, &current.layout, true)?;
        self.files.verify()?;
        plan.check(&self.files)?;
        if boot_text(&self.files, "cmdline.txt", 8192)? != plan.default_selector
            || boot_text(&self.files, "spacewars-device.txt", 4096)? != plan.hardware_selector
            || hostname(&self.files)? != plan.hostname
            || Sha256::digest(&private_read(&self.data, RECOVERY, 0, 128 * 1024)?)
                != plan.recovery_sha256
            || Sha256::digest(&private_read(&self.data, COMPLETED, 0, 512 * 1024)?)
                != plan.completed_bootstrap_sha256
        {
            return Err(invalid(
                "normal selectors or persistent recovery evidence changed",
            ));
        }
        let trial = boot_text(&self.files, SELECTOR, 8192)?;
        if trial != plan.original_selector && trial != plan.fault_selector {
            return Err(invalid("unexpected trial selector; left unchanged"));
        }
        Ok(())
    }
}

/// Return a bound plan. No persistent files, selectors or recovery records change.
pub fn inspect(id: Sha256) -> Result<Value> {
    let device = Device::open()?;
    let plan = device.plan(id)?;
    Ok(json!({"plan_sha256": plan.fingerprint()?, "plan": plan}))
}

pub fn prepare(bytes: &[u8], expected: Sha256) -> Result<Value> {
    if bytes.len() as u64 > MAX_PLAN_BYTES {
        return Err(invalid("plan exceeds size bound"));
    }
    let plan: Plan = serde_json::from_slice(bytes)?;
    if plan.fingerprint()? != expected {
        return Err(invalid("plan differs from the separately selected digest"));
    }
    let device = Device::open()?;
    if device.plan(plan.id)? != plan {
        return Err(invalid("live preflight differs from the inspected plan"));
    }
    require_empty_parameter(&mut SystemBus)?;
    let mut store = Store::create(device.parent(true)?, 0, plan)?;
    publish_selector(
        &device.files,
        &store,
        false,
        || device.guard(&store.state.plan, true),
        |_| Ok(()),
    )?;
    store.state.phase = Phase::Prepared;
    store.persist()?;
    report(&store, &device.runtime.boot_id)
}

pub fn activate(id: Sha256) -> Result<Value> {
    let device = Device::open()?;
    let mut store = Store::open(device.parent(false)?, 0, id)?;
    request_once(&mut store, &mut SystemBus, |store| {
        store.check()?;
        device.guard(&store.state.plan, true)?;
        if boot_text(&device.files, SELECTOR, 8192)? != store.state.plan.fault_selector {
            return Err(invalid("diagnostic selector not prepared"));
        }
        missing_root()
    })?;
    let mut result = report(&store, &device.runtime.boot_id)?;
    result["reboot_job_accepted"] = json!(true);
    Ok(result)
}

pub fn restore(id: Sha256) -> Result<Value> {
    let device = Device::open()?;
    let mut store = Store::open(device.parent(false)?, 0, id)?;
    require_empty_parameter(&mut SystemBus)?;
    device.guard(&store.state.plan, false)?;
    store.begin_restore(&device.runtime.boot_id)?;
    if store.state.phase != Phase::Restored {
        publish_selector(
            &device.files,
            &store,
            true,
            || device.guard(&store.state.plan, false),
            |_| Ok(()),
        )?;
        // Full native production admission must work again before completion.
        let boot = crate::boot_linux::BootController::open(&device.lock)?;
        let journal = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
        boot.authorize_maintenance(&journal)?;
        store.state.phase = Phase::Restored;
        store.persist()?;
    }
    device.guard(&store.state.plan, false)?;
    if boot_text(&device.files, SELECTOR, 8192)? != store.state.plan.original_selector {
        return Err(invalid("restored selector does not match the original"));
    }
    report(&store, &device.runtime.boot_id)
}

pub fn status(id: Sha256) -> Result<Value> {
    let device = Device::open()?;
    let store = Store::open(device.parent(false)?, 0, id)?;
    device.guard(&store.state.plan, false)?;
    report(&store, &device.runtime.boot_id)
}

fn report(store: &Store, current_boot: &str) -> Result<Value> {
    store.check()?;
    Ok(
        json!({"schema":1, "state":store.state, "current_boot_id":current_boot,
        "record_sha256":Sha256::digest(&store.bytes), "reboot_job_accepted":false,
        "production_recovery_unchanged":true, "automatic_updates_enabled":false,
        "scope":"Operator missing-root boot diagnostic; no OTA candidate, quarantine or production qualification flag"}),
    )
}

fn publish_selector(
    files: &BootFiles,
    store: &Store,
    restore: bool,
    mut guard: impl FnMut() -> Result<()>,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let allowed = if restore {
        Phase::Restoring
    } else {
        Phase::Preparing
    };
    if store.state.phase != allowed {
        return Err(invalid("selector write lacks diagnostic intent"));
    }
    store.check()?;
    files.verify()?;
    guard()?;
    let plan = &store.state.plan;
    let desired = if restore {
        &plan.original_selector
    } else {
        &plan.fault_selector
    };
    let current = boot_text(files, SELECTOR, 8192)?;
    if current != plan.original_selector && current != plan.fault_selector {
        return Err(invalid("unexpected selector before replacement"));
    }
    let pending = format!(".spacewars-boot-{}.tmp", &plan.id.to_string()[..16]);
    let directory = files.directory();
    let mut temporary = open_at(directory, &pending, libc::O_RDWR | libc::O_CREAT, 0o600)?;
    owned(&temporary.metadata()?, false, store.owner, false)?;
    let previous = read_file(temporary.try_clone()?, store.owner, 8192)?;
    // Only this recorded attempt's bounded partial selector may be resumed or
    // replaced during restoration. Unrelated temporary bytes stop the operation.
    if !plan.original_selector.as_bytes().starts_with(&previous)
        && !plan.fault_selector.as_bytes().starts_with(&previous)
    {
        return Err(invalid("unexpected diagnostic selector temporary"));
    }
    temporary.set_len(0)?;
    temporary.seek(SeekFrom::Start(0))?;
    checkpoint("selector-opened")?;
    temporary.write_all(desired.as_bytes())?;
    checkpoint("selector-written")?;
    temporary.sync_all()?;
    checkpoint("selector-synced")?;
    guard()?;
    store.check()?;
    let named = open_at(directory, &pending, libc::O_RDONLY, 0)?;
    if !crate::owned_fs::unchanged(&temporary.metadata()?, &named.metadata()?)
        || boot_text(files, SELECTOR, 8192)? != current
    {
        return Err(invalid("selector or temporary replaced before rename"));
    }
    let from = name(&pending)?;
    let to = name(SELECTOR)?;
    io_result(unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
        )
    })?;
    checkpoint("selector-renamed")?;
    directory.sync_all()?;
    checkpoint("selector-directory-synced")?;
    syncfs(directory)?;
    checkpoint("selector-filesystem-synced")?;
    if boot_text(files, SELECTOR, 8192)? != *desired {
        return Err(invalid("selector readback differs"));
    }
    guard()?;
    Ok(())
}

trait RebootBus {
    fn parameter(&mut self) -> Result<String>;
    fn set_parameter(&mut self, value: &str) -> Result<()>;
    fn request(&mut self) -> Result<()>;
}

fn require_empty_parameter(bus: &mut impl RebootBus) -> Result<()> {
    if !bus.parameter()?.is_empty() {
        return Err(invalid(
            "an existing reboot parameter is present; left unchanged",
        ));
    }
    Ok(())
}

fn request_once(
    store: &mut Store,
    bus: &mut impl RebootBus,
    mut guard: impl FnMut(&Store) -> Result<()>,
) -> Result<()> {
    if store.state.phase != Phase::Prepared || store.state.request_consumed {
        return Err(invalid(
            "diagnostic activation already consumed or not prepared",
        ));
    }
    require_empty_parameter(bus)?;
    guard(store)?;
    store.consume()?;
    let mut attempted_parameter = false;
    let result = (|| {
        guard(store)?;
        require_empty_parameter(bus)?;
        attempted_parameter = true;
        bus.set_parameter(PARAMETER)?;
        if bus.parameter()? != PARAMETER {
            return Err(invalid("diagnostic reboot parameter did not match"));
        }
        guard(store)?;
        if bus.parameter()? != PARAMETER {
            return Err(invalid("reboot parameter changed before request"));
        }
        bus.request()
    })();
    if let Err(error) = result {
        // A lost reply is ambiguous. Never repeat the request. Clear only the
        // argument this attempt tried to set, leaving other operators' requests.
        if attempted_parameter && bus.parameter().ok().as_deref() == Some(PARAMETER) {
            let _ = bus.set_parameter("");
        }
        return Err(invalid(format!(
            "{error}; attempt consumed, inspect before further action"
        )));
    }
    Ok(())
}

struct SystemBus;
impl RebootBus for SystemBus {
    fn parameter(&mut self) -> Result<String> {
        let text = busctl(&[
            "get-property",
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "RebootParameter",
        ])?;
        let quoted = text
            .trim()
            .strip_prefix("s ")
            .ok_or_else(|| invalid("unexpected reboot parameter reply"))?;
        Ok(serde_json::from_str(quoted)?)
    }
    fn set_parameter(&mut self, value: &str) -> Result<()> {
        busctl(&[
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "SetRebootParameter",
            "s",
            value,
        ])?;
        Ok(())
    }
    fn request(&mut self) -> Result<()> {
        let reply = busctl(&[
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartUnit",
            "ss",
            "reboot.target",
            "replace-irreversibly",
        ])?;
        let id = reply
            .trim()
            .strip_prefix("o \"/org/freedesktop/systemd1/job/")
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| invalid("unexpected reboot job reply"))?;
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid("invalid reboot job ID"));
        }
        Ok(())
    }
}

fn busctl(args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/busctl")
        .arg("--timeout=5s")
        .args(args)
        .output()?;
    if !output.status.success() || output.stdout.len() > 4096 || output.stderr.len() > 4096 {
        return Err(invalid("bounded system-bus operation failed"));
    }
    String::from_utf8(output.stdout).map_err(invalid)
}

fn missing_root() -> Result<()> {
    // The image's BusyBox blkid has neither PARTUUID filtering nor meaningful
    // no-match status. Require precisely the four independently inspected USB
    // partitions instead; any other partitioned medium blocks the experiment.
    let layout = crate::recovery_linux::inspect_layout()?;
    let mut found = BTreeSet::new();
    for (index, entry) in fs::read_dir("/sys/class/block")?.enumerate() {
        if index >= 1024 {
            return Err(invalid("too many block devices"));
        }
        let entry = entry?;
        match fs::metadata(entry.path().join("partition")) {
            Ok(_) => {
                found.insert(entry.file_name().to_string_lossy().into_owned());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let expected = layout
        .partitions
        .iter()
        .map(|p| (p.name.clone(), p.partuuid.clone()))
        .collect::<Vec<_>>();
    check_missing_partitions(&expected, &found)
}

fn check_missing_partitions(expected: &[(String, String)], found: &BTreeSet<String>) -> Result<()> {
    if expected.len() != 4
        || expected.iter().any(|(_, uuid)| uuid == MISSING_ROOT)
        || expected
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>()
            != *found
    {
        return Err(invalid(
            "missing PARTUUID cannot be excluded: unexpected partition inventory",
        ));
    }
    Ok(())
}

fn fault_selector(files: &BootFiles, id: Sha256) -> Result<String> {
    let roots = SlotRoots::new(MISSING_ROOT.into(), "fffffffe-ff".into())?;
    Ok(files.template().render(&roots, Slot::A, Some(id)))
}

fn boot_text(files: &BootFiles, path: &str, limit: u64) -> Result<String> {
    String::from_utf8(read_file(
        open_at(files.directory(), path, libc::O_RDONLY, 0)?,
        files.directory().metadata()?.uid(),
        limit,
    )?)
    .map_err(invalid)
}

fn hostname(files: &BootFiles) -> Result<Option<String>> {
    match boot_text(files, "hostname.txt", 64) {
        Ok(text) => Ok(Some(text)),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn private_read(parent: &File, path: &str, owner: u32, limit: u64) -> Result<Vec<u8>> {
    let mut parent = parent.try_clone()?;
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        let directory = components.peek().is_some();
        let file = open_at(
            &parent,
            component,
            libc::O_RDONLY | if directory { libc::O_DIRECTORY } else { 0 },
            0,
        )?;
        owned(&file.metadata()?, directory, owner, true)?;
        if !directory {
            return read_file(file, owner, limit);
        }
        parent = file;
    }
    Err(invalid("empty private path"))
}

fn mkdir(parent: &File, component: &str, owner: u32, existing: bool) -> Result<()> {
    let component = name(component)?;
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o700) };
    if result < 0
        && !(existing && std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST))
    {
        io_result(result)?;
    }
    let directory = open_at(
        parent,
        component.to_str().map_err(invalid)?,
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    owned(&directory.metadata()?, true, owner, true)
}

fn fd_path(file: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

fn syncfs(file: &File) -> Result<()> {
    io_result(unsafe { libc::syncfs(file.as_raw_fd()) })
}

fn invalid(reason: impl std::fmt::Display) -> Error {
    Error::Boot(format!("recovery qualification: {reason}"))
}

#[cfg(test)]
#[path = "recovery_qualification_tests.rs"]
mod tests;
