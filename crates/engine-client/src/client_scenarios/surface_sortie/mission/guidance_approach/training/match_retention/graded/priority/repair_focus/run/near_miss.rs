//! Recover a predeclared, unselected offspring without repeating the search.
use super::*;
use std::collections::BTreeMap;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-near-miss-v1-plan.json"
));

struct RecordedTask {
    manifest: TaskManifest,
    scores: BTreeMap<String, FocusFitness<true>>,
}

impl EvaluationTask for RecordedTask {
    type Fitness = FocusFitness<true>;

    fn manifest(&self) -> TaskManifest {
        self.manifest.clone()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        self.scores
            .get(model.id())
            .copied()
            .ok_or_else(|| EvolutionError(format!("unrecorded genome {}", model.id())))
    }
}

fn reconstruct(root: &Path, plan: &Value) {
    let checkpoint: Value =
        serde_json::from_slice(&fs::read(root.join("history.json")).unwrap()).unwrap();
    let original: EvolutionRun<FocusFitness<true>> =
        serde_json::from_value(checkpoint["run"].clone()).unwrap();
    assert_eq!(
        hash(&serde_json::to_vec(&original).unwrap()),
        checkpoint["sha256"]
    );
    assert!(original.complete());
    let mut scores = BTreeMap::new();
    for row in original.history() {
        for (id, score) in row.population_ids.iter().zip(&row.fitness) {
            if let Some(previous) = scores.insert(id.clone(), *score) {
                assert_eq!(previous, *score);
            }
        }
    }
    let task = RecordedTask {
        manifest: original.task().clone(),
        scores,
    };
    let mut run = EvolutionRun::new(
        &task,
        serde_json::from_value(checkpoint["run"]["template"].clone()).unwrap(),
        original.seeds().to_vec(),
        original.config().clone(),
    )
    .unwrap();
    let target_generation = plan["target"]["generation"].as_u64().unwrap() as usize - 1;
    let target_slot = plan["target"]["slot_zero_based"].as_u64().unwrap() as usize;
    let mut retained = None;
    while !run.complete() {
        let generation = run.generation();
        let ids: Vec<_> = run
            .population()
            .iter()
            .map(|c| DenseModel::new(c.genome.clone()).unwrap().id().to_string())
            .collect();
        assert_eq!(ids, original.history()[generation].population_ids);
        if generation == target_generation {
            assert_eq!(ids[target_slot], plan["target"]["genome"].as_str().unwrap());
            retained = Some(run.population()[target_slot].clone());
        }
        for _ in 0..run.config().population {
            assert!(run.advance(&task).unwrap());
        }
        assert_eq!(run.history(), &original.history()[..generation + 1]);
    }
    assert_eq!(run.population(), original.population());
    assert_eq!(run.fitness(), original.fitness());
    let mut normalized: Value = serde_json::from_slice(&serde_json::to_vec(&run).unwrap()).unwrap();
    normalized["runtime"] = checkpoint["run"]["runtime"].clone();
    assert_eq!(normalized, checkpoint["run"]);
    let candidate = retained.unwrap();
    save(root.join("near-miss.json"), &candidate.genome);
    save(
        root.join("reconstruction.json"),
        &json!({
            "runtime":engine_agent::evolution::runtime_id().unwrap(),
            "historical_runtime":checkpoint["run"]["runtime"],
            "plan_sha256":hash(PLAN),"history_sha256":hash(&fs::read(root.join("history.json")).unwrap()),
            "generations_checked":run.history().len(),"population_slots_checked":run.history().len()*run.config().population,
            "unique_scores":task.scores.len(),"target":plan["target"],"candidate":candidate,
            "fitness":original.history()[target_generation].fitness[target_slot],
            "cached_scores_only":true,"passed":true
        }),
    );
}

