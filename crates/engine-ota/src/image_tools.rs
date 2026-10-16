//! Fixed read-only filesystem tools. No downloaded hook or shell is executed.
use crate::{ImageIdentity, MAX_IMAGE_IDENTITY_BYTES, Result, file_command::FileCommand};
use serde::Deserialize;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::Command;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct ImageTools {
    command: FileCommand,
}

impl ImageTools {
    /// Bootstrap cannot rely on the production recovery protocol alone: older
    /// images use the same protocol but cannot finish a supervised first boot.
    pub(crate) fn check_bootstrap(&mut self, file: &File) -> Result<ImageIdentity> {
        let image = self.check(file)?;
        let mut inspect = Command::new("/usr/sbin/debugfs");
        inspect.args([
            "-R",
            "cat /etc/spacewars/bootstrap-runtime.json",
            &format!("/proc/self/fd/{}", file.as_raw_fd()),
        ]);
        let bytes = self
            .command
            .run(inspect, file, Duration::from_secs(10), 1024)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Runtime {
            schema: u32,
            protocol: u32,
        }
        let runtime: Runtime = serde_json::from_slice(&bytes)?;
        if runtime.schema != 1 || runtime.protocol != crate::bootstrap::BOOTSTRAP_PROTOCOL {
            return Err(crate::Error::Boot("unsupported bootstrap runtime".into()));
        }
        Ok(image)
    }

    pub(crate) fn check(&mut self, file: &File) -> Result<ImageIdentity> {
        let path = format!("/proc/self/fd/{}", file.as_raw_fd());
        let mut fsck = Command::new("/usr/sbin/e2fsck");
        fsck.args(["-f", "-n", &path]);
        self.command
            .run(fsck, file, Duration::from_secs(120), 64 * 1024)?;
        let mut inspect = Command::new("/usr/sbin/debugfs");
        inspect.args(["-R", "cat /etc/spacewars/image.json", &path]);
        let record = self.command.run(
            inspect,
            file,
            Duration::from_secs(10),
            MAX_IMAGE_IDENTITY_BYTES,
        )?;
        ImageIdentity::parse(&record)
    }
}
