//! Compare full-network evolution with a frozen network's brake readout search.
use super::*;
use spacewars_ai::model::{Activation, DenseGenome, DenseLayer, ModelContract};

const RECIPE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-brake-readout-v1-plan.json"
));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    All,
    Brake,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Brake => "brake",
        }
    }

    fn seed(self, baseline: &DenseModel) -> DenseModel {
        if self == Self::All {
            return baseline.clone();
        }
        let genome = baseline.genome();
        assert_eq!(genome.layers.len(), 2);
        assert_eq!(genome.layers[0].outputs, 8);
        assert_eq!(genome.layers[1].outputs, 3);
        assert_eq!(genome.layers[1].activation, Activation::Linear);
        DenseModel::new(DenseGenome {
            version: genome.version,
            memory: genome.memory,
            contract: ModelContract {
                organism: genome.contract.organism.clone(),
                role: "arrival_brake_readout_search_v1".into(),
                observation: "frozen_arrival_hidden_8_v1".into(),
                action: "brake_logit_v1".into(),
                inputs: 8,
                outputs: 1,
                cadence_ticks: 1,
            },
            layers: vec![DenseLayer {
                outputs: 1,
                activation: Activation::Linear,
                weights: genome.layers[1].weights[16..24].to_vec(),
                biases: vec![genome.layers[1].biases[2]],
            }],
        })
        .unwrap()
    }

    fn expand(self, candidate: &DenseModel, baseline: &DenseModel) -> DenseModel {
        assert!(
            candidate
                .genome()
                .same_architecture(self.seed(baseline).genome())
        );
        if self == Self::All {
            return candidate.clone();
        }
        let mut genome = baseline.genome().clone();
        genome.layers[1].weights[16..24].copy_from_slice(&candidate.genome().layers[0].weights);
        genome.layers[1].biases[2] = candidate.genome().layers[0].biases[0];
        DenseModel::new(genome).unwrap()
    }
}

struct Search<'a, 'b> {
    task: &'a TrainingTask<'b>,
    baseline: &'a DenseModel,
    arm: Arm,
}

impl EvaluationTask for Search<'_, '_> {
    type Fitness = Fitness;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = self.task.manifest();
        manifest.id.push_str(self.arm.name());
        manifest.contract = self.arm.seed(self.baseline).genome().contract.clone();
        manifest.configuration["arm"] = json!(self.arm.name());
        manifest.configuration["baseline_genome"] = json!(self.baseline.id());
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Fitness, EvolutionError> {
        self.task.evaluate(&self.arm.expand(model, self.baseline))
    }
}

fn train_arm(search: &Search<'_, '_>, plan: &Value, out: &Path, resume: bool) {
    let seed = search.arm.seed(search.baseline);
    let path = out.join("checkpoint.json");
    let mut run = if resume {
        EvolutionRun::load(&path, search).unwrap()
    } else {
        EvolutionRun::new(
            search,
            seed.genome().clone(),
            vec![seed.genome().clone()],
            configuration(plan),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &configuration(plan));
    assert_eq!(run.seeds(), [seed.genome().clone()]);
    run.save(&path).unwrap();
    let steps = std::env::var("SPACEWARS_BRAKE_STEPS")
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
        save(
            out.join("workers.json"),
            &search.task.pool.control().snapshot(),
        );
        if run.complete() || run.fitness().iter().all(Option::is_none) {
            eprintln!(
                "{}",
                json!({"arm":search.arm.name(),
                "generations":run.history().len(),"complete":run.complete(),
                "seconds":started.elapsed().as_secs_f64(),
                "best":run.history().last().unwrap().best_fitness})
            );
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, search).unwrap());
    if run.complete() {
        let candidate = DenseModel::new(run.history().last().unwrap().best_genome.clone()).unwrap();
        let full = search.arm.expand(&candidate, search.baseline);
        save(out.join("search-finalist.json"), candidate.genome());
        save(out.join("finalist.json"), full.genome());
        save(
            out.join("selection.json"),
            &json!({"arm":search.arm.name(),
            "search_genome":candidate.id(),"full_genome":full.id(),
            "fitness":run.history().last().unwrap().best_fitness,
            "checkpoint_sha256":hash(&fs::read(&path).unwrap())}),
        );
    }
    save(
        out.join("timing.json"),
        &json!({"initial_generations":initial_generations,
        "completed_generations":run.history().len(),"complete":run.complete(),
        "evaluation_and_save_seconds":started.elapsed().as_secs_f64()}),
    );
}

