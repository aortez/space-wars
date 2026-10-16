use super::*;
use crate::bootstrap::canonical::{CanonicalAction, CanonicalPlan};
use crate::bootstrap::trial_store::tests::{Fixture, sample};
use crate::bootstrap::{CanonicalFailure, CanonicalObservation, TrialPhase};
use crate::recovery::BootObservation;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

pub(in crate::bootstrap) struct Migration {
    pub fixture: Fixture,
    pub boot: PathBuf,
    pub sources: PathBuf,
    pub normal: CanonicalObservation,
    pub plan: CanonicalPlan,
    retained_inodes: BTreeMap<String, u64>,
}

fn modes(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    for e in fs::read_dir(path).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            modes(&e.path());
        } else {
            fs::set_permissions(e.path(), fs::Permissions::from_mode(0o644)).unwrap();
        }
    }
}

pub(in crate::bootstrap) fn confirm_normal(
    store: &mut TrialStore,
    fixture: &Fixture,
) -> BootObservation {
    let mut boot = fixture.boot();
    boot.tryboot = false;
    boot.trial_id = None;
    boot.default_slot = Some(fixture.inputs.plan.target);
    boot.boot_id = "bb606400-dc75-446e-b26b-385592b0f562".into();
    store.advance(|r| r.reconcile(&boot)).unwrap();
    let mut monitor = store.durable().unwrap().monitor(&boot).unwrap();
    let mut proof = None;
    for n in 0..=30 {
        boot.uptime_ms = 5_000 + n * 2_000;
        proof = monitor.observe(Some(sample(&boot, n + 1))).unwrap();
    }
    store.advance(|r| r.healthy(proof.unwrap(), &boot)).unwrap();
    assert_eq!(store.phase(), TrialPhase::NormalConfirmed);
    boot
}

