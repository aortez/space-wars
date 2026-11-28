use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU32, Ordering},
};

#[test]
fn interrupted_generation_resumes_with_the_same_rankings_and_records_boundary_edits() {
    let root = tempfile::tempdir().unwrap();
    let config = TrainerConfig {
        population: 2,
        tournament: 2,
        cases_per_generation: 1,
        generations: 2,
        workers: 2,
        ..Default::default()
    };
    let path = root.path().join("interrupted");
    let mut job = Trainer::new(config.clone(), path.clone()).unwrap();
    assert!(Trainer::resume(path.clone()).is_err());
    let changed = TrainerConfig {
        population: 3,
        mutation_scale: 0.31,
        ..config.clone()
    };
    let applied = AtomicU32::new(0);
    let first = job
        .run_generation_with(|_| {
            applied.store(
                Trainer::submit_edit(&path, changed.clone()).unwrap(),
                Ordering::Relaxed,
            );
        })
        .unwrap();
    assert_eq!(applied.load(Ordering::Relaxed), 2);
    assert_eq!(first.config, config);
    drop(job);
    // A torn append following the committed manifest is ignored on reopen.
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(path.join("generations.jsonl"))
        .unwrap()
        .write_all(b"partial")
        .unwrap();
    let mut resumed = Trainer::resume(path.clone()).unwrap();
    assert_eq!(resumed.pending(), Some(&changed));
    assert_eq!(resumed.latest().unwrap().best.visit, first.best.visit);
    let previous_population = resumed.state.population.clone();
    let workers = resumed.worker_control();
    let second = resumed
        .run_generation_observed(
            |generation, settings| {
                assert_eq!(generation, 2);
                let stats = workers.snapshot();
                assert_eq!(stats.batch_completed, 0);
                assert_eq!(
                    stats.queued,
                    settings.population * settings.cases_per_generation
                );
            },
            |sample| {
                assert!(!previous_population.contains(&sample.genome_id));
                let genome = Trainer::read_genome(&path, &sample.genome_id).unwrap();
                assert_eq!(genome.id(), sample.genome_id);
            },
        )
        .unwrap();
    assert_eq!(workers.snapshot().batch_completed, 3);
    assert_eq!(second.config, changed);
    let history = resumed.history().unwrap();
    assert_eq!(history[0].config, config);
    assert_eq!(history[1].config, changed);
    let mut reference = Trainer::new(config, root.path().join("continuous")).unwrap();
    reference.run_generation().unwrap();
    reference.queue_edit(changed).unwrap();
    let expected = reference.run_generation().unwrap();
    assert_eq!(second.best_genome, expected.best_genome);
    assert_eq!(second.best.visit, expected.best.visit);
    assert_eq!(second.mean_terms, expected.mean_terms);
    assert_eq!(resumed.state.population, reference.state.population);
    assert!(Trainer::submit_edit(&path, resumed.config().clone()).is_err());
    drop(resumed);
    let champion = path
        .join("genomes")
        .join(format!("{}.json", second.best_genome));
    fs::rename(&champion, root.path().join("missing")).unwrap();
    assert!(
        Trainer::resume(path)
            .err()
            .unwrap()
            .contains(&second.best_genome)
    );
}

