//! Bounded child processes for fixed tools reading an already-open file.
//! Callers choose fixed executables/arguments; never an external hook or shell.
use crate::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const MAX_DIAGNOSTIC: usize = 64 * 1024;

#[derive(Default)]
pub(crate) struct FileCommand {
    child: Option<Child>,
}

impl FileCommand {
    pub(crate) fn run(
        &mut self,
        mut command: Command,
        file: &File,
        timeout: Duration,
        limit: usize,
    ) -> Result<Vec<u8>> {
        if self.child.is_some() {
            self.finish()?;
            if self.child.is_some() {
                return Err(invalid("previous file checker has not exited"));
            }
        }
        let fd = file.as_raw_fd();
        command
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        // SAFETY: only the async-signal-safe fcntl syscall runs between fork
        // and exec. The parent retains the live target and exclusive claim.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let deadline = Instant::now() + timeout;
        let mut child = command.spawn()?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| invalid("missing checker stdout"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| invalid("missing checker stderr"))?;
        self.child = Some(child);
        let result = (|| {
            nonblocking(stdout.as_raw_fd())?;
            nonblocking(stderr.as_raw_fd())?;
            let (mut output, mut diagnostics) = (Vec::new(), Vec::new());
            loop {
                drain(&mut stdout, &mut output, limit)?;
                drain(&mut stderr, &mut diagnostics, MAX_DIAGNOSTIC)?;
                if let Some(status) = self.finish()? {
                    drain(&mut stdout, &mut output, limit)?;
                    drain(&mut stderr, &mut diagnostics, MAX_DIAGNOSTIC)?;
                    if !status.success() {
                        // Diagnostics may contain input bytes (e.g. key comments).
                        // Bound and drain them, but do not expose them to logs.
                        return Err(invalid(&format!("file checker rejected input ({status})")));
                    }
                    return Ok(output);
                }
                if Instant::now() >= deadline {
                    return Err(invalid("file checker timed out"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if result.is_err() {
            self.kill()?;
            let _ = self.finish();
        }
        result
    }

    fn kill(&self) -> Result<()> {
        if let Some(child) = &self.child {
            // This PID is unreaped and reserved until its whole group is killed.
            if unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) } < 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<Option<ExitStatus>> {
        let Some(child) = &self.child else {
            return Ok(None);
        };
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: the initialized writable siginfo is valid, and WNOWAIT keeps
        // the child PID reserved until group cleanup has completed.
        if unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        self.kill()?;
        let status = self.child.as_mut().expect("unreaped checker").try_wait()?;
        if status.is_some() {
            self.child = None;
        }
        Ok(status)
    }
}

impl Drop for FileCommand {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.finish();
    }
}

fn nonblocking(fd: i32) -> Result<()> {
    // SAFETY: fd is a live pipe and the flags are checked before changing them.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> Result<()> {
    let mut buffer = [0u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                if count > limit.saturating_sub(bytes.len()) {
                    return Err(invalid("file checker output exceeded its bound"));
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn invalid(message: &str) -> Error {
    Error::Install(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checker_deadline_output_exit_status_and_inherited_file_are_enforced() {
        let file = tempfile::tempfile().unwrap();
        let mut tools = FileCommand::default();
        for script in [
            "exit 1",
            "exit 2",
            "exit 4",
            "head -c 65 /dev/zero",
            "sleep 30",
        ] {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            let start = Instant::now();
            assert!(
                tools
                    .run(command, &file, Duration::from_millis(100), 64)
                    .is_err()
            );
            assert!(start.elapsed() < Duration::from_secs(2));
            for _ in 0..100 {
                tools.finish().unwrap();
                if tools.child.is_none() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(tools.child.is_none());
        }
        let mut command = Command::new("/usr/bin/stat");
        command.args(["-Lc", "%s", &format!("/proc/self/fd/{}", file.as_raw_fd())]);
        assert_eq!(
            tools
                .run(command, &file, Duration::from_secs(1), 64)
                .unwrap(),
            b"0\n"
        );
    }
}
