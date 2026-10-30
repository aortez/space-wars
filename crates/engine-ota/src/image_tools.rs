//! Fixed read-only filesystem tools. No downloaded hook or shell is executed.
use crate::{ImageIdentity, MAX_IMAGE_IDENTITY_BYTES, Result, file_command::FileCommand};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::process::Command;
use std::time::Duration;

/// Bounded reads of fixed paths, rejecting links and writable/non-root-owned
/// ancestors. The caller supplies paths from trusted code, never image input.
pub(crate) struct ImageReader<'a> {
    root: &'a File,
    command: FileCommand,
    directories: BTreeSet<String>,
}

impl<'a> ImageReader<'a> {
    pub(crate) fn new(root: &'a File) -> Self {
        Self {
            root,
            command: FileCommand::default(),
            directories: BTreeSet::new(),
        }
    }

    pub(crate) fn file(&mut self, path: &str, mode: u32, maximum: usize) -> Result<Vec<u8>> {
        // debugfs has its own command grammar. Only literal absolute paths in
        // our code are supported, not quoting, whitespace or traversal.
        if !path.starts_with('/')
            || path == "/"
            || path[1..].split('/').any(|p| {
                p.is_empty()
                    || p == "."
                    || p == ".."
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
            })
        {
            return Err(invalid("invalid fixed image path"));
        }
        let mut parents = vec!["/".to_owned()];
        let mut parent = String::new();
        for part in path[1..].split('/').take(path[1..].split('/').count() - 1) {
            parent.push('/');
            parent.push_str(part);
            parents.push(parent.clone());
        }
        for parent in parents {
            if !self.directories.contains(&parent) {
                let stat = self.stat(&parent)?;
                if stat.kind != "directory" || stat.mode & 0o7022 != 0 {
                    return Err(invalid("unsafe image source directory"));
                }
                self.directories.insert(parent);
            }
        }
        let stat = self.stat(path)?;
        if stat.kind != "regular"
            || stat.mode != mode
            || stat.links != 1
            || !(1..=maximum as u64).contains(&stat.bytes)
        {
            return Err(invalid("unsafe image source file"));
        }
        let contents = self.run(&format!("cat {path}"), maximum)?;
        if contents.len() as u64 != stat.bytes {
            return Err(invalid("image source length changed"));
        }
        Ok(contents)
    }

    fn run(&mut self, request: &str, limit: usize) -> Result<Vec<u8>> {
        let mut command = Command::new("/usr/sbin/debugfs");
        command.args([
            "-R",
            request,
            &format!("/proc/self/fd/{}", self.root.as_raw_fd()),
        ]);
        self.command
            .run(command, self.root, Duration::from_secs(30), limit)
    }

    fn stat(&mut self, path: &str) -> Result<ImageStat> {
        let bytes = self.run(&format!("stat {path}"), 8192)?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| invalid("invalid image inode description"))?;
        ImageStat::parse(text)
    }
}

struct ImageStat {
    kind: String,
    mode: u32,
    links: u64,
    bytes: u64,
}

impl ImageStat {
    fn parse(text: &str) -> Result<Self> {
        let field = |prefix: &str, key: &str| -> Result<&str> {
            let mut lines = text.lines().filter(|line| line.starts_with(prefix));
            let line = lines.next().ok_or_else(|| invalid("missing image inode"))?;
            if lines.next().is_some() {
                return Err(invalid("ambiguous image inode"));
            }
            let mut words = line.split_ascii_whitespace();
            words.find(|word| *word == key);
            words
                .next()
                .ok_or_else(|| invalid("missing image inode field"))
        };
        let decimal = |prefix: &str, key: &str| -> Result<u64> {
            field(prefix, key)?
                .parse()
                .map_err(|_| invalid("invalid image inode field"))
        };
        if decimal("User:", "User:")? != 0 || decimal("User:", "Group:")? != 0 {
            return Err(invalid("image source must be owned by root"));
        }
        Ok(Self {
            kind: field("Inode:", "Type:")?.into(),
            mode: u32::from_str_radix(field("Inode:", "Mode:")?, 8)
                .map_err(|_| invalid("invalid image inode mode"))?,
            links: decimal("Links:", "Links:")?,
            bytes: decimal("User:", "Size:")?,
        })
    }
}

fn invalid(message: &str) -> crate::Error {
    crate::Error::Install(message.into())
}

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

    /// A development trial's record is schema 3. Older images refuse it, so
    /// they could never finish the trial; refuse them before writing a slot.
    pub(crate) fn check_development(&mut self, file: &File) -> Result<ImageIdentity> {
        let image = self.check(file)?;
        let mut inspect = Command::new("/usr/sbin/debugfs");
        inspect.args([
            "-R",
            "cat /etc/spacewars/development-runtime.json",
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
        match serde_json::from_slice::<Runtime>(&bytes) {
            Ok(runtime)
                if runtime.schema == 1
                    && runtime.protocol == crate::development::DEVELOPMENT_PROTOCOL =>
            {
                Ok(image)
            }
            _ => Err(crate::Error::Install(
                "image cannot finish a development trial; build it from a source with development installs".into(),
            )),
        }
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
