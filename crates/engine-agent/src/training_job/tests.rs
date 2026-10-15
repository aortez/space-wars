use super::*;
use std::time::Duration;

fn diagnostic() -> JobSpec {
    JobSpec {
        recipe: Recipe::DiagnosticLine,
        seed: 7,
        population: 4,
        generations: 3,
        initial_genomes: None,
    }
}

fn wait_for(control: &JobControl, condition: impl Fn(&JobSnapshot) -> bool) -> JobSnapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = control.snapshot();
        if condition(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "timed out: {snapshot:?}");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn saved_continuation_crosses_generation_boundary_identically_to_uninterrupted_run() {
    let root = tempfile::tempdir().unwrap();
    let mut job = Job::new(diagnostic()).unwrap();
    assert!(job.snapshot(JobState::Running).best_id.is_none());
    for _ in 0..5 {
        job.advance().unwrap();
    }
    job.save(root.path()).unwrap();
    atomic_write(&root.path().join("latest"), job.id.as_bytes()).unwrap();
    let mut restored = Job::load_latest(root.path()).unwrap().unwrap();
    let paused = restored.snapshot(JobState::Paused);
    assert_eq!(paused.completed_generations, 1);
    assert_eq!(paused.saved_candidates, 5);
    assert_eq!(
        paused.evaluated_episodes,
        5 * Recipe::DiagnosticLine.cases()
    );
    assert_eq!(restored.run.encode().unwrap(), job.run.encode().unwrap());
    while !restored.run.complete() {
        restored.advance().unwrap();
    }
    let mut control = Job::new(diagnostic()).unwrap();
    while !control.run.complete() {
        control.advance().unwrap();
    }
    assert_eq!(
        restored.run.encode().unwrap(),
        control.run.encode().unwrap()
    );
    assert_eq!(
        restored.snapshot(JobState::Completed).evaluated_candidates,
        12
    );
}

#[test]
fn both_ship_recipes_retain_declared_config_and_seed_on_restore() {
    for recipe in [Recipe::StationaryArrival, Recipe::MixedArrival] {
        let root = tempfile::tempdir().unwrap();
        let spec = JobSpec {
            recipe,
            seed: 91,
            population: 4,
            generations: 1,
            initial_genomes: None,
        };
        let mut job = Job::new(spec.clone()).unwrap();
        job.advance().unwrap();
        job.save(root.path()).unwrap();
        atomic_write(&root.path().join("latest"), job.id.as_bytes()).unwrap();
        let restored = Job::load_latest(root.path()).unwrap().unwrap();
        assert_eq!(restored.spec, spec);
        assert_eq!(restored.run.encode().unwrap(), job.run.encode().unwrap());
        assert_eq!(
            restored.snapshot(JobState::Paused).evaluated_episodes,
            recipe.cases()
        );
        assert!(restored.run.best().is_none());
    }
}

#[test]
fn corrupted_metadata_or_changed_configuration_cannot_be_resumed() {
    let root = tempfile::tempdir().unwrap();
    let mut job = Job::new(diagnostic()).unwrap();
    job.save(root.path()).unwrap();
    atomic_write(&root.path().join("latest"), job.id.as_bytes()).unwrap();
    let path = root.path().join(format!("{}.json", job.id));
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope.job.spec.generations += 1;
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(
        Job::load_latest(root.path())
            .err()
            .unwrap()
            .0
            .contains("checksum")
    );
    envelope.sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&envelope.job).unwrap())
    );
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(
        Job::load_latest(root.path())
            .err()
            .unwrap()
            .0
            .contains("configuration")
    );
    atomic_write(&root.path().join("latest"), b"../elsewhere").unwrap();
    assert!(
        Job::load_latest(root.path())
            .err()
            .unwrap()
            .0
            .contains("ID")
    );
}

#[test]
fn worker_restores_paused_and_finishes_only_after_explicit_resume() {
    let root = tempfile::tempdir().unwrap();
    let mut job = Job::new(diagnostic()).unwrap();
    job.advance().unwrap();
    job.save(root.path()).unwrap();
    atomic_write(&root.path().join("latest"), job.id.as_bytes()).unwrap();
    let manager = TrainingManager::new(root.path().into()).unwrap();
    let control = manager.control();
    let paused = wait_for(&control, |s| s.state == JobState::Paused);
    assert_eq!(paused.saved_candidates, 1);
    thread::sleep(Duration::from_millis(30));
    assert_eq!(control.snapshot().evaluated_candidates, 1);
    control.resume().unwrap();
    let completed = wait_for(&control, |s| s.state == JobState::Completed);
    assert_eq!(completed.evaluated_candidates, 12);
    assert_eq!(completed.saved_candidates, 12);
    assert_eq!(completed.history.len(), 3);
    drop(manager);
    let restored = TrainingManager::new(root.path().into()).unwrap();
    let finished = wait_for(&restored.control(), |s| s.state == JobState::Completed);
    assert!(!finished.can_resume);
    assert!(restored.control().resume().is_err());
    assert_eq!(finished.best_id, completed.best_id);
}

