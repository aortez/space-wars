//! Descriptor-relative access beneath directories owned by the service.
use crate::{Error, Result};
use std::ffi::CString;
use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(crate) fn open_directory(path: &Path, owner: u32) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    owned(&file.metadata()?, true, owner, false)?;
    Ok(file)
}

pub(crate) fn name(name: &str) -> Result<CString> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(Error::Store("invalid descriptor-relative filename".into()));
    }
    CString::new(name).map_err(|_| Error::Store("NUL in filename".into()))
}

pub(crate) fn open_at(parent: &File, component: &str, flags: i32, mode: u32) -> Result<File> {
    let component = name(component)?;
    // SAFETY: the directory descriptor and C string remain live. openat creates
    // a new owned descriptor; O_NOFOLLOW and O_NONBLOCK prevent link/device waits.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            component.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode as libc::mode_t,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: openat returned a new, uniquely owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

pub(crate) fn owned(metadata: &Metadata, directory: bool, owner: u32, private: bool) -> Result<()> {
    if metadata.uid() != owner
        || metadata.mode() & if private { 0o077 } else { 0o022 } != 0
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file() || metadata.nlink() != 1
        }
    {
        return Err(Error::Store(
            "unsafe file ownership, type, links or permissions".into(),
        ));
    }
    Ok(())
}

pub(crate) fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.uid() == after.uid()
        && before.gid() == after.gid()
        && before.mode() == after.mode()
        && before.nlink() == after.nlink()
        && before.len() == after.len()
        && before.mtime() == after.mtime()
        && before.mtime_nsec() == after.mtime_nsec()
        && before.ctime() == after.ctime()
        && before.ctime_nsec() == after.ctime_nsec()
}

pub(crate) fn read_file(mut file: File, owner: u32, limit: u64) -> Result<Vec<u8>> {
    let before = file.metadata()?;
    owned(&before, false, owner, false)?;
    if before.len() > limit {
        return Err(Error::Store("file exceeds its size bound".into()));
    }
    let mut bytes = Vec::new();
    (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
    if !unchanged(&before, &file.metadata()?) || before.len() != bytes.len() as u64 {
        return Err(Error::Store("file changed while being read".into()));
    }
    Ok(bytes)
}

pub(crate) fn io_result(result: i32) -> Result<()> {
    if result < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}
