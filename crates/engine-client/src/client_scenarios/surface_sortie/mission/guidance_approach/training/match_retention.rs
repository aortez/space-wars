//! Preserve inspected match successes before rewarding additional approaches.
use super::*;
use std::collections::BTreeSet;

mod focused;
mod graded;

const RECIPE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-match-retention-v1-plan.json"
));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedFitness {
    precision_completed: u32,
    protected_completed: u32,
    additional_completed: u32,
    approach_speed_points: u32,
    approach_position_points: u32,
    precision_position_points: u32,
    precision_velocity_points: u32,
    spare_ticks: u32,
}

impl RetainedFitness {
    fn from_quality(quality: Fitness, protected_completed: u32) -> Self {
        Self {
            precision_completed: quality.retained_completed,
            protected_completed,
            additional_completed: quality.approach_completed - protected_completed,
            approach_speed_points: quality.approach_speed_points,
            approach_position_points: quality.approach_position_points,
            precision_position_points: quality.retained_position_points,
            precision_velocity_points: quality.retained_velocity_points,
            spare_ticks: quality.spare_ticks,
        }
    }
}

struct Search<'a, 'b> {
    task: &'a TrainingTask<'b>,
    protected: BTreeSet<String>,
}

impl EvaluationTask for Search<'_, '_> {
    type Fitness = RetainedFitness;

    fn manifest(&self) -> TaskManifest {
        self.task.manifest()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<RetainedFitness, EvolutionError> {
        self.evaluate_progress(model, false)
    }
}

impl Search<'_, '_> {
    fn evaluate_progress(
        &self,
        model: &DenseModel,
        remaining_only: bool,
    ) -> Result<RetainedFitness, EvolutionError> {
        let mut quality = Fitness::default();
        for result in self.task.pool.map(&self.task.retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false)
        }) {
            let result = result?;
            quality.retained_completed += result.fitness.completed;
            quality.retained_position_points += result.fitness.position_points;
            quality.retained_velocity_points += result.fitness.velocity_points;
            quality.spare_ticks += result.fitness.spare_ticks;
        }
        let mut protected_completed = 0;
        let mut remaining = Fitness::default();
        for result in self.task.pool.map(self.task.sources, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, None)
        }) {
            protected_completed += u32::from(
                self.protected.contains(&result.case) && result.outcome == Outcome::Arrived,
            );
            quality.record_approach(&result);
            if !self.protected.contains(&result.case) {
                remaining.record_approach(&result);
            }
        }
        let mut score = RetainedFitness::from_quality(quality, protected_completed);
        if remaining_only {
            score.approach_speed_points = remaining.approach_speed_points;
            score.approach_position_points = remaining.approach_position_points;
        }
        Ok(score)
    }
}

