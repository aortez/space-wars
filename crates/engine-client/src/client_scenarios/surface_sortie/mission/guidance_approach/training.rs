//! Bounded evolution over real match continuations. Only the training sources
//! and declared practice cases enter selection; reserved continuations run last.
use super::*;
use engine_agent::{
    evolution::{EvaluationTask, EvolutionConfig, EvolutionError, EvolutionRun, TaskManifest},
    ship_arrival,
    ship_curriculum::{self, Partition},
    ship_training,
};

mod diagnostic;
mod full_retention;
pub(super) mod performance;

const LEARNING_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-match-training-v1-plan.json"
));
const LEARNING_PLAN_V2: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-match-training-v2-plan.json"
));
const LEARNING_PLAN_V3: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-match-training-v3-plan.json"
));

type Source = (Case, MaterialMissionClientScenario);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fitness {
    retained_completed: u32,
    approach_completed: u32,
    approach_speed_points: u32,
    approach_position_points: u32,
    retained_position_points: u32,
    retained_velocity_points: u32,
    spare_ticks: u32,
}

impl Fitness {
    fn record_approach(&mut self, result: &Episode) {
        self.approach_completed += u32::from(result.outcome == Outcome::Arrived);
        if result.outcome == Outcome::Arrived {
            self.spare_ticks += (MAX_TICKS - result.elapsed_ticks) as u32;
        }
        if !matches!(
            result.outcome,
            Outcome::Arrived | Outcome::Timeout | Outcome::HostPreempted
        ) {
            return;
        }
        if let Some(f) = &result.final_flight {
            let speed = (f.ship.velocity - f.planet_motion.velocity).length();
            let distance = f
                .ship
                .position
                .distance_to(target(f, result.frozen_target_offset).position);
            self.approach_speed_points += ((1.0 - speed / 70.0).clamp(0.0, 1.0) * 1000.0) as u32;
            self.approach_position_points +=
                ((1.0 - distance / 100.0).clamp(0.0, 1.0) * 1000.0) as u32;
        }
    }
}

struct TrainingTask<'a> {
    sources: &'a [Source],
    retained: Vec<ship_arrival::ArrivalCase>,
    retention_workers: usize,
    manifest: TaskManifest,
}

impl EvaluationTask for TrainingTask<'_> {
    type Fitness = Fitness;

    fn manifest(&self) -> TaskManifest {
        self.manifest.clone()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Fitness, EvolutionError> {
        let mut score = Fitness::default();
        for result in full_retention::episodes(&self.retained, model, self.retention_workers)? {
            score.retained_completed += result.fitness.completed;
            score.retained_position_points += result.fitness.position_points;
            score.retained_velocity_points += result.fitness.velocity_points;
            score.spare_ticks += result.fitness.spare_ticks;
        }
        for (case, source) in self.sources {
            score.record_approach(&episode(source, case, "model", model, None));
        }
        Ok(score)
    }
}

fn retention_cases(plan: &Value) -> Vec<ship_arrival::ArrivalCase> {
    let partitions = plan
        .get("retention_partitions")
        .cloned()
        .unwrap_or_else(|| json!(["training"]));
    let cases: Vec<_> = partitions
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|label| {
            let partition = ship_curriculum::PARTITIONS
                .into_iter()
                .find(|p| serde_json::to_value(p).unwrap() == *label)
                .expect("declared retention partition");
            ship_curriculum::cases(partition)
        })
        .collect();
    if plan["retention_deduplicate"] != true {
        return cases;
    }
    // The original partitions reuse physical starts under different names.
    // Compare every other field and retain the first occurrence's name/order.
    let mut seen = Vec::new();
    cases
        .into_iter()
        .filter(|case| {
            let mut identity = case.clone();
            identity.name.clear();
            if seen.contains(&identity) {
                false
            } else {
                seen.push(identity);
                true
            }
        })
        .collect()
}

fn sources(cases: Vec<Case>) -> Vec<Source> {
    cases
        .into_iter()
        .map(|case| {
            let run = prepare(&case);
            (case, run)
        })
        .collect()
}

