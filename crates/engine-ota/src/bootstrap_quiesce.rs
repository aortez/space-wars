//! Supervised root-file migration requires a quiet runtime account and drained
//! legacy helpers. This guard observes; it never kills processes or services.
use super::invalid;
use crate::owned_fs::{open_at, open_directory, owned};
use crate::{Error, Result};
use std::fs::{self, File};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

const FAST_LOCK: &str = "spacewars-fast-update.lock";
const RUNTIME_UID: u32 = 1000;
const MAX_PROCESSES: usize = 32768;
const MAX_TEXT: u64 = 64 * 1024;
const HELPERS: &[&[u8]] = &[
    b"ab-update",
    b"ab-update-with-key",
    b"ab-boot-manager",
    b"spacewars-fast-update",
    b"spacewars-renderer",
    b"persistent-data-init",
    b"engine-os-manager",
];

pub(super) struct QuietRuntime {
    directory: File,
    lock: File,
}

impl QuietRuntime {
    pub(super) fn acquire() -> Result<Self> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(invalid("legacy migration requires a root operator"));
        }
        require_runtime_account()?;
        let directory = lock_directory()?;
        let lock = match open_at(
            &directory,
            FAST_LOCK,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        ) {
            Ok(file) => file,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                open_at(&directory, FAST_LOCK, libc::O_RDWR, 0)?
            }
            Err(e) => return Err(e),
        };
        owned(&lock.metadata()?, false, 0, false)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } < 0 {
            let error = std::io::Error::last_os_error();
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                invalid("legacy fast update is still running")
            } else {
                error.into()
            });
        }
        let guard = Self { directory, lock };
        guard.check()?;
        Ok(guard)
    }

    pub(super) fn check(&self) -> Result<()> {
        let named_directory = lock_directory()?;
        let named = open_at(&named_directory, FAST_LOCK, libc::O_RDONLY, 0)?;
        owned(&named.metadata()?, false, 0, false)?;
        if !same_inode(&named_directory, &self.directory)? || !same_inode(&named, &self.lock)? {
            return Err(invalid("legacy helper lock changed"));
        }
        let proc = File::open("/proc")?;
        inspect(&proc)
    }
}

fn lock_directory() -> Result<File> {
    let run = open_directory(Path::new("/run"), 0)?;
    lock_directory_under(&run, 0)
}

