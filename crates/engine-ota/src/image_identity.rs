//! Image-contained evidence, authenticated by the signed rootfs payload hash.
//! This record is not itself a signature or permission to install. Build-time
//! tree hashing supplies a stable name; runtime health checks the executable.
use crate::{Blob, DataCompatibility, Error, IMAGE_LAYOUT, ImageDescriptor, Result, Sha256};
use serde::{Deserialize, Serialize};

pub const IMAGE_IDENTITY_SCHEMA: u32 = 1;
pub const MAX_IMAGE_IDENTITY_BYTES: usize = 16 * 1024;
pub const MAX_IMAGE_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;
pub const UPDATER_PROTOCOL: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSource {
    pub schema: u32,
    pub commit: String,
    pub dirty: bool,
    pub revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageBinaries {
    pub client: Blob,
    pub os_manager: Blob,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageIdentity {
    pub schema: u32,
    pub build_id: String,
    pub rootfs_tree: Sha256,
    pub source: BuildSource,
    pub machine: String,
    pub architecture: String,
    pub layout: String,
    pub boot_identity: Sha256,
    pub updater_protocol: u32,
    pub recovery_protocol: u32,
    pub data: DataCompatibility,
    pub binaries: ImageBinaries,
}

impl ImageIdentity {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_IMAGE_IDENTITY_BYTES {
            return invalid("image record exceeds its size bound");
        }
        let identity: Self = serde_json::from_slice(bytes)?;
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != IMAGE_IDENTITY_SCHEMA || self.source.schema != 1 {
            return invalid("unsupported schema");
        }
        if self.source.commit.len() != 40
            || !self
                .source
                .commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return invalid("source commit must be a complete Git SHA-1");
        }
        let revision = format!(
            "{}{}",
            &self.source.commit[..12],
            if self.source.dirty { "-dirty" } else { "" }
        );
        if self.source.revision != revision {
            return invalid("compiled revision does not match packaged source");
        }
        if self.build_id != format!("rootfs-{}", self.rootfs_tree)
            || self.rootfs_tree.as_bytes() == &[0; 32]
            || self.boot_identity.as_bytes() == &[0; 32]
            || self.layout != IMAGE_LAYOUT
            || !identifier(&self.machine)
            || !identifier(&self.architecture)
            || self.updater_protocol == 0
            || self.recovery_protocol == 0
            || self.data.reads.min > self.data.reads.max
            || !self.data.reads.contains(self.data.writes)
        {
            return invalid("invalid build or compatibility identity");
        }
        for binary in [&self.binaries.client, &self.binaries.os_manager] {
            if !(1..=MAX_IMAGE_EXECUTABLE_BYTES).contains(&binary.bytes)
                || binary.sha256.as_bytes() == &[0; 32]
            {
                return invalid("invalid packaged executable identity");
            }
        }
        Ok(())
    }

    /// Only meaningful after the containing image bytes have been verified.
    /// Generations/channels are publisher metadata; promotion retains the same
    /// packaged record and never rewrites the rootfs to relabel it.
    pub fn matches_descriptor(&self, image: &ImageDescriptor) -> Result<()> {
        self.validate()?;
        image.validate()?;
        if self.source.dirty {
            return invalid("dirty development source is not eligible for the signed feed");
        }
        if self.source.commit != image.source_commit
            || self.build_id != image.build_id
            || self.machine != image.machine
            || self.architecture != image.architecture
            || self.layout != image.layout
            || self.boot_identity != image.boot_identity
            || self.data != image.data
            || self.updater_protocol < image.min_updater_protocol
            || self.recovery_protocol < image.min_recovery_protocol
        {
            return invalid("packaged image does not match its signed descriptor");
        }
        Ok(())
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn invalid<T>(reason: &str) -> Result<T> {
    Err(Error::ImageIdentity(reason.into()))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::ffi::CStr;
    use std::fs::{File, Metadata, OpenOptions};
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;

    impl ImageIdentity {
        /// Read only the fixed installed record, through root-owned directory
        /// handles. A public socket must never supply an alternate image path.
        pub fn read_installed() -> Result<Self> {
            read_root(Path::new("/"), 0)
        }
    }

    fn owned(metadata: &Metadata, directory: bool, owner: u32) -> Result<()> {
        if metadata.uid() != owner
            || metadata.mode() & 0o022 != 0
            || if directory {
                !metadata.is_dir()
            } else {
                !metadata.is_file()
            }
        {
            return invalid("installed image record has unsafe ownership, type or permissions");
        }
        Ok(())
    }

    fn child(parent: &File, name: &CStr, directory: bool) -> Result<File> {
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: parent remains open and name is a fixed valid C string. No
        // creation is requested; the successful new descriptor is uniquely owned.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: openat returned a new owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn read_root(root: &Path, owner: u32) -> Result<ImageIdentity> {
        let mut directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)?;
        owned(&directory.metadata()?, true, owner)?;
        for name in [c"etc", c"spacewars"] {
            directory = child(&directory, name, true)?;
            owned(&directory.metadata()?, true, owner)?;
        }
        let mut file = child(&directory, c"image.json", false)?;
        let before = file.metadata()?;
        owned(&before, false, owner)?;
        if before.len() > MAX_IMAGE_IDENTITY_BYTES as u64 {
            return invalid("installed image record exceeds its size bound");
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_IMAGE_IDENTITY_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.uid() != after.uid()
            || before.gid() != after.gid()
            || before.mode() != after.mode()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
            || after.len() != bytes.len() as u64
        {
            return invalid("installed image record changed during reading");
        }
        ImageIdentity::parse(&bytes)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;
        use std::os::unix::fs::{PermissionsExt, symlink};

        #[test]
        fn installed_reader_rejects_symlinks_permissions_owner_size_and_missing_state() {
            let root = tempfile::tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let directory = root.path().join("etc/spacewars");
            fs::create_dir_all(&directory).unwrap();
            for dir in [root.path().join("etc"), directory.clone()] {
                fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let path = directory.join("image.json");
            let bytes = include_bytes!("../tests/fixtures/image-identity-v1.json");
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            // SAFETY: geteuid is a side-effect-free query with no preconditions.
            let owner = unsafe { libc::geteuid() };
            assert_eq!(
                read_root(root.path(), owner).unwrap(),
                ImageIdentity::parse(bytes).unwrap()
            );
            assert!(read_root(root.path(), owner.wrapping_add(1)).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
            assert!(read_root(root.path(), owner).is_err());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
            assert!(read_root(root.path(), owner).is_err());
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
            fs::write(&path, vec![b' '; MAX_IMAGE_IDENTITY_BYTES + 1]).unwrap();
            assert!(read_root(root.path(), owner).is_err());
            fs::remove_file(&path).unwrap();
            assert!(read_root(root.path(), owner).is_err());
            let target = root.path().join("outside.json");
            fs::write(&target, bytes).unwrap();
            symlink(&target, &path).unwrap();
            assert!(read_root(root.path(), owner).is_err());
            fs::remove_file(&path).unwrap();
            fs::remove_dir(&directory).unwrap();
            symlink(root.path(), &directory).unwrap();
            assert!(read_root(root.path(), owner).is_err());
        }
    }
}

#[cfg(test)]
#[path = "image_identity_tests.rs"]
mod tests;
