use super::*;
use crate::boot_files::tests::Fixture as BootFixture;
use std::os::unix::fs::{PermissionsExt, symlink};

const ORIGIN: &str = "11111111-1111-1111-1111-111111111111";
const RETURNED: &str = "22222222-2222-2222-2222-222222222222";

struct Fixture {
    boot: BootFixture,
    history: tempfile::TempDir,
    plan: Plan,
}

impl Fixture {
    fn new() -> Self {
        let boot = BootFixture::new();
        let files = boot.open().unwrap();
        let mut image =
            ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json"))
                .unwrap();
        image.boot_identity = boot.identity;
        let id = Sha256::digest(b"one missing-root diagnostic");
        let plan = Plan {
            schema: 1,
            case: "missing-root".into(),
            id,
            origin_boot_id: ORIGIN.into(),
            stamp: Stamp {
                device: Sha256::digest(b"board and medium"),
                layout: Sha256::digest(b"layout"),
                image,
                kernel: "a".repeat(40),
                profile: "hdmi".into(),
                slot: Slot::A,
                root_a: "076c4a2a-02".into(),
                root_b: "076c4a2a-03".into(),
            },
            default_selector: boot_text(&files, "cmdline.txt", 8192).unwrap(),
            original_selector: UNPREPARED_TRIAL.into(),
            fault_selector: fault_selector(&files, id).unwrap(),
            hardware_selector: boot_text(&files, "spacewars-device.txt", 4096).unwrap(),
            hostname: None,
            recovery_sha256: Sha256::digest(b"untouched production recovery"),
            completed_bootstrap_sha256: Sha256::digest(b"untouched completed bootstrap"),
        };
        let history = tempfile::tempdir().unwrap();
        fs::set_permissions(history.path(), fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            boot,
            history,
            plan,
        }
    }

    fn parent(&self) -> File {
        open_directory(self.history.path(), self.boot.owner).unwrap()
    }

    fn store(&self) -> Store {
        Store::create(self.parent(), self.boot.owner, self.plan.clone()).unwrap()
    }

    fn reopen(&self) -> Result<Store> {
        Store::open(self.parent(), self.boot.owner, self.plan.id)
    }

    fn selector(&self) -> String {
        fs::read_to_string(self.boot.directory.path().join(SELECTOR)).unwrap()
    }

    fn guard(&self, files: &BootFiles) -> Result<()> {
        files.verify()?;
        self.plan.check(files)?;
        if boot_text(files, "cmdline.txt", 8192)? != self.plan.default_selector {
            return Err(invalid("normal default changed"));
        }
        Ok(())
    }
}

#[test]
fn missing_root_requires_exact_single_disk_partition_inventory() {
    let expected: Vec<_> = (1..=4)
        .map(|n| (format!("sda{n}"), format!("076c4a2a-0{n}")))
        .collect();
    let mut found = expected
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<BTreeSet<_>>();
    check_missing_partitions(&expected, &found).unwrap();
    found.insert("sdb1".into());
    assert!(check_missing_partitions(&expected, &found).is_err());
    found.remove("sdb1");
    found.remove("sda4");
    assert!(check_missing_partitions(&expected, &found).is_err());
    found.insert("sda4".into());
    let mut existing = expected.clone();
    existing[2].1 = MISSING_ROOT.into();
    assert!(check_missing_partitions(&existing, &found).is_err());
}

#[test]
fn plan_is_bound_to_original_default_and_fixed_fault_not_arbitrary_boot_arguments() {
    let fixture = Fixture::new();
    let files = fixture.boot.open().unwrap();
    fixture.plan.check(&files).unwrap();
    for field in [
        "case",
        "origin_boot_id",
        "default_selector",
        "original_selector",
        "fault_selector",
    ] {
        let mut value = serde_json::to_value(&fixture.plan).unwrap();
        value[field] = json!("unexpected");
        let altered: Plan = serde_json::from_value(value).unwrap();
        assert!(altered.check(&files).is_err(), "{field}");
        assert_ne!(
            altered.fingerprint().unwrap(),
            fixture.plan.fingerprint().unwrap()
        );
    }
    let mut json = serde_json::to_value(&fixture.plan).unwrap();
    json["arbitrary_command"] = json!("false");
    assert!(serde_json::from_value::<Plan>(json).is_err());
}