fn train<T: EvaluationTask>(
    search: &T,
    seeds: &[DenseModel],
    plan: &Value,
    out: &Path,
    resume: bool,
    pool: &EpisodePool,
) where
    T::Fitness: std::fmt::Debug,
{
    let path = out.join("checkpoint.json");
    let genomes: Vec<_> = seeds.iter().map(|s| s.genome().clone()).collect();
    let mut run = if resume {
        EvolutionRun::load(&path, search).unwrap()
    } else {
        EvolutionRun::new(
            search,
            genomes[0].clone(),
            genomes.clone(),
            configuration(plan),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &configuration(plan));
    assert_eq!(run.seeds(), genomes);
    run.save(&path).unwrap();
    let steps = std::env::var("SPACEWARS_RETENTION_STEPS")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(usize::MAX);
    let started = std::time::Instant::now();
    let initial_generations = run.history().len();
    for _ in 0..steps {
        if !run.advance(search).unwrap() {
            break;
        }
        run.save(&path).unwrap();
        save(out.join("workers.json"), &pool.control().snapshot());
        if run.complete() || run.fitness().iter().all(Option::is_none) {
            eprintln!(
                "{}",
                json!({"generations":run.history().len(),
                "complete":run.complete(),"seconds":started.elapsed().as_secs_f64(),
                "best":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, search).unwrap());
    if run.complete() {
        let winner = run.history().last().unwrap();
        let model = DenseModel::new(winner.best_genome.clone()).unwrap();
        save(out.join("finalist.json"), model.genome());
        save(
            out.join("selection.json"),
            &json!({"genome":model.id(),
            "fitness":winner.best_fitness,"checkpoint_sha256":hash(&fs::read(&path).unwrap())}),
        );
    }
    save(
        out.join("timing.json"),
        &json!({"initial_generations":initial_generations,
        "completed_generations":run.history().len(),"complete":run.complete(),
        "evaluation_and_save_seconds":started.elapsed().as_secs_f64()}),
    );
}

fn comparison(
    search: &Search<'_, '_>,
    seeds: &[DenseModel],
    plan: &Value,
    reserved: &[Source],
    missing: &[Value],
    out: &Path,
) {
    let directory = std::env::var_os("SPACEWARS_RETENTION_RUN").unwrap();
    let directory = Path::new(&directory);
    let run = EvolutionRun::load(&directory.join("checkpoint.json"), search).unwrap();
    assert!(run.complete(), "select before reserved continuations");
    assert_eq!(run.config(), &configuration(plan));
    assert_eq!(
        run.seeds(),
        seeds.iter().map(|s| s.genome().clone()).collect::<Vec<_>>()
    );
    let winner = run.history().last().unwrap();
    let finalist = DenseModel::new(winner.best_genome.clone()).unwrap();
    assert_eq!(
        finalist.id(),
        DenseModel::from_json(&fs::read(directory.join("finalist.json")).unwrap())
            .unwrap()
            .id()
    );
    assert_eq!(search.evaluate(&finalist).unwrap(), winner.best_fitness);
    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("mission", "mission", &seeds[1]),
        ("reference", "reference", &seeds[1]),
        ("baseline", "model", &seeds[1]),
        ("candidate", "model", &seeds[0]),
        ("finalist", "model", &finalist),
    ] {
        let mut groups = Vec::new();
        for partition in ["protected", "additional", "validation"] {
            let cases: Vec<_> = if partition == "validation" {
                reserved.iter().collect()
            } else {
                search
                    .task
                    .sources
                    .iter()
                    .filter(|(c, _)| {
                        search.protected.contains(&c.name) == (partition == "protected")
                    })
                    .collect()
            };
            let traces = out.join(format!("{label}-{partition}"));
            fs::create_dir(&traces).unwrap();
            let episodes = search.task.pool.map(&cases, |(case, source)| {
                episode(&source.lock().unwrap(), case, driver, model, Some(&traces))
            });
            let arrived = episodes
                .iter()
                .filter(|e| e.outcome == Outcome::Arrived)
                .count();
            eprintln!("{label} {partition}: {arrived}/{}", episodes.len());
            groups.push(json!({"partition":partition,"arrived":arrived,"episodes":episodes}));
        }
        let original = (driver != "mission").then(|| {
            ship_curriculum::evaluate(if driver == "reference" {
                Policy::Reference
            } else {
                Policy::Model(model.clone())
            })
            .unwrap()
        });
        let passed = missing.is_empty()
            && reserved.len() == 16
            && groups.iter().all(|g| {
                g["arrived"].as_u64().unwrap() as usize == g["episodes"].as_array().unwrap().len()
            })
            && original.as_ref().is_some_and(|o| o.passed);
        let settling = original
            .as_ref()
            .map(|o| full_retention::settling(o, plan, model, driver, label, out));
        reports.push(
            json!({"controller":label,"genome":if driver=="model" {Some(model.id())} else {None},
            "approaches":groups,"original_arrival":original,"settling":settling,"passed":passed}),
        );
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(RECIPE),"missing_validation":missing,"controllers":reports,
            "production_controls_changed":false}),
        );
    }
}

#[test]
#[ignore = "frozen match-retention experiment; explicit mode and fresh output required"]
fn learned_guidance_match_retention() {
    let plan: Value = serde_json::from_slice(RECIPE).unwrap();
    let mode = std::env::var("SPACEWARS_RETENTION_MODE").unwrap();
    assert!(["preflight", "train", "resume", "evaluate"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_RETENTION_OUT").unwrap();
    let out = Path::new(&out);
    if mode != "resume" {
        fs::create_dir(out).expect("retain previous evidence");
    }
    let seed_path = std::env::var_os("SPACEWARS_RETENTION_SEEDS").unwrap();
    let seeds: Vec<_> = plan["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seed| {
            let path =
                Path::new(&seed_path).join(format!("{}.json", seed["label"].as_str().unwrap()));
            let bytes = fs::read(path).unwrap();
            assert_eq!(hash(&bytes), seed["file_sha256"]);
            let model = DenseModel::from_json(&bytes).unwrap();
            assert_eq!(model.id(), seed["genome"].as_str().unwrap());
            model
        })
        .collect();
    assert_eq!(seeds.len(), 2);
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let (reserved, missing) = validation(&plan);
    let retained = retention_cases(&plan);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    assert!(
        protected
            .iter()
            .all(|name| training.iter().any(|(c, _)| &c.name == name))
    );
    let receipt = json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(RECIPE),"seeds":seeds.iter().map(DenseModel::id).collect::<Vec<_>>(),
        "training":records(&training),"protected_cases":protected,
        "validation":records(&reserved),"missing_validation":missing,"retention_cases":retained});
    let receipt = serde_json::to_vec_pretty(&receipt).unwrap();
    if mode != "preflight" {
        let expected =
            fs::read(std::env::var_os("SPACEWARS_RETENTION_PREFLIGHT").unwrap()).unwrap();
        assert_eq!(
            receipt, expected,
            "physical sources or frozen executable changed"
        );
    }
    fs::write(out.join("plan.json"), RECIPE).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    if mode == "preflight" {
        eprintln!(
            "Preflight: {} training ({} protected), {} reserved, {} missing",
            training.len(),
            protected.len(),
            reserved.len(),
            missing.len()
        );
        return;
    }
    let task = TrainingTask {
        sources: &training,
        retained,
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"plan_utf8":std::str::from_utf8(RECIPE).unwrap(),"preflight_sha256":hash(&receipt)}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    if mode == "evaluate" {
        comparison(&search, &seeds, &plan, &reserved, &missing, out);
    } else {
        train(&search, &seeds, &plan, out, mode == "resume", &task.pool);
    }
}

#[cfg(test)]
mod tests;
