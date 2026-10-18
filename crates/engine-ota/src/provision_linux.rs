//! Explicit root-operator setup after the supervised recovery bootstrap.
//! Publish trust, data identity and staging together; a retained outer receipt
//! distinguishes interrupted setup from missing previously published state.
use crate::boot_linux::{BootController, BootEvidence};
use crate::install_lock::InstallLock;
use crate::owned_fs::{io_result, name, open_at, open_directory, owned, read_file, unchanged};
use crate::recovery::BootBinding;
use crate::recovery_linux::{DeviceNumber, DiskLayout};
use crate::{Error, FeedLocation, RecoveryStore, Result, Sha256, TrustStore, store};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, File};
use std::io::{Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const BUNDLE_PATH: &str = "/etc/spacewars/ota-bootstrap.json";
pub const OPERATOR_BUNDLE_PATH: &str = "/data/spacewars-system/ota-bootstrap.json";
pub const RECEIPT_PATH: &str = "/data/spacewars-system/update-provision.json";
pub const TRUST_PATH: &str = "/data/spacewars-system/updates/trust";
pub const STAGING_PATH: &str = "/data/spacewars-system/updates/staging";
pub const DATA_SCHEMA_PATH: &str = "/data/spacewars-system/updates/data-schema.json";
pub const MAX_BUNDLE_BYTES: u64 = 512 * 1024;
const RECEIPT: &str = "update-provision.json";
const PENDING: &str = ".updates-pending";
const UPDATES: &str = "updates";
const RECORD_LIMIT: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleFile {
    schema: u32,
    root: serde_json::Value,
    location: FeedLocation,
}

/// Public trust material only. Its exact bytes must be selected independently
/// by the root operator; an image supplied through the update socket cannot
/// establish or replace this initial authority.
pub struct Bundle {
    fingerprint: Sha256,
    root: Vec<u8>,
    location: FeedLocation,
    initial_trust: Vec<u8>,
}
impl Bundle {
    pub fn parse(bytes: &[u8], expected: Sha256) -> Result<Self> {
        if bytes.len() as u64 > MAX_BUNDLE_BYTES || Sha256::digest(bytes) != expected {
            return Err(invalid("bootstrap bundle differs from the selected digest"));
        }
        let parsed: BundleFile = serde_json::from_slice(bytes)?;
        if parsed.schema != 1 {
            return Err(invalid("unsupported bootstrap bundle schema"));
        }
        public_root(&parsed.root)?;
        checked_location(&parsed.location)?;
        let root = serde_json::to_vec(&parsed.root)?;
        let initial_trust = store::initial_bytes(&root, parsed.location.clone())?;
        Ok(Self {
            fingerprint: expected,
            root,
            location: parsed.location,
            initial_trust,
        })
    }
}

fn public_root(root: &serde_json::Value) -> Result<()> {
    fn object<'a>(
        value: &'a serde_json::Value,
        fields: &[&str],
    ) -> Result<&'a serde_json::Map<String, serde_json::Value>> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("expected public TUF root object"))?;
        if object.len() != fields.len() || !fields.iter().all(|f| object.contains_key(*f)) {
            return Err(invalid("unsupported fields in public TUF bootstrap root"));
        }
        Ok(object)
    }
    object(root, &["signed", "signatures"])?;
    let signed = &root["signed"];
    object(
        signed,
        &[
            "_type",
            "spec_version",
            "version",
            "expires",
            "consistent_snapshot",
            "keys",
            "roles",
        ],
    )?;
    if signed["consistent_snapshot"] != true {
        return Err(invalid("bootstrap requires consistent snapshots"));
    }
    object(
        &signed["roles"],
        &["root", "targets", "snapshot", "timestamp"],
    )?;
    for role in signed["roles"]
        .as_object()
        .expect("checked object")
        .values()
    {
        object(role, &["keyids", "threshold"])?;
    }
    for signature in root["signatures"]
        .as_array()
        .ok_or_else(|| invalid("invalid root signatures"))?
    {
        object(signature, &["keyid", "sig"])?;
    }
    for key in signed["keys"]
        .as_object()
        .ok_or_else(|| invalid("invalid root keys"))?
        .values()
    {
        object(key, &["keytype", "scheme", "keyval"])?;
        object(&key["keyval"], &["public"])?;
        if key["keyval"]["public"]
            .as_str()
            .is_none_or(|s| s.contains("PRIVATE KEY"))
        {
            return Err(invalid("bootstrap accepts public key material only"));
        }
    }
    Ok(())
}

