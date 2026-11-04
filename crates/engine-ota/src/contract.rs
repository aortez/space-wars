use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeSet;
use std::fmt;
use std::io::Read;

pub const DESCRIPTOR_SCHEMA: u32 = 1;
pub const IMAGE_LAYOUT: &str = "spacewars-ab-v1";
pub const MAX_COMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024 - 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Release,
    Nightly,
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Nightly => "nightly",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256([u8; 32]);

impl Sha256 {
    pub fn digest(bytes: &[u8]) -> Self {
        Self(Hasher::digest(bytes).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl TryFrom<String> for Sha256 {
    type Error = &'static str;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("expected 64 lowercase SHA-256 hexadecimal characters");
        }
        let mut bytes = [0; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte =
                u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| "invalid SHA-256")?;
        }
        Ok(Self(bytes))
    }
}

impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.to_string()
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    pub bytes: u64,
    pub sha256: Sha256,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRange {
    pub min: u32,
    pub max: u32,
}

impl SchemaRange {
    pub fn contains(self, version: u32) -> bool {
        self.min <= version && version <= self.max
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataCompatibility {
    pub reads: SchemaRange,
    pub writes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageFormat {
    #[serde(rename = "rootfs-ext4-zstd")]
    RootfsExt4Zstd,
}

/// Stored under the signed TUF target's `custom.spacewars` object. Channel
/// publication can change; the image's generation and bytes cannot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageDescriptor {
    pub schema: u32,
    pub generation: u64,
    pub source_commit: String,
    pub build_id: String,
    pub channel: Channel,
    pub machine: String,
    pub architecture: String,
    pub layout: String,
    pub format: ImageFormat,
    pub compressed: Blob,
    pub expanded: Blob,
    pub boot_identity: Sha256,
    pub min_updater_protocol: u32,
    pub min_recovery_protocol: u32,
    pub data: DataCompatibility,
}

impl ImageDescriptor {
    pub fn validate(&self) -> Result<()> {
        let invalid = |reason: &str| Error::Descriptor(reason.to_owned());
        if self.schema != DESCRIPTOR_SCHEMA {
            return Err(invalid("unsupported schema"));
        }
        if self.generation == 0 {
            return Err(invalid("image generation must be positive"));
        }
        if self.source_commit.len() != 40
            || !self
                .source_commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("source_commit must be a complete Git SHA-1"));
        }
        for value in [
            &self.build_id,
            &self.machine,
            &self.architecture,
            &self.layout,
        ] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            {
                return Err(invalid("invalid build or compatibility identifier"));
            }
        }
        if self.compressed.bytes == 0
            || self.compressed.bytes > MAX_COMPRESSED_BYTES
            || self.expanded.bytes == 0
        {
            return Err(invalid("invalid compressed or expanded image length"));
        }
        if self.min_updater_protocol == 0 || self.min_recovery_protocol == 0 {
            return Err(invalid("updater and recovery protocols must be positive"));
        }
        if self.data.reads.min > self.data.reads.max || !self.data.reads.contains(self.data.writes)
        {
            return Err(invalid("invalid persistent data compatibility range"));
        }
        Ok(())
    }

    /// Flat, channel-scoped TUF names also work as immutable GitHub release
    /// assets. Release promotion keeps the original generation and payload hash.
    pub fn target_name(&self) -> String {
        format!(
            "{}.{}.ext4.zst",
            self.channel.name(),
            self.compressed.sha256
        )
    }

    pub fn candidate_id(&self) -> Sha256 {
        // Struct serialization is deterministic and includes all approval-relevant
        // fields, including the selected channel and compatibility requirements.
        Sha256::digest(&serde_json::to_vec(self).expect("descriptor serializes"))
    }

    pub fn eligibility(&self, device: &DeviceIdentity, history: &ImageHistory) -> Eligibility {
        if self.machine != device.machine {
            return Eligibility::WrongMachine;
        }
        if self.architecture != device.architecture {
            return Eligibility::WrongArchitecture;
        }
        if self.layout != device.layout {
            return Eligibility::WrongLayout;
        }
        if self.boot_identity != device.boot_identity {
            return Eligibility::ManualBootUpgrade;
        }
        if self.min_updater_protocol > device.updater_protocol {
            return Eligibility::UpdaterTooOld;
        }
        if self.min_recovery_protocol > device.recovery_protocol {
            return Eligibility::RecoveryUnavailable;
        }
        if self.expanded.bytes > device.inactive_slot_bytes {
            return Eligibility::ImageTooLarge;
        }
        if !self.data.reads.contains(device.persistent_schema)
            || !device.rollback_reads.contains(self.data.writes)
        {
            return Eligibility::IncompatibleData;
        }
        if history.failed_images.contains(&self.compressed.sha256) {
            return Eligibility::Quarantined;
        }
        if history.installed_sha256 == Some(self.compressed.sha256) {
            return Eligibility::AlreadyInstalled;
        }
        if self.generation <= history.highest_attempted_generation {
            return Eligibility::NotNewer;
        }
        Eligibility::Eligible
    }
}

/// Obtained from the installed image and actual disk layout, never from UI input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIdentity {
    pub machine: String,
    pub architecture: String,
    pub layout: String,
    pub boot_identity: Sha256,
    pub updater_protocol: u32,
    pub recovery_protocol: u32,
    pub inactive_slot_bytes: u64,
    pub persistent_schema: u32,
    pub rollback_reads: SchemaRange,
}

/// Persisted outside both root slots. Restoring a previous-good slot must not
/// lower the generation floor or forget the failed image.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageHistory {
    pub installed_sha256: Option<Sha256>,
    pub highest_attempted_generation: u64,
    pub failed_images: BTreeSet<Sha256>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Eligibility {
    Eligible,
    WrongMachine,
    WrongArchitecture,
    WrongLayout,
    ManualBootUpgrade,
    UpdaterTooOld,
    RecoveryUnavailable,
    ImageTooLarge,
    IncompatibleData,
    Quarantined,
    AlreadyInstalled,
    NotNewer,
}

/// Hash exactly the signed length, rejecting both truncation and trailing data.
/// Call with an already opened private staging file, not a caller-owned pathname.
pub fn verify_blob(reader: &mut impl Read, expected: &Blob) -> Result<()> {
    let mut digest = Hasher::new();
    let mut remaining = expected.bytes;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining != 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = reader.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(Error::ImageMismatch);
        }
        digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if reader.read(&mut buffer[..1])? != 0
        || digest.finalize().as_slice() != expected.sha256.as_bytes()
    {
        return Err(Error::ImageMismatch);
    }
    Ok(())
}
