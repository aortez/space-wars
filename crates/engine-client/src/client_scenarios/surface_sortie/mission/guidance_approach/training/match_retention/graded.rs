//! Matched continuation with a bounded brake reward on fixed inspected starts.
use super::*;

mod broad;
mod checkpoint_review;
mod coverage;
mod long_run;
mod priority;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-graded-braking-v1-plan.json"
));

#[derive(Clone, Copy)]
enum Arm {
    Control,
    Graded,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Graded => "graded",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GradedFitness {
    precision_completed: u32,
    protected_completed: u32,
    additional_completed: u32,
    brake_progress_points: u32,
    approach_speed_points: u32,
    approach_position_points: u32,
    precision_position_points: u32,
    precision_velocity_points: u32,
    spare_ticks: u32,
}

impl GradedFitness {
    fn from_retained(score: RetainedFitness, brake_progress_points: u32) -> Self {
        Self {
            precision_completed: score.precision_completed,
            protected_completed: score.protected_completed,
            additional_completed: score.additional_completed,
            brake_progress_points,
            approach_speed_points: score.approach_speed_points,
            approach_position_points: score.approach_position_points,
            precision_position_points: score.precision_position_points,
            precision_velocity_points: score.precision_velocity_points,
            spare_ticks: score.spare_ticks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct BrakeProbe {
    case: String,
    inputs: [f32; 9],
}

fn brake_probes(sources: &[Source], protected: &BTreeSet<String>) -> Vec<BrakeProbe> {
    sources
        .iter()
        .filter(|(case, _)| !protected.contains(&case.name))
        .map(|(case, source)| {
            let initial = flight(&source.lock().unwrap(), case.seat);
            let offset = (initial.ship.position - initial.planet_motion.position).normalized()
                * (initial.planet_radius + 85.0);
            let encoded = Features::read(&initial, target(&initial, offset))
                .encode()
                .unwrap();
            assert!(encoded.saturated.iter().all(|s| !s));
            BrakeProbe {
                case: case.name.clone(),
                inputs: encoded.inputs,
            }
        })
        .collect()
}

fn brake_points(raw: f32) -> u32 {
    assert!(raw.is_finite());
    (1000.0 * ((raw + 2.0) / 2.125).clamp(0.0, 1.0)) as u32
}

fn probe_outputs(probes: &[BrakeProbe], model: &DenseModel) -> Result<Vec<f32>, EvolutionError> {
    probes
        .iter()
        .map(|p| {
            model
                .infer(&p.inputs)
                .map(|v| v[2])
                .map_err(|e| EvolutionError(e.to_string()))
        })
        .collect()
}

struct Guided<'a, 'b, 'c> {
    search: &'a Search<'b, 'c>,
    probes: &'a [BrakeProbe],
    arm: Arm,
}

impl EvaluationTask for Guided<'_, '_, '_> {
    type Fitness = GradedFitness;

    fn manifest(&self) -> TaskManifest {
        let mut manifest = self.search.manifest();
        manifest.id.push_str(self.arm.name());
        manifest.configuration["arm"] = json!(self.arm.name());
        manifest
    }

    fn evaluate(&self, model: &DenseModel) -> Result<GradedFitness, EvolutionError> {
        let retained = self.search.evaluate_progress(model, true)?;
        let points = match self.arm {
            Arm::Control => 0,
            Arm::Graded => probe_outputs(self.probes, model)?
                .into_iter()
                .map(brake_points)
                .sum(),
        };
        Ok(GradedFitness::from_retained(retained, points))
    }
}

fn comparison(
    search: &Search<'_, '_>,
    probes: &[BrakeProbe],
    plan: &Value,
    baseline: &DenseModel,
    reserved: &[Source],
    out: &Path,
) {
    let runs = std::env::var_os("SPACEWARS_GRADED_RUNS").unwrap();
    let mut finalists = Vec::new();
    for arm in [Arm::Control, Arm::Graded] {
        let task = Guided {
            search,
            probes,
            arm,
        };
        let directory = Path::new(&runs).join(arm.name());
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
        ("control", "model", &finalists[0]),
        ("graded", "model", &finalists[1]),
    ] {
        let mut report =
            focused::model_report(search, plan, Some(reserved), label, driver, model, out);
        if driver == "model" {
            let outputs = probe_outputs(probes, model).unwrap();
            report["brake_probes"] = json!(probes.iter().zip(outputs).map(|(p, raw)| {
                json!({"case":p.case,"raw_brake":raw,"points":brake_points(raw),"brake":raw>0.0})
            }).collect::<Vec<_>>());
        }
        reports.push(report);
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(PLAN),"controllers":reports,"production_controls_changed":false}),
        );
    }
}