impl Migration {
    pub fn new() -> Self {
        let fixture = Fixture::new();
        let (mut store, _) = fixture.promoted();
        let normal = CanonicalObservation {
            boot: confirm_normal(&mut store, &fixture),
            default: CanonicalDefault::Staged,
        };
        let assets = crate::bootstrap::tests::Assets::new();
        let manifest_files: BTreeMap<_, _> = assets
            .contents
            .iter()
            .map(|(p, b)| (p.clone(), blob(b)))
            .collect();
        let manifest =
            serde_json::to_string(&serde_json::json!({"schema":2,"files":manifest_files})).unwrap();
        let mut contents = assets.contents;
        contents.insert(
            "spacewars-boot-manifest.json".into(),
            manifest.as_bytes().to_vec(),
        );
        contents.insert(
            "spacewars-boot-id".into(),
            format!("{}\n", fixture.inputs.image.boot_identity).into_bytes(),
        );
        contents.insert(
            "spacewars-device.txt".into(),
            format!(
                "[all]\ninclude spacewars-profiles/{}.txt\n[all]\n",
                fixture.inputs.plan.profile
            )
            .into_bytes(),
        );
        contents.insert(
            "cmdline.txt".into(),
            fixture.inputs.plan.normal_cmdline.as_bytes().to_vec(),
        );
        contents.insert(
            "spacewars-trial.txt".into(),
            crate::boot::UNPREPARED_TRIAL.as_bytes().to_vec(),
        );
        let plan = CanonicalPlan {
            manifest,
            config: String::from_utf8(contents["config.txt"].clone()).unwrap(),
            files: contents.iter().map(|(p, b)| (p.clone(), blob(b))).collect(),
        };
        let boot = fixture.temp.path().join("boot");
        let sources = fixture.temp.path().join("sources");
        for (p, b) in &fixture.boot_files {
            let path = boot.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                if p == "config.txt" {
                    fixture.inputs.plan.normal_config.as_bytes()
                } else {
                    b.as_slice()
                },
            )
            .unwrap();
        }
        for (p, b) in &contents {
            let path = sources.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b).unwrap();
        }
        modes(&boot);
        modes(&sources);
        let retained_inodes = fixture
            .boot_files
            .keys()
            .filter(|p| {
                p.starts_with("sw-bootstrap/")
                    || matches!(p.as_str(), "hostname.txt" | "boot_slot")
                    || fixture.inputs.plan.unchanged_firmware.contains_key(*p)
            })
            .map(|p| (p.clone(), fs::metadata(boot.join(p)).unwrap().ino()))
            .collect();
        store
            .advance(|r| r.start_canonical(plan.clone(), &normal.boot))
            .unwrap();
        drop(store);
        Self {
            fixture,
            boot,
            sources,
            normal,
            plan,
            retained_inodes,
        }
    }

    pub fn files(&self, store: &TrialStore) -> CanonicalFiles {
        CanonicalFiles::open(
            File::open(&self.boot).unwrap(),
            self.fixture.owner,
            self.fixture.inputs.clone(),
            store.canonical().unwrap(),
        )
        .unwrap()
    }

    pub fn prepare(
        &self,
        store: &mut TrialStore,
        point: impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        self.files(store).prepare(
            store,
            |p| Ok(File::open(self.sources.join(p))?),
            |store| {
                store
                    .canonical()?
                    .authorize_copy(&self.fixture.inputs, &self.normal)
            },
            point,
        )
    }

    pub fn retained(&self) {
        for (path, original) in &self.fixture.boot_files {
            if path.starts_with("sw-bootstrap/")
                || matches!(path.as_str(), "hostname.txt" | "boot_slot")
                || self
                    .fixture
                    .inputs
                    .plan
                    .unchanged_firmware
                    .contains_key(path)
            {
                assert_eq!(&fs::read(self.boot.join(path)).unwrap(), original, "{path}");
                assert_eq!(
                    fs::metadata(self.boot.join(path)).unwrap().ino(),
                    self.retained_inodes[path],
                    "retained inode: {path}"
                );
            }
        }
    }

    pub fn selected(&self, store: &mut TrialStore) -> CanonicalObservation {
        self.prepare(store, |_| Ok(())).unwrap();
        store
            .advance_canonical(|c, inputs| c.begin_selection(inputs, &self.normal))
            .unwrap();
        self.files(store)
            .select(true, store, |_| Ok(()), |_| Ok(()))
            .unwrap();
        let mut selected = self.normal.clone();
        selected.default = CanonicalDefault::Canonical;
        store
            .advance_canonical(|c, inputs| c.selected(inputs, &selected))
            .unwrap();
        selected
    }
}

#[test]
fn canonical_copy_boundaries_resume_without_touching_staged_fallback_or_default() {
    let initial = Migration::new();
    let mut store = initial.fixture.open();
    let mut points = Vec::new();
    initial
        .prepare(&mut store, |p| {
            points.push(p.to_string());
            Ok(())
        })
        .unwrap();
    let mut cases = BTreeSet::new();
    for point in &points {
        cases.insert((point.clone(), 1));
    }
    for point in [
        "canonical-file-intent",
        "canonical-renamed",
        "canonical-file-completed",
    ] {
        let count = points.iter().filter(|p| *p == point).count();
        for n in [count / 2, count] {
            if n > 0 {
                cases.insert((point.into(), n));
            }
        }
    }
    drop(store);
    for (point, at) in cases {
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        let mut count = 0;
        let result = fixture.prepare(&mut store, |p| {
            if p == point {
                count += 1;
                if count == at {
                    return Err(invalid("interrupted canonical copy"));
                }
            }
            Ok(())
        });
        assert!(result.is_err(), "{point}:{at}");
        fixture.retained();
        assert_eq!(
            fs::read(fixture.boot.join("config.txt")).unwrap(),
            fixture.fixture.inputs.plan.normal_config.as_bytes()
        );
        drop(store);
        let mut store = fixture.fixture.open();
        fixture.prepare(&mut store, |_| Ok(())).unwrap();
        assert_eq!(store.canonical_phase(), Some(CanonicalPhase::Prepared));
        fixture.retained();
        for (path, expected) in &fixture.plan.files {
            if path != "config.txt" {
                assert_eq!(blob(&fs::read(fixture.boot.join(path)).unwrap()), *expected);
            }
        }
        assert!(
            store
                .advance(|r| r.reconcile(&fixture.normal.boot))
                .is_err()
        );
    }
}