fn source_case(
    run: &MaterialMissionClientScenario,
    name: String,
    shape: DirtMode,
    seat: usize,
) -> Option<Case> {
    let f = run.sortie.state.free_flight_observation(seat)?;
    if failure(&f, &f).is_some() {
        return None;
    }
    let offset =
        (f.ship.position - f.planet_motion.position).normalized() * (f.planet_radius + 85.0);
    let features = Features::read(&f, target(&f, offset));
    let encoded = features.encode().ok()?;
    if encoded.saturated.iter().any(|s| *s) {
        return None;
    }
    Some(Case {
        name,
        seed: run.seed,
        shape,
        seat,
        source_tick: f.tick,
        planet: f.planet,
        source_entry_distance: features.target_offset.length(),
        source_relative_speed: features.target_velocity.length(),
    })
}

fn validation(plan: &Value) -> (Vec<Source>, Vec<Value>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    let prefix = plan["validation_prefix_ticks"].as_u64().unwrap();
    let distance = plan["validation_entry_distance"].as_f64().unwrap() as f32;
    for seed in plan["validation_seeds"].as_array().unwrap() {
        let seed = seed.as_u64().unwrap();
        for shape in [DirtMode::WeakAngular, DirtMode::WeakRound] {
            let mut run = start_match(seed, shape);
            let mut selected = [false; 2];
            while run.sortie.state.tick() < prefix && !run.is_game_over() {
                for (seat, selected) in selected.iter_mut().enumerate() {
                    if *selected {
                        continue;
                    }
                    let name = format!("validation-{seed}-{shape:?}-p{}", seat + 1);
                    let Some(case) = source_case(&run, name, shape, seat) else {
                        continue;
                    };
                    if case.source_entry_distance > distance
                        || !direct_transfer(&probe(&run, seat), &case)
                    {
                        continue;
                    }
                    let before = flight(&run, seat);
                    let mut neutral = fork(&run);
                    neutral.bots[seat] = false;
                    neutral.step(&Movement::default().encode(before.owner), STEP);
                    neutral.bots[seat] = true;
                    let after = flight(&neutral, seat);
                    let after_case = source_case(&neutral, case.name.clone(), shape, seat);
                    if failure(&after, &before).is_some()
                        || after_case.is_none_or(|c| c.source_entry_distance > distance)
                        || !direct_transfer(&probe(&neutral, seat), &case)
                    {
                        continue;
                    }
                    assert_eq!(after.applied, [0.0; 3]);
                    found.push((case, neutral));
                    *selected = true;
                }
                if selected.iter().all(|s| *s) {
                    break;
                }
                run.step(&[], STEP);
            }
            for (seat, selected) in selected.into_iter().enumerate() {
                if !selected {
                    missing.push(json!({"seed":seed,"shape":shape,"seat":seat,
                        "end_tick":run.sortie.state.tick(),"round_finished":run.is_game_over()}));
                }
            }
        }
    }
    (found, missing)
}

fn records(sources: &[Source]) -> Vec<Value> {
    sources.iter().map(|(c, s)| initial_record(s, c)).collect()
}

fn configuration(plan: &Value) -> EvolutionConfig {
    serde_json::from_value(plan["evolution"].clone()).unwrap()
}

fn validate_run(
    run: &EvolutionRun<Fitness>,
    task: &TrainingTask<'_>,
    plan: &Value,
    parent: &DenseModel,
) {
    assert_eq!(run.config(), &configuration(plan));
    assert_eq!(run.task(), &task.manifest());
    assert_eq!(run.seeds(), [parent.genome().clone()]);
    assert!(
        run.population()
            .iter()
            .all(|c| c.genome.same_architecture(parent.genome()))
    );
}