#[test]
#[ignore = "frozen graded-braking comparison; explicit mode and fresh output required"]
fn learned_guidance_graded_braking() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_GRADED_MODE").unwrap();
    assert!(
        [
            "preflight",
            "train-control",
            "train-graded",
            "resume-control",
            "resume-graded",
            "evaluate"
        ]
        .contains(&mode.as_str())
    );
    let out = std::env::var_os("SPACEWARS_GRADED_OUT").unwrap();
    let out = Path::new(&out);
    if !mode.starts_with("resume-") {
        fs::create_dir(out).expect("retain earlier evidence");
    }
    let bytes = fs::read(std::env::var_os("SPACEWARS_GRADED_MODEL").unwrap()).unwrap();
    assert_eq!(hash(&bytes), plan["baseline_file_sha256"]);
    let baseline = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let probes = brake_probes(&training, &protected);
    assert_eq!(
        probes.iter().map(|p| p.case.as_str()).collect::<Vec<_>>(),
        plan["additional_cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    let retained = retention_cases(&plan);
    let prior = fs::read(std::env::var_os("SPACEWARS_GRADED_PRIOR").unwrap()).unwrap();
    assert_eq!(hash(&prior), plan["prior_preflight_sha256"]);
    let prior: Value = serde_json::from_slice(&prior).unwrap();
    let inspected = records(&training);
    let readback =
        |v: &Value| serde_json::from_slice::<Value>(&serde_json::to_vec(v).unwrap()).unwrap();
    assert_eq!(readback(&json!(inspected)), prior["approaches"]);
    assert_eq!(readback(&json!(retained)), prior["retention_cases"]);
    let expected = (mode != "preflight")
        .then(|| fs::read(std::env::var_os("SPACEWARS_GRADED_PREFLIGHT").unwrap()).unwrap());
    let (reserved, discovery) = if let Some(bytes) = &expected {
        let frozen: Value = serde_json::from_slice(bytes).unwrap();
        let cases: Vec<Case> = frozen["validation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
            .collect();
        let selected: BTreeSet<_> = cases.iter().map(|c| c.seed).collect();
        assert_eq!(selected.len(), 4);
        assert!(selected.iter().all(|seed| {
            focused::complete_world(
                &cases
                    .iter()
                    .filter(|c| c.seed == *seed)
                    .cloned()
                    .collect::<Vec<_>>(),
                *seed,
            )
        }));
        (sources(cases), frozen["validation_discovery"].clone())
    } else {
        focused::discover(&plan)
    };
    assert_eq!(
        reserved.len(),
        16,
        "stop before training if predeclared reserve lacks four complete worlds"
    );
    let receipt = serde_json::to_vec_pretty(&json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"baseline":baseline.id(),"training":inspected,"protected_cases":protected,
        "retention_cases":retained,"brake_probes":probes,"validation":records(&reserved),"validation_discovery":discovery})).unwrap();
    if let Some(expected) = expected {
        assert_eq!(receipt, expected, "frozen sources or executable changed");
    }
    fs::write(out.join("plan.json"), PLAN).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    if mode == "preflight" {
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
    if mode == "evaluate" {
        comparison(&search, &probes, &plan, &baseline, &reserved, out);
    } else {
        let arm = if mode.ends_with("-control") {
            Arm::Control
        } else {
            Arm::Graded
        };
        train(
            &Guided {
                search: &search,
                probes: &probes,
                arm,
            },
            &[baseline],
            &plan,
            out,
            mode.starts_with("resume-"),
            &task.pool,
        );
    }
}

#[cfg(test)]
mod tests;