#[test]
fn canonical_selector_is_last_and_each_interruption_keeps_a_complete_known_config() {
    for point in [
        "canonical-parent-ready",
        "canonical-file-opened",
        "canonical-file-written",
        "canonical-file-synced",
        "canonical-before-rename",
        "canonical-renamed",
        "canonical-directory-synced",
        "canonical-filesystem-synced",
    ] {
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        assert!(
            store
                .advance_canonical(|c, i| c.begin_selection(i, &fixture.normal))
                .is_err()
        );
        fixture.prepare(&mut store, |_| Ok(())).unwrap();
        store
            .advance_canonical(|c, i| c.begin_selection(i, &fixture.normal))
            .unwrap();
        let files = fixture.files(&store);
        assert!(
            files
                .select(
                    true,
                    &store,
                    |_| Ok(()),
                    |p| if p == point {
                        Err(invalid("selector interruption"))
                    } else {
                        Ok(())
                    }
                )
                .is_err(),
            "{point}"
        );
        let actual = fs::read(fixture.boot.join("config.txt")).unwrap();
        assert!(
            actual == fixture.plan.config.as_bytes()
                || actual == fixture.fixture.inputs.plan.normal_config.as_bytes()
        );
        fixture.retained();
        drop(store);
        let mut store = fixture.fixture.open();
        let files = fixture.files(&store);
        files.select(true, &store, |_| Ok(()), |_| Ok(())).unwrap();
        let mut selected = fixture.normal.clone();
        selected.default = CanonicalDefault::Canonical;
        store
            .advance_canonical(|c, i| c.selected(i, &selected))
            .unwrap();
        assert!(
            store
                .advance_canonical(|c, i| c.reconcile(i, &selected))
                .is_err()
        );
        store
            .advance_canonical(|c, i| c.request_reboot(i, &selected))
            .unwrap();
        assert!(
            store
                .advance_canonical(|c, i| c.request_reboot(i, &selected))
                .is_err()
        );
    }
}

#[test]
fn damaged_completed_assets_unknown_paths_and_links_cannot_be_repaired_by_resume() {
    for case in [
        "completed",
        "missing",
        "staged",
        "hostname",
        "unknown",
        "empty-directory",
        "link",
        "oversized-pending",
    ] {
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        fixture.prepare(&mut store, |_| Ok(())).unwrap();
        match case {
            "completed" => fs::write(fixture.boot.join("kernel8.img"), b"changed").unwrap(),
            "missing" => fs::remove_file(fixture.boot.join("spacewars-boot-id")).unwrap(),
            "staged" => {
                fs::write(fixture.boot.join("sw-bootstrap/kernel8.img"), b"changed").unwrap()
            }
            "hostname" => fs::write(fixture.boot.join("hostname.txt"), b"changed\n").unwrap(),
            "unknown" => fs::write(fixture.boot.join("surprise"), b"x").unwrap(),
            "empty-directory" => fs::create_dir(fixture.boot.join("surprise")).unwrap(),
            "link" => {
                fs::remove_file(fixture.boot.join("kernel8.img")).unwrap();
                symlink("sw-bootstrap/kernel8.img", fixture.boot.join("kernel8.img")).unwrap();
            }
            "oversized-pending" => fs::write(fixture.boot.join(PENDING), vec![0; 20000]).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            CanonicalFiles::open(
                File::open(&fixture.boot).unwrap(),
                fixture.fixture.owner,
                fixture.fixture.inputs.clone(),
                store.canonical().unwrap()
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(
            fs::read(fixture.boot.join("config.txt")).unwrap(),
            fixture.fixture.inputs.plan.normal_config.as_bytes()
        );
    }
}

#[test]
fn changed_source_and_failed_final_guard_never_publish_canonical_default() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    fs::write(fixture.sources.join("kernel8.img"), b"wrong fixture data").unwrap();
    assert!(fixture.prepare(&mut store, |_| Ok(())).is_err());
    assert_eq!(
        fs::read(fixture.boot.join("config.txt")).unwrap(),
        fixture.fixture.inputs.plan.normal_config.as_bytes()
    );
    fixture.retained();
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    fixture.prepare(&mut store, |_| Ok(())).unwrap();
    store
        .advance_canonical(|c, i| c.begin_selection(i, &fixture.normal))
        .unwrap();
    assert!(
        fixture
            .files(&store)
            .select(true, &store, |_| Err(invalid("device changed")), |_| Ok(()))
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.boot.join("config.txt")).unwrap(),
        fixture.fixture.inputs.plan.normal_config.as_bytes()
    );
}

