use super::*;
use std::cell::Cell;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

struct Fixture {
    temp: tempfile::TempDir,
    owner: u32,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            temp: tempfile::tempdir().unwrap(),
            owner: unsafe { libc::geteuid() },
        };
        for directory in ["spacewars-system", "spacewars-bootstrap"] {
            fixture.directory(directory);
        }
        for store in [
            "install-handoff",
            "network",
            "recovery",
            "time",
            "time-check",
            "update-check",
            "updates",
        ] {
            fixture.directory(&format!("spacewars-system/{store}"));
        }
        fixture.file("spacewars-system/recovery/state.json", b"journal");
        fixture.file("spacewars-system/time/state.json", b"time");
        fixture.file("spacewars-system/ota-bootstrap.json", b"bundle");
        fixture.file("spacewars-system/update-provision.json", b"receipt");
        fixture.directory("spacewars-bootstrap/completed");
        fixture.directory("spacewars-bootstrap/package");
        fixture
    }

    fn path(&self, path: &str) -> PathBuf {
        self.temp.path().join(path)
    }

    fn directory(&self, path: &str) {
        fs::create_dir(self.path(path)).unwrap();
        fs::set_permissions(self.path(path), fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn file(&self, path: &str, bytes: &[u8]) {
        fs::write(self.path(path), bytes).unwrap();
        fs::set_permissions(self.path(path), fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn data(&self) -> File {
        File::open(self.temp.path()).unwrap()
    }

    fn names(&self, path: &str) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(self.path(path))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    fn retire(&self, evidence: &Evidence) -> Result<(Retirement, String, bool)> {
        self.confirming(evidence, GOOD)
    }

    fn confirming(&self, evidence: &Evidence, good: &str) -> Result<(Retirement, String, bool)> {
        retire_under(&self.data(), self.owner, evidence, good, || Ok(()))
    }

    /// Fail the guard call numbered `stop`, as a crash at that point would.
    fn interrupt(&self, evidence: &Evidence, stop: usize) -> Result<(Retirement, String, bool)> {
        let calls = Cell::new(0);
        retire_under(&self.data(), self.owner, evidence, GOOD, || {
            calls.set(calls.get() + 1);
            if calls.get() == stop {
                Err(invalid("interrupted fixture"))
            } else {
                Ok(())
            }
        })
    }

    fn retired_boot(&self, files: &BTreeMap<String, Blob>) -> Result<bool> {
        retired_boot(&self.data(), self.owner, files)
    }

    /// A boot drops the runtime masks. Kiosk startup reconciliation
    /// (`install_reconcile::native_locked`, unchanged since Release generation
    /// 20) then creates an absent handoff store whenever the recovery journal
    /// exists and no bootstrap is active.
    fn reboot(&self) {
        let journal = self.path("spacewars-system/recovery").exists();
        let unguarded =
            super::super::state::require_no_active_under(&self.data(), self.owner).is_ok();
        if journal && unguarded && !self.path("spacewars-system/install-handoff").exists() {
            self.directory("spacewars-system/install-handoff");
            self.file(
                "spacewars-system/install-handoff/state.json",
                b"startup journal",
            );
        }
    }
}

fn boot_files(default: &[u8]) -> BTreeMap<String, Blob> {
    [
        ("cmdline.txt", default),
        ("config.txt", b"config".as_slice()),
    ]
    .into_iter()
    .map(|(path, bytes)| {
        (
            path.into(),
            Blob {
                bytes: bytes.len() as u64,
                sha256: Sha256::digest(bytes),
            },
        )
    })
    .collect()
}

fn evidence(journal: bool) -> Evidence {
    Evidence {
        boot_id: "de606400-dc75-446e-b26b-385592b0f562".into(),
        binding: BootBinding {
            device_id: Sha256::digest(b"device"),
            layout_id: Sha256::digest(b"layout"),
            boot_identity: Sha256::digest(b"boot set"),
            hardware_profile: Sha256::digest(b"profile"),
        },
        boot_files: boot_files(b"root=PARTUUID=076c4a2a-03"),
        journal: journal.then(|| Journal {
            fingerprint: Sha256::digest(b"journal"),
            good_slot: Slot::B,
            good_build_id: GOOD.into(),
        }),
    }
}

const GOOD: &str = "rootfs-good";

// Seven moves, plus the guard before planning and the one before publication.
const GUARD_CALLS: usize = 9;

// The guard call before the journal's move, which follows the two bootstrap
// entries'.
const JOURNAL_MOVE: usize = 4;

#[test]
fn a_mistaken_good_image_confirmation_stops_before_anything_changes() {
    let fixture = Fixture::new();
    assert!(fixture.confirming(&evidence(true), "rootfs-other").is_err());
    assert!(fixture.names("spacewars-retired").is_empty());
    assert!(fixture.path("spacewars-bootstrap/completed").exists());
    // An interrupted retirement is finished only under the same confirmation.
    assert!(fixture.interrupt(&evidence(true), 4).is_err());
    assert!(fixture.confirming(&evidence(true), "rootfs-other").is_err());
    assert!(fixture.path("spacewars-system/install-handoff").exists());
    let (_, id, _) = fixture.retire(&evidence(true)).unwrap();
    assert!(
        fixture
            .confirming(&evidence(false), "rootfs-other")
            .is_err()
    );
    assert_eq!(fixture.retire(&evidence(false)).unwrap().1, id);
}

#[test]
fn retirement_moves_production_records_and_keeps_service_stores() {
    let fixture = Fixture::new();
    assert!(!fixture.retired_boot(&evidence(true).boot_files).unwrap());
    let (record, id, resumed) = fixture.retire(&evidence(true)).unwrap();
    assert!(!resumed);
    assert_eq!(
        record.system,
        [
            "install-handoff",
            "ota-bootstrap.json",
            "recovery",
            "update-provision.json",
            "updates"
        ]
    );
    assert_eq!(record.bootstrap, ["completed", "package"]);
    assert_eq!(fixture.names("spacewars-system"), PRESERVED);
    assert_eq!(
        fs::read(fixture.path("spacewars-system/time/state.json")).unwrap(),
        b"time"
    );
    assert!(fixture.names("spacewars-bootstrap").is_empty());
    assert_eq!(
        fixture.names("spacewars-retired"),
        std::slice::from_ref(&id)
    );
    let retired = format!("spacewars-retired/{id}");
    assert_eq!(
        fixture.names(&retired),
        ["bootstrap", "retirement.json", "system"]
    );
    assert_eq!(
        fs::read(fixture.path(&format!("{retired}/system/recovery/state.json"))).unwrap(),
        b"journal"
    );
    assert_eq!(
        fixture.names(&format!("{retired}/bootstrap")),
        ["completed", "package"]
    );
    // Only the exact retired boot set qualifies, and repeating reports it.
    assert!(fixture.retired_boot(&evidence(true).boot_files).unwrap());
    assert!(!fixture.retired_boot(&boot_files(b"other")).unwrap());
    let (again, same, resumed) = fixture.retire(&evidence(false)).unwrap();
    assert!(resumed);
    assert_eq!((again, same), (record, id));
    // Lost production state is never retirable, even beside a retirement.
    let mut other = evidence(false);
    other.boot_files = boot_files(b"other");
    assert!(fixture.retire(&other).is_err());
}

#[test]
fn an_interrupted_retirement_finishes_from_every_checkpoint() {
    for stop in 1..=GUARD_CALLS {
        let fixture = Fixture::new();
        assert!(fixture.interrupt(&evidence(true), stop).is_err(), "{stop}");
        // A partial retirement authorizes nothing until it completes.
        if stop > 1 {
            assert!(fixture.retired_boot(&evidence(true).boot_files).is_err());
        }
        // The journal moves right after the bootstrap entries.
        let journal = fixture.path("spacewars-system/recovery").exists();
        assert_eq!(journal, stop <= JOURNAL_MOVE, "{stop}");
        assert_eq!(
            fixture.path("spacewars-bootstrap/completed").exists(),
            stop < 3,
            "{stop}"
        );
        let (record, id, resumed) = fixture.retire(&evidence(journal)).unwrap();
        assert_eq!(resumed, stop > 1, "{stop}");
        assert_eq!(fixture.names("spacewars-system"), PRESERVED);
        assert_eq!(record.system.len(), 5);
        assert!(
            fixture
                .path(&format!(
                    "spacewars-retired/{id}/system/recovery/state.json"
                ))
                .exists()
        );
        assert!(fixture.retired_boot(&evidence(true).boot_files).unwrap());
    }
}

#[test]
fn a_reboot_at_any_checkpoint_leaves_the_retirement_resumable() {
    for stop in 1..=GUARD_CALLS {
        let fixture = Fixture::new();
        assert!(fixture.interrupt(&evidence(true), stop).is_err(), "{stop}");
        fixture.reboot();
        let mut later = evidence(fixture.path("spacewars-system/recovery").exists());
        later.boot_id = "7c1f0a52-5b7e-4d83-a2f4-0e9c6d1b3a57".into();
        let (record, _, _) = fixture
            .retire(&later)
            .unwrap_or_else(|error| panic!("{stop}: {error:?}"));
        assert_eq!(record.system.len(), 5, "{stop}");
        assert_eq!(fixture.names("spacewars-system"), PRESERVED, "{stop}");
    }
}

#[test]
fn a_resumed_retirement_requires_the_same_boot_set_and_journal() {
    let fixture = Fixture::new();
    assert!(fixture.interrupt(&evidence(true), 4).is_err());
    let mut changed = evidence(true);
    changed.boot_files = boot_files(b"root=PARTUUID=076c4a2a-02");
    assert!(fixture.retire(&changed).is_err());
    let mut changed = evidence(true);
    changed.binding.boot_identity = Sha256::digest(b"another boot set");
    assert!(fixture.retire(&changed).is_err());
    let mut changed = evidence(true);
    changed.journal.as_mut().unwrap().fingerprint = Sha256::digest(b"changed journal");
    assert!(fixture.retire(&changed).is_err());
    fixture.retire(&evidence(true)).unwrap();
}

#[test]
fn retirement_refuses_lost_journals_unknown_state_and_unsafe_entries() {
    // No journal and nothing retired: the state was lost, not retired.
    let fixture = Fixture::new();
    assert!(fixture.retire(&evidence(false)).is_err());
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.path("spacewars-system/recovery")).unwrap();
    assert!(fixture.retire(&evidence(true)).is_err());
    // An active reservation is never retired.
    let fixture = Fixture::new();
    fixture.directory("spacewars-bootstrap/active");
    assert!(fixture.retire(&evidence(true)).is_err());
    assert!(fixture.path("spacewars-system/recovery").exists());
    // Unsafe entries stop the retirement before anything is created or moved.
    for case in ["link", "shared", "hidden"] {
        let fixture = Fixture::new();
        match case {
            "link" => symlink("/tmp", fixture.path("spacewars-system/linked")).unwrap(),
            "shared" => {
                fixture.file("spacewars-system/shared.json", b"shared");
                fs::set_permissions(
                    fixture.path("spacewars-system/shared.json"),
                    fs::Permissions::from_mode(0o666),
                )
                .unwrap();
            }
            _ => fixture.directory("spacewars-system/.hidden"),
        }
        assert!(fixture.retire(&evidence(true)).is_err(), "{case}");
        assert!(fixture.names("spacewars-retired").is_empty(), "{case}");
        assert!(fixture.path("spacewars-bootstrap/completed").exists());
        assert!(fixture.path("spacewars-system/recovery").exists());
    }
}

#[test]
fn only_empty_unpublished_scratch_is_discarded() {
    let scratch = "spacewars-retired/.0123456789abcdef.creating";
    let fixture = Fixture::new();
    fixture.directory("spacewars-retired");
    fixture.directory(scratch);
    fixture.directory(&format!("{scratch}/system"));
    fixture.file(&format!("{scratch}/retirement.json"), b"partial");
    let (_, id, resumed) = fixture.retire(&evidence(true)).unwrap();
    assert!(!resumed);
    assert_eq!(fixture.names("spacewars-retired"), [id]);

    // Scratch never holds moved state; if it does, a person must look.
    let fixture = Fixture::new();
    fixture.directory("spacewars-retired");
    fixture.directory(scratch);
    fixture.directory(&format!("{scratch}/system"));
    fixture.directory(&format!("{scratch}/system/recovery"));
    assert!(fixture.retire(&evidence(true)).is_err());
    assert!(fixture.path(&format!("{scratch}/system/recovery")).exists());
    assert!(fixture.path("spacewars-system/recovery").exists());

    for unexpected in ["unexpected", ".0123456789abcdef.pending.old"] {
        let fixture = Fixture::new();
        fixture.directory("spacewars-retired");
        fixture.directory(&format!("spacewars-retired/{unexpected}"));
        assert!(fixture.retire(&evidence(true)).is_err(), "{unexpected}");
        assert!(fixture.retired_boot(&evidence(true).boot_files).is_err());
    }
}