fn evaluate(root: &Path, plan: &Value) {
    let base: Value =
        serde_json::from_slice(&fs::read(root.join("base-plan.json")).unwrap()).unwrap();
    let inputs: Value =
        serde_json::from_slice(&fs::read(root.join("inputs.json")).unwrap()).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("expected.json")).unwrap()).unwrap();
    let reconstructed: Value =
        serde_json::from_slice(&fs::read(root.join("reconstruction.json")).unwrap()).unwrap();
    assert_eq!(
        reconstructed["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(reconstructed["plan_sha256"], hash(PLAN));
    let all = restore(&inputs, "sources");
    let included: BTreeSet<String> = base["retained_match_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(base["focal_cases"].as_array().unwrap())
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    let (training, excluded): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(case, _)| included.contains(&case.name));
    assert_eq!((training.len(), excluded.len()), (464, 18));
    let task = task::<true>(&training, &base, "near-miss-diagnostic");
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let mut reports = Vec::new();
    let mut models = Vec::new();
    let mut parity_traces = 0;
    for label in ["reference", "focused-final", "near-miss"] {
        let model =
            DenseModel::from_json(&fs::read(root.join(format!("{label}.json"))).unwrap()).unwrap();
        let result = report(&task, &excluded, &model, label, &base, &out);
        if label == "near-miss" {
            assert_eq!(model.id(), plan["target"]["genome"].as_str().unwrap());
            assert_eq!(result["fitness"], reconstructed["fitness"]);
        } else {
            let old = expected["controllers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["controller"] == label)
                .unwrap();
            assert_eq!(parsed(result["groups"].clone()), old["groups"]);
            assert_eq!(result["fitness"], old["fitness"]);
            assert_eq!(result["genome"], old["genome"]);
            for group in result["groups"].as_array().unwrap() {
                for e in group["episodes"].as_array().unwrap() {
                    let name = format!(
                        "{label}-{}/{}-model.jsonl",
                        group["partition"].as_str().unwrap(),
                        e["case"].as_str().unwrap()
                    );
                    assert_eq!(
                        fs::read(out.join(&name)).unwrap(),
                        fs::read(
                            Path::new(plan["prior_directory"].as_str().unwrap())
                                .join("evaluate")
                                .join(&name)
                        )
                        .unwrap()
                    );
                    parity_traces += 1;
                }
            }
            let mut normalized = old["precision_retention"].clone();
            for e in normalized.as_array_mut().unwrap() {
                e["runtime"] = json!(engine_agent::evolution::runtime_id().unwrap());
            }
            assert_eq!(parsed(result["precision_retention"].clone()), normalized);
        }
        reports.push(result);
        models.push((label, model));
        atomic(
            &out,
            "results.json",
            &json!({"controllers":reports,"complete":false}),
        );
    }
    let failures: Vec<ship_arrival::ArrivalCase> = reports.last().unwrap()["precision_retention"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["outcome"] != "arrived")
        .map(|e| serde_json::from_value(e["case"].clone()).unwrap())
        .collect();
    for (index, (label, model)) in models.iter().enumerate() {
        for case in &failures {
            let mut e = ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true)
                .unwrap();
            save(
                out.join(format!("{label}-precision/{}.json", case.name)),
                &e,
            );
            e.trace.clear();
            let endpoint = reports[index]["precision_retention"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["case"]["name"] == case.name)
                .unwrap();
            assert_eq!(parsed(json!(e)), parsed(endpoint.clone()));
        }
    }
    atomic(
        &out,
        "results.json",
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"controllers":reports,"failed_precision_cases":failures,
        "control_byte_parity_traces":parity_traces,"control_precision_endpoint_parity":314,
        "complete":true,"production_controls_changed":false,"fresh_generalization_test":false}),
    );
}

#[test]
#[ignore = "explicit frozen near-miss reconstruction and diagnostic"]
fn learned_guidance_near_miss() {
    let root = std::env::var_os("SPACEWARS_NEAR_MISS_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), PLAN);
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    for (name, digest) in plan["inputs"].as_object().unwrap() {
        assert_eq!(
            hash(&fs::read(root.join(name)).unwrap()),
            digest.as_str().unwrap()
        );
    }
    match std::env::var("SPACEWARS_NEAR_MISS_MODE").unwrap().as_str() {
        "reconstruct" => reconstruct(root, &plan),
        "evaluate" => evaluate(root, &plan),
        other => panic!("unknown diagnostic mode {other}"),
    }
}
