use super::*;
use crate::{
    brain_assembly,
    genome_library::{self, Practice},
    training_job::{JobSpec, JobState, Recipe, TrainingManager},
};
use std::{
    thread,
    time::{Duration, Instant},
};

fn wait(control: &crate::training_job::JobControl, state: JobState) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while control.snapshot().state != state {
        assert!(Instant::now() < deadline, "{:?}", control.snapshot());
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn completed_and_incomplete_runs_survive_restart_and_bad_neighbors() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let manager = TrainingManager::new(root.into()).unwrap();
    let control = manager.control();
    wait(&control, JobState::Ready);
    control
        .start(JobSpec {
            recipe: Recipe::DiagnosticLine,
            seed: 3,
            population: 4,
            generations: 1,
            initial_genomes: None,
        })
        .unwrap();
    wait(&control, JobState::Completed);
    let a = control.snapshot();
    control
        .start(JobSpec {
            recipe: Recipe::MixedArrival,
            population: 64,
            generations: 64,
            ..JobSpec::default()
        })
        .unwrap();
    // Pause only through the explicit live control; history itself has none.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !matches!(
        control.snapshot().state,
        JobState::Running | JobState::Validating | JobState::Saving
    ) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    control.pause().unwrap();
    wait(&control, JobState::Paused);
    let b = control.snapshot();
    drop(manager);
    fs::write(root.join("run-0.json"), b"damaged").unwrap();
    let keys = catalog(root, false).unwrap().keys;
    assert_eq!(keys.len(), 3);
    let akey = keys
        .iter()
        .find(|k| Some(&k.id) == a.run_id.as_ref())
        .unwrap();
    let bkey = keys
        .iter()
        .find(|k| Some(&k.id) == b.run_id.as_ref())
        .unwrap();
    let Record::Run(run) = inspect(root, akey).unwrap() else {
        panic!()
    };
    assert!(run.complete);
    assert_eq!(run.evaluated_candidates, 4);
    assert_eq!(run.winners[0].genome_id, a.best_id.unwrap());
    let Record::Run(run) = inspect(root, bkey).unwrap() else {
        panic!()
    };
    assert!(!run.complete);
    assert_eq!(run.evaluated_candidates, b.evaluated_candidates);
    assert!(inspect(root, keys.iter().find(|k| k.id == "run-0").unwrap()).is_err());
    let latest = fs::read(root.join("latest")).unwrap();
    let bytes = fs::read(root.join(format!("{}.json", bkey.id))).unwrap();
    for key in &keys {
        let _ = inspect(root, key);
    }
    assert_eq!(fs::read(root.join("latest")).unwrap(), latest);
    assert_eq!(
        fs::read(root.join(format!("{}.json", bkey.id))).unwrap(),
        bytes
    );
    let restarted = TrainingManager::new(root.into()).unwrap();
    wait(&restarted.control(), JobState::Paused);
    assert_eq!(restarted.control().snapshot().run_id, b.run_id);
}

#[test]
fn receipt_identity_and_measurements_survive_missing_artifacts_but_not_tampering() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let entry = genome_library::catalog(&root.join("genomes"))
        .unwrap()
        .entries
        .remove(0);
    let trial =
        genome_library::practice(&root.join("trials"), &entry, Practice::Stationary).unwrap();
    let assembly = brain_assembly::create(root, &entry.id, "History fixture").unwrap();
    let brain_trial = brain_assembly::practice(root, &assembly.id, Practice::Stationary).unwrap();
    let catalog = catalog(root, true).unwrap();
    assert_eq!(catalog.keys.len(), 2);
    fs::remove_file(
        root.join("assemblies")
            .join(format!("{}.json", assembly.id)),
    )
    .unwrap();
    for key in &catalog.keys {
        let Record::Trial(record) = inspect(root, key).unwrap() else {
            panic!()
        };
        assert_eq!(record.brain.controller, entry.id);
        assert_eq!(record.elapsed_ticks, trial.episode.elapsed_ticks);
        assert_eq!(record.measure.distance, trial.episode.measure.distance);
        assert_eq!(record.assembly.is_some(), key.kind == Kind::AssemblyTrial);
    }
    fs::write(brain_trial.receipt, b"{}").unwrap();
    assert!(
        inspect(
            root,
            catalog
                .keys
                .iter()
                .find(|k| k.kind == Kind::AssemblyTrial)
                .unwrap()
        )
        .unwrap_err()
        .0
        .contains("checksum")
    );
    assert!(
        inspect(
            root,
            &Key {
                kind: Kind::GenomeTrial,
                id: "../outside".into(),
                receipt: "0".repeat(64)
            }
        )
        .is_err()
    );
}

#[test]
fn comparisons_require_exact_domain_build_and_physical_protocol() {
    let case = Practice::Stationary.case();
    let a = Protocol::new("practice", "build-a", &case).unwrap();
    assert_eq!(a.mismatch(&a), None);
    assert!(
        a.mismatch(&Protocol::new("validation", "build-a", &case).unwrap())
            .is_some()
    );
    assert!(
        a.mismatch(&Protocol::new("practice", "build-b", &case).unwrap())
            .is_some()
    );
    let mut changed = case.clone();
    changed.seed += 1;
    assert!(
        a.mismatch(&Protocol::new("practice", "build-a", &changed).unwrap())
            .is_some()
    );
    changed = case;
    changed.max_ticks -= 1;
    assert!(
        a.mismatch(&Protocol::new("practice", "build-a", &changed).unwrap())
            .is_some()
    );
}

#[test]
fn scan_limit_is_explicit_and_files_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    for i in 0..MAX_RECORDS + 1 {
        fs::write(
            directory.path().join(format!("run-{i}.json")),
            b"not decoded during discovery",
        )
        .unwrap();
    }
    let catalog = catalog(directory.path(), false).unwrap();
    assert!(catalog.truncated);
    assert_eq!(catalog.keys.len(), MAX_RECORDS);
    assert!(!catalog.warnings.is_empty());
    assert_eq!(
        fs::read_dir(directory.path()).unwrap().count(),
        MAX_RECORDS + 1
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_do_not_redirect_history_reads() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("record"), b"secret").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("record"),
        directory.path().join("run-1.json"),
    )
    .unwrap();
    let keys = catalog(directory.path(), false).unwrap().keys;
    assert!(
        inspect(directory.path(), &keys[0])
            .unwrap_err()
            .0
            .contains("regular")
    );
}