fn train(task: &TrainingTask<'_>, plan: &Value, parent: &DenseModel, out: &Path, resume: bool) {
    let checkpoint = out.join("checkpoint.json");
    let mut run = if resume {
        EvolutionRun::load(&checkpoint, task).unwrap()
    } else {
        EvolutionRun::new(
            task,
            ship_training::template(),
            vec![parent.genome().clone()],
            configuration(plan),
        )
        .unwrap()
    };
    validate_run(&run, task, plan, parent);
    run.save(&checkpoint).unwrap();
    let steps = std::env::var("SPACEWARS_APPROACH_LEARNING_STEPS")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(usize::MAX);
    for step in 0..steps {
        if !run.advance(task).unwrap() {
            break;
        }
        run.save(&checkpoint).unwrap();
        if step == 0
            || !run.history().is_empty()
                && (run.complete() || run.fitness().iter().all(Option::is_none))
        {
            eprintln!(
                "{}",
                json!({"completed_generations":run.history().len(),
                "evaluated_in_generation":run.fitness().iter().filter(|s|s.is_some()).count(),
                "complete":run.complete(),"best":run.history().last().map(|r|r.best_fitness)})
            );
        }
    }
    // Exercise the actual on-disk resume path, including recipe and binary checks.
    let restored = EvolutionRun::load(&checkpoint, task).unwrap();
    assert_eq!(run, restored);
    if run.complete() {
        save(
            out.join("finalist.json"),
            &run.history().last().unwrap().best_genome,
        );
    }
    save(
        out.join("progress.json"),
        &json!({
            "complete":run.complete(), "runtime":engine_agent::evolution::runtime_id().unwrap(),
            "history":run.history().iter().map(|h|json!({"generation":h.generation,
                "best_id":h.best_id,"best_fitness":h.best_fitness,"fitness":h.fitness})).collect::<Vec<_>>()
        }),
    );
}

fn evaluate(
    task: &TrainingTask<'_>,
    plan: &Value,
    parent: &DenseModel,
    development: &[Source],
    validation: &[Source],
    missing: &[Value],
    out: &Path,
) {
    let trained = std::env::var_os("SPACEWARS_APPROACH_LEARNING_RUN").unwrap();
    let run = EvolutionRun::load(&Path::new(&trained).join("checkpoint.json"), task).unwrap();
    validate_run(&run, task, plan, parent);
    assert!(
        run.complete(),
        "reserved evaluation requires the full frozen budget"
    );
    let finalist = DenseModel::new(run.history().last().unwrap().best_genome.clone()).unwrap();
    save(out.join("finalist.json"), finalist.genome());
    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("mission", "mission", parent),
        ("reference", "reference", parent),
        ("parent", "model", parent),
        ("finalist", "model", &finalist),
    ] {
        let mut groups = Vec::new();
        for (partition, cases) in [
            ("training", task.sources),
            ("development", development),
            ("validation", validation),
        ] {
            let traces = out.join(format!("{label}-{partition}"));
            fs::create_dir(&traces).unwrap();
            let episodes: Vec<_> = cases
                .iter()
                .map(|(c, s)| episode(s, c, driver, model, Some(&traces)))
                .collect();
            let arrived = episodes
                .iter()
                .filter(|r| r.outcome == Outcome::Arrived)
                .count();
            eprintln!("{label} {partition}: {arrived}/{} arrived", episodes.len());
            groups.push(json!({"partition":partition,"arrived":arrived,"episodes":episodes}));
        }
        let original = if driver == "mission" {
            None
        } else {
            Some(
                ship_curriculum::evaluate(if driver == "reference" {
                    Policy::Reference
                } else {
                    Policy::Model(model.clone())
                })
                .unwrap(),
            )
        };
        let passed = missing.is_empty()
            && validation.len() == plan["validation_seeds"].as_array().unwrap().len() * 4
            && groups.iter().all(|g| {
                g["arrived"].as_u64().unwrap() as usize == g["episodes"].as_array().unwrap().len()
            })
            && original.as_ref().is_some_and(|r| r.passed);
        let settling = if plan["version"] == 3 {
            original
                .as_ref()
                .map(|original| full_retention::settling(original, plan, model, driver, label, out))
        } else {
            None
        };
        let mut report = json!({"controller":label,"genome":if driver=="model" {Some(model.id())} else {None},
            "approaches":groups,"original_arrival":original,"passed":passed});
        if let Some(settling) = settling {
            report["settling"] = settling;
        }
        reports.push(report);
        save(
            out.join("results.json"),
            &json!({"plan_sha256":hash(task.manifest.configuration["plan_utf8"].as_str().unwrap().as_bytes()),
            "runtime":engine_agent::evolution::runtime_id().unwrap(),"missing_validation":missing,
            "controllers":reports,"production_controls_changed":false}),
        );
    }
}