#[test]
fn canonical_failure_restores_only_staged_configuration_and_preserves_all_copies() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    let mut current = fixture.selected(&mut store);
    current.boot.boot_id = "cc606400-dc75-446e-b26b-385592b0f562".into();
    current.boot.uptime_ms = 5_000;
    store
        .advance_canonical(|c, i| c.reconcile(i, &current))
        .unwrap();
    assert_eq!(
        store
            .advance_canonical(|c, i| c.fail(i, &current, CanonicalFailure::CanonicalHealthTimeout))
            .unwrap(),
        CanonicalAction::RestoreStaged
    );
    fixture
        .files(&store)
        .select(false, &store, |_| Ok(()), |_| Ok(()))
        .unwrap();
    current.default = CanonicalDefault::Staged;
    assert_eq!(
        store
            .advance_canonical(|c, i| c.reconcile(i, &current))
            .unwrap(),
        CanonicalAction::RebootStaged
    );
    fixture.retained();
    current.boot.boot_id = "dd606400-dc75-446e-b26b-385592b0f562".into();
    assert_eq!(
        store
            .advance_canonical(|c, i| c.reconcile(i, &current))
            .unwrap(),
        CanonicalAction::NeedsOperator
    );
    assert!(store.advance(|r| r.reconcile(&current.boot)).is_err());
}

#[test]
fn copy_completion_flush_failures_preserve_intent_and_never_rewrite_completed_bytes() {
    for point in ["write", "file-sync", "rename", "directory-sync"] {
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        let path = "kernel8.img";
        store.advance_canonical(|c, _| c.begin_file(path)).unwrap();
        let expected = fixture.plan.files[path].clone();
        fixture
            .files(&store)
            .replace(
                path,
                &expected,
                &mut File::open(fixture.sources.join(path)).unwrap(),
                &store,
                &mut |_| Ok(()),
                &mut |_| Ok(()),
            )
            .unwrap();
        let inode = fs::metadata(fixture.boot.join(path)).unwrap().ino();
        store.fault = Some(point);
        assert!(
            store.advance_canonical(|c, _| c.finish_file(path)).is_err(),
            "{point}"
        );
        assert!(store.durable().is_err());
        drop(store);
        let mut store = fixture.fixture.open();
        fixture.prepare(&mut store, |_| Ok(())).unwrap();
        assert_eq!(fs::metadata(fixture.boot.join(path)).unwrap().ino(), inode);
        fixture.retained();
    }
}

