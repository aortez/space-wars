//! Historical decoding shares integrity checks, never the live controller.
use super::*;
use crate::{
    evolution::TaskManifest,
    training_history::{RunRecord, Winner},
};

pub(crate) fn inspect(root: &Path, id: &str) -> Result<RunRecord, EvolutionError> {
    if !valid_id(id) {
        return Err(error("invalid training run identity"));
    }
    let path = root.join(format!("{id}.json"));
    let bytes = crate::training_history::read_regular(&path, MAX_JOB_BYTES)?;
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(error)?;
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&envelope.job).map_err(error)?)
    );
    if envelope.version != JOB_VERSION || envelope.sha256 != digest || envelope.job.id != id {
        return Err(error("saved training job version or checksum differs"));
    }
    let saved = envelope.job;
    let run = match saved.spec.recipe {
        Recipe::StationaryArrival => {
            Run::Stationary(EvolutionRun::inspect(saved.checkpoint.as_bytes())?)
        }
        Recipe::MixedArrival => Run::Mixed(EvolutionRun::inspect(saved.checkpoint.as_bytes())?),
        Recipe::DiagnosticLine => Run::Line(EvolutionRun::inspect(saved.checkpoint.as_bytes())?),
    };
    struct Description {
        runtime: String,
        config: EvolutionConfig,
        task: TaskManifest,
        initial_genomes: Vec<String>,
        winners: Vec<Winner>,
    }
    fn describe<F: ProgressFitness>(run: &EvolutionRun<F>) -> Result<Description, EvolutionError> {
        Ok(Description {
            runtime: run.runtime().into(),
            config: run.config().clone(),
            task: run.task().clone(),
            initial_genomes: run
                .seeds()
                .iter()
                .map(|g| {
                    DenseModel::new(g.clone())
                        .map(|m| m.id().to_owned())
                        .map_err(error)
                })
                .collect::<Result<_, _>>()?,
            winners: run
                .history()
                .iter()
                .map(|h| Winner {
                    generation: h.generation + 1,
                    genome_id: h.best_id.clone(),
                    fitness: h.best_fitness.terms(),
                    arrivals: h.best_fitness.arrivals(),
                })
                .collect(),
        })
    }
    let Description {
        runtime,
        config,
        task,
        initial_genomes,
        winners,
    } = match &run {
        Run::Stationary(r) => describe(r)?,
        Run::Mixed(r) => describe(r)?,
        Run::Line(r) => describe(r)?,
    };
    if config.seed != saved.spec.seed
        || config.population != saved.spec.population
        || config.generations != saved.spec.generations
    {
        return Err(error("saved configuration differs from job specification"));
    }
    if let Some(seeds) = &saved.spec.initial_genomes {
        let ids: Vec<_> = seeds
            .iter()
            .map(|g| {
                DenseModel::new(g.clone())
                    .map(|m| m.id().to_owned())
                    .map_err(error)
            })
            .collect::<Result<_, _>>()?;
        if ids != initial_genomes {
            return Err(error("saved initial genomes differ from job specification"));
        }
    }
    let mut previous = 0;
    for v in &saved.validation {
        // Historical cases may differ from today's monitoring suite. Validate
        // their recorded measurements and exact generation link, not today's recipe.
        if v.generation <= previous
            || run.best_id_at(v.generation) != Some(&v.genome_id)
            || v.suite.is_empty()
            || v.cases.is_empty()
            || v.cases.len() > 128
            || v.cases.iter().any(|c| {
                !crate::training_history::valid_measurements(
                    &c.case,
                    c.elapsed_ticks,
                    c.distance,
                    c.relative_speed,
                )
            })
        {
            return Err(error("invalid historical validation result"));
        }
        previous = v.generation;
    }
    let mut progress = JobSnapshot::default();
    run.progress(&mut progress);
    let compatibility = Run::decode(&saved.spec, saved.checkpoint.as_bytes())
        .err()
        .map(|e| e.0);
    Ok(RunRecord {
        id: saved.id,
        recipe: saved.spec.recipe,
        selected_initial_genomes: saved.spec.initial_genomes.is_some(),
        runtime,
        config,
        task,
        initial_genomes,
        complete: run.complete(),
        evaluated_candidates: progress.evaluated_candidates,
        evaluation_millis: saved.evaluation_millis,
        progress: progress.history,
        winners,
        validation: saved.validation,
        compatibility,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_build_is_visible_but_cannot_be_restored_as_the_current_job() {
        let root = tempfile::tempdir().unwrap();
        let mut job = Job::new(JobSpec {
            recipe: Recipe::DiagnosticLine,
            population: 4,
            generations: 1,
            ..JobSpec::default()
        })
        .unwrap();
        while !job.run.complete() {
            job.advance().unwrap();
        }
        job.save(root.path()).unwrap();
        let path = root.path().join(format!("{}.json", job.id));
        let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut checkpoint: serde_json::Value =
            serde_json::from_str(&envelope.job.checkpoint).unwrap();
        checkpoint["run"]["runtime"] = "retained/old/build".into();
        let old: EvolutionRun<LineFitness> =
            serde_json::from_value(checkpoint["run"].clone()).unwrap();
        envelope.job.checkpoint = String::from_utf8(old.encode().unwrap()).unwrap();
        envelope.sha256 = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&envelope.job).unwrap())
        );
        fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        fs::write(root.path().join("latest"), &job.id).unwrap();
        let record = inspect(root.path(), &job.id).unwrap();
        assert_eq!(record.runtime, "retained/old/build");
        assert!(record.compatibility.unwrap().contains("runtime differs"));
        assert_eq!(record.winners.len(), 1);
        assert!(Job::load_latest(root.path()).is_err());
    }
}
