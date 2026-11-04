//! Read-only native qualification admission. A separately authenticated owner
//! approves retained physical reports; neither a successful boot nor this code
//! creates that approval. No UI path, Boolean or supervised trial is authority.
use crate::boot_linux::BootEvidence;
use crate::owned_fs::{open_at, owned, read_file, unchanged};
use crate::recovery::{BootBinding, Plan, RECOVERY_PROTOCOL, RecoveryCapability, RecoveryRecord};
use crate::{Blob, Error, Result, Sha256, VerifiedCandidate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;

const DIRECTORY: &str = "recovery-qualification";
const APPROVAL: &str = "approval.json";
const APPROVAL_BYTES: u64 = 16 * 1024;
const REPORT_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Case {
    HealthyTrial,
    NormalBoot,
    RootStartupFailure,
    RecoveryServiceFailure,
    KioskCrash,
    KioskHang,
    TrialReset,
    WatchdogHandoff,
    RepeatedFailure,
    InterruptedPromotion,
    PowerLoss,
}
const CASES: [Case; 11] = [
    Case::HealthyTrial,
    Case::NormalBoot,
    Case::RootStartupFailure,
    Case::RecoveryServiceFailure,
    Case::KioskCrash,
    Case::KioskHang,
    Case::TrialReset,
    Case::WatchdogHandoff,
    Case::RepeatedFailure,
    Case::InterruptedPromotion,
    Case::PowerLoss,
];
impl Case {
    fn filename(self) -> String {
        // Enum serialization is the fixed case name, never a caller-supplied path.
        format!(
            "{}.json",
            serde_json::to_value(self).unwrap().as_str().unwrap()
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    case: Case,
    report: Blob,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema: u32,
    purpose: String,
    binding: BootBinding,
    kernel_build_id: String,
    recovery_protocol: u32,
    passed: Vec<Evidence>,
}
impl Approval {
    fn validate(&self, boot: &BootEvidence) -> Result<()> {
        if self.schema != 1
            || self.purpose != "physical-recovery-admission"
            || self.binding != boot.observation.binding
            || self.kernel_build_id != boot.kernel_build_id
            || self.kernel_build_id.len() != 40
            || !self
                .kernel_build_id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.recovery_protocol != RECOVERY_PROTOCOL
            || boot.observation.recovery_protocol != RECOVERY_PROTOCOL
            || [
                self.binding.device_id,
                self.binding.layout_id,
                self.binding.boot_identity,
                self.binding.hardware_profile,
            ]
            .iter()
            .any(|h| h.as_bytes() == &[0; 32])
            || self.passed.len() != CASES.len()
            || self.passed.iter().map(|e| e.case).collect::<BTreeSet<_>>()
                != CASES.into_iter().collect()
            || self.passed.iter().any(|e| {
                !(1..=REPORT_BYTES).contains(&e.report.bytes)
                    || e.report.sha256.as_bytes() == &[0; 32]
            })
        {
            return Err(invalid(
                "incomplete or mismatched physical qualification approval",
            ));
        }
        Ok(())
    }
}

struct Opened {
    name: String,
    file: File,
    before: Metadata,
}
impl Opened {
    fn read(directory: &File, name: String, owner: u32, limit: u64) -> Result<(Self, Vec<u8>)> {
        // Pin and inspect before opening for data: even a mistaken device node
        // must not activate hardware while being rejected as invalid evidence.
        let pinned = open_at(directory, &name, libc::O_PATH, 0)?;
        let before = pinned.metadata()?;
        owned(&before, false, owner, true)?;
        if before.dev() != directory.metadata()?.dev() {
            return Err(invalid("qualification evidence crosses filesystems"));
        }
        let file = File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd()))?;
        let bytes = read_file(file.try_clone()?, owner, limit)?;
        let opened = Self { name, file, before };
        opened.check(directory, owner)?;
        Ok((opened, bytes))
    }

    fn check(&self, directory: &File, owner: u32) -> Result<()> {
        let named = open_at(directory, &self.name, libc::O_PATH, 0)?;
        owned(&named.metadata()?, false, owner, true)?;
        if !unchanged(&self.before, &self.file.metadata()?)
            || !unchanged(&self.before, &named.metadata()?)
        {
            return Err(invalid("qualification evidence changed; reopen required"));
        }
        Ok(())
    }
}

/// Internal, non-serializable evidence retained until the final plan check.
/// Production construction always walks the validated /data partition.
pub(crate) struct Qualification {
    parent: File,
    directory: File,
    owner: u32,
    approval: Approval,
    files: Vec<Opened>,
}
impl Qualification {
    pub(crate) fn open(boot: &BootEvidence) -> Result<Self> {
        Self::under(
            crate::provision_linux::system_directory(&boot.layout)?,
            0,
            boot,
        )
    }

    fn under(parent: File, owner: u32, boot: &BootEvidence) -> Result<Self> {
        owned(&parent.metadata()?, true, owner, true)?;
        let directory = open_at(&parent, DIRECTORY, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&directory.metadata()?, true, owner, true)?;
        if directory.metadata()?.dev() != parent.metadata()?.dev() {
            return Err(invalid("qualification is not on the data partition"));
        }
        let (file, bytes) = Opened::read(&directory, APPROVAL.into(), owner, APPROVAL_BYTES)?;
        let approval: Approval = serde_json::from_slice(&bytes)?;
        approval.validate(boot)?;
        let mut files = vec![file];
        for evidence in &approval.passed {
            let (file, bytes) =
                Opened::read(&directory, evidence.case.filename(), owner, REPORT_BYTES)?;
            if bytes.len() as u64 != evidence.report.bytes
                || Sha256::digest(&bytes) != evidence.report.sha256
            {
                return Err(invalid(
                    "retained physical report differs from its approval",
                ));
            }
            files.push(file);
        }
        let proof = Self {
            parent,
            directory,
            owner,
            approval,
            files,
        };
        proof.check()?;
        Ok(proof)
    }

    fn check(&self) -> Result<()> {
        owned(&self.parent.metadata()?, true, self.owner, true)?;
        owned(&self.directory.metadata()?, true, self.owner, true)?;
        let named = open_at(
            &self.parent,
            DIRECTORY,
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        if !same_directory(&self.directory, &named)? {
            return Err(invalid("qualification directory changed; reopen required"));
        }
        for file in &self.files {
            file.check(&self.directory, self.owner)?;
        }
        Ok(())
    }

    pub(crate) fn plan_native(
        &self,
        record: &RecoveryRecord,
        boot: &BootEvidence,
        candidate: &VerifiedCandidate,
    ) -> Result<Plan> {
        let parent = crate::provision_linux::system_directory(&boot.layout)?;
        if !same_directory(&self.parent, &parent)? {
            return Err(invalid("qualification parent changed; reopen required"));
        }
        self.plan(record, boot, candidate)
    }

    fn plan(
        &self,
        record: &RecoveryRecord,
        boot: &BootEvidence,
        candidate: &VerifiedCandidate,
    ) -> Result<Plan> {
        self.check()?;
        self.approval.validate(boot)?;
        // Admission is only from a confirmed normal boot. Neither a successful
        // trial nor an owner-selected supervised transaction grants qualification.
        if boot.observation.tryboot {
            return Err(invalid("installation requires a confirmed normal boot"));
        }
        let capability = RecoveryCapability {
            binding: self.approval.binding.clone(),
            tryboot_qualified: true,
            watchdog_qualified: true,
        };
        Ok(record.begin(candidate, &boot.observation, &capability)?)
    }
}

fn same_directory(left: &File, right: &File) -> Result<bool> {
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

fn invalid(detail: &str) -> Error {
    Error::Boot(format!("recovery qualification: {detail}"))
}

#[cfg(test)]
#[path = "recovery_admission_tests.rs"]
mod tests;
