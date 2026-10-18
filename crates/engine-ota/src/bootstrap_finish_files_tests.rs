use super::*;
use crate::bootstrap::canonical::{CanonicalAction, blob};
use crate::bootstrap::canonical_files::tests::Migration;
use crate::bootstrap::trial_store::tests::sample;
use crate::bootstrap::{CanonicalFailure, CanonicalObservation, CanonicalPhase};
use std::os::unix::fs::{PermissionsExt, symlink};

pub(in crate::bootstrap) struct Retirement {
    pub migration: Migration,
    pub boot: CanonicalObservation,
    keep: BTreeMap<String, (Vec<u8>, u64)>,
    pub retired: BTreeMap<String, Vec<u8>>,
}

impl Retirement {
    pub fn new() -> Self {
        Self::from_migration(Migration::new())
    }

    pub fn fresh() -> Self {
        Self::from_migration(Migration::fresh())
    }

    fn from_migration(migration: Migration) -> Self {
        let mut store = migration.fixture.open();
        let mut boot = migration.selected(&mut store);
        boot.boot.boot_id = "cc606400-dc75-446e-b26b-385592b0f562".into();
        boot.boot.uptime_ms = 5_000;
        store
            .advance_canonical(|c, i| c.reconcile(i, &boot))
            .unwrap();
        let mut monitor = store
            .canonical()
            .unwrap()
            .monitor(&migration.fixture.inputs, &boot)
            .unwrap();
        let mut proof = None;
        for n in 0..=30 {
            boot.boot.uptime_ms = 5_000 + n * 2_000;
            proof = monitor.observe(Some(sample(&boot.boot, n + 1))).unwrap();
        }
        store
            .advance_canonical(|c, i| c.healthy(i, &boot, proof.unwrap()))
            .unwrap();
        let temporary = ".spacewars-bootstrap-0123456789abcdef.tmp";
        fs::write(
            migration.boot.join(temporary),
            b"retained selector temporary",
        )
        .unwrap();
        fs::set_permissions(
            migration.boot.join(temporary),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let mut files = crate::bootstrap::stage_files::inventory(
            &File::open(&migration.boot).unwrap(),
            migration.fixture.owner,
        )
        .unwrap();
        let keep = files
            .keys()
            .filter(|p| migration.plan.files.contains_key(*p) || p.as_str() == "hostname.txt")
            .map(|p| {
                (
                    p.clone(),
                    (
                        fs::read(migration.boot.join(p)).unwrap(),
                        fs::metadata(migration.boot.join(p)).unwrap().ino(),
                    ),
                )
            })
            .collect();
        files.retain(|p, _| !migration.plan.files.contains_key(p) && p != "hostname.txt");
        let retired = files
            .keys()
            .map(|p| (p.clone(), fs::read(migration.boot.join(p)).unwrap()))
            .collect();
        store
            .advance_canonical(|c, i| c.begin_finish(i, &boot, files))
            .unwrap();
        drop(store);
        Self {
            migration,
            boot,
            keep,
            retired,
        }
    }

    pub fn open(&self) -> TrialStore {
        TrialStore::completion_under(
            File::open(self.migration.fixture.temp.path()).unwrap(),
            File::open(self.migration.fixture.temp.path().join("data")).unwrap(),
            self.migration.fixture.owner,
        )
        .unwrap()
        .unwrap()
    }

    pub fn run(&self, store: &mut TrialStore, point: impl FnMut(&str) -> Result<()>) -> Result<()> {
        let boot = self.migration.files(store);
        retire(
            &boot,
            store,
            self.migration.fixture.owner,
            |store| {
                boot.verify(store.canonical()?)?;
                store
                    .canonical()?
                    .authorize_finish(&self.migration.fixture.inputs, &self.boot)
            },
            point,
        )
    }

    pub fn preserved(&self) {
        for (path, (bytes, inode)) in &self.keep {
            assert_eq!(
                &fs::read(self.migration.boot.join(path)).unwrap(),
                bytes,
                "{path}"
            );
            assert_eq!(
                fs::metadata(self.migration.boot.join(path)).unwrap().ino(),
                *inode,
                "{path}"
            );
        }
    }

    pub fn archive(&self, store: &TrialStore) -> std::path::PathBuf {
        self.migration
            .fixture
            .temp
            .path()
            .join("data/spacewars-bootstrap")
            .join(if store.is_completed() {
                "completed"
            } else {
                "active"
            })
            .join(DIRECTORY)
    }

    pub fn verified(&self, store: &TrialStore) {
        self.preserved();
        for (path, bytes) in &self.retired {
            assert!(!self.migration.boot.join(path).exists(), "{path}");
            assert_eq!(
                fs::read(self.archive(store).join(blob(bytes).sha256.to_string())).unwrap(),
                *bytes,
                "{path}"
            );
        }
        assert!(!self.migration.boot.join("sw-bootstrap").exists());
        crate::boot_files::BootFiles::open(
            File::open(&self.migration.boot).unwrap(),
            self.migration.fixture.owner,
            self.migration.fixture.inputs.image.boot_identity,
        )
        .unwrap();
    }
}

#[test]
fn retirement_boundaries_resume_with_verified_copies_and_unchanged_canonical_files() {
    let initial = Retirement::new();
    let mut store = initial.open();
    let mut points = Vec::new();
    initial
        .run(&mut store, |p| {
            points.push(p.to_owned());
            Ok(())
        })
        .unwrap();
    let mut cases: BTreeSet<_> = points.iter().map(|p| (p.clone(), 1usize)).collect();
    for point in [
        "retirement-copy-renamed",
        "retirement-file-unlinked",
        "retirement-file-completed",
    ] {
        let count = points.iter().filter(|p| *p == point).count();
        for at in [count / 2, count] {
            if at > 0 {
                cases.insert((point.to_owned(), at));
            }
        }
    }
    initial.verified(&store);
    drop(store);
    for (point, at) in cases {
        let fixture = Retirement::new();
        let mut store = fixture.open();
        let mut count = 0;
        assert!(
            fixture
                .run(&mut store, |p| {
                    if p == point {
                        count += 1;
                        if count == at {
                            return Err(invalid("interrupted retirement"));
                        }
                    }
                    Ok(())
                })
                .is_err(),
            "{point}:{at}"
        );
        fixture.preserved();
        drop(store);
        let mut store = fixture.open();
        fixture.run(&mut store, |_| Ok(())).unwrap();
        assert_eq!(store.finish_phase(), Some(FinishPhase::Ready));
        fixture.verified(&store);
        fixture.run(&mut store, |_| Ok(())).unwrap();
        fixture.verified(&store);
    }
}

#[test]
fn missing_corrupt_or_linked_retired_copies_prevent_further_removal_without_repairs() {
    for damage in ["missing", "corrupt", "link", "unknown"] {
        let fixture = Retirement::new();
        let mut store = fixture.open();
        assert!(
            fixture
                .run(&mut store, |p| if p == "retirement-file-completed" {
                    Err(invalid("stop"))
                } else {
                    Ok(())
                })
                .is_err()
        );
        let removed = store.canonical().unwrap().finish().unwrap().removed.clone();
        let path = removed.first().unwrap();
        let saved = fixture
            .archive(&store)
            .join(blob(&fixture.retired[path]).sha256.to_string());
        match damage {
            "missing" => fs::remove_file(&saved).unwrap(),
            "corrupt" => fs::write(&saved, b"corrupt").unwrap(),
            "link" => {
                fs::remove_file(&saved).unwrap();
                symlink(fixture.migration.boot.join("config.txt"), &saved).unwrap();
            }
            _ => fs::write(fixture.archive(&store).join("unknown"), b"unknown").unwrap(),
        }
        assert!(fixture.run(&mut store, |_| Ok(())).is_err(), "{damage}");
        assert_eq!(
            store.canonical().unwrap().finish().unwrap().removed,
            removed
        );
        fixture.preserved();
    }
}

#[test]
fn retirement_cannot_reuse_staged_health_or_restore_a_deleted_fallback() {
    let fixture = Retirement::new();
    let mut store = fixture.open();
    assert!(
        store
            .advance_canonical(|c, _| {
                c.finish = None;
                Ok(())
            })
            .is_err()
    );
    let mut other = fixture.boot.clone();
    other.boot.boot_id = "dd606400-dc75-446e-b26b-385592b0f562".into();
    assert!(
        store
            .canonical()
            .unwrap()
            .authorize_finish(&fixture.migration.fixture.inputs, &other)
            .is_err()
    );
    fixture.run(&mut store, |_| Ok(())).unwrap();
    store
        .advance_canonical(|c, i| c.reconcile(i, &other))
        .unwrap();
    assert_eq!(store.canonical_phase(), Some(CanonicalPhase::Observing));
    assert_eq!(
        store
            .advance_canonical(|c, i| c.fail(i, &other, CanonicalFailure::CanonicalHealthTimeout))
            .unwrap(),
        CanonicalAction::NeedsOperator
    );
    assert!(
        fixture
            .migration
            .files(&store)
            .select(false, &store, |_| Ok(()), |_| Ok(()))
            .is_err()
    );
    fixture.verified(&store);
}

#[test]
fn source_or_guard_changes_never_remove_unarchived_boot_data() {
    let fixture = Retirement::new();
    let mut store = fixture.open();
    let boot = fixture.migration.files(&store);
    assert!(
        retire(
            &boot,
            &mut store,
            fixture.migration.fixture.owner,
            |_| Err(invalid("guard denied")),
            |_| Ok(())
        )
        .is_err()
    );
    for (p, bytes) in &fixture.retired {
        assert_eq!(fs::read(fixture.migration.boot.join(p)).unwrap(), *bytes);
    }
    let path = fixture.retired.keys().next().unwrap();
    fs::write(fixture.migration.boot.join(path), b"changed").unwrap();
    assert!(boot.verify(store.canonical().unwrap()).is_err());
    assert_eq!(store.finish_phase(), Some(FinishPhase::Archiving));
    fixture.preserved();
}

#[test]
fn retirement_journal_flush_failures_preserve_copies_and_resume_without_recreating_files() {
    for phase in ["archive-completion", "remove-intent", "remove-completion"] {
        for point in ["write", "file-sync", "rename", "directory-sync"] {
            let fixture = Retirement::new();
            let mut store = fixture.open();
            if phase == "remove-completion" {
                assert!(
                    fixture
                        .run(&mut store, |p| if p == "retirement-removal-synced" {
                            Err(invalid("stop"))
                        } else {
                            Ok(())
                        })
                        .is_err()
                );
                let pending = store
                    .canonical()
                    .unwrap()
                    .finish()
                    .unwrap()
                    .pending
                    .clone()
                    .unwrap();
                assert!(!fixture.migration.boot.join(&pending).exists());
                store.fault = Some(point);
                assert!(fixture.run(&mut store, |_| Ok(())).is_err());
                assert!(!fixture.migration.boot.join(pending).exists());
            } else {
                let boot = fixture.migration.files(&store);
                let archive = RetiredFiles::open(
                    store.active_directory().unwrap(),
                    fixture.migration.fixture.owner,
                    store.canonical().unwrap().finish().unwrap(),
                )
                .unwrap();
                archive
                    .archive(
                        &boot,
                        &store,
                        &mut |store| boot.verify(store.canonical()?),
                        &mut |_| Ok(()),
                    )
                    .unwrap();
                if phase == "archive-completion" {
                    store.fault = Some(point);
                    assert!(store.advance_finish(FinishState::archived).is_err());
                } else {
                    store.advance_finish(FinishState::archived).unwrap();
                    let path = fixture.retired.keys().next().unwrap();
                    store.fault = Some(point);
                    assert!(store.advance_finish(|f| f.begin_file(path)).is_err());
                }
                for (p, bytes) in &fixture.retired {
                    assert_eq!(fs::read(fixture.migration.boot.join(p)).unwrap(), *bytes);
                }
            }
            assert!(store.durable().is_err());
            drop(store);
            let mut store = fixture.open();
            fixture.run(&mut store, |_| Ok(())).unwrap();
            fixture.verified(&store);
            assert!(
                store
                    .advance_finish(|f| {
                        f.removed.clear();
                        f.phase = FinishPhase::Removing;
                        Ok(())
                    })
                    .is_err()
            );
        }
    }
}