#[test]
fn selector_interruption_at_every_checkpoint_restores_original_without_changing_other_boot_files() {
    for stop in [
        "selector-opened",
        "selector-written",
        "selector-synced",
        "selector-renamed",
        "selector-directory-synced",
        "selector-filesystem-synced",
        "success",
    ] {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        let files = fixture.boot.open().unwrap();
        let before = fs::read(fixture.boot.directory.path().join("cmdline.txt")).unwrap();
        let result = publish_selector(
            &files,
            &store,
            false,
            || fixture.guard(&files),
            |point| {
                if point == stop {
                    Err(invalid("injected interruption"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.is_ok(), stop == "success");
        drop(store);
        store = fixture.reopen().unwrap();
        assert!(!store.state.request_consumed);
        store.begin_restore(ORIGIN).unwrap();
        let files = fixture.boot.open().unwrap();
        publish_selector(&files, &store, true, || fixture.guard(&files), |_| Ok(())).unwrap();
        assert_eq!(fixture.selector(), fixture.plan.original_selector);
        assert_eq!(
            fs::read(fixture.boot.directory.path().join("cmdline.txt")).unwrap(),
            before
        );
        fixture.boot.open().unwrap().verify().unwrap();
        assert!(
            !fixture
                .boot
                .directory
                .path()
                .join(format!(
                    ".spacewars-boot-{}.tmp",
                    &fixture.plan.id.to_string()[..16]
                ))
                .exists()
        );
    }
}

#[test]
fn interrupted_restoration_can_resume_on_another_normal_boot() {
    for stop in [
        "selector-opened",
        "selector-written",
        "selector-synced",
        "selector-renamed",
        "selector-directory-synced",
        "selector-filesystem-synced",
    ] {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        let files = fixture.boot.open().unwrap();
        publish_selector(&files, &store, false, || fixture.guard(&files), |_| Ok(())).unwrap();
        store.state.phase = Phase::Prepared;
        store.persist().unwrap();
        store.consume().unwrap();
        assert!(store.begin_restore(ORIGIN).is_err());
        store.begin_restore(RETURNED).unwrap();
        assert!(
            publish_selector(
                &files,
                &store,
                true,
                || fixture.guard(&files),
                |point| {
                    if point == stop {
                        Err(invalid("interrupted restore"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        drop(store);
        let mut resumed = fixture.reopen().unwrap();
        resumed
            .begin_restore("33333333-3333-3333-3333-333333333333")
            .unwrap();
        let files = fixture.boot.open().unwrap();
        publish_selector(&files, &resumed, true, || fixture.guard(&files), |_| Ok(())).unwrap();
        assert_eq!(fixture.selector(), UNPREPARED_TRIAL);
        assert!(resumed.consume().is_err());
    }
}

#[test]
fn changed_default_unknown_trial_and_unrelated_temporary_stop_selector_writes() {
    for damage in ["default", "trial", "temporary", "immutable"] {
        let fixture = Fixture::new();
        let store = fixture.store();
        let files = fixture.boot.open().unwrap();
        let path = match damage {
            "default" => "cmdline.txt".into(),
            "trial" => SELECTOR.into(),
            "temporary" => format!(".spacewars-boot-{}.tmp", &fixture.plan.id.to_string()[..16]),
            _ => "kernel8.img".into(),
        };
        fs::write(
            fixture.boot.directory.path().join(&path),
            b"unrelated bytes",
        )
        .unwrap();
        fs::set_permissions(
            fixture.boot.directory.path().join(&path),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let before = fixture.selector();
        assert!(
            publish_selector(&files, &store, false, || fixture.guard(&files), |_| Ok(())).is_err()
        );
        assert_eq!(fixture.selector(), before);
        assert_eq!(
            fs::read(fixture.boot.directory.path().join(path)).unwrap(),
            b"unrelated bytes"
        );
    }
}

#[test]
fn changed_default_during_copy_is_detected_before_rename() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let files = fixture.boot.open().unwrap();
    assert!(
        publish_selector(
            &files,
            &store,
            false,
            || fixture.guard(&files),
            |point| {
                if point == "selector-written" {
                    fs::write(
                        fixture.boot.directory.path().join("cmdline.txt"),
                        b"another operator's default\n",
                    )?;
                }
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(fixture.selector(), UNPREPARED_TRIAL);
}

#[derive(Default)]
struct Bus {
    value: String,
    writes: Vec<String>,
    requests: usize,
    fail_set: bool,
    fail_request: bool,
    foreign_after_set: bool,
}

impl RebootBus for Bus {
    fn parameter(&mut self) -> Result<String> {
        Ok(self.value.clone())
    }
    fn set_parameter(&mut self, value: &str) -> Result<()> {
        self.writes.push(value.into());
        self.value = if self.foreign_after_set {
            "other operator".into()
        } else {
            value.into()
        };
        if self.fail_set {
            Err(invalid("lost SetRebootParameter reply"))
        } else {
            Ok(())
        }
    }
    fn request(&mut self) -> Result<()> {
        self.requests += 1;
        if self.fail_request {
            Err(invalid("lost accepted reboot reply"))
        } else {
            Ok(())
        }
    }
}

fn prepared(fixture: &Fixture) -> Store {
    let mut store = fixture.store();
    store.state.phase = Phase::Prepared;
    store.persist().unwrap();
    store
}

#[test]
fn activation_consumes_before_bus_mutation_and_cannot_repeat_after_success_or_lost_reply() {
    for failure in ["none", "set", "request", "foreign"] {
        let fixture = Fixture::new();
        let mut store = prepared(&fixture);
        let mut bus = Bus {
            fail_set: failure == "set",
            fail_request: failure == "request",
            foreign_after_set: failure == "foreign",
            ..Bus::default()
        };
        let result = request_once(&mut store, &mut bus, |s| {
            let reopened = fixture.reopen()?;
            assert_eq!(reopened.state.phase, s.state.phase);
            assert_eq!(reopened.state.request_consumed, s.state.request_consumed);
            Ok(())
        });
        assert_eq!(result.is_ok(), failure == "none");
        let mut reopened = fixture.reopen().unwrap();
        assert!(reopened.state.request_consumed);
        assert_eq!(reopened.state.phase, Phase::Requested);
        let counts = (bus.writes.len(), bus.requests);
        assert!(request_once(&mut reopened, &mut bus, |_| Ok(())).is_err());
        assert_eq!((bus.writes.len(), bus.requests), counts);
        if failure == "foreign" {
            assert_eq!(bus.value, "other operator");
        }
        if matches!(failure, "set" | "request") {
            assert!(bus.value.is_empty());
        }
        assert!(reopened.begin_restore(ORIGIN).is_err());
        reopened.begin_restore(RETURNED).unwrap();
        assert!(reopened.consume().is_err());
    }
}

#[test]
fn existing_reboot_parameter_or_failed_admission_has_no_side_effects() {
    for foreign in [true, false] {
        let fixture = Fixture::new();
        let mut store = prepared(&fixture);
        let mut bus = Bus {
            value: if foreign {
                "someone else's reboot".into()
            } else {
                String::new()
            },
            ..Bus::default()
        };
        let before = store.bytes.clone();
        assert!(request_once(&mut store, &mut bus, |_| Err(invalid("changed device"))).is_err());
        assert_eq!(store.bytes, before);
        assert!(bus.writes.is_empty());
        assert_eq!(bus.requests, 0);
    }
}

#[test]
fn changed_record_directory_links_and_permissions_are_rejected() {
    for damage in ["checksum", "symlink", "hardlink", "mode", "directory"] {
        let fixture = Fixture::new();
        let store = fixture.store();
        let dir = fixture.history.path().join(fixture.plan.id.to_string());
        let path = dir.join("record.json");
        match damage {
            "checksum" => {
                let mut value: Value = serde_json::from_slice(&store.bytes).unwrap();
                value["state"]["plan"]["origin_boot_id"] = json!(RETURNED);
                fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "symlink" => {
                fs::rename(&path, dir.join("old")).unwrap();
                symlink("old", &path).unwrap();
            }
            "hardlink" => {
                fs::hard_link(&path, dir.join("old")).unwrap();
            }
            "mode" => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
            }
            _ => {
                fs::rename(&dir, fixture.history.path().join("replaced")).unwrap();
            }
        }
        assert!(fixture.reopen().is_err(), "{damage}");
        assert!(store.check().is_err(), "{damage}");
    }
}

#[test]
fn incomplete_attempts_and_reused_ids_block_new_diagnostics_but_completed_records_are_retained() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let mut next = fixture.plan.clone();
    next.id = Sha256::digest(b"explicitly selected next diagnostic");
    assert!(Store::create(fixture.parent(), fixture.boot.owner, next.clone()).is_err());
    store.begin_restore(ORIGIN).unwrap();
    store.state.phase = Phase::Restored;
    store.persist().unwrap();
    let old = store.bytes.clone();
    assert!(Store::create(fixture.parent(), fixture.boot.owner, fixture.plan.clone()).is_err());
    Store::create(fixture.parent(), fixture.boot.owner, next).unwrap();
    assert_eq!(fixture.reopen().unwrap().bytes, old);
}