fn capture(task: &TrainingTask<'_>, baseline: &DenseModel, out: &Path) {
    use std::io::Write;
    let precision = out.join("precision");
    fs::create_dir(&precision).unwrap();
    let endpoints = task.pool.map(&task.retained, |case| {
        let mut traced =
            ship_arrival::run_episode(case.clone(), Policy::Model(baseline.clone()), true).unwrap();
        let mut rows = std::io::BufWriter::new(
            fs::File::create(precision.join(format!("{}.jsonl", case.name))).unwrap(),
        );
        for tick in &traced.trace {
            serde_json::to_writer(
                &mut rows,
                &json!({"tick":tick.before.tick,
                "inputs":tick.encoded.inputs,"raw":tick.raw_outputs,"movement":tick.movement,
                "measure":tick.measure}),
            )
            .unwrap();
            rows.write_all(b"\n").unwrap();
        }
        rows.flush().unwrap();
        traced.trace.clear();
        let ordinary =
            ship_arrival::run_episode(case.clone(), Policy::Model(baseline.clone()), false)
                .unwrap();
        assert_eq!(traced, ordinary, "tracing changed a precision episode");
        traced
    });
    let approaches = out.join("approaches");
    fs::create_dir(&approaches).unwrap();
    let matches = task.pool.map(task.sources, |(case, source)| {
        episode(
            &source.lock().unwrap(),
            case,
            "model",
            baseline,
            Some(&approaches),
        )
    });
    save(
        out.join("capture.json"),
        &json!({"baseline":baseline.id(),
        "precision":endpoints,"approaches":matches,"traced_endpoint_parity":true}),
    );
}

fn comparison(
    task: &TrainingTask<'_>,
    plan: &Value,
    baseline: &DenseModel,
    development: &[Source],
    reserved: &[Source],
    missing: &[Value],
    out: &Path,
) {
    let runs = std::env::var_os("SPACEWARS_BRAKE_RUNS").unwrap();
    let mut models = Vec::new();
    for arm in [Arm::All, Arm::Brake] {
        let search = Search {
            task,
            baseline,
            arm,
        };
        let directory = Path::new(&runs).join(arm.name());
        let run = EvolutionRun::load(&directory.join("checkpoint.json"), &search).unwrap();
        assert!(
            run.complete(),
            "both arms must finish before reserved evaluation"
        );
        assert_eq!(run.config(), &configuration(plan));
        let candidate = DenseModel::new(run.history().last().unwrap().best_genome.clone()).unwrap();
        let full = arm.expand(&candidate, baseline);
        assert_eq!(
            full.id(),
            DenseModel::from_json(&fs::read(directory.join("finalist.json")).unwrap())
                .unwrap()
                .id()
        );
        models.push(full);
    }
    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("mission", "mission", baseline),
        ("reference", "reference", baseline),
        ("baseline", "model", baseline),
        ("all", "model", &models[0]),
        ("brake", "model", &models[1]),
    ] {
        let mut groups = Vec::new();
        for (partition, cases) in [
            ("training", task.sources),
            ("development", development),
            ("validation", reserved),
        ] {
            let traces = out.join(format!("{label}-{partition}"));
            fs::create_dir(&traces).unwrap();
            let episodes = task.pool.map(cases, |(case, source)| {
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
#[ignore = "explicit frozen brake-readout experiment, mode and fresh output required"]
fn learned_guidance_brake_readout() {
    let plan: Value = serde_json::from_slice(RECIPE).unwrap();
    let mode = std::env::var("SPACEWARS_BRAKE_MODE").unwrap();
    assert!(
        [
            "preflight",
            "capture",
            "train-all",
            "train-brake",
            "resume-all",
            "resume-brake",
            "evaluate"
        ]
        .contains(&mode.as_str())
    );
    let out = std::env::var_os("SPACEWARS_BRAKE_OUT").unwrap();
    let out = Path::new(&out);
    if !mode.starts_with("resume-") {
        fs::create_dir(out).expect("retain earlier experiment evidence");
    }
    let bytes = fs::read(std::env::var_os("SPACEWARS_BRAKE_MODEL").unwrap()).unwrap();
    assert_eq!(hash(&bytes), plan["baseline_file_sha256"]);
    let baseline = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let development = sources(serde_json::from_value(plan["development_cases"].clone()).unwrap());
    let (reserved, missing) = validation(&plan);
    let retained = retention_cases(&plan);
    let receipt = json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(RECIPE),"baseline":baseline.id(),"training":records(&training),
        "development":records(&development),"validation":records(&reserved),
        "missing_validation":missing,"retention_cases":retained});
    let receipt = serde_json::to_vec_pretty(&receipt).unwrap();
    if mode != "preflight" {
        let expected = fs::read(std::env::var_os("SPACEWARS_BRAKE_PREFLIGHT").unwrap()).unwrap();
        assert_eq!(
            receipt, expected,
            "physical sources or frozen executable changed"
        );
    }
    fs::write(out.join("plan.json"), RECIPE).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    if mode == "preflight" {
        eprintln!(
            "Preflight: {} training, {} development, {} reserved, {} missing",
            training.len(),
            development.len(),
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
    match mode.as_str() {
        "capture" => capture(&task, &baseline, out),
        "evaluate" => comparison(
            &task,
            &plan,
            &baseline,
            &development,
            &reserved,
            &missing,
            out,
        ),
        _ => {
            let arm = if mode.ends_with("-all") {
                Arm::All
            } else {
                Arm::Brake
            };
            train_arm(
                &Search {
                    task: &task,
                    baseline: &baseline,
                    arm,
                },
                &plan,
                out,
                mode.starts_with("resume-"),
            );
        }
    }
}

#[cfg(test)]
mod tests;
