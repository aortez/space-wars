//! Matched evolution runs differing only in which approaches earn progress.
use super::*;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-focused-progress-v1-plan.json"
));

#[derive(Clone, Copy)]
enum Scope {
    All,
    Remaining,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Remaining => "remaining",
        }
    }
}

struct Scoped<'a, 'b, 'c> {
    search: &'a Search<'b, 'c>,
    scope: Scope,
}

impl EvaluationTask for Scoped<'_, '_, '_> {
    type Fitness = RetainedFitness;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = self.search.manifest();
        manifest.id.push_str(self.scope.name());
        manifest.configuration["progress_scope"] = json!(self.scope.name());
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<RetainedFitness, EvolutionError> {
        self.search
            .evaluate_progress(model, matches!(self.scope, Scope::Remaining))
    }
}

pub(super) fn complete_world(cases: &[Case], seed: u64) -> bool {
    cases.len() == 4
        && [DirtMode::WeakAngular, DirtMode::WeakRound]
            .into_iter()
            .all(|shape| {
                (0..2).all(|seat| {
                    cases
                        .iter()
                        .filter(|c| c.seed == seed && c.shape == shape && c.seat == seat)
                        .count()
                        == 1
                })
            })
}

pub(super) fn discover(plan: &Value) -> (Vec<Source>, Value) {
    let requested = plan["validation_worlds"].as_u64().unwrap() as usize;
    let mut selected = Vec::new();
    let mut audit = Vec::new();
    for seed in plan["validation_seeds"].as_array().unwrap() {
        let mut one = plan.clone();
        one["validation_seeds"] = json!([seed]);
        let (cases, missing) = validation(&one);
        let accepted = complete_world(
            &cases.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(),
            seed.as_u64().unwrap(),
        ) && missing.is_empty();
        audit.push(json!({"seed":seed,"accepted":accepted,
            "available":cases.iter().map(|(c,_)|&c.name).collect::<Vec<_>>(),"missing":missing}));
        if accepted {
            selected.extend(cases);
        }
        if selected.len() == requested * 4 {
            break;
        }
    }
    let count = selected.len() / 4;
    (
        selected,
        json!({"scanned":audit,"selected_worlds":count,"requested_worlds":requested}),
    )
}

pub(super) fn model_report(
    search: &Search<'_, '_>,
    plan: &Value,
    reserved: Option<&[Source]>,
    label: &str,
    driver: &str,
    model: &DenseModel,
    out: &Path,
) -> Value {
    let mut groups = Vec::new();
    for partition in ["protected", "additional", "validation"] {
        let cases: Vec<_> = if partition == "validation" {
            let Some(reserved) = reserved else {
                continue;
            };
            reserved.iter().collect()
        } else {
            search
                .task
                .sources
                .iter()
                .filter(|(c, _)| search.protected.contains(&c.name) == (partition == "protected"))
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
    // The diagnostic retains all endpoints; full settling traces belong to the
    // final comparison, avoiding hundreds of known constant-brake timeouts.
    let settling = original
        .as_ref()
        .filter(|_| reserved.is_some())
        .map(|o| full_retention::settling(o, plan, model, driver, label, out));
    let passed = reserved.is_some_and(|s| s.len() == 16)
        && groups.iter().all(|g| {
            g["arrived"].as_u64().unwrap() as usize == g["episodes"].as_array().unwrap().len()
        })
        && original.as_ref().is_some_and(|o| o.passed);
    json!({"controller":label,"genome":if driver=="model" {Some(model.id())} else {None},
        "approaches":groups,"original_arrival":original,"settling":settling,"passed":passed})
}

fn diagnostic(search: &Search<'_, '_>, plan: &Value, baseline: &DenseModel, out: &Path) {
    let intervention = super::super::diagnostic::brake_on(baseline);
    save(out.join("brake-on.json"), intervention.genome());
    let mut reports = Vec::new();
    for (label, model) in [("baseline", baseline), ("brake_on", &intervention)] {
        reports.push(model_report(search, plan, None, label, "model", model, out));
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(PLAN),"controllers":reports,"reserved_continuations":false,
            "production_controls_changed":false}),
        );
    }
}