fn checked_location(location: &FeedLocation) -> Result<()> {
    location.validate()?;
    // Keep both bases within the bounded durable plan, before writing intent.
    if location.metadata.as_str().len() > 4096 || location.targets.as_str().len() > 4096 {
        return Err(invalid("bootstrap feed location exceeds size bound"));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    bundle: Sha256,
    root: Sha256,
    location: FeedLocation,
    binding: BootBinding,
    baseline: Sha256,
    initial_data_version: u32,
}
impl Plan {
    fn validate(&self) -> Result<()> {
        checked_location(&self.location)?;
        if self.initial_data_version == 0
            || [
                self.bundle,
                self.root,
                self.baseline,
                self.binding.device_id,
                self.binding.layout_id,
                self.binding.boot_identity,
                self.binding.hardware_profile,
            ]
            .iter()
            .any(|h| h.as_bytes() == &[0; 32])
        {
            return Err(invalid("invalid provisioning identity"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Prepared,
    Complete,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    phase: Phase,
    plan: Plan,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    record: T,
    checksum: Sha256,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DataVersion {
    schema: u32,
    version: u32,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub schema: u32,
    pub bundle: Sha256,
    pub initial_root: Sha256,
    pub persistent_schema: u32,
}

/// Requires an independently selected public bundle and explicitly confirmed
/// current data version. This is a local operator command, never a socket action.
pub fn provision_installed(expected_bundle: Sha256, data_version: u32) -> Result<Status> {
    provision_selected(expected_bundle, data_version, BundleSource::Image)
}

/// Explicit owner import for a bootstrapped image that did not embed a bundle.
/// Reads only OPERATOR_BUNDLE_PATH on the validated data partition. It cannot
/// replace a conflicting embedded root or reset any retained provisioning state.
pub fn provision_operator(expected_bundle: Sha256, data_version: u32) -> Result<Status> {
    provision_selected(expected_bundle, data_version, BundleSource::Operator)
}

#[derive(Clone, Copy)]
enum BundleSource {
    Image,
    Operator,
}

fn read_bundle(image: &File, system: &File, owner: u32, source: BundleSource) -> Result<Vec<u8>> {
    let embedded = optional(image, "ota-bootstrap.json", libc::O_RDONLY)?
        .map(|file| read_file(file, owner, MAX_BUNDLE_BYTES))
        .transpose()?;
    match source {
        BundleSource::Image => embedded.ok_or_else(|| invalid("image has no OTA bootstrap bundle")),
        BundleSource::Operator => {
            let file = open_at(system, "ota-bootstrap.json", libc::O_RDONLY, 0)?;
            same_device(system, &file)?;
            let bytes = read_file(file, owner, MAX_BUNDLE_BYTES)?;
            if embedded.is_some_and(|embedded| embedded != bytes) {
                return Err(invalid(
                    "operator bundle conflicts with the embedded authority",
                ));
            }
            Ok(bytes)
        }
    }
}

fn provision_selected(
    expected_bundle: Sha256,
    data_version: u32,
    source: BundleSource,
) -> Result<Status> {
    let lock = InstallLock::acquire()?;
    crate::bootstrap::require_no_active_bootstrap(&lock)?;
    let recovery = RecoveryStore::open(Path::new("/data/spacewars-system/recovery"))?;
    let controller = BootController::open(&lock)?;
    controller.authorize_maintenance(&recovery)?;
    let boot = controller.observe()?;
    if data_version == 0
        || data_version != boot.image.data.writes
        || !boot.image.data.reads.contains(data_version)
    {
        return Err(invalid(
            "operator data version must match the confirmed image's current schema",
        ));
    }
    let root = open_directory(Path::new("/"), 0)?;
    let etc = child(&root, "etc", 0, false)?;
    let image = child(&etc, "spacewars", 0, false)?;
    let system = system_directory(&boot.layout)?;
    let bytes = read_bundle(&image, &system, 0, source)?;
    let bundle = Bundle::parse(&bytes, expected_bundle)?;
    let plan = Plan {
        bundle: bundle.fingerprint,
        root: Sha256::digest(&bundle.root),
        location: bundle.location.clone(),
        binding: boot.observation.binding.clone(),
        baseline: recovery.record().fingerprint(),
        initial_data_version: data_version,
    };
    // Once any signed update has been attempted, absence of setup evidence must
    // not be treated as first provisioning, even on the original good image.
    let pristine_baseline = recovery.record().good().generation == 0
        && recovery
            .record()
            .image_history()
            .highest_attempted_generation
            == 0;
    provision_under(
        &system,
        0,
        &bundle,
        plan,
        pristine_baseline,
        || {
            lock.check()?;
            crate::bootstrap::require_no_active_bootstrap(&lock)?;
            let named_system = system_directory(&boot.layout)?;
            if system.metadata()?.dev() != named_system.metadata()?.dev()
                || system.metadata()?.ino() != named_system.metadata()?.ino()
            {
                return Err(invalid("system data directory changed"));
            }
            if read_bundle(&image, &named_system, 0, source)? != bytes {
                return Err(invalid(
                    "selected bootstrap bundle changed during provisioning",
                ));
            }
            let current = BootController::open(&lock)?;
            current.authorize_maintenance(&recovery)?;
            check_boot(&boot, &current.observe()?)
        },
        |_| Ok(()),
    )
}

fn check_boot(before: &BootEvidence, now: &BootEvidence) -> Result<()> {
    if before.image != now.image
        || before.layout != now.layout
        || before.physical != now.physical
        || before.hardware_profile != now.hardware_profile
        || before.kernel_build_id != now.kernel_build_id
        || before.observation.binding != now.observation.binding
        || before.observation.boot_id != now.observation.boot_id
        || now.observation.uptime_ms < before.observation.uptime_ms
    {
        return Err(invalid("boot identity changed during provisioning"));
    }
    Ok(())
}

/// Read the established data identity; never infer it from a booted or rolled
/// back image and never repair setup from this path.
pub fn inspect(layout: &DiskLayout, binding: &BootBinding) -> Result<Status> {
    inspect_under(&system_directory(layout)?, 0, Some(binding))
}

pub(crate) fn system_directory(layout: &DiskLayout) -> Result<File> {
    let root = open_directory(Path::new("/"), 0)?;
    let data = child(&root, "data", 0, false)?;
    if DeviceNumber::from_dev(data.metadata()?.dev()) != layout.partitions[3].device {
        return Err(invalid(
            "update state is not on the validated data partition",
        ));
    }
    let system = child(&data, "spacewars-system", 0, true)?;
    same_device(&data, &system)?;
    Ok(system)
}

pub(crate) fn updates_directory(layout: &DiskLayout) -> Result<File> {
    let system = system_directory(layout)?;
    inspect_under(&system, 0, None)?;
    child(&system, UPDATES, 0, true)
}

fn inspect_under(system: &File, owner: u32, binding: Option<&BootBinding>) -> Result<Status> {
    let record =
        load_record(system, owner)?.ok_or_else(|| invalid("updates are not provisioned"))?;
    if record.phase != Phase::Complete || binding.is_some_and(|b| b != &record.plan.binding) {
        return Err(invalid(
            "update provisioning is incomplete or belongs to another device",
        ));
    }
    if optional(system, PENDING, libc::O_RDONLY | libc::O_DIRECTORY)?.is_some()
        || optional(system, ".update-provision.next", libc::O_RDONLY)?.is_some()
    {
        return Err(invalid("published updates conflict with pending setup"));
    }
    let updates = child(system, UPDATES, owner, true)?;
    same_device(system, &updates)?;
    let saved: DataVersion = read_record(&updates, "data-schema.json", owner, 4096)?;
    if saved.schema != 1 || saved.version < record.plan.initial_data_version {
        return Err(invalid("persistent data identity is invalid or regressed"));
    }
    for component in ["trust", "staging"] {
        let dir = child(&updates, component, owner, true)?;
        same_device(&updates, &dir)?;
    }
    Ok(Status {
        schema: 1,
        bundle: record.plan.bundle,
        initial_root: record.plan.root,
        persistent_schema: saved.version,
    })
}

fn provision_under(
    system: &File,
    owner: u32,
    bundle: &Bundle,
    plan: Plan,
    pristine_baseline: bool,
    mut guard: impl FnMut() -> Result<()>,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<Status> {
    owned(&system.metadata()?, true, owner, true)?;
    plan.validate()?;
    guard()?;
    let mut record = match load_record(system, owner)? {
        Some(record) => {
            if record.plan.bundle != plan.bundle
                || record.plan.root != plan.root
                || record.plan.location != plan.location
                || record.plan.binding != plan.binding
                || record.plan.initial_data_version != plan.initial_data_version
                || (record.phase == Phase::Prepared && record.plan != plan)
            {
                return Err(invalid("provisioning request differs from retained intent"));
            }
            if record.phase == Phase::Complete {
                let status = inspect_under(system, owner, Some(&plan.binding))?;
                let trust = TrustStore::open(&fd_path(system).join(UPDATES).join("trust"))?;
                if trust.state.location != record.plan.location {
                    return Err(invalid("retained trust location differs from setup"));
                }
                guard()?;
                return Ok(status);
            }
            record
        }
        None => {
            if !pristine_baseline {
                return Err(invalid(
                    "missing setup cannot be initialized after an update attempt",
                ));
            }
            for component in [UPDATES, PENDING, "trust", "staging", "data-schema.json"] {
                if optional(system, component, libc::O_RDONLY)?.is_some() {
                    return Err(invalid(
                        "existing update files have no provisioning receipt",
                    ));
                }
            }
            let record = Record {
                schema: 1,
                phase: Phase::Prepared,
                plan,
            };
            write_record(system, owner, &record, None, &mut guard, &mut checkpoint)?;
            record
        }
    };
    let published = optional(system, UPDATES, libc::O_RDONLY | libc::O_DIRECTORY)?;
    if let Some(updates) = published {
        if optional(system, PENDING, libc::O_RDONLY | libc::O_DIRECTORY)?.is_some() {
            return Err(invalid("published and pending update directories conflict"));
        }
        verify_initial(&updates, system, owner, bundle, &record.plan, true)?;
    } else {
        guard()?;
        let pending = directory(system, PENDING, owner)?;
        verify_initial(&pending, system, owner, bundle, &record.plan, false)?;
        let trust = directory(&pending, "trust", owner)?;
        let staging = directory(&pending, "staging", owner)?;
        for dir in [&trust, &staging] {
            write_exact(dir, "lock", b"", owner, &mut guard, &mut checkpoint)?;
        }
        write_exact(
            &trust,
            "state.json",
            &bundle.initial_trust,
            owner,
            &mut guard,
            &mut checkpoint,
        )?;
        let data = bytes(&DataVersion {
            schema: 1,
            version: record.plan.initial_data_version,
        })?;
        write_exact(
            &pending,
            "data-schema.json",
            &data,
            owner,
            &mut guard,
            &mut checkpoint,
        )?;
        verify_initial(&pending, system, owner, bundle, &record.plan, true)?;
        trust.sync_all()?;
        staging.sync_all()?;
        pending.sync_all()?;
        system.sync_all()?;
        checkpoint("directory-synced")?;
        guard()?;
        check_record(system, owner, Some(&record))?;
        check_named(system, PENDING, &pending, owner)?;
        rename(system, PENDING, UPDATES, true)?;
        checkpoint("directory-published")?;
    }
    system.sync_all()?;
    checkpoint("publication-synced")?;
    guard()?;
    let updates = child(system, UPDATES, owner, true)?;
    verify_initial(&updates, system, owner, bundle, &record.plan, true)?;
    let previous = record.clone();
    record.phase = Phase::Complete;
    write_record(
        system,
        owner,
        &record,
        Some(&previous),
        &mut guard,
        &mut checkpoint,
    )?;
    guard()?;
    inspect_under(system, owner, Some(&record.plan.binding))
}

fn verify_initial(
    dir: &File,
    system: &File,
    owner: u32,
    bundle: &Bundle,
    plan: &Plan,
    complete: bool,
) -> Result<()> {
    owned(&dir.metadata()?, true, owner, true)?;
    same_device(system, dir)?;
    inventory(
        dir,
        &[
            "trust",
            "staging",
            "data-schema.json",
            ".data-schema.json.next",
        ],
    )?;
    verify_file(
        dir,
        "data-schema.json",
        &bytes(&DataVersion {
            schema: 1,
            version: plan.initial_data_version,
        })?,
        owner,
        complete,
    )?;
    for component in ["trust", "staging"] {
        let Some(sub) = optional(dir, component, libc::O_RDONLY | libc::O_DIRECTORY)? else {
            if complete {
                return Err(invalid("incomplete initial update directory"));
            }
            continue;
        };
        owned(&sub.metadata()?, true, owner, true)?;
        same_device(dir, &sub)?;
        if component == "trust" {
            inventory(
                &sub,
                &["lock", ".lock.next", "state.json", ".state.json.next"],
            )?;
            verify_file(&sub, "state.json", &bundle.initial_trust, owner, complete)?;
        } else {
            inventory(&sub, &["lock", ".lock.next"])?;
        }
        verify_file(&sub, "lock", b"", owner, complete)?;
    }
    Ok(())
}

fn verify_file(
    dir: &File,
    component: &str,
    expected: &[u8],
    owner: u32,
    complete: bool,
) -> Result<()> {
    let temporary = format!(".{component}.next");
    let pending = optional(dir, &temporary, libc::O_RDONLY)?;
    let published = optional(dir, component, libc::O_RDONLY)?;
    if pending.is_some() && (complete || published.is_some()) {
        return Err(invalid("conflicting initial file and temporary"));
    }
    if let Some(file) = pending {
        owned(&file.metadata()?, false, owner, true)?;
        same_device(dir, &file)?;
        if file.metadata()?.len() > expected.len() as u64 {
            return Err(invalid("oversized provisioning temporary"));
        }
    }
    match published {
        Some(file) => {
            owned(&file.metadata()?, false, owner, true)?;
            same_device(dir, &file)?;
            if read_file(file, owner, expected.len() as u64)? != expected {
                return Err(invalid("published initial file differs from its plan"));
            }
        }
        None if complete => return Err(invalid("missing initial update file")),
        None => {}
    }
    Ok(())
}

fn write_exact(
    dir: &File,
    component: &str,
    expected: &[u8],
    owner: u32,
    guard: &mut impl FnMut() -> Result<()>,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    verify_file(dir, component, expected, owner, false)?;
    if optional(dir, component, libc::O_RDONLY)?.is_some() {
        return Ok(());
    }
    let temp = format!(".{component}.next");
    guard()?;
    write_temporary(dir, &temp, expected, owner, checkpoint)?;
    guard()?;
    verify_temporary(dir, &temp, expected, owner)?;
    rename(dir, &temp, component, true)?;
    checkpoint("file-published")?;
    dir.sync_all()?;
    checkpoint("file-publication-synced")?;
    Ok(())
}

fn bytes<T: Serialize>(record: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Envelope {
        record,
        checksum: Sha256::digest(&serde_json::to_vec(record)?),
    })?)
}
fn read_record<T: DeserializeOwned + Serialize>(
    dir: &File,
    component: &str,
    owner: u32,
    limit: u64,
) -> Result<T> {
    let file = open_at(dir, component, libc::O_RDONLY, 0)?;
    owned(&file.metadata()?, false, owner, true)?;
    same_device(dir, &file)?;
    let stored: Envelope<T> = serde_json::from_slice(&read_file(file, owner, limit)?)?;
    if Sha256::digest(&serde_json::to_vec(&stored.record)?) != stored.checksum {
        return Err(invalid("provisioning record checksum mismatch"));
    }
    Ok(stored.record)
}
fn load_record(system: &File, owner: u32) -> Result<Option<Record>> {
    if optional(system, RECEIPT, libc::O_RDONLY)?.is_none() {
        return Ok(None);
    }
    let record: Record = read_record(system, RECEIPT, owner, RECORD_LIMIT)?;
    record.plan.validate()?;
    if record.schema != 1 {
        return Err(invalid("unsupported provisioning receipt"));
    }
    Ok(Some(record))
}
fn check_record(system: &File, owner: u32, expected: Option<&Record>) -> Result<()> {
    if load_record(system, owner)?.as_ref() != expected {
        return Err(invalid("provisioning receipt changed"));
    }
    Ok(())
}
fn write_record(
    system: &File,
    owner: u32,
    record: &Record,
    previous: Option<&Record>,
    guard: &mut impl FnMut() -> Result<()>,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    guard()?;
    check_record(system, owner, previous)?;
    let temporary = ".update-provision.next";
    write_temporary(system, temporary, &bytes(record)?, owner, checkpoint)?;
    guard()?;
    check_record(system, owner, previous)?;
    verify_temporary(system, temporary, &bytes(record)?, owner)?;
    rename(system, temporary, RECEIPT, previous.is_none())?;
    checkpoint("receipt-published")?;
    system.sync_all()?;
    checkpoint("receipt-synced")?;
    Ok(())
}
fn write_temporary(
    dir: &File,
    component: &str,
    bytes: &[u8],
    owner: u32,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut file = open_at(dir, component, libc::O_RDWR | libc::O_CREAT, 0o600)?;
    owned(&file.metadata()?, false, owner, true)?;
    same_device(dir, &file)?;
    if file.metadata()?.len() > bytes.len() as u64 {
        return Err(invalid("oversized setup temporary"));
    }
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(bytes)?;
    checkpoint("file-written")?;
    file.sync_all()?;
    checkpoint("file-synced")?;
    let named = open_at(dir, component, libc::O_RDONLY, 0)?;
    if !unchanged(&file.metadata()?, &named.metadata()?) {
        return Err(invalid("setup temporary changed"));
    }
    Ok(())
}
fn verify_temporary(dir: &File, component: &str, expected: &[u8], owner: u32) -> Result<()> {
    let file = open_at(dir, component, libc::O_RDONLY, 0)?;
    owned(&file.metadata()?, false, owner, true)?;
    same_device(dir, &file)?;
    if read_file(file, owner, expected.len() as u64)? != expected {
        return Err(invalid("setup temporary differs from selected bytes"));
    }
    Ok(())
}
fn child(parent: &File, component: &str, owner: u32, private: bool) -> Result<File> {
    let directory = open_at(parent, component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    owned(&directory.metadata()?, true, owner, private)?;
    Ok(directory)
}
fn directory(parent: &File, component: &str, owner: u32) -> Result<File> {
    let component_name = name(component)?;
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), component_name.as_ptr(), 0o700) };
    if result < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
        io_result(result)?;
    }
    let dir = child(parent, component, owner, true)?;
    same_device(parent, &dir)?;
    parent.sync_all()?;
    Ok(dir)
}
fn optional(parent: &File, component: &str, flags: i32) -> Result<Option<File>> {
    match open_at(parent, component, flags, 0) {
        Ok(file) => Ok(Some(file)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn same_device(a: &File, b: &File) -> Result<()> {
    if a.metadata()?.dev() != b.metadata()?.dev() {
        return Err(invalid("setup file is on another filesystem"));
    }
    Ok(())
}
fn check_named(parent: &File, component: &str, held: &File, owner: u32) -> Result<()> {
    let named = child(parent, component, owner, true)?;
    if !unchanged(&held.metadata()?, &named.metadata()?) {
        return Err(invalid("setup directory changed"));
    }
    Ok(())
}
fn inventory(dir: &File, allowed: &[&str]) -> Result<()> {
    for (count, entry) in fs::read_dir(fd_path(dir))?.enumerate() {
        let component = entry?.file_name();
        if count >= allowed.len() || !component.to_str().is_some_and(|n| allowed.contains(&n)) {
            return Err(invalid("unexpected provisioning file"));
        }
    }
    Ok(())
}
fn rename(dir: &File, from: &str, to: &str, exclusive: bool) -> Result<()> {
    let from = name(from)?;
    let to = name(to)?;
    io_result(unsafe {
        libc::renameat2(
            dir.as_raw_fd(),
            from.as_ptr(),
            dir.as_raw_fd(),
            to.as_ptr(),
            if exclusive { libc::RENAME_NOREPLACE } else { 0 },
        )
    })
}
fn fd_path(dir: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()))
}
fn invalid(message: &str) -> Error {
    Error::Store(format!("update provisioning: {message}"))
}

#[cfg(test)]
#[path = "provision_tests.rs"]
mod tests;
