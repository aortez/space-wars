//! Fixed read-only filesystem tools. No downloaded hook or shell is executed.
use crate::{ImageIdentity, MAX_IMAGE_IDENTITY_BYTES, Result, file_command::FileCommand};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::Command;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct ImageTools {
    command: FileCommand,
}

impl ImageTools {
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
