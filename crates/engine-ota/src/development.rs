//! Locally built images that the root operator installs over SSH, without the
//! signed feed. They never take a generation, so official images still install
//! over them. The descriptor names exact bytes; it grants no authority itself.
use crate::{
    Blob, DataCompatibility, DeviceIdentity, Error, IMAGE_LAYOUT, ImageFormat, ImageIdentity,
    MAX_COMPRESSED_BYTES, Result, Sha256,
};
use serde::{Deserialize, Serialize};

pub const DEVELOPMENT_SCHEMA: u32 = 1;
/// Declared by `/etc/spacewars/development-runtime.json` in images whose
/// recovery service can finish a development trial.
pub const DEVELOPMENT_PROTOCOL: u32 = 1;
pub const MAX_DEVELOPMENT_BYTES: usize = 16 * 1024;

/// The signed descriptor's fields without generation or channel, plus the
/// source's dirty flag. The workstation derives it from the packed image.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentImage {
    pub schema: u32,
    pub source_commit: String,
    pub source_dirty: bool,
    pub build_id: String,
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

impl DevelopmentImage {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_DEVELOPMENT_BYTES {
            return Err(invalid("development descriptor exceeds its size bound"));
        }
        let image: Self = serde_json::from_slice(bytes)?;
        image.validate()?;
        Ok(image)
    }

    pub fn from_identity(identity: &ImageIdentity, compressed: Blob, expanded: Blob) -> Self {
        Self {
            schema: DEVELOPMENT_SCHEMA,
            source_commit: identity.source.commit.clone(),
            source_dirty: identity.source.dirty,
            build_id: identity.build_id.clone(),
            machine: identity.machine.clone(),
            architecture: identity.architecture.clone(),
            layout: identity.layout.clone(),
            format: ImageFormat::RootfsExt4Zstd,
            compressed,
            expanded,
            boot_identity: identity.boot_identity,
            min_updater_protocol: identity.updater_protocol,
            min_recovery_protocol: identity.recovery_protocol,
            data: identity.data,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != DEVELOPMENT_SCHEMA {
            return Err(invalid("unsupported development descriptor schema"));
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
        if self.layout != IMAGE_LAYOUT {
            return Err(invalid("unsupported image layout"));
        }
        if self.compressed.bytes == 0
            || self.compressed.bytes > MAX_COMPRESSED_BYTES
            || self.expanded.bytes == 0
        {
            return Err(invalid("invalid compressed or expanded image length"));
        }
        if self.boot_identity.as_bytes() == &[0; 32] {
            return Err(invalid("missing boot identity"));
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

    /// Deterministic serialization covers every field, so the transaction is
    /// bound to these exact bytes and compatibility claims.
    pub fn candidate_id(&self) -> Sha256 {
        Sha256::digest(&serde_json::to_vec(self).expect("descriptor serializes"))
    }

    /// The same compatibility rules as feed eligibility, with reasons a
    /// developer can act on. Installation history is the recovery record's.
    pub fn check_device(&self, device: &DeviceIdentity) -> Result<()> {
        self.validate()?;
        if self.machine != device.machine || self.architecture != device.architecture {
            return Err(invalid("image was built for another machine"));
        }
        if self.layout != device.layout {
            return Err(invalid("image uses another slot layout"));
        }
        if self.boot_identity != device.boot_identity {
            return Err(invalid(
                "image boot files differ from this unit's; they change only by USB flash",
            ));
        }
        if self.min_updater_protocol > device.updater_protocol {
            return Err(invalid("image needs a newer updater than this unit runs"));
        }
        if self.min_recovery_protocol != device.recovery_protocol {
            return Err(invalid(
                "image recovery protocol differs from this unit's; it changes only by USB flash",
            ));
        }
        if self.expanded.bytes > device.inactive_slot_bytes {
            return Err(invalid("image does not fit the inactive slot"));
        }
        if !self.data.reads.contains(device.persistent_schema)
            || !device.rollback_reads.contains(self.data.writes)
        {
            return Err(invalid(
                "image persistent data version is incompatible with this unit's",
            ));
        }
        Ok(())
    }

    /// Unlike the signed feed, dirty sources are allowed. Everything the
    /// descriptor claims must still match the record embedded in the image.
    pub fn matches_identity(&self, identity: &ImageIdentity) -> Result<()> {
        identity.validate()?;
        self.validate()?;
        if identity.source.commit != self.source_commit
            || identity.source.dirty != self.source_dirty
            || identity.build_id != self.build_id
            || identity.machine != self.machine
            || identity.architecture != self.architecture
            || identity.layout != self.layout
            || identity.boot_identity != self.boot_identity
            || identity.data != self.data
            || identity.updater_protocol < self.min_updater_protocol
            || identity.recovery_protocol < self.min_recovery_protocol
        {
            return Err(invalid(
                "written image does not match its development descriptor",
            ));
        }
        Ok(())
    }
}

fn invalid(reason: &str) -> Error {
    Error::Descriptor(reason.to_owned())
}

#[cfg(test)]
#[path = "development_tests.rs"]
mod tests;
