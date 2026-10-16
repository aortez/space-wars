use super::*;
use crate::bootstrap::finish_files::tests::Retirement;
use crate::recovery::SlotImage;
use crate::{RecoveryStore, Sha256};
use std::fs;
use std::os::unix::fs::MetadataExt;

fn plan(fixture: &Retirement, store: &mut TrialStore) -> RecoveryRecord {
    fixture.run(store, |_| Ok(())).unwrap();
    let inputs = &fixture.migration.fixture.inputs;
    let baseline = RecoveryRecord::bootstrap(
        fixture.boot.boot.binding.clone(),
        SlotImage {
            slot: inputs.plan.target,
            build_id: inputs.image.build_id.clone(),
            generation: 0,
            payload: Sha256::digest(b"bootstrap compressed"),
            expanded: Sha256::digest(b"bootstrap expanded"),
            data: inputs.image.data,
        },
        &fixture.boot.boot,
    )
    .unwrap();
    store
        .advance_finish(|f| f.handoff(baseline.clone()))
        .unwrap();
    baseline
}

fn run(
    fixture: &Retirement,
    store: &mut TrialStore,
    point: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    publish(
        store,
        fixture.migration.fixture.owner,
        |store| {
            store.durable()?;
            fixture.migration.files(store).verify(store.canonical()?)?;
            Ok(())
        },
        point,
    )
}

#[test]
fn handoff_boundaries_publish_one_exact_baseline_before_archiving_the_active_attempt() {
    for point in [
        "handoff-directory-ready",
        "handoff-record-written",
        "handoff-record-synced",
        "handoff-record-renamed",
        "handoff-directory-synced",
        "handoff-recovery-published",
        "handoff-recovery-synced",
        "handoff-complete-recorded",
        "handoff-reservation-renamed",
        "handoff-reservation-synced",
    ] {
        let fixture = Retirement::new();
        let mut store = fixture.open();
        let baseline = plan(&fixture, &mut store);
        assert!(
            run(&fixture, &mut store, |p| if p == point {
                Err(invalid("handoff interrupted"))
            } else {
                Ok(())
            })
            .is_err(),
            "{point}"
        );
        fixture.preserved();
        drop(store);
        let mut store = fixture.open();
        run(&fixture, &mut store, |_| Ok(())).unwrap();
        assert!(store.is_completed());
        assert_eq!(store.finish_phase(), Some(FinishPhase::Complete));
        fixture.verified(&store);
        let data = fixture.migration.fixture.temp.path().join("data");
        assert!(!data.join("spacewars-bootstrap/active").exists());
        assert!(
            data.join("spacewars-bootstrap/completed/boot-trial/record.json")
                .exists()
        );
        let recovery = data.join("spacewars-system/recovery");
        assert_eq!(RecoveryStore::open(&recovery).unwrap().record(), &baseline);
        let inode = fs::metadata(recovery.join("state.json")).unwrap().ino();
        run(&fixture, &mut store, |_| Ok(())).unwrap();
        assert_eq!(
            fs::metadata(recovery.join("state.json")).unwrap().ino(),
            inode
        );
        crate::bootstrap::require_no_active_under(
            &File::open(&data).unwrap(),
            fixture.migration.fixture.owner,
        )
        .unwrap();
        assert!(fixture.migration.fixture.try_open().unwrap().is_none());
    }
}

#[test]
fn published_corrupt_or_missing_production_state_is_never_reinitialized() {
    for completed in [false, true] {
        for damage in ["missing", "corrupt", "unknown"] {
            let fixture = Retirement::new();
            let mut store = fixture.open();
            plan(&fixture, &mut store);
            if completed {
                run(&fixture, &mut store, |_| Ok(())).unwrap();
            } else {
                assert!(
                    run(&fixture, &mut store, |p| {
                        if p == "handoff-complete-recorded" {
                            Err(invalid("stop"))
                        } else {
                            Ok(())
                        }
                    })
                    .is_err()
                );
            }
            let data = fixture.migration.fixture.temp.path().join("data");
            let recovery = data.join("spacewars-system/recovery");
            match damage {
                "missing" => fs::remove_dir_all(&recovery).unwrap(),
                "corrupt" => fs::write(recovery.join("state.json"), b"corrupt").unwrap(),
                _ => fs::write(recovery.join("unknown"), b"unexpected").unwrap(),
            }
            assert!(
                run(&fixture, &mut store, |_| Ok(())).is_err(),
                "{completed}/{damage}"
            );
            drop(store);
            assert!(
                TrialStore::completion_under(
                    File::open(fixture.migration.fixture.temp.path()).unwrap(),
                    File::open(&data).unwrap(),
                    fixture.migration.fixture.owner
                )
                .is_err()
            );
            if damage == "missing" {
                assert!(!recovery.exists());
                assert!(
                    crate::bootstrap::require_no_active_under(
                        &File::open(&data).unwrap(),
                        fixture.migration.fixture.owner
                    )
                    .is_err()
                );
                assert!(fixture.migration.fixture.try_open().is_err());
            }
            fixture.preserved();
        }
    }
}

#[test]
fn handoff_intent_and_completion_flush_failures_do_not_release_the_reservation() {
    for phase in ["intent", "complete"] {
        for point in ["write", "file-sync", "rename", "directory-sync"] {
            let fixture = Retirement::new();
            let mut store = fixture.open();
            let baseline = plan(&fixture, &mut store);
            if phase == "intent" {
                // Persist an identical fresh handoff from Ready with injected I/O.
                drop(store);
                let fresh = Retirement::new();
                let mut store = fresh.open();
                fresh.run(&mut store, |_| Ok(())).unwrap();
                store.fault = Some(point);
                assert!(
                    store
                        .advance_finish(|f| f.handoff(baseline.clone()))
                        .is_err()
                );
                assert!(store.durable().is_err());
                assert!(
                    !fresh
                        .migration
                        .fixture
                        .temp
                        .path()
                        .join("data/spacewars-system/recovery")
                        .exists()
                );
                continue;
            }
            assert!(
                run(
                    &fixture,
                    &mut store,
                    |p| if p == "handoff-recovery-synced" {
                        Err(invalid("stop before completion"))
                    } else {
                        Ok(())
                    }
                )
                .is_err()
            );
            store.fault = Some(point);
            assert!(run(&fixture, &mut store, |_| Ok(())).is_err());
            assert!(
                fixture
                    .migration
                    .fixture
                    .temp
                    .path()
                    .join("data/spacewars-bootstrap/active")
                    .exists()
            );
            drop(store);
            let mut store = fixture.open();
            run(&fixture, &mut store, |_| Ok(())).unwrap();
            assert!(store.is_completed());
        }
    }
}
