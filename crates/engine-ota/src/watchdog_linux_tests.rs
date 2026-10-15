use super::*;
use crate::recovery::{RecoveryRecord, Slot};
use std::cell::RefCell;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::rc::Rc;

type Events = Rc<RefCell<Vec<&'static str>>>;

struct FakeDevice {
    events: Events,
    fault: Option<&'static str>,
}
impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.events.borrow_mut().push("close");
    }
}
impl WatchdogDevice for FakeDevice {
    fn support(&mut self) -> Result<WatchdogInfo> {
        self.events.borrow_mut().push("support");
        if self.fault == Some("support") {
            return Err(std::io::Error::other("support error").into());
        }
        Ok(WatchdogInfo {
            options: if self.fault == Some("options") {
                0
            } else {
                REQUIRED_OPTIONS
            },
            firmware_version: 0,
            identity: if self.fault == Some("driver") {
                [0; 32]
            } else {
                DRIVER_IDENTITY
            },
        })
    }
    fn timeout(&mut self) -> Result<i32> {
        self.events.borrow_mut().push("timeout");
        if self.fault == Some("timeout-error") {
            return Err(std::io::Error::other("timeout error").into());
        }
        Ok(if self.fault == Some("timeout") {
            15
        } else {
            210
        })
    }
    fn disable(&mut self) -> Result<()> {
        self.events.borrow_mut().push("disable");
        if self.fault == Some("disable") {
            return Err(std::io::Error::from_raw_os_error(libc::EPERM).into());
        }
        Ok(())
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    runtime: RuntimeDirectory,
    store: RecoveryStore,
    boot: BootObservation,
}
impl Fixture {
    fn new(committed: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        // SAFETY: geteuid has no preconditions or side effects.
        let owner = unsafe { libc::geteuid() };
        let runtime = RuntimeDirectory::from_directory(
            open_directory(directory.path(), owner).unwrap(),
            owner,
        )
        .unwrap();
        let (record, mut boot) = crate::recovery::tests::promoting();
        boot.default_slot = Some(Slot::B);
        let record = if committed {
            record.finish_promotion(&boot).unwrap().record
        } else {
            record
        };
        let store = RecoveryStore::initialize(&directory.path().join("journal"), record).unwrap();
        let fixture = Self {
            directory,
            runtime,
            store,
            boot,
        };
        fixture.write_early(&fixture.early());
        fixture
    }

    fn early(&self) -> EarlyWatchdog {
        EarlyWatchdog {
            schema: 1,
            boot_id: self.boot.boot_id.clone(),
            trial_id: self.boot.trial_id.unwrap(),
            armed_boottime_ms: 2000,
            timeout_seconds: 210,
        }
    }
    fn write_early(&self, record: &EarlyWatchdog) {
        let path = self.directory.path().join("early-watchdog.json");
        fs::write(&path, serde_json::to_vec(record).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn finish(&self, events: &Events, fault: Option<&'static str>) -> Result<WatchdogCompletion> {
        finish_with(&self.store, &self.boot, &self.runtime, || {
            events.borrow_mut().push("open");
            Ok(FakeDevice {
                events: events.clone(),
                fault,
            })
        })
    }
}

#[test]
fn only_durable_success_can_open_and_disarm_and_repeat_calls_do_not_reopen() {
    let mut fixture = Fixture::new(true);
    let events = Events::default();
    assert!(check_completed(&fixture.store, &fixture.boot, &fixture.runtime).is_err());
    assert!(events.borrow().is_empty());
    assert_eq!(
        fixture.finish(&events, None).unwrap(),
        WatchdogCompletion::Stopped
    );
    assert_eq!(
        *events.borrow(),
        ["open", "support", "timeout", "disable", "close"]
    );
    events.borrow_mut().clear();
    check_completed(&fixture.store, &fixture.boot, &fixture.runtime).unwrap();
    // After the early allowance has long elapsed, a retained completion in
    // this committed boot still prevents accidentally rearming the device.
    fixture.boot.uptime_ms += 86_400_000;
    assert_eq!(
        fixture.finish(&events, None).unwrap(),
        WatchdogCompletion::AlreadyStopped
    );
    assert!(events.borrow().is_empty());

    let pending = Fixture::new(false);
    assert!(check_completed(&pending.store, &pending.boot, &pending.runtime).is_err());
    assert!(pending.finish(&events, None).is_err());
    assert!(events.borrow().is_empty());
}

#[test]
fn invalid_early_or_completion_evidence_never_opens_the_device() {
    let events = Events::default();
    for fault in 0..7 {
        let fixture = Fixture::new(true);
        let mut early = fixture.early();
        match fault {
            0 => early.schema = 2,
            1 => early.boot_id = "11111111-1111-1111-1111-111111111111".into(),
            2 => early.trial_id = Sha256::digest(b"other trial"),
            3 => early.armed_boottime_ms = fixture.boot.uptime_ms + 1,
            4 => early.timeout_seconds = 211,
            5 => early.armed_boottime_ms = u64::MAX,
            _ => early.boot_id.clear(),
        }
        fixture.write_early(&early);
        assert!(fixture.finish(&events, None).is_err());
        assert!(events.borrow().is_empty());
    }
    let mut expired = Fixture::new(true);
    expired.boot.uptime_ms = 212_000;
    assert!(expired.finish(&events, None).is_err());
    assert!(events.borrow().is_empty());
    let fixture = Fixture::new(true);
    let completed = fixture.directory.path().join("completed-watchdog.json");
    fs::write(&completed, b"corrupt").unwrap();
    fs::set_permissions(&completed, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(fixture.finish(&events, None).is_err());
    assert!(events.borrow().is_empty());
}

#[test]
fn driver_timeout_and_disable_failures_never_record_success() {
    for fault in [
        "support",
        "options",
        "driver",
        "timeout-error",
        "timeout",
        "disable",
    ] {
        let fixture = Fixture::new(true);
        let events = Events::default();
        assert!(fixture.finish(&events, Some(fault)).is_err(), "{fault}");
        assert_eq!(events.borrow().first(), Some(&"open"));
        assert_eq!(events.borrow().last(), Some(&"close"));
        assert!(
            !fixture
                .directory
                .path()
                .join("completed-watchdog.json")
                .exists()
        );
        let record: &RecoveryRecord = fixture.store.record();
        assert_eq!(record.good().slot, Slot::B);
    }
}

#[test]
fn early_record_must_be_private_bounded_and_not_a_link() {
    let fixture = Fixture::new(true);
    let events = Events::default();
    let record = fixture.directory.path().join("early-watchdog.json");
    fs::set_permissions(&record, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(fixture.finish(&events, None).is_err());
    fs::set_permissions(&record, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&record, vec![b'x'; 1025]).unwrap();
    assert!(fixture.finish(&events, None).is_err());
    fs::remove_file(&record).unwrap();
    symlink(fixture.directory.path().join("absent"), &record).unwrap();
    assert!(fixture.finish(&events, None).is_err());
    assert!(events.borrow().is_empty());
}
