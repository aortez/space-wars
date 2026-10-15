//! Fixed boot assets and the only mutable root-selection grammar accepted by
//! recovery. Firmware preparation is a supervised operation, never OTA payload.
use crate::recovery::Slot;
use crate::{Blob, Error, Result, Sha256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_BOOT_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_BOOT_ASSET_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_BOOT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub const UNPREPARED_TRIAL: &str = "spacewars.trial=unprepared\n";
pub const MUTABLE_BOOT_FILES: &[&str] = &[
    "cmdline.txt",
    "spacewars-trial.txt",
    "spacewars-device.txt",
    "spacewars-boot-id",
    "spacewars-boot-manifest.json",
];
// Flash-time device naming is not kernel or slot-selection authority. Fresh
// images omit this file; existing devices keep their validated hostname.
pub const OPTIONAL_BOOT_FILES: &[&str] = &["hostname.txt"];
const REQUIRED_ASSETS: &[&str] = &[
    "config.txt",
    "tryboot.txt",
    "spacewars-cmdline.txt",
    "spacewars-kernel-build-id",
    "spacewars-recovery.cpio.gz",
    "kernel8.img",
    "kernel_2712.img",
    "spacewars-profiles/hdmi.txt",
    "spacewars-profiles/hyperpixel.txt",
    "spacewars-profiles/picade.txt",
    "overlays/spacewars-profile-hdmi.dtbo",
    "overlays/spacewars-profile-hyperpixel.dtbo",
    "overlays/spacewars-profile-picade.dtbo",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootManifest {
    schema: u32,
    #[serde(deserialize_with = "unique_files")]
    files: BTreeMap<String, Blob>,
}

impl BootManifest {
    pub fn parse(bytes: &[u8], expected: Sha256) -> Result<Self> {
        if bytes.len() > MAX_BOOT_MANIFEST_BYTES {
            return invalid("manifest exceeds its bound");
        }
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        if manifest.identity() != expected {
            return invalid("manifest does not match the installed image's boot identity");
        }
        Ok(manifest)
    }

    pub fn identity(&self) -> Sha256 {
        let mut hash = Hasher::new();
        hash.update(b"spacewars-boot-v2\n");
        for (path, blob) in &self.files {
            hash.update(format!("{path}\0{}\0{}\n", blob.bytes, blob.sha256).as_bytes());
        }
        // Preserve the public checked hexadecimal representation of Sha256.
        format!("{:x}", hash.finalize())
            .try_into()
            .expect("SHA-256 hexadecimal")
    }

    pub fn files(&self) -> &BTreeMap<String, Blob> {
        &self.files
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 2
            || self.files.len() > 1024
            || REQUIRED_ASSETS
                .iter()
                .any(|path| !self.files.contains_key(*path))
        {
            return invalid("unsupported or incomplete immutable asset manifest");
        }
        let mut folded = BTreeSet::new();
        let mut total = 0u64;
        for (path, blob) in &self.files {
            if !asset_path(path)
                || MUTABLE_BOOT_FILES.contains(&path.to_ascii_lowercase().as_str())
                || OPTIONAL_BOOT_FILES.contains(&path.to_ascii_lowercase().as_str())
                || !folded.insert(path.to_ascii_lowercase())
                || blob.bytes > MAX_BOOT_ASSET_BYTES
            {
                return invalid("unsafe, mutable, colliding or oversized boot asset");
            }
            total = total
                .checked_add(blob.bytes)
                .ok_or_else(|| failure("asset length overflow"))?;
        }
        if total > MAX_BOOT_TOTAL_BYTES
            || self.files["kernel8.img"] != self.files["kernel_2712.img"]
        {
            return invalid("boot set is too large or does not use the common Pi kernel");
        }
        Ok(())
    }
}

fn unique_files<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Blob>, D::Error> {
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = BTreeMap<String, Blob>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique boot asset names")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut files = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Blob>()? {
                if files.len() >= 1024 || files.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive boot assets",
                    ));
                }
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

pub(crate) fn asset_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 160
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// One bounded DNS host label, as written by the flash tool. This is device
/// naming only, never an alternate boot configuration or command line.
pub fn hostname(bytes: &[u8]) -> Result<&str> {
    let text = std::str::from_utf8(bytes).map_err(|_| failure("invalid hostname encoding"))?;
    let value = text.strip_suffix('\n').unwrap_or(text);
    if !(1..=63).contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
    {
        return invalid("invalid device hostname");
    }
    Ok(value)
}

