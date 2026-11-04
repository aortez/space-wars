//! Fixed one-coordinate reversions of a reconstructed near-miss offspring.
use super::*;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-near-repair-v1-plan.json"
));

fn measure(
    task: &FocusTask<'_, true>,
    case: &ship_arrival::ArrivalCase,
    model: &DenseModel,
    label: &str,
    root: &Path,
) -> Value {
    let fitness = task.evaluate(model).unwrap();
    let mut target =
        ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), true).unwrap();
    save(root.join(format!("target-traces/{label}.json")), &target);
    save(root.join(format!("models/{label}.json")), model.genome());
    let maximum_dwell = target.trace.iter().map(|t| t.measure.dwell).max().unwrap();
    target.trace.clear();
    let total =
        fitness.precision_completed + fitness.retained_match_completed + fitness.focal_completed;
    let protected = total - target.fitness.completed;
    assert!(protected <= 620);
    assert_eq!(eligible(fitness), total == 621);
    json!({"label":label,"genome":model.id(),"fitness":fitness,"total_completed":total,
        "preserved_of_620":protected,"target_completed":target.fitness.completed,
        "maximum_target_dwell":maximum_dwell,"target":target,"eligible":eligible(fitness)})
}

#[test]
#[ignore = "fixed 24-candidate repair; explicit frozen root required"]
fn learned_guidance_near_repair() {
    let root = std::env::var_os("SPACEWARS_NEAR_REPAIR_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), PLAN);
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    for (name, digest) in plan["inputs"].as_object().unwrap() {
        assert_eq!(
            hash(&fs::read(root.join(name)).unwrap()),
            digest.as_str().unwrap()
        );
    }
    let mut base: Value =
        serde_json::from_slice(&fs::read(root.join("base-plan.json")).unwrap()).unwrap();
    let target: ship_arrival::ArrivalCase =
        serde_json::from_value(plan["target_case"].clone()).unwrap();
    assert!(
        !base["precision_replay_cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == target.name)
    );
    base["precision_replay_cases"]
        .as_array_mut()
        .unwrap()
        .push(plan["target_case"].clone());
    let inputs: Value =
        serde_json::from_slice(&fs::read(root.join("inputs.json")).unwrap()).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("expected.json")).unwrap()).unwrap();
    let baseline = expected["controllers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["controller"] == "near-miss")
        .unwrap();
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
    let initial = records(&training);
    let task = task::<true>(&training, &base, "near-repair");
    assert_eq!(
        task.base.retained.iter().filter(|c| **c == target).count(),
        1
    );
    let seed = DenseModel::from_json(&fs::read(root.join("near-miss.json")).unwrap()).unwrap();
    let parent = DenseModel::from_json(&fs::read(root.join("parent.json")).unwrap()).unwrap();
    assert_eq!(seed.id(), plan["seed_genome"].as_str().unwrap());
    assert_eq!(parent.id(), plan["parent_genome"].as_str().unwrap());
    assert_eq!(seed.genome().parameter_count(), 107);
    let changed: Vec<_> = seed.genome().layers[0]
        .weights
        .iter()
        .zip(&parent.genome().layers[0].weights)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    let declared: Vec<_> = plan["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            assert_eq!(c["layer"], 0);
            assert_eq!(c["kind"], "weights");
            c["index"].as_u64().unwrap() as usize
        })
        .collect();
    assert_eq!(changed, declared);
    let mut normalized_parent = parent.genome().clone();
    for &index in &changed {
        normalized_parent.layers[0].weights[index] = seed.genome().layers[0].weights[index];
    }
    assert_eq!(&normalized_parent, seed.genome());
    fs::create_dir(root.join("target-traces")).unwrap();
    fs::create_dir(root.join("models")).unwrap();
    let seed_result = measure(&task, &target, &seed, "near-miss", root);
    assert_eq!(seed_result["fitness"], baseline["fitness"]);
    let parent_result = measure(&task, &target, &parent, "parent", root);
    assert_eq!(parent_result["fitness"], plan["parent_fitness"]);
    save(
        root.join("baseline.json"),
        &json!([seed_result, parent_result]),
    );
    let fractions: Vec<f32> = serde_json::from_value(plan["reversion_fractions"].clone()).unwrap();
    assert_eq!(fractions, [0.25, 0.5, 0.75, 1.0]);
    let mut rows = Vec::new();
    let mut eligible_models = Vec::new();
    let mut identities = BTreeSet::new();
    let start = std::time::Instant::now();
    for &index in &changed {
        let old = seed.genome().layers[0].weights[index];
        let back = parent.genome().layers[0].weights[index];
        for &fraction in &fractions {
            let value = if fraction == 1.0 {
                back
            } else {
                old + fraction * (back - old)
            };
            let delta = (value - old).abs();
            assert!(delta > 0.0);
            let mut genome = seed.genome().clone();
            genome.layers[0].weights[index] = value;
            let model = DenseModel::new(genome).unwrap();
            assert!(identities.insert(model.id().to_string()));
            let label = format!("weight-{index}-revert-{}", (fraction * 100.0) as u32);
            let mut row = measure(&task, &target, &model, &label, root);
            row["edit"] = json!({"layer":0,"weight_index":index,"fraction":fraction,"old":old,"parent":back,"new":value,"absolute_delta":delta});
            eprintln!(
                "{}",
                json!({"candidate":label,"total":row["total_completed"],"preserved":row["preserved_of_620"],"target":row["target_completed"],"maximum_dwell":row["maximum_target_dwell"],"seconds":start.elapsed().as_secs_f64()})
            );
            if row["eligible"] == true {
                eligible_models.push((delta, index, fraction, label, model));
            }
            rows.push(row);
            atomic(
                root,
                "search.json",
                &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),"candidates":rows,"complete":false}),
            );
        }
    }
    assert_eq!(rows.len(), 24);
    assert_eq!(records(&training), initial);
    eligible_models.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    let selected = eligible_models.first().map(|(_, _, _, label, model)| {
        save(root.join("selected.json"), model.genome());
        json!({"label":label,"genome":model.id()})
    });
    save(
        root.join("selection.json"),
        &json!({"selected":selected,"eligible":eligible_models.iter().map(|(_,_,_,label,m)|json!({"label":label,"genome":m.id()})).collect::<Vec<_>>(),"candidates":24,"seconds":start.elapsed().as_secs_f64(),"plan_sha256":hash(PLAN)}),
    );
    atomic(
        root,
        "search.json",
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),"candidates":rows,"complete":true}),
    );
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let near = report(&task, &excluded, &seed, "near-miss", &base, &out);
    assert_eq!(near["fitness"], baseline["fitness"]);
    assert_eq!(parsed(near["groups"].clone()), baseline["groups"]);
    let mut normalized = baseline["precision_retention"].clone();
    for e in normalized.as_array_mut().unwrap() {
        e["runtime"] = json!(engine_agent::evolution::runtime_id().unwrap());
    }
    assert_eq!(parsed(near["precision_retention"].clone()), normalized);
    let mut parity = 0;
    for group in near["groups"].as_array().unwrap() {
        for e in group["episodes"].as_array().unwrap() {
            let name = format!(
                "near-miss-{}/{}-model.jsonl",
                group["partition"].as_str().unwrap(),
                e["case"].as_str().unwrap()
            );
            assert_eq!(
                fs::read(out.join(&name)).unwrap(),
                fs::read(
                    Path::new(plan["prior_directory"].as_str().unwrap())
                        .join("evaluate")
                        .join(name)
                )
                .unwrap()
            );
            parity += 1;
        }
    }
    assert_eq!(parity, 482);
    let mut reports = vec![near];
    if let Some((_, _, _, label, model)) = eligible_models.first() {
        let result = report(&task, &excluded, model, "selected", &base, &out);
        assert_eq!(result["eligible"], true);
        assert_eq!(
            result["fitness"],
            rows.iter().find(|r| r["label"] == *label).unwrap()["fitness"]
        );
        reports.push(result);
    }
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),"controllers":reports,"control_byte_parity_traces":parity,"control_precision_endpoint_parity":157,"complete":true,"fresh_generalization_test":false,"production_controls_changed":false}),
    );
}