#[test]
fn concurrent_sparse_edits_preserve_both_changes_and_validate_the_pending_population() {
    let root = tempfile::tempdir().unwrap();
    let config = TrainerConfig {
        population: 2,
        tournament: 2,
        workers: 1,
        ..Default::default()
    };
    let _job = Trainer::new(config.clone(), root.path().to_owned()).unwrap();
    let ready = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let population = threads.spawn(|| {
            ready.wait();
            Trainer::modify_edit(root.path(), |settings| {
                settings.population = 4;
                Ok(())
            })
        });
        let mutation = threads.spawn(|| {
            ready.wait();
            Trainer::modify_edit(root.path(), |settings| {
                settings.mutation_scale = 0.3;
                Ok(())
            })
        });
        assert_eq!(population.join().unwrap().unwrap(), 1);
        assert_eq!(mutation.join().unwrap().unwrap(), 1);
    });
    assert_eq!(
        Trainer::read_config(root.path()).unwrap(),
        TrainerConfig {
            population: 4,
            mutation_scale: 0.3,
            ..config.clone()
        }
    );
    Trainer::modify_edit(root.path(), |settings| {
        settings.tournament = 4;
        Ok(())
    })
    .unwrap();
    assert!(
        Trainer::modify_edit(root.path(), |settings| {
            settings.population = 3;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(
        Trainer::read_config(root.path()).unwrap(),
        TrainerConfig {
            population: 4,
            mutation_scale: 0.3,
            tournament: 4,
            ..config
        }
    );
}

#[test]
fn latest_visit_replaces_unread_completions_and_archives_the_exact_genome() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    let config = TrainerConfig {
        population: 2,
        tournament: 2,
        cases_per_generation: 2,
        generations: 1,
        workers: 1,
        ..Default::default()
    };
    let last_case = *duck_recipe::training_cases(
        config.seed,
        1,
        &config.case_mix,
        config.cases_per_generation,
    )
    .unwrap()
    .last()
    .unwrap();
    let mut job = Trainer::new(config, dir.into()).unwrap();
    let last_genome = job.population[1].id().to_owned();
    let mut feed = None;
    let record = job
        .run_generation_live(|_, _, latest| {
            assert!(latest.take().is_none());
            feed = Some(latest);
        })
        .unwrap();
    // A single worker finishes in input order. Even when the viewer never polls,
    // it receives the last case of the last candidate, with the same weights.
    let feed = feed.unwrap();
    let latest = feed.take().unwrap();
    assert!(feed.take().is_none());
    assert_eq!(latest.case, last_case);
    assert_ne!(record.current.case, last_case);
    assert_eq!(latest.genome_id, last_genome);
    let genome = latest.live_genome().unwrap();
    assert_eq!(genome.id(), last_genome);
    assert_eq!(
        duck_recipe::run_model(last_case, &genome, true).unwrap(),
        latest.visit
    );
    let saved = Trainer::read_replay(dir, Some(1)).unwrap().0;
    let archived = saved.last_finished.unwrap();
    assert_eq!(archived.visit, latest.visit);
    assert_eq!(archived.genome_id, last_genome);
    assert!(archived.live_genome().is_none());
    assert_eq!(
        Trainer::read_genome(dir, &archived.genome_id).unwrap().id(),
        last_genome
    );
}

#[test]
fn champion_entrances_match_its_visits_and_older_replays_can_omit_counts() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    let config = TrainerConfig {
        seed: 57,
        population: 2,
        tournament: 2,
        cases_per_generation: 3,
        generations: 1,
        workers: 1,
        ..Default::default()
    };
    let mut job = Trainer::new(config, dir.into()).unwrap();
    let record = job.run_generation().unwrap();
    let expected = [[0, 1], [2, 2]];
    assert_eq!(record.door_exits, Some(expected));
    let saved = Trainer::read_replay(dir, Some(1)).unwrap().0;
    assert_eq!(saved.door_exits, Some(expected));
    assert!(saved.last_finished.is_some());

    let without_counts = |mut json: serde_json::Value| {
        json.as_object_mut().unwrap().remove("door_exits").unwrap();
        json.as_object_mut().unwrap().remove("last_finished");
        serde_json::to_vec(&json).unwrap()
    };
    let replay = without_counts(serde_json::to_value(&record).unwrap());
    fs::write(dir.join("replays/1.json"), replay).unwrap();
    let mut history = without_counts(serde_json::to_value(record.summary()).unwrap());
    history.push(b'\n');
    fs::write(dir.join("generations.jsonl"), &history).unwrap();
    job.state.history_bytes = history.len() as u64;
    job.state.save(dir).unwrap();
    drop(job);
    let (old, history) = Trainer::read_replay(dir, Some(1)).unwrap();
    assert!(old.last_finished.is_none());
    assert_eq!(old.current.visit, record.current.visit);
    assert!(old.door_exits.is_none() && history[0].door_exits.is_none());
    assert_eq!(
        Trainer::reconstruct_door_exits(dir, old.summary()).unwrap(),
        expected
    );
    let mut mismatched = history[0].clone();
    mismatched.best_terms[4] += 1.;
    assert!(
        Trainer::reconstruct_door_exits(dir, &mismatched)
            .unwrap_err()
            .contains("fitness terms")
    );
    assert!(
        serde_json::to_value(old.summary())
            .unwrap()
            .get("door_exits")
            .is_none()
    );
}
