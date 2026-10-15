use super::*;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Instant;

/// At most one observation process, retained until reaped even after a timeout.
/// A blocked filesystem must not accumulate workers or stall the coordinator.
#[derive(Default)]
pub struct KioskObserver {
    child: Option<Child>,
}

impl KioskObserver {
    pub fn observe(&mut self, expected: &KioskExpectation) -> Result<KioskHealth> {
        expected.validate()?;
        let mut command = Command::new("/usr/bin/engine-os-manager");
        command.args(["kiosk-health-worker", &serde_json::to_string(expected)?]);
        let result = self.collect(command, DEADLINE)?;
        let health: std::result::Result<KioskHealth, String> = serde_json::from_slice(&result)?;
        let health = health.map_err(Error::Invalid)?;
        if health.build_id != expected.build_id || health.process_id <= 1 || health.start_ticks == 0
        {
            return invalid("invalid kiosk health worker result");
        }
        Ok(health)
    }

    fn reap(&mut self) -> Result<()> {
        if self.child.is_some() && self.finish()?.is_none() {
            return invalid("previous kiosk health worker has not exited");
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<Option<ExitStatus>> {
        let Some(child) = &self.child else {
            return Ok(None);
        };
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // Peek without reaping: the child's PID must remain reserved until the
        // entire group is killed. try_wait() followed by killpg() would race
        // PID reuse even when the child had already exited unsuccessfully.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        self.kill()?;
        let status = self.child.as_mut().expect("unreaped worker").try_wait()?;
        if status.is_some() {
            self.child = None;
        }
        Ok(status)
    }

    fn collect(&mut self, mut command: Command, timeout: Duration) -> Result<Vec<u8>> {
        self.reap()?;
        let deadline = Instant::now() + timeout;
        command
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("missing health worker pipe".into()))?;
        self.child = Some(child);
        let result = futures_lite::future::block_on(futures_lite::future::or(
            async {
                let reader = async_io::Async::new(stdout)?;
                let mut bytes = Vec::new();
                reader
                    .take(MAX_RESPONSE as u64 + 1)
                    .read_to_end(&mut bytes)
                    .await?;
                if bytes.len() > MAX_RESPONSE {
                    return invalid("oversized kiosk health worker output");
                }
                loop {
                    if let Some(status) = self.finish()? {
                        if !status.success() {
                            return invalid("kiosk health worker rejected the observation");
                        }
                        return Ok(bytes);
                    }
                    async_io::Timer::after(Duration::from_millis(5)).await;
                }
            },
            async {
                async_io::Timer::at(deadline).await;
                invalid("kiosk health observation timed out")
            },
        ));
        if result.is_err() {
            self.kill()?;
        }
        // Keep a killed worker if it is not reaped yet. Subsequent calls fail
        // closed until it exits, including uninterruptible kernel I/O.
        let _ = self.reap();
        result
    }

    fn kill(&mut self) -> Result<()> {
        if let Some(child) = &self.child {
            // The child established its own process group and remains unreaped,
            // so its PID cannot be reused. Include the unprivileged state helper.
            if unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }
}

impl Drop for KioskObserver {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.reap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_bounds_output_and_rejects_failure() {
        for script in ["exit 1", "head -c 4097 /dev/zero"] {
            let mut probe = KioskObserver::default();
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            assert!(probe.collect(command, Duration::from_secs(2)).is_err());
        }
        let mut probe = KioskObserver::default();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf 'fixture'"]);
        assert_eq!(
            probe.collect(command, Duration::from_secs(2)).unwrap(),
            b"fixture"
        );
    }

    #[test]
    fn worker_deadline_covers_slow_output_and_stuck_descendants() {
        let mut probe = KioskObserver::default();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf partial; sleep 30"]);
        let started = Instant::now();
        let error = probe
            .collect(command, Duration::from_millis(100))
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        for _ in 0..200 {
            if probe.reap().is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("killed health process did not exit");
    }

    #[test]
    fn existing_worker_prevents_a_second_probe_and_eof_does_not_bypass_deadline() {
        let mut probe = KioskObserver::default();
        probe.child = Some(
            Command::new("/bin/sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("must-not-run");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        assert!(
            probe
                .collect(command, Duration::from_secs(1))
                .unwrap_err()
                .to_string()
                .contains("previous")
        );
        assert!(!marker.exists());
        drop(probe);

        let mut probe = KioskObserver::default();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec 1>&-; sleep 30"]);
        assert!(
            probe
                .collect(command, Duration::from_millis(100))
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }

    #[test]
    fn exited_worker_group_is_cleaned_before_its_pid_can_be_reused() {
        let mut probe = KioskObserver::default();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 >/dev/null & printf '%s' $!"]);
        let bytes = probe.collect(command, Duration::from_secs(2)).unwrap();
        let pid: u32 = std::str::from_utf8(&bytes).unwrap().parse().unwrap();
        assert!(probe.child.is_none());
        for _ in 0..200 {
            match fs::read_to_string(format!("/proc/{pid}/stat")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Ok(stat)
                    if stat.rsplit_once(')').unwrap().1.split_whitespace().next() == Some("Z") =>
                {
                    return;
                }
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        panic!("health worker left its descendant running");
    }
}
