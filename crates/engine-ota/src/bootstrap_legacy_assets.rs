//! Fixed fallback-root replacements, extracted from the selected rootfs.
//! Packaging never installs them or treats them as an executable migration hook.
use super::invalid;
use crate::Result;
use crate::file_command::FileCommand;
use std::collections::BTreeSet;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::Command;
use std::time::Duration;

pub(super) const PREFIX: &str = "legacy-root/";
pub(super) const MAX_BYTES: u64 = 64 * 1024;

// Destination modes are fixed by the migration protocol, not package input.
// The manager binary is already separately checked against the image identity.
pub(super) const FILES: &[(&str, u32)] = &[
    ("etc/sudoers.d/spacewars-ota", 0o440),
    ("etc/ssh/sshd_config.d/10-spacewars-access.conf", 0o644),
    ("usr/sbin/ab-update", 0o755),
    ("usr/sbin/ab-update-with-key", 0o755),
    ("usr/sbin/ab-boot-manager", 0o755),
    ("usr/sbin/spacewars-fast-update", 0o755),
    ("usr/sbin/spacewars-renderer", 0o755),
    ("usr/sbin/persistent-data-init", 0o755),
    ("usr/libexec/spacewars/fast-update", 0o700),
    ("usr/libexec/spacewars/renderer", 0o700),
    ("usr/lib/tmpfiles.d/spacewars-install.conf", 0o644),
    ("usr/lib/systemd/system/persistent-data-init.service", 0o644),
    ("usr/lib/systemd/system/persistent-data-bind.service", 0o644),
    (
        "usr/lib/systemd/system/NetworkManager.service.d/spacewars-persistent-network.conf",
        0o644,
    ),
    ("usr/lib/systemd/system/spacewars-access.service", 0o644),
    (
        "usr/lib/systemd/system/sshd.service.d/spacewars-access.conf",
        0o644,
    ),
    (
        "usr/lib/systemd/system/sshd@.service.d/spacewars-access.conf",
        0o644,
    ),
];

pub(super) fn package_path(path: &str) -> Option<u32> {
    let relative = path.strip_prefix(PREFIX)?;
    FILES
        .iter()
        .find_map(|(p, mode)| (*p == relative).then_some(*mode))
}

/// Read only known files from an already-pinned, checked ext4 artifact. Reject
/// links in every source component and refuse to silently normalize bad modes.
pub(super) fn extract(root: &File, mut save: impl FnMut(&str, &[u8]) -> Result<()>) -> Result<()> {
    let mut reader = Reader {
        root,
        command: FileCommand::default(),
    };
    let mut directories = BTreeSet::new();
    let root_stat = reader.stat("/")?;
    if root_stat.kind != "directory" || root_stat.mode & 0o7022 != 0 {
        return Err(invalid("unsafe legacy replacement root directory"));
    }
    for &(path, mode) in FILES {
        let mut parent = String::new();
        for component in path.split('/').take(path.split('/').count() - 1) {
            parent.push('/');
            parent.push_str(component);
            if directories.insert(parent.clone()) {
                let stat = reader.stat(&parent)?;
                if stat.kind != "directory" || stat.mode & 0o7022 != 0 {
                    return Err(invalid("unsafe legacy replacement source directory"));
                }
            }
        }
        let path = format!("/{path}");
        let stat = reader.stat(&path)?;
        if stat.kind != "regular"
            || stat.mode != mode
            || stat.links != 1
            || !(1..=MAX_BYTES).contains(&stat.bytes)
        {
            return Err(invalid("unsafe legacy replacement source file"));
        }
        let contents = reader.run(&format!("cat {path}"), MAX_BYTES as usize)?;
        if contents.len() as u64 != stat.bytes {
            return Err(invalid("legacy replacement source length changed"));
        }
        save(&format!("{PREFIX}{}", &path[1..]), &contents)?;
    }
    Ok(())
}

struct Reader<'a> {
    root: &'a File,
    command: FileCommand,
}

impl Reader<'_> {
    fn run(&mut self, request: &str, limit: usize) -> Result<Vec<u8>> {
        let mut command = Command::new("/usr/sbin/debugfs");
        command.args([
            "-R",
            request,
            &format!("/proc/self/fd/{}", self.root.as_raw_fd()),
        ]);
        self.command
            .run(command, self.root, Duration::from_secs(10), limit)
    }

    fn stat(&mut self, path: &str) -> Result<Stat> {
        let bytes = self.run(&format!("stat {path}"), 8192)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("invalid legacy replacement inode description"))?;
        Stat::parse(text)
    }
}

struct Stat {
    kind: String,
    mode: u32,
    links: u64,
    bytes: u64,
}

impl Stat {
    fn parse(text: &str) -> Result<Self> {
        let field = |prefix: &str, key: &str| -> Result<&str> {
            let mut lines = text.lines().filter(|line| line.starts_with(prefix));
            let line = lines
                .next()
                .ok_or_else(|| invalid("missing legacy replacement inode"))?;
            if lines.next().is_some() {
                return Err(invalid("ambiguous legacy replacement inode"));
            }
            let mut words = line.split_ascii_whitespace();
            words.find(|word| *word == key);
            words
                .next()
                .ok_or_else(|| invalid("missing legacy replacement inode field"))
        };
        let decimal = |prefix: &str, key: &str| -> Result<u64> {
            field(prefix, key)?
                .parse()
                .map_err(|_| invalid("invalid legacy replacement inode field"))
        };
        if decimal("User:", "User:")? != 0 || decimal("User:", "Group:")? != 0 {
            return Err(invalid("legacy replacement source must be owned by root"));
        }
        Ok(Self {
            kind: field("Inode:", "Type:")?.into(),
            mode: u32::from_str_radix(field("Inode:", "Mode:")?, 8)
                .map_err(|_| invalid("invalid legacy replacement inode mode"))?,
            links: decimal("Links:", "Links:")?,
            bytes: decimal("User:", "Size:")?,
        })
    }
}
