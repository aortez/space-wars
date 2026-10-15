//! One lock shared by recovery and authenticated workstation maintenance.
use crate::owned_fs::{io_result, name, open_at, open_directory, owned};
use crate::{Error, Result};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub const INSTALL_LOCK_PATH: &str = "/run/spacewars-install/lock";

/// Keep this handle alive through admission, writes, selector changes and the
/// final handoff. The directory is not a service RuntimeDirectory: removing it
/// on restart could create a second lock inode while an operator holds the first.
pub struct InstallLock {
    run: File,
    directory: File,
    lock: File,
    owner: u32,
}

impl InstallLock {
    pub fn acquire() -> Result<Self> {
        // SAFETY: geteuid has no preconditions or side effects.
        if unsafe { libc::geteuid() } != 0 {
            return Err(Error::Store(
                "installation requires the root service or operator".into(),
            ));
        }
        let root = open_directory(Path::new("/"), 0)?;
        let run = open_at(&root, "run", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        owned(&run.metadata()?, true, 0, false)?;
        Self::under(&run, 0)
    }

    fn under(run: &File, owner: u32) -> Result<Self> {
        let component = name("spacewars-install")?;
        // SAFETY: run and component remain live, with a fixed private mode.
        let created = unsafe { libc::mkdirat(run.as_raw_fd(), component.as_ptr(), 0o700) };
        if created < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            io_result(created)?;
        }
        let directory = open_at(
            run,
            "spacewars-install",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        owned(&directory.metadata()?, true, owner, true)?;
        let lock = open_at(&directory, "lock", libc::O_RDWR | libc::O_CREAT, 0o600)?;
        owned(&lock.metadata()?, false, owner, true)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::Busy,
            std::fs::TryLockError::Error(error) => Error::Io(error),
        })?;
        let held = Self {
            run: run.try_clone()?,
            directory,
            lock,
            owner,
        };
        held.check()?;
        Ok(held)
    }

    /// Recheck the lock name before each privileged mutation. Cooperative
    /// callers must never unlink or replace the lock file to clear a busy lock.
    pub(crate) fn check(&self) -> Result<()> {
        let named_directory = open_at(
            &self.run,
            "spacewars-install",
            libc::O_RDONLY | libc::O_DIRECTORY,
            0,
        )?;
        let held_directory = self.directory.metadata()?;
        let named_directory = named_directory.metadata()?;
        if held_directory.dev() != named_directory.dev()
            || held_directory.ino() != named_directory.ino()
        {
            return Err(Error::Store(
                "installation lock directory was replaced".into(),
            ));
        }
        owned(&self.directory.metadata()?, true, self.owner, true)?;
        let current = open_at(&self.directory, "lock", libc::O_RDONLY, 0)?;
        let held = self.lock.metadata()?;
        let current = current.metadata()?;
        owned(&held, false, self.owner, true)?;
        owned(&current, false, self.owner, true)?;
        if held.dev() != current.dev() || held.ino() != current.ino() {
            return Err(Error::Store("installation lock was replaced".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn setup() -> (tempfile::TempDir, File, u32) {
        let path = tempfile::tempdir().unwrap();
        fs::set_permissions(path.path(), fs::Permissions::from_mode(0o700)).unwrap();
        // SAFETY: geteuid has no preconditions or side effects.
        let owner = unsafe { libc::geteuid() };
        let directory = open_directory(path.path(), owner).unwrap();
        (path, directory, owner)
    }

    #[test]
    fn independently_opened_installers_and_flock_tools_share_one_lock() {
        let (path, run, owner) = setup();
        let held = InstallLock::under(&run, owner).unwrap();
        assert!(matches!(InstallLock::under(&run, owner), Err(Error::Busy)));
        let independent = File::open(path.path().join("spacewars-install/lock")).unwrap();
        // SAFETY: independent remains open. This is the same flock used by
        // operator shell tools, with no blocking wait.
        assert_eq!(
            unsafe { libc::flock(independent.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EWOULDBLOCK)
        );
        drop(held);
        let next = InstallLock::under(&run, owner).unwrap();
        next.check().unwrap();
    }

    #[test]
    fn replacing_or_unlinking_the_held_lock_does_not_silently_allow_writes() {
        let (path, run, owner) = setup();
        let held = InstallLock::under(&run, owner).unwrap();
        let lock = path.path().join("spacewars-install/lock");
        fs::rename(&lock, lock.with_file_name("old-lock")).unwrap();
        let replacement = InstallLock::under(&run, owner).unwrap();
        assert!(held.check().is_err());
        replacement.check().unwrap();
        fs::rename(
            path.path().join("spacewars-install"),
            path.path().join("old-directory"),
        )
        .unwrap();
        let next = InstallLock::under(&run, owner).unwrap();
        assert!(replacement.check().is_err());
        next.check().unwrap();
    }

    #[test]
    fn symlinks_hardlinks_wrong_owner_and_writable_private_state_are_rejected() {
        let (path, run, owner) = setup();
        let directory = path.path().join("spacewars-install");
        symlink(path.path(), &directory).unwrap();
        assert!(InstallLock::under(&run, owner).is_err());
        fs::remove_file(&directory).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(InstallLock::under(&run, owner).is_err());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let lock = directory.join("lock");
        fs::write(path.path().join("outside"), b"").unwrap();
        symlink(path.path().join("outside"), &lock).unwrap();
        assert!(InstallLock::under(&run, owner).is_err());
        fs::remove_file(&lock).unwrap();
        fs::hard_link(path.path().join("outside"), &lock).unwrap();
        assert!(InstallLock::under(&run, owner).is_err());
        fs::remove_file(&lock).unwrap();
        fs::write(&lock, b"").unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(InstallLock::under(&run, owner).is_err());
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(InstallLock::under(&run, owner.wrapping_add(1)).is_err());
        InstallLock::under(&run, owner).unwrap();
    }
}
