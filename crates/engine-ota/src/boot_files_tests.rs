use super::*;
use crate::boot::UNPREPARED_TRIAL;
use crate::owned_fs::open_directory;
use std::collections::BTreeMap;
use std::os::unix::fs::{PermissionsExt, symlink};

pub(crate) struct Fixture {
    pub directory: tempfile::TempDir,
    pub identity: Sha256,
    pub owner: u32,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        // SAFETY: geteuid has no preconditions or side effects.
        let owner = unsafe { libc::geteuid() };
        let names: BootManifest =
            serde_json::from_slice(include_bytes!("../tests/fixtures/boot-manifest-v2.json"))
                .unwrap();
        let mut blobs = BTreeMap::new();
        for path in names.files().keys() {
            let bytes = match path.as_str() {
                "spacewars-cmdline.txt" => {
                    b"root=@SPACEWARS_ROOT@ rootfstype=ext4 rootwait roottimeout=30 console=tty1\n"
                        .to_vec()
                }
                "kernel8.img" | "kernel_2712.img" => b"same common kernel".to_vec(),
                "spacewars-kernel-build-id" => format!("{}\n", "a".repeat(40)).into_bytes(),
                _ => format!("fixture: {path}\n").into_bytes(),
            };
            let target = directory.path().join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::set_permissions(target.parent().unwrap(), fs::Permissions::from_mode(0o700))
                .unwrap();
            fs::write(target, &bytes).unwrap();
            fs::set_permissions(
                directory.path().join(path),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            blobs.insert(
                path.clone(),
                Blob {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes),
                },
            );
        }
        let manifest: BootManifest =
            serde_json::from_value(serde_json::json!({"schema": 2, "files": blobs})).unwrap();
        let identity = manifest.identity();
        fs::write(
            directory.path().join("spacewars-boot-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.path().join("spacewars-boot-id"),
            format!("{identity}\n"),
        )
        .unwrap();
        let template = BootCommandLine::parse(
            &fs::read(directory.path().join("spacewars-cmdline.txt")).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.path().join("cmdline.txt"),
            template.render(&roots(), Slot::A, None),
        )
        .unwrap();
        fs::write(
            directory.path().join("spacewars-trial.txt"),
            UNPREPARED_TRIAL,
        )
        .unwrap();
        fs::write(
            directory.path().join("spacewars-device.txt"),
            "[all]\ninclude spacewars-profiles/hdmi.txt\n[all]\n",
        )
        .unwrap();
        for name in MUTABLE_BOOT_FILES {
            fs::set_permissions(
                directory.path().join(name),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        Self {
            directory,
            identity,
            owner,
        }
    }

    pub fn open(&self) -> Result<BootFiles> {
        BootFiles::open(
            open_directory(self.directory.path(), self.owner)?,
            self.owner,
            self.identity,
        )
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}

pub(crate) fn roots() -> SlotRoots {
    SlotRoots::new("076c4a2a-02".into(), "076c4a2a-03".into()).unwrap()
}

#[test]
fn trial_writes_preserve_default_and_manifest_and_promotion_preserves_trial() {
    let fixture = Fixture::new();
    let mut files = fixture.open().unwrap();
    let default = fs::read(fixture.path("cmdline.txt")).unwrap();
    assert_eq!(
        files.selections(&roots()).unwrap(),
        Selections {
            default: Slot::A,
            trial: None,
            profile: "hdmi".into()
        }
    );
    let transaction = Sha256::digest(b"durable transaction");
    files
        .select(&roots(), Slot::B, Some(transaction), |_| Ok(()))
        .unwrap();
    assert_eq!(fs::read(fixture.path("cmdline.txt")).unwrap(), default);
    let trial = fs::read(fixture.path("spacewars-trial.txt")).unwrap();
    assert_eq!(
        files.selections(&roots()).unwrap().trial,
        Some(TrialSelection {
            slot: Slot::B,
            transaction
        })
    );
    files.select(&roots(), Slot::B, None, |_| Ok(())).unwrap();
    assert_eq!(files.selections(&roots()).unwrap().default, Slot::B);
    assert_eq!(
        fs::read(fixture.path("spacewars-trial.txt")).unwrap(),
        trial
    );
    assert_eq!(files.identity(), fixture.identity);
    fixture.open().unwrap().verify().unwrap();
}

#[test]
fn optional_device_hostname_survives_trial_and_promotion_without_affecting_boot_identity() {
    let fixture = Fixture::new();
    fixture.open().unwrap();
    fs::write(fixture.path("hostname.txt"), b"sw-picade\n").unwrap();
    fs::set_permissions(
        fixture.path("hostname.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let mut files = fixture.open().unwrap();
    files
        .select(
            &roots(),
            Slot::B,
            Some(Sha256::digest(b"trial")),
            |_| Ok(()),
        )
        .unwrap();
    files.select(&roots(), Slot::B, None, |_| Ok(())).unwrap();
    assert_eq!(files.identity(), fixture.identity);
    assert_eq!(
        fs::read(fixture.path("hostname.txt")).unwrap(),
        b"sw-picade\n"
    );
    fs::write(fixture.path("hostname.txt"), b"spacewars").unwrap();
    files.verify().unwrap();
    for bad in [
        b"".as_slice(),
        b"-host",
        b"host-",
        b"host.example",
        b"host\nroot=/dev/sda2",
        b"host\0",
        &[b'a'; 64],
    ] {
        fs::write(fixture.path("hostname.txt"), bad).unwrap();
        assert!(files.verify().is_err());
    }
    fs::remove_file(fixture.path("hostname.txt")).unwrap();
    symlink("cmdline.txt", fixture.path("hostname.txt")).unwrap();
    assert!(files.verify().is_err());
}

#[test]
fn changed_immutable_assets_or_selectors_block_writes_without_overwriting_evidence() {
    for name in [
        "kernel8.img",
        "spacewars-profiles/hdmi.txt",
        "spacewars-cmdline.txt",
        "spacewars-boot-id",
        "spacewars-boot-manifest.json",
        "cmdline.txt",
        "spacewars-trial.txt",
        "spacewars-device.txt",
    ] {
        let fixture = Fixture::new();
        let mut files = fixture.open().unwrap();
        fs::write(fixture.path(name), b"tampered or ambiguous\n").unwrap();
        let before = fs::read(fixture.path("cmdline.txt")).unwrap();
        assert!(
            files.select(&roots(), Slot::B, None, |_| Ok(())).is_err(),
            "{name}"
        );
        assert_eq!(fs::read(fixture.path("cmdline.txt")).unwrap(), before);
        assert_eq!(
            fs::read(fixture.path(name)).unwrap(),
            b"tampered or ambiguous\n"
        );
    }
}

#[test]
fn late_journal_or_deadline_rejection_prevents_even_creating_a_temporary() {
    let fixture = Fixture::new();
    let mut files = fixture.open().unwrap();
    let before = fs::read(fixture.path("cmdline.txt")).unwrap();
    let entries = fs::read_dir(fixture.directory.path()).unwrap().count();
    assert!(
        files
            .select(&roots(), Slot::B, None, |_| Err(
                crate::recovery::RecoveryError::Unhealthy.into()
            ))
            .is_err()
    );
    assert_eq!(fs::read(fixture.path("cmdline.txt")).unwrap(), before);
    assert_eq!(
        fs::read_dir(fixture.directory.path()).unwrap().count(),
        entries
    );
    files.verify().unwrap();
}

#[test]
fn links_permissions_missing_and_unlisted_assets_fail_closed() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path("autoboot.txt"),
        b"unexpected alternate boot policy",
    )
    .unwrap();
    assert!(fixture.open().is_err());
    fs::remove_file(fixture.path("autoboot.txt")).unwrap();
    let kernel = fixture.path("kernel8.img");
    fs::remove_file(&kernel).unwrap();
    assert!(fixture.open().is_err());
    symlink(fixture.path("kernel_2712.img"), &kernel).unwrap();
    assert!(fixture.open().is_err());
    fs::remove_file(&kernel).unwrap();
    fs::hard_link(fixture.path("kernel_2712.img"), &kernel).unwrap();
    assert!(fixture.open().is_err());
    fs::remove_file(&kernel).unwrap();
    fs::copy(fixture.path("kernel_2712.img"), &kernel).unwrap();
    fs::set_permissions(&kernel, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(fixture.open().is_err());
    fs::set_permissions(&kernel, fs::Permissions::from_mode(0o644)).unwrap();
    fixture.open().unwrap();
    assert!(
        BootFiles::open(
            open_directory(fixture.directory.path(), fixture.owner).unwrap(),
            fixture.owner.wrapping_add(1),
            fixture.identity
        )
        .is_err()
    );
    let mut files = fixture.open().unwrap();
    files.device ^= 1;
    assert!(files.verify().is_err());
    let profiles = fixture.path("spacewars-profiles");
    let outside = tempfile::tempdir().unwrap();
    fs::rename(&profiles, outside.path().join("profiles")).unwrap();
    symlink(outside.path().join("profiles"), &profiles).unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn interrupted_temporaries_are_bounded_and_cannot_become_boot_configuration() {
    let fixture = Fixture::new();
    let temporary = fixture.path(".spacewars-boot-0123456789ABCDEF.tmp");
    fs::write(&temporary, b"partial selector").unwrap();
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
    fixture.open().unwrap();
    fs::write(&temporary, vec![b'x'; 8193]).unwrap();
    assert!(fixture.open().is_err());
    fs::remove_file(&temporary).unwrap();
    for index in 0..16 {
        fs::write(
            fixture.path(&format!(".spacewars-boot-{index:016x}.tmp")),
            b"partial",
        )
        .unwrap();
        fs::set_permissions(
            fixture.path(&format!(".spacewars-boot-{index:016x}.tmp")),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    fixture.open().unwrap();
    fs::write(
        fixture.path(".spacewars-boot-FFFFFFFFFFFFFFFF.tmp"),
        b"partial",
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn every_persistence_failure_poisons_the_handle_and_reopen_observes_old_or_new() {
    for point in [
        FaultPoint::Created,
        FaultPoint::Written,
        FaultPoint::FileSynced,
        FaultPoint::Renamed,
        FaultPoint::DirectorySynced,
        FaultPoint::FilesystemSynced,
    ] {
        let fixture = Fixture::new();
        let mut files = fixture.open().unwrap();
        files.fault = Some(point);
        let transaction = Sha256::digest(b"one transaction");
        assert!(
            files
                .select(&roots(), Slot::B, Some(transaction), |_| Ok(()))
                .is_err(),
            "{point:?}"
        );
        assert!(files.verify().is_err());
        assert!(files.selections(&roots()).is_err());
        assert!(files.select(&roots(), Slot::B, None, |_| Ok(())).is_err());
        drop(files);
        let reopened = fixture.open().unwrap();
        let selection = reopened.selections(&roots()).unwrap();
        assert_eq!(selection.default, Slot::A);
        let renamed = matches!(
            point,
            FaultPoint::Renamed | FaultPoint::DirectorySynced | FaultPoint::FilesystemSynced
        );
        assert_eq!(
            selection.trial,
            renamed.then_some(TrialSelection {
                slot: Slot::B,
                transaction
            })
        );
    }
}