#[test]
fn pause_and_stop_are_visible_requests_and_shutdown_keeps_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let manager = TrainingManager::new(root.path().into()).unwrap();
    let control = manager.control();
    wait_for(&control, |s| s.state == JobState::Ready);
    control
        .start(JobSpec {
            recipe: Recipe::MixedArrival,
            population: 4,
            generations: 4,
            ..JobSpec::default()
        })
        .unwrap();
    wait_for(&control, |s| {
        matches!(s.state, JobState::Running | JobState::Saving)
    });
    control.pause().unwrap();
    assert!(matches!(
        control.snapshot().state,
        JobState::PauseRequested | JobState::Paused
    ));
    let paused = wait_for(&control, |s| s.state == JobState::Paused);
    assert_eq!(paused.evaluated_candidates, paused.saved_candidates);
    let before = paused.evaluated_candidates;
    thread::sleep(Duration::from_millis(30));
    assert_eq!(control.snapshot().evaluated_candidates, before);
    control.resume().unwrap();
    wait_for(&control, |s| {
        matches!(s.state, JobState::Running | JobState::Saving)
    });
    control.stop().unwrap();
    let stopped = wait_for(&control, |s| s.state == JobState::Stopped);
    assert!(stopped.can_resume);
    drop(manager);
    let restored = Job::load_latest(root.path()).unwrap().unwrap();
    assert_eq!(
        restored.snapshot(JobState::Paused).saved_candidates,
        stopped.saved_candidates
    );
}

#[test]
fn second_worker_cannot_write_the_same_training_store() {
    let root = tempfile::tempdir().unwrap();
    let first = TrainingManager::new(root.path().into()).unwrap();
    wait_for(&first.control(), |s| s.state == JobState::Ready);
    let second = TrainingManager::new(root.path().into()).unwrap();
    let blocked = wait_for(&second.control(), |s| s.state == JobState::Unavailable);
    assert!(blocked.error.unwrap().contains("already in use"));
    assert!(second.control().start(diagnostic()).is_err());
}

#[test]
fn failed_save_does_not_report_unwritten_progress_as_durable() {
    let root = tempfile::tempdir().unwrap();
    let mut job = Job::new(diagnostic()).unwrap();
    job.save(root.path()).unwrap();
    let path = root.path().join(format!("{}.json", job.id));
    let previous = fs::read(&path).unwrap();
    job.advance().unwrap();
    let invalid_root = root.path().join("not-a-directory");
    fs::write(&invalid_root, b"obstruction").unwrap();
    assert!(job.save(&invalid_root).is_err());
    let failed = job.snapshot(JobState::Failed);
    assert_eq!(failed.evaluated_candidates, 1);
    assert_eq!(failed.saved_candidates, 0);
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert!(failed.can_resume);
}

#[test]
fn selected_genome_seeds_a_fresh_run_and_survives_exact_resume() {
    let root = tempfile::tempdir().unwrap();
    let entry = genome_library::catalog(&root.path().join("genomes"))
        .unwrap()
        .entries
        .remove(1);
    let spec = JobSpec {
        recipe: Recipe::MixedArrival,
        seed: 12,
        population: 4,
        generations: 2,
        initial_genomes: Some(vec![entry.genome.clone()]),
    };
    let mut job = Job::new(spec.clone()).unwrap();
    if let Run::Mixed(run) = &job.run {
        assert_eq!(run.seeds(), [entry.genome]);
        assert_eq!(
            DenseModel::new(run.population()[0].genome.clone())
                .unwrap()
                .id(),
            entry.id
        );
        assert!(run.fitness().iter().all(Option::is_none));
    } else {
        panic!("expected mixed run");
    }
    job.advance().unwrap();
    job.save(root.path()).unwrap();
    atomic_write(&root.path().join("latest"), job.id.as_bytes()).unwrap();
    let mut restored = Job::load_latest(root.path()).unwrap().unwrap();
    while !restored.run.complete() {
        restored.advance().unwrap();
    }
    let mut control = Job::new(spec).unwrap();
    while !control.run.complete() {
        control.advance().unwrap();
    }
    assert_eq!(
        restored.run.encode().unwrap(),
        control.run.encode().unwrap()
    );
}
