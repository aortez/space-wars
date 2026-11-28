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
    let second = resumed.run_generation().unwrap();
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