#[derive(Clone, Debug)]
pub struct SlotRoots {
    a: String,
    b: String,
}

impl SlotRoots {
    pub fn new(a: String, b: String) -> Result<Self> {
        if a == b || !partition_uuid(&a) || !partition_uuid(&b) {
            return invalid("missing, duplicated or invalid root PARTUUIDs");
        }
        Ok(Self { a, b })
    }
    fn root(&self, slot: Slot) -> &str {
        match slot {
            Slot::A => &self.a,
            Slot::B => &self.b,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BootCommandLine(String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TrialSelection {
    pub slot: Slot,
    pub transaction: Sha256,
}

impl BootCommandLine {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let line = std::str::from_utf8(bytes).map_err(|_| failure("non-ASCII command line"))?;
        let args: Vec<_> = line.split_ascii_whitespace().collect();
        if line.len() > 4096
            || line != args.join(" ") + "\n"
            || args.iter().any(|arg| {
                !arg.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_=.,:/-@".contains(&b))
            })
            || args.iter().any(|arg| forbidden_argument(arg))
            || ["root=", "rootfstype=", "roottimeout="]
                .iter()
                .any(|prefix| args.iter().filter(|arg| arg.starts_with(prefix)).count() != 1)
            || [
                "root=@SPACEWARS_ROOT@",
                "rootfstype=ext4",
                "rootwait",
                "roottimeout=30",
            ]
            .iter()
            .any(|needed| args.iter().filter(|arg| *arg == needed).count() != 1)
            || line.matches("@SPACEWARS_ROOT@").count() != 1
        {
            return invalid("unsafe or unsupported root command-line template");
        }
        Ok(Self(line.into()))
    }

    pub fn render(&self, roots: &SlotRoots, slot: Slot, trial: Option<Sha256>) -> String {
        let mut line = self.0.trim_end().replace(
            "@SPACEWARS_ROOT@",
            &format!("PARTUUID={}", roots.root(slot)),
        );
        if let Some(id) = trial {
            line.push_str(&format!(" spacewars.trial={id}"));
        }
        line.push('\n');
        line
    }

    pub fn default_slot(&self, roots: &SlotRoots, bytes: &[u8]) -> Result<Slot> {
        for slot in [Slot::A, Slot::B] {
            if bytes == self.render(roots, slot, None).as_bytes() {
                return Ok(slot);
            }
        }
        invalid("default selector is not a canonical provisioned root")
    }

    pub fn trial(&self, roots: &SlotRoots, bytes: &[u8]) -> Result<Option<TrialSelection>> {
        if bytes == UNPREPARED_TRIAL.as_bytes() {
            return Ok(None);
        }
        if bytes.len() > 8192 {
            return invalid("trial command line exceeds its bound");
        }
        let line = std::str::from_utf8(bytes).map_err(|_| failure("invalid trial command line"))?;
        let tokens: Vec<_> = line
            .split_ascii_whitespace()
            .filter_map(|arg| arg.strip_prefix("spacewars.trial="))
            .collect();
        if tokens.len() != 1 {
            return invalid("missing or duplicate trial token");
        }
        let id = tokens[0].to_owned().try_into().map_err(failure)?;
        for slot in [Slot::A, Slot::B] {
            if bytes == self.render(roots, slot, Some(id)).as_bytes() {
                return Ok(Some(TrialSelection {
                    slot,
                    transaction: id,
                }));
            }
        }
        invalid("trial selector does not match the provisioned root template")
    }

    /// Pi firmware prepends board arguments. The configured suffix and every
    /// recovery-critical argument must still agree with the actual selected root.
    pub fn check_running(
        &self,
        roots: &SlotRoots,
        slot: Slot,
        trial: Option<Sha256>,
        actual: &str,
    ) -> Result<()> {
        if actual.len() > 16 * 1024 || !actual.is_ascii() || actual.contains('\0') {
            return invalid("invalid running kernel command line");
        }
        let actual: Vec<_> = actual.split_ascii_whitespace().collect();
        let expected = self.render(roots, slot, trial);
        let expected: Vec<_> = expected.split_ascii_whitespace().collect();
        if !actual.ends_with(&expected)
            || actual
                .iter()
                .any(|arg| forbidden_argument(arg) && !arg.starts_with("spacewars.trial="))
            || ["root=", "rootfstype=", "roottimeout="]
                .iter()
                .any(|prefix| actual.iter().filter(|arg| arg.starts_with(prefix)).count() != 1)
            || actual
                .iter()
                .filter(|arg| arg.starts_with("spacewars.trial="))
                .count()
                != usize::from(trial.is_some())
        {
            return invalid("loaded command line disagrees with provisioned recovery selection");
        }
        Ok(())
    }
}

impl BootCommandLine {
    /// The mutable trial file may already describe a subsequent update. The
    /// running transaction comes from firmware mode and the loaded command line.
    pub fn running_trial(
        &self,
        roots: &SlotRoots,
        slot: Slot,
        tryboot: bool,
        actual: &str,
    ) -> Result<Option<Sha256>> {
        if actual.len() > 16 * 1024 || !actual.is_ascii() || actual.contains('\0') {
            return invalid("invalid running kernel command line");
        }
        let tokens: Vec<_> = actual
            .split_ascii_whitespace()
            .filter_map(|arg| arg.strip_prefix("spacewars.trial="))
            .collect();
        if tokens.len() != usize::from(tryboot) {
            return invalid("firmware trial mode and loaded transaction disagree");
        }
        let trial = tokens
            .first()
            .map(|token| (*token).to_owned().try_into().map_err(failure))
            .transpose()?;
        self.check_running(roots, slot, trial, actual)?;
        Ok(trial)
    }
}

fn forbidden_argument(argument: &str) -> bool {
    matches!(
        argument.split('=').next(),
        Some("init" | "rdinit" | "rootdelay" | "init_fatal_sh" | "spacewars.trial")
    )
}

fn partition_uuid(value: &str) -> bool {
    (value.len() == 11 || value.len() == 36)
        && value.bytes().enumerate().all(|(i, b)| {
            if i == 8 || (value.len() == 36 && [13, 18, 23].contains(&i)) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

/// The kernel's exported AArch64 notes contain native little-endian ELF notes.
/// Do not scan arbitrary bytes for an ID or accept a duplicate/truncated note.
pub fn kernel_build_id(notes: &[u8]) -> Result<String> {
    if notes.len() > 64 * 1024 {
        return invalid("kernel notes exceed their bound");
    }
    let mut offset = 0usize;
    let mut found = None;
    while offset < notes.len() {
        let header = notes
            .get(offset..offset + 12)
            .ok_or_else(|| failure("truncated kernel note"))?;
        let namesz = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        let descsz = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(header[8..12].try_into().unwrap());
        if namesz > 64 * 1024 || descsz > 64 * 1024 {
            return invalid("oversized kernel note");
        }
        offset += 12;
        let name = notes
            .get(offset..offset + namesz)
            .ok_or_else(|| failure("truncated note owner"))?;
        offset += (namesz + 3) & !3;
        let value = notes
            .get(offset..offset + descsz)
            .ok_or_else(|| failure("truncated note value"))?;
        offset += (descsz + 3) & !3;
        if offset > notes.len() {
            return invalid("truncated note padding");
        }
        if name == b"GNU\0" && kind == 3 {
            if descsz != 20 || found.is_some() {
                return invalid("ambiguous kernel build ID");
            }
            found = Some(value.iter().map(|b| format!("{b:02x}")).collect());
        }
    }
    found.ok_or_else(|| failure("kernel has no supported GNU build ID"))
}

pub fn hardware_profile(selection: &str) -> Result<&str> {
    if selection.len() > 4096 {
        return invalid("hardware selection exceeds its bound");
    }
    let mut found = None;
    for line in selection.lines() {
        if line.trim().is_empty() || line.starts_with('#') || line == "[all]" {
            continue;
        }
        let profile = line
            .strip_prefix("include spacewars-profiles/")
            .and_then(|p| p.strip_suffix(".txt"));
        if !matches!(profile, Some("hdmi" | "hyperpixel" | "picade"))
            || found.replace(profile.unwrap()).is_some()
        {
            return invalid("unsupported or ambiguous hardware selection");
        }
    }
    found.ok_or_else(|| failure("missing hardware selection"))
}

fn failure(reason: impl Into<String>) -> Error {
    Error::Boot(reason.into())
}
fn invalid<T>(reason: &str) -> Result<T> {
    Err(failure(reason))
}

#[cfg(test)]
#[path = "boot_tests.rs"]
mod tests;