fn lock_directory_under(run: &File, owner: u32) -> Result<File> {
    let directory = open_at(run, "lock", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let metadata = directory.metadata()?;
    // Yocto uses root-owned 01777 /run/lock. Its sticky bit protects our
    // root-owned lock from other users; an existing foreign lock is rejected.
    if metadata.uid() != owner
        || !metadata.is_dir()
        || metadata.mode() & 0o6000 != 0
        || metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0
        || metadata.dev() != run.metadata()?.dev()
        || crate::recovery_linux::opened_mount_id(&directory)?
            != crate::recovery_linux::opened_mount_id(run)?
    {
        return Err(invalid("unsafe legacy lock directory"));
    }
    Ok(directory)
}

fn require_runtime_account() -> Result<()> {
    let mut buffer = [0u8; 16 * 1024];
    // SAFETY: getpwnam_r initializes the passwd structure using this live
    // buffer; result is checked before any fields are read.
    let mut account = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = std::ptr::null_mut();
    let status = unsafe {
        libc::getpwnam_r(
            c"spacewars".as_ptr(),
            account.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() {
        return Err(invalid("cannot establish the runtime account identity"));
    }
    let account = unsafe { account.assume_init() };
    if account.pw_uid != RUNTIME_UID || account.pw_gid != RUNTIME_UID {
        return Err(invalid("unsupported runtime account identity"));
    }
    Ok(())
}

fn same_inode(a: &File, b: &File) -> Result<bool> {
    Ok(a.metadata()?.dev() == b.metadata()?.dev() && a.metadata()?.ino() == b.metadata()?.ino())
}

fn inspect(proc: &File) -> Result<()> {
    let mut count = 0;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", proc.as_raw_fd()))? {
        let name = entry?.file_name();
        let Some(pid) = name
            .to_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        else {
            continue;
        };
        count += 1;
        if count > MAX_PROCESSES {
            return Err(invalid("too many processes for supervised migration"));
        }
        let process = match open_at(proc, pid, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
            Ok(file) => file,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        match inspect_process(&process) {
            Ok(()) => (),
            // A process exiting during inspection is no longer a writer.
            Err(Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound
                    || e.raw_os_error() == Some(libc::ESRCH) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn inspect_process(process: &File) -> Result<()> {
    let tasks = open_at(process, "task", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let mut count = 0;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", tasks.as_raw_fd()))? {
        count += 1;
        if count > MAX_PROCESSES {
            return Err(invalid("too many threads for supervised migration"));
        }
        let name = entry?.file_name();
        let tid = name
            .to_str()
            .ok_or_else(|| invalid("invalid thread identity"))?;
        let status = open_at(&tasks, tid, libc::O_RDONLY | libc::O_DIRECTORY, 0)
            .and_then(|task| bounded(&task, "status"));
        match status {
            Ok(status) => quiet_uids(&status)?,
            Err(Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound
                    || e.raw_os_error() == Some(libc::ESRCH) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    let command = bounded(process, "cmdline")?;
    if command
        .split(|b| *b == 0)
        .any(|arg| HELPERS.contains(&arg.rsplit(|b| *b == b'/').next().unwrap_or(b"")))
    {
        return Err(invalid(
            "legacy maintenance or state helper is still running",
        ));
    }
    let fds = open_at(process, "fd", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let info = open_at(process, "fdinfo", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    let mut count = 0;
    for entry in fs::read_dir(format!("/proc/self/fd/{}", fds.as_raw_fd()))? {
        count += 1;
        if count > MAX_PROCESSES {
            return Err(invalid("too many descriptors for supervised migration"));
        }
        let name = entry?.file_name();
        let component = name
            .to_str()
            .ok_or_else(|| invalid("invalid descriptor identity"))?;
        // Metadata follows only kernel-owned /proc descriptor links, without
        // opening a device (in particular, never opening the watchdog).
        let metadata =
            match fs::metadata(format!("/proc/self/fd/{}/{}", fds.as_raw_fd(), component)) {
                Ok(metadata) => metadata,
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        || e.raw_os_error() == Some(libc::ESRCH) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
        if !metadata.file_type().is_block_device() {
            continue;
        }
        let flags = match bounded(&info, component) {
            Ok(flags) => flags,
            Err(Error::Io(e))
                if e.kind() == std::io::ErrorKind::NotFound
                    || e.raw_os_error() == Some(libc::ESRCH) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        if writable_flags(&flags)? {
            return Err(invalid("a process still holds a block device for writing"));
        }
    }
    Ok(())
}

fn bounded(directory: &File, component: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open_at(directory, component, libc::O_RDONLY, 0)?
        .take(MAX_TEXT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TEXT {
        return Err(invalid("process evidence exceeds its bound"));
    }
    Ok(bytes)
}

fn quiet_uids(bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("invalid process status"))?;
    let mut matches = text.lines().filter_map(|line| line.strip_prefix("Uid:"));
    let field = matches
        .next()
        .ok_or_else(|| invalid("missing process UIDs"))?;
    let ids = field
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| invalid("invalid process UIDs"))?;
    if matches.next().is_some() || ids.len() != 4 {
        return Err(invalid("ambiguous process UIDs"));
    }
    if ids.contains(&RUNTIME_UID) {
        return Err(invalid(
            "runtime account is still active; stop its applications and close its sessions before root migration",
        ));
    }
    Ok(())
}

fn writable_flags(bytes: &[u8]) -> Result<bool> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("invalid descriptor evidence"))?;
    let mut matches = text.lines().filter_map(|line| line.strip_prefix("flags:"));
    let flags = u32::from_str_radix(
        matches
            .next()
            .ok_or_else(|| invalid("missing descriptor flags"))?
            .trim(),
        8,
    )
    .map_err(|_| invalid("invalid descriptor flags"))?;
    if matches.next().is_some() {
        return Err(invalid("ambiguous descriptor flags"));
    }
    Ok(flags & libc::O_ACCMODE as u32 != libc::O_RDONLY as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn root_owned_sticky_lock_directory_is_allowed_but_plain_writable_directory_is_not() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("lock")).unwrap();
        let run = File::open(temp.path()).unwrap();
        let owner = run.metadata().unwrap().uid();
        for mode in [0o755, 0o1777] {
            fs::set_permissions(temp.path().join("lock"), fs::Permissions::from_mode(mode))
                .unwrap();
            lock_directory_under(&run, owner).unwrap();
        }
        for mode in [0o777, 0o775, 0o2777] {
            fs::set_permissions(temp.path().join("lock"), fs::Permissions::from_mode(mode))
                .unwrap();
            assert!(lock_directory_under(&run, owner).is_err());
        }
    }
    #[test]
    fn real_effective_saved_and_filesystem_runtime_uids_all_block_migration() {
        for i in 0..4 {
            let mut ids = [0; 4];
            ids[i] = RUNTIME_UID;
            assert!(
                quiet_uids(
                    format!(
                        "Name:\tfixture\nUid:\t{} {} {} {}\n",
                        ids[0], ids[1], ids[2], ids[3]
                    )
                    .as_bytes()
                )
                .is_err()
            );
        }
        quiet_uids(b"Name:\troot operator\nUid:\t0 0 0 0\n").unwrap();
        for bad in [
            b"Uid: 0 0 0\n".as_slice(),
            b"Uid: 0 0 x 0\n",
            b"Uid: 0 0 0 0\nUid: 0 0 0 0\n",
            b"Name: unknown\n",
        ] {
            assert!(quiet_uids(bad).is_err());
        }
    }

    #[test]
    fn only_read_only_block_descriptors_are_acceptable() {
        assert!(!writable_flags(b"pos: 0\nflags: 02100000\n").unwrap());
        for text in [b"flags: 02100001\n".as_slice(), b"flags: 02100002\n"] {
            assert!(writable_flags(text).unwrap());
        }
        for text in [
            b"flags: 8\n".as_slice(),
            b"flags: 0\nflags: 0\n",
            b"pos: 0\n",
        ] {
            assert!(writable_flags(text).is_err());
        }
    }

    #[test]
    fn process_walk_detects_runtime_threads_and_privileged_legacy_helpers() {
        let temp = tempfile::tempdir().unwrap();
        let process = temp.path().join("123");
        fs::create_dir_all(process.join("task/123")).unwrap();
        fs::create_dir_all(process.join("task/456")).unwrap();
        fs::create_dir(process.join("fd")).unwrap();
        fs::create_dir(process.join("fdinfo")).unwrap();
        for thread in ["123", "456"] {
            fs::write(
                process.join("task").join(thread).join("status"),
                b"Uid: 0 0 0 0\n",
            )
            .unwrap();
        }
        fs::write(process.join("cmdline"), b"sshd\0root@pts/1\0").unwrap();
        let proc = File::open(temp.path()).unwrap();
        inspect(&proc).unwrap();
        for helper in HELPERS {
            fs::write(
                process.join("cmdline"),
                [b"/bin/bash\0/usr/sbin/".as_slice(), helper, b"\0"].concat(),
            )
            .unwrap();
            assert!(inspect(&proc).is_err());
        }
        fs::write(process.join("cmdline"), b"sshd\0root@pts/1\0").unwrap();
        fs::write(process.join("task/456/status"), b"Uid: 0 0 1000 0\n").unwrap();
        assert!(inspect(&proc).is_err());
        fs::remove_dir_all(process.join("task/456")).unwrap();
        inspect(&proc).unwrap();
    }
}