fn comparison(
    search: &Search<'_, '_>,
    plan: &Value,
    baseline: &DenseModel,
    reserved: &[Source],
    out: &Path,
) {
    let runs = std::env::var_os("SPACEWARS_FOCUSED_RUNS").unwrap();
    let mut finalists = Vec::new();
    for scope in [Scope::All, Scope::Remaining] {
        let task = Scoped { search, scope };
        let directory = Path::new(&runs).join(scope.name());
        let run = EvolutionRun::load(&directory.join("checkpoint.json"), &task).unwrap();
        assert!(
            run.complete(),
            "freeze both finalists before reserved continuations"
        );
        assert_eq!(run.config(), &configuration(plan));
        assert_eq!(run.seeds(), [baseline.genome().clone()]);
        let winner = run.history().last().unwrap();
        let model = DenseModel::new(winner.best_genome.clone()).unwrap();
        assert_eq!(
            model.id(),
            DenseModel::from_json(&fs::read(directory.join("finalist.json")).unwrap())
                .unwrap()
                .id()
        );
        assert_eq!(task.evaluate(&model).unwrap(), winner.best_fitness);
        finalists.push(model);
    }
    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("mission", "mission", baseline),
        ("reference", "reference", baseline),
        ("baseline", "model", baseline),
        ("all", "model", &finalists[0]),
        ("remaining", "model", &finalists[1]),
    ] {
        reports.push(model_report(
            search,
            plan,
            Some(reserved),
            label,
            driver,
            model,
            out,
        ));
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(PLAN),"controllers":reports,"reserved_continuations":true,
            "production_controls_changed":false}),
        );
    }
}

#[test]
#[ignore = "frozen matched progress experiment; explicit mode and fresh output required"]
fn learned_guidance_focused_progress() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_FOCUSED_MODE").unwrap();
    assert!(
        [
            "preflight",
            "diagnostic",
            "train-all",
            "train-remaining",
            "resume-all",
            "resume-remaining",
            "evaluate"
        ]
        .contains(&mode.as_str())
    );
    let out = std::env::var_os("SPACEWARS_FOCUSED_OUT").unwrap();
    let out = Path::new(&out);
    if !mode.starts_with("resume-") {
        fs::create_dir(out).expect("retain earlier evidence");
    }
    let bytes = fs::read(std::env::var_os("SPACEWARS_FOCUSED_MODEL").unwrap()).unwrap();
    assert_eq!(hash(&bytes), plan["baseline_file_sha256"]);
    let baseline = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let expected = (mode != "preflight")
        .then(|| fs::read(std::env::var_os("SPACEWARS_FOCUSED_PREFLIGHT").unwrap()).unwrap());
    let (reserved, discovery) = if let Some(bytes) = &expected {
        let frozen: Value = serde_json::from_slice(bytes).unwrap();
        let cases: Vec<Case> = frozen["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
            .collect();
        let selected_seeds: Vec<_> = frozen["validation_discovery"]["scanned"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["accepted"] == true)
            .map(|r| r["seed"].as_u64().unwrap())
            .collect();
        assert_eq!(cases.len(), selected_seeds.len() * 4);
        assert!(selected_seeds.iter().all(|s| {
            complete_world(
                &cases
                    .iter()
                    .filter(|c| c.seed == *s)
                    .cloned()
                    .collect::<Vec<_>>(),
                *s,
            )
        }));
        (sources(cases), frozen["validation_discovery"].clone())
    } else {
        discover(&plan)
    };
    let retained = retention_cases(&plan);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    assert!(
        protected
            .iter()
            .all(|n| training.iter().any(|(c, _)| &c.name == n))
    );
    let receipt = serde_json::to_vec_pretty(
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"baseline":baseline.id(),"training":records(&training),
        "protected_cases":protected,"retention_cases":retained,"validation":records(&reserved),
        "validation_discovery":discovery}),
    )
    .unwrap();
    if let Some(expected) = expected {
        assert_eq!(receipt, expected, "frozen sources or executable changed");
    }
    fs::write(out.join("plan.json"), PLAN).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    if mode == "preflight" {
        eprintln!(
            "Preflight: {} inspected, {} protected, {} reserved",
            training.len(),
            protected.len(),
            reserved.len()
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
            configuration: json!({"plan_utf8":std::str::from_utf8(PLAN).unwrap(),"preflight_sha256":hash(&receipt)}),
        },
    };
    let search = Search {
        task: &task,
        protected,
    };
    match mode.as_str() {
        "diagnostic" => diagnostic(&search, &plan, &baseline, out),
        "evaluate" => comparison(&search, &plan, &baseline, &reserved, out),
        _ => {
            let scope = if mode.ends_with("-all") {
                Scope::All
            } else {
                Scope::Remaining
            };
            train(
                &Scoped {
                    search: &search,
                    scope,
                },
                &[baseline],
                &plan,
                out,
                mode.starts_with("resume-"),
                &task.pool,
            );
        }
    }
}

#[cfg(test)]
mod tests;