#[test]
#[ignore = "bounded match training; explicit mode, frozen preflight, and output paths required"]
fn learned_guidance_match_training() {
    let plan_bytes = match std::env::var("SPACEWARS_APPROACH_LEARNING_VERSION")
        .unwrap_or_else(|_| "1".into())
        .as_str()
    {
        "1" => LEARNING_PLAN,
        "2" => LEARNING_PLAN_V2,
        "3" => LEARNING_PLAN_V3,
        _ => panic!("experiment version must be 1, 2 or 3"),
    };
    let mode = std::env::var("SPACEWARS_APPROACH_LEARNING_MODE").unwrap();
    assert!(["preflight", "train", "resume", "evaluate"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_APPROACH_LEARNING_OUT").unwrap();
    let out = Path::new(&out);
    if mode != "resume" {
        fs::create_dir(out).expect("retain earlier evidence");
    }
    let plan: Value = serde_json::from_slice(plan_bytes).unwrap();
    assert_eq!(
        plan["validation_shapes"],
        json!(["weak-angular", "weak-round"])
    );
    assert_eq!(plan["validation_seats"], json!([0, 1]));
    let parent = DenseModel::from_json(GENOME).unwrap();
    assert_eq!(parent.id(), plan["parent_genome"].as_str().unwrap());
    assert_eq!(hash(GENOME), plan["parent_file_sha256"]);
    let training = sources(serde_json::from_value(plan["training_cases"].clone()).unwrap());
    assert!(!training.is_empty());
    let retained = retention_cases(&plan);
    assert!(!retained.is_empty());
    let old_plan: Value = serde_json::from_slice(PLAN).unwrap();
    let development = sources(
        serde_json::from_value(
            plan.get("development_cases")
                .unwrap_or(&old_plan["cases"])
                .clone(),
        )
        .unwrap(),
    );
    let (validation, missing) = validation(&plan);
    let mut receipt = json!({"plan_sha256":hash(plan_bytes),"probe_plan_sha256":hash(PLAN),
        "runtime":engine_agent::evolution::runtime_id().unwrap(),
        "training":records(&training),"development":records(&development),
        "validation":records(&validation),"missing_validation":missing});
    if plan["version"].as_u64().unwrap() >= 2 {
        receipt["retention_cases"] = json!(retained);
    }
    let receipt = serde_json::to_vec_pretty(&receipt).unwrap();
    if mode == "preflight" {
        fs::write(out.join("plan.json"), plan_bytes).unwrap();
        fs::write(out.join("preflight.json"), &receipt).unwrap();
        eprintln!(
            "Preflight: {} training, {} development, {} reserved starts, {} missing; no component continuations evaluated.",
            training.len(),
            development.len(),
            validation.len(),
            missing.len()
        );
        return;
    }
    let expected = std::env::var_os("SPACEWARS_APPROACH_LEARNING_PREFLIGHT").unwrap();
    assert_eq!(
        hash(&receipt),
        hash(&fs::read(expected).unwrap()),
        "physical starts or frozen build changed"
    );
    let task = TrainingTask {
        sources: &training,
        retained,
        retention_workers: plan["retention_workers"].as_u64().unwrap_or(1) as usize,
        manifest: TaskManifest {
            id: plan["experiment"].as_str().unwrap().into(),
            contract: spacewars_ai::guidance::contract(),
            // Preserve recipe bytes as a string; checkpoint parsing must not round
            // arbitrary receipt floats and change an otherwise identical identity.
            configuration: json!({"plan_utf8":std::str::from_utf8(plan_bytes).unwrap(),
            "preflight_sha256":hash(&receipt),"retention":plan.get("retention_partitions")
                .cloned().unwrap_or_else(|| json!(ship_curriculum::TRAINING_RECIPE))}),
        },
    };
    fs::write(out.join("plan.json"), plan_bytes).unwrap();
    fs::write(out.join("preflight.json"), &receipt).unwrap();
    match mode.as_str() {
        "train" | "resume" => train(&task, &plan, &parent, out, mode == "resume"),
        "evaluate" => evaluate(
            &task,
            &plan,
            &parent,
            &development,
            &validation,
            &missing,
            out,
        ),
        _ => unreachable!(),
    }
}

#[test]
fn expanded_curriculum_retains_regressions_and_reserves_new_worlds() {
    let previous: Value = serde_json::from_slice(LEARNING_PLAN).unwrap();
    let plan: Value = serde_json::from_slice(LEARNING_PLAN_V2).unwrap();
    let retained = retention_cases(&plan);
    assert_eq!(retention_cases(&previous).len(), 12);
    assert_eq!(retained.len(), 28);
    for case in ship_curriculum::cases(Partition::Training) {
        assert!(retained.iter().any(|c| c == &case));
    }
    for name in [
        "mixed_fresh_moving_b0_p0",
        "mixed_fresh_moving_b1_p0",
        "mixed_fresh_moving_b2_p0",
    ] {
        assert!(retained.iter().any(|c| c.name == name));
    }
    let unique: std::collections::BTreeSet<_> = retained
        .iter()
        .map(|c| {
            let mut value = serde_json::to_value(c).unwrap();
            value.as_object_mut().unwrap().remove("name");
            value.to_string()
        })
        .collect();
    assert_eq!(unique.len(), retained.len());

    let training = plan["training_cases"].as_array().unwrap();
    assert_eq!(training.len(), 15);
    assert!(
        previous["training_cases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| training.contains(c))
    );
    let development = plan["development_cases"].as_array().unwrap();
    assert_eq!(development.len(), 24);
    for case in training.iter().chain(development) {
        assert!(
            !plan["validation_seeds"]
                .as_array()
                .unwrap()
                .contains(&case["seed"])
        );
    }
    assert_eq!(plan["evolution"], previous["evolution"]);
    let config = configuration(&plan);
    assert_eq!(
        config.population * config.generations * (retained.len() + training.len()),
        16512
    );
}

#[test]
fn approach_trace_switch_preserves_known_outcomes() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let case: Case = serde_json::from_value(plan["cases"][0].clone()).unwrap();
    let source = prepare(&case);
    let model = DenseModel::from_json(GENOME).unwrap();
    let out = std::env::temp_dir().join(format!(
        "spacewars-approach-trace-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&out).unwrap();
    for (driver, outcome, ticks) in [
        ("mission", Outcome::Arrived, 50),
        ("reference", Outcome::Arrived, 20),
        ("model", Outcome::HostPreempted, 52),
    ] {
        let plain = episode(&source, &case, driver, &model, None);
        let traced = episode(&source, &case, driver, &model, Some(&out));
        assert_eq!(plain, traced);
        assert_eq!((plain.outcome, plain.elapsed_ticks), (outcome, ticks));
    }
    fs::remove_dir_all(out).unwrap();
}

#[test]
fn approach_selection_preserves_arrivals_before_progress() {
    let retained = Fitness {
        retained_completed: 12,
        ..Default::default()
    };
    assert!(
        retained
            > Fitness {
                retained_completed: 11,
                approach_completed: 7,
                approach_speed_points: 7000,
                approach_position_points: 7000,
                ..Default::default()
            }
    );
    assert!(
        Fitness {
            approach_completed: 1,
            ..retained
        } > Fitness {
            approach_speed_points: 7000,
            approach_position_points: 7000,
            ..retained
        }
    );
}