#[test]
fn migration_intent_and_reboot_consumption_must_be_durable_before_actions() {
    for point in ["write", "file-sync", "rename", "directory-sync"] {
        let migration = Migration::new();
        let fixture = Fixture::new();
        let (mut store, _) = fixture.promoted();
        let boot = confirm_normal(&mut store, &fixture);
        store.fault = Some(point);
        assert!(
            store
                .advance(|r| r.start_canonical(migration.plan.clone(), &boot))
                .is_err()
        );
        assert!(store.durable().is_err());
        drop(store);
        let mut store = fixture.open();
        let published = matches!(point, "rename" | "directory-sync");
        assert_eq!(store.canonical_phase().is_some(), published);
        if published {
            assert!(store.advance(|r| r.reconcile(&boot)).is_err());
        }
        let fixture = Migration::new();
        let mut store = fixture.fixture.open();
        let selected = fixture.selected(&mut store);
        store.fault = Some(point);
        assert!(
            store
                .advance_canonical(|c, i| c.request_reboot(i, &selected))
                .is_err()
        );
        assert!(store.durable().is_err());
        drop(store);
        let mut store = fixture.fixture.open();
        assert_eq!(
            store
                .canonical()
                .unwrap()
                .authorize_reboot(&fixture.fixture.inputs, &selected, false)
                .is_ok(),
            published
        );
        if published {
            assert!(
                store
                    .advance_canonical(|c, i| c.request_reboot(i, &selected))
                    .is_err()
            );
        }
    }
}

#[test]
fn canonical_intent_is_immutable_and_an_unhealthy_partial_stage_stays_on_stage() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    assert!(
        store
            .advance(|r| {
                r.canonical = None;
                Ok(())
            })
            .is_err()
    );
    assert!(
        store
            .advance_canonical(|c, _| {
                c.plan.config.push_str("changed");
                Ok(())
            })
            .is_err()
    );
    assert!(
        fixture
            .prepare(&mut store, |p| if p == "canonical-file-intent" {
                Err(invalid("stop"))
            } else {
                Ok(())
            })
            .is_err()
    );
    let pending = store.canonical().unwrap().pending.clone();
    assert_eq!(
        store
            .advance_canonical(|c, i| c.fail(
                i,
                &fixture.normal,
                CanonicalFailure::StagedHealthTimeout
            ))
            .unwrap(),
        CanonicalAction::NeedsOperator
    );
    assert_eq!(store.canonical().unwrap().pending, pending);
    fixture
        .files(&store)
        .verify(store.canonical().unwrap())
        .unwrap();
    assert!(fixture.prepare(&mut store, |_| Ok(())).is_err());
    for reason in [None, Some(CanonicalFailure::RebootRequestFailed)] {
        assert!(
            store
                .advance_canonical(|c, _| {
                    c.failure = reason;
                    if reason.is_none() {
                        c.phase = CanonicalPhase::Preparing;
                    }
                    Ok(())
                })
                .is_err()
        );
    }
}

#[test]
fn canonical_consumed_reboot_and_completed_files_cannot_be_reset() {
    let fixture = Migration::new();
    let mut store = fixture.fixture.open();
    let selected = fixture.selected(&mut store);
    store
        .advance_canonical(|c, i| c.request_reboot(i, &selected))
        .unwrap();
    assert!(
        store
            .advance_canonical(|c, _| {
                let mut value = serde_json::to_value(&*c)?;
                value["reboot_requested"] = false.into();
                *c = serde_json::from_value(value)?;
                Ok(())
            })
            .is_err()
    );
    assert!(
        store
            .advance_canonical(|c, _| {
                c.phase = CanonicalPhase::Failed;
                c.failure = Some(CanonicalFailure::RebootRequestFailed);
                c.completed.clear();
                Ok(())
            })
            .is_err()
    );
    assert_eq!(store.canonical_phase(), Some(CanonicalPhase::Selected));
    assert!(
        store
            .advance_canonical(|c, i| c.request_reboot(i, &selected))
            .is_err()
    );
    fixture.retained();
}

#[test]
fn canonical_plan_rejects_fat_case_and_directory_collisions_before_copying() {
    let fixture = Migration::new();
    for path in [
        "KERNEL8.IMG",
        "SPACEWARS-PROFILES/extra.txt",
        "spacewars-profiles",
    ] {
        let mut inputs = fixture.fixture.inputs.clone();
        inputs
            .binding
            .original_boot_files
            .insert(path.into(), blob(b"retained"));
        inputs.validate().unwrap();
        assert!(fixture.plan.validate(&inputs).is_err(), "{path}");
    }
    fixture.retained();
}
