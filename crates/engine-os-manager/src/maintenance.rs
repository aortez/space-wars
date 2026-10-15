//! Root/operator maintenance, outside the kiosk's signed-update authority.
//! Helpers have fixed paths and inherit the same lock as recovery and OTA.
use crate::{Error, Result};
use engine_ota::{RecoveryStore, boot_linux::BootController, install_lock::InstallLock};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

const RECOVERY: &str = "/data/spacewars-system/recovery";

pub struct Request {
    helper: &'static str,
    arguments: Vec<String>,
    read_only: bool,
}

impl Request {
    pub fn fast(arguments: Vec<String>) -> Result<Self> {
        let check = arguments == ["--check"];
        if !check
            && (arguments.len() != 4
                || !stage(&arguments[0])
                || !arguments[1..].iter().all(|s| hash(s)))
        {
            return Err(invalid(
                "expected --check, or a fixed staging name and three SHA-256 hashes",
            ));
        }
        Ok(Self {
            helper: "/usr/libexec/spacewars/fast-update",
            arguments,
            read_only: check,
        })
    }

    pub fn renderer(renderer: String) -> Result<Self> {
        if !matches!(renderer.as_str(), "software" | "gpu") {
            return Err(invalid("renderer must be software or gpu"));
        }
        Ok(Self {
            helper: "/usr/libexec/spacewars/renderer",
            arguments: vec![renderer],
            read_only: false,
        })
    }
}

pub fn run(request: Request) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(invalid(
            "maintenance requires separately authenticated root operator access",
        ));
    }
    let mut command = Command::new(request.helper);
    command
        .args(&request.arguments)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C");
    if request.read_only {
        use std::os::unix::process::CommandExt;
        return Err(command.exec().into());
    }
    let lock = InstallLock::acquire()?;
    authorize(&lock)?;
    // exec keeps the lock in the worker, including through kiosk stop/start.
    lock.exec_maintenance(command)?;
    Ok(())
}

pub fn import_root_keys() -> Result<engine_ota::access_linux::AccessStatus> {
    let lock = InstallLock::acquire()?;
    authorize(&lock)?;
    Ok(engine_ota::access_linux::import_root_key(&lock)?)
}

fn authorize(lock: &InstallLock) -> Result<()> {
    // New, unprovisioned development images may use the operator tools. Once
    // recovery exists, missing/corrupt state must never masquerade as that case.
    let mut provisioned = false;
    match fs::symlink_metadata("/data/spacewars-system") {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(parent) => {
            if !parent.is_dir() || parent.uid() != 0 || parent.mode() & 0o077 != 0 {
                return Err(invalid("system data must be private and root-owned"));
            }
            match fs::symlink_metadata(RECOVERY) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => provisioned = true,
            }
        }
    }
    if provisioned {
        let journal = RecoveryStore::open(Path::new(RECOVERY))?;
        BootController::open(lock)?.authorize_maintenance(&journal)?;
    }
    Ok(())
}

fn stage(value: &str) -> bool {
    value
        .strip_prefix("/tmp/spacewars-fast.")
        .is_some_and(|suffix| {
            suffix.len() == 10 && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_fixed_operations_and_canonical_arguments_are_admitted() {
        assert!(Request::fast(vec!["--check".into()]).unwrap().read_only);
        let good = vec![
            "/tmp/spacewars-fast.A123456789".into(),
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
        ];
        assert!(!Request::fast(good.clone()).unwrap().read_only);
        for bad in [
            "/tmp/spacewars-fast.A123456789/..",
            "/dev/sda2",
            "/tmp/spacewars-fast.1",
            "--check",
            "/tmp/spacewars-fast.123456789;",
        ] {
            let mut args = good.clone();
            args[0] = bad.into();
            assert!(Request::fast(args).is_err());
        }
        let mut args = good;
        args[1] = "A".repeat(64);
        assert!(Request::fast(args).is_err());
        assert!(Request::fast(vec![]).is_err());
        assert!(Request::fast(vec!["--check".into(), "extra".into()]).is_err());
        assert!(Request::renderer("gpu".into()).is_ok());
        assert!(Request::renderer("software".into()).is_ok());
        assert!(Request::renderer("gpu;reboot".into()).is_err());
    }
}
