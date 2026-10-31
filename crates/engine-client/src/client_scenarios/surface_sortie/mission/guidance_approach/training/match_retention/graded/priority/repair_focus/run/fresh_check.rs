//! Fixed-model qualification on new worlds and precision geometry.
use super::*;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-fresh-check-v1-plan.json"
));

fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn load_model(root: &Path, plan: &Value, name: &str) -> DenseModel {
    let specification = &plan["models"][name];
    let model = DenseModel::from_json(
        &fs::read(root.join(specification["file"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    assert_eq!(model.id(), specification["genome"].as_str().unwrap());
    model
}

fn fresh_precision(plan: &Value) -> Vec<ship_arrival::ArrivalCase> {
    plan["precision_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| serde_json::from_value(c["case"].clone()).unwrap())
        .collect()
}

fn gate(
    plan: &Value,
    complete_worlds: usize,
    strata: &[usize; 4],
    available: usize,
    matches: usize,
    precision: usize,
    retained: bool,
) -> Value {
    let rule = &plan["acceptance"];
    let coverage = complete_worlds >= rule["minimum_complete_worlds"].as_u64().unwrap() as usize
        && strata
            .iter()
            .all(|&n| n >= rule["minimum_available_per_shape_seat"].as_u64().unwrap() as usize);
    let match_complete = available > 0 && matches == available;
    let precision_complete = precision == fresh_precision(plan).len();
    json!({"coverage_passed":coverage,"match_completion_passed":match_complete,
        "precision_completion_passed":precision_complete,"known_retention_passed":retained,
        "passed":coverage && match_complete && precision_complete && retained})
}

fn preflight(root: &Path, plan: &Value) {
    let base = read(root, "base-plan.json");
    let inputs = read(root, "inputs.json");
    let all = restore(&inputs, "sources");
    let included: BTreeSet<String> = base["retained_match_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(base["focal_cases"].as_array().unwrap())
        .map(|n| n.as_str().unwrap().into())
        .collect();
    let (training, excluded): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(c, _)| included.contains(&c.name));
    assert_eq!((training.len(), excluded.len()), (464, 18));
    let task = task::<true>(&training, &base, "fresh-preflight");
    let out = root.join("preflight");
    fs::create_dir(&out).unwrap();
    let mut reports = Vec::new();
    let mut parity = 0;
    for (label, key, expected_file, old_label, directory) in [
        (
            "candidate",
            "candidate",
            "expected-candidate.json",
            "selected",
            "prior_directory",
        ),
        (
            "learned-reference",
            "learned_reference",
            "expected-reference.json",
            "reference",
            "reference_directory",
        ),
    ] {
        let model = load_model(root, plan, key);
        let result = report(&task, &excluded, &model, label, &base, &out);
        let previous = read(root, expected_file);
        let old = previous["controllers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["controller"] == old_label)
            .unwrap();
        assert_eq!(result["genome"], old["genome"]);
        assert_eq!(result["fitness"], old["fitness"]);
        assert_eq!(parsed(result["groups"].clone()), old["groups"]);
        for group in result["groups"].as_array().unwrap() {
            for e in group["episodes"].as_array().unwrap() {
                let partition = group["partition"].as_str().unwrap();
                let file = format!("{}-model.jsonl", e["case"].as_str().unwrap());
                assert_eq!(
                    fs::read(out.join(format!("{label}-{partition}/{file}"))).unwrap(),
                    fs::read(
                        Path::new(plan[directory].as_str().unwrap())
                            .join(format!("evaluate/{old_label}-{partition}/{file}"))
                    )
                    .unwrap()
                );
                parity += 1;
            }
        }
        let mut normalized = old["precision_retention"].clone();
        for e in normalized.as_array_mut().unwrap() {
            e["runtime"] = json!(engine_agent::evolution::runtime_id().unwrap());
        }
        assert_eq!(parsed(result["precision_retention"].clone()), normalized);
        if label == "candidate" {
            assert_eq!(result["eligible"], true);
        }
        reports.push(result);
    }
    assert_eq!(parity, 964);
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"controllers":reports,"control_match_trace_parity":parity,
        "control_precision_endpoint_parity":314,"known_621_retained":true,"passed":true}),
    );
}

fn qualify(root: &Path, plan: &Value) {
    let receipt = read(root, "preflight/results.json");
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["plan_sha256"], hash(PLAN));
    assert_eq!(receipt["passed"], true);
    let candidate = load_model(root, plan, "candidate");
    let reference = load_model(root, plan, "learned_reference");
    let pool = EpisodePool::new(worker_config()).unwrap();
    let seeds = plan["validation_seeds"].as_array().unwrap();
    let out = root.join("qualify");
    fs::create_dir(&out).unwrap();
    let discovered = pool.map(seeds, |seed| {
        let mut one = plan.clone();
        one["validation_seeds"] = json!([seed]);
        let result = validation(&one);
        eprintln!(
            "discovered seed {seed}: {} available, {} missing",
            result.0.len(),
            result.1.len()
        );
        result
    });
    let mut sources = Vec::new();
    let mut missing = Vec::new();
    for (found, absent) in discovered {
        sources.extend(found);
        missing.extend(absent);
    }
    assert_eq!(sources.len() + missing.len(), seeds.len() * 4);
    let initial = records(&sources);
    let complete_worlds = seeds
        .iter()
        .filter(|seed| {
            sources
                .iter()
                .filter(|(c, _)| json!(c.seed) == **seed)
                .count()
                == 4
        })
        .count();
    let mut strata = [0_usize; 4];
    for (case, _) in &sources {
        let index = match case.shape {
            DirtMode::WeakAngular => 0,
            DirtMode::WeakRound => 2,
            _ => panic!("unexpected shape"),
        } + case.seat;
        strata[index] += 1;
    }
    save(
        out.join("discovery.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"sources":initial,"missing":missing,"complete_worlds":complete_worlds,
        "strata_order":["weak-angular/p1","weak-angular/p2","weak-round/p1","weak-round/p2"],"strata":strata,
        "no_controller_outcome_filter":true}),
    );
    let discovery_hash = hash(&fs::read(out.join("discovery.json")).unwrap());
    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("candidate", "model", &candidate),
        ("learned-reference", "model", &reference),
        ("native-mission", "mission", &reference),
        ("handwritten-guidance", "reference", &reference),
    ] {
        let traces = out.join(label);
        fs::create_dir(&traces).unwrap();
        let episodes = pool.map(&sources, |(case, source)| {
            episode(&source.lock().unwrap(), case, driver, model, Some(&traces))
        });
        let arrived = episodes
            .iter()
            .filter(|e| e.outcome == Outcome::Arrived)
            .count();
        for (e, (case, source)) in episodes.iter().zip(&sources) {
            assert_eq!(e.initial, flight(&source.lock().unwrap(), case.seat));
        }
        eprintln!("{label}: {arrived}/{} fresh approaches", sources.len());
        reports.push(json!({"controller":label,"driver":driver,"genome":if driver == "model" {Some(model.id())} else {None},"arrived":arrived,"episodes":episodes}));
        atomic(
            &out,
            "matches.json",
            &json!({"controllers":reports,"complete":false}),
        );
    }
    let cases = fresh_precision(plan);
    let mut precision = Vec::new();
    for (label, policy) in [
        ("candidate", Policy::Model(candidate.clone())),
        ("learned-reference", Policy::Model(reference.clone())),
        ("handwritten-guidance", Policy::Reference),
    ] {
        let traces = out.join(format!("{label}-precision"));
        fs::create_dir(&traces).unwrap();
        let episodes = pool.map(&cases, |case| {
            let mut e = ship_arrival::run_episode(case.clone(), policy.clone(), true).unwrap();
            save(traces.join(format!("{}.json", case.name)), &e);
            e.trace.clear();
            e
        });
        let arrived = episodes
            .iter()
            .filter(|e| e.outcome == ship_arrival::ArrivalOutcome::Arrived)
            .count();
        eprintln!("{label}: {arrived}/{} fresh precision", cases.len());
        precision.push(json!({"controller":label,"arrived":arrived,"episodes":episodes}));
        atomic(
            &out,
            "precision.json",
            &json!({"controllers":precision,"complete":false}),
        );
    }
    // Replay all candidate misses under every applicable controller. The model
    // identities and the full suite remain fixed regardless of these outcomes.
    let failed: BTreeSet<String> = reports[0]["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["outcome"] != "arrived")
        .map(|e| e["case"].as_str().unwrap().into())
        .collect();
    let failed_sources: Vec<_> = sources
        .iter()
        .filter(|(c, _)| failed.contains(&c.name))
        .collect();
    let mut repeated_matches = 0;
    for (label, driver, model) in [
        ("candidate", "model", &candidate),
        ("learned-reference", "model", &reference),
        ("native-mission", "mission", &reference),
        ("handwritten-guidance", "reference", &reference),
    ] {
        let directory = out.join(format!("repeat-{label}"));
        fs::create_dir(&directory).unwrap();
        for e in pool.map(&failed_sources, |(c, s)| {
            episode(&s.lock().unwrap(), c, driver, model, Some(&directory))
        }) {
            let old = reports.iter().find(|r| r["controller"] == label).unwrap()["episodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["case"] == e.case)
                .unwrap();
            assert_eq!(parsed(json!(e)), parsed(old.clone()));
            let filename = format!("{}-{driver}.jsonl", e.case);
            assert_eq!(
                fs::read(directory.join(&filename)).unwrap(),
                fs::read(out.join(label).join(filename)).unwrap()
            );
            repeated_matches += 1;
        }
    }
    let failed: BTreeSet<String> = precision[0]["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["outcome"] != "arrived")
        .map(|e| e["case"]["name"].as_str().unwrap().into())
        .collect();
    let failed_cases: Vec<_> = cases.iter().filter(|c| failed.contains(&c.name)).collect();
    let mut repeated_precision = 0;
    for (label, policy) in [
        ("candidate", Policy::Model(candidate)),
        ("learned-reference", Policy::Model(reference)),
        ("handwritten-guidance", Policy::Reference),
    ] {
        let directory = out.join(format!("repeat-{label}-precision"));
        fs::create_dir(&directory).unwrap();
        for e in pool.map(&failed_cases, |case| {
            ship_arrival::run_episode((*case).clone(), policy.clone(), true).unwrap()
        }) {
            let filename = format!("{}.json", e.case.name);
            save(directory.join(&filename), &e);
            assert_eq!(
                fs::read(directory.join(&filename)).unwrap(),
                fs::read(out.join(format!("{label}-precision")).join(filename)).unwrap()
            );
            repeated_precision += 1;
        }
    }
    assert_eq!(records(&sources), initial);
    assert_eq!(
        discovery_hash,
        hash(&fs::read(out.join("discovery.json")).unwrap())
    );
    let qualification = gate(
        plan,
        complete_worlds,
        &strata,
        sources.len(),
        reports[0]["arrived"].as_u64().unwrap() as usize,
        precision[0]["arrived"].as_u64().unwrap() as usize,
        receipt["known_621_retained"] == true,
    );
    atomic(
        &out,
        "matches.json",
        &json!({"controllers":reports,"complete":true}),
    );
    atomic(
        &out,
        "precision.json",
        &json!({"controllers":precision,"complete":true}),
    );
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),
        "discovery_sha256":discovery_hash,"matches":reports,"precision":precision,"gate":qualification,
        "repeated_match_trace_parity":repeated_matches,"repeated_precision_trace_parity":repeated_precision,
        "complete":true,"no_training_or_selection":true,"production_controls_changed":false}),
    );
}

#[test]
fn fresh_check_freezes_disjoint_geometry_and_requires_every_completion() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let cases = fresh_precision(&plan);
    let known: Value = serde_json::from_slice(FOCUS_PLAN).unwrap();
    let old = retention_cases(&known);
    assert_eq!(cases.len(), 96);
    let identity = |c: &ship_arrival::ArrivalCase| {
        let mut c = c.clone();
        c.name.clear();
        c.seed = 0;
        c
    };
    for (i, case) in cases.iter().enumerate() {
        assert!(!old.iter().any(|c| identity(c) == identity(case)));
        assert!(!cases[..i].iter().any(|c| identity(c) == identity(case)));
        assert!(
            !plan["validation_seeds"]
                .as_array()
                .unwrap()
                .contains(&json!(case.seed))
        );
    }
    assert_eq!(
        gate(&plan, 64, &[64; 4], 256, 256, 96, true)["passed"],
        true
    );
    for (worlds, strata, available, arrivals, precision, retained) in [
        (31, [64; 4], 256, 256, 96, true),
        (64, [47, 64, 64, 64], 239, 239, 96, true),
        (64, [64; 4], 256, 255, 96, true),
        (64, [64; 4], 256, 256, 95, true),
        (64, [64; 4], 256, 256, 96, false),
    ] {
        assert_eq!(
            gate(
                &plan, worlds, &strata, available, arrivals, precision, retained
            )["passed"],
            false
        );
    }
}

#[test]
#[ignore = "frozen fresh qualification; explicit mode and root required"]
fn learned_guidance_fresh_check() {
    let root = std::env::var_os("SPACEWARS_FRESH_CHECK_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), PLAN);
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    for (name, digest) in plan["inputs"].as_object().unwrap() {
        assert_eq!(
            hash(&fs::read(root.join(name)).unwrap()),
            digest.as_str().unwrap()
        );
    }
    assert_eq!(
        hash(&fs::read(root.join("novelty.json")).unwrap()),
        plan["novelty_sha256"]
    );
    match std::env::var("SPACEWARS_FRESH_CHECK_MODE")
        .unwrap()
        .as_str()
    {
        "preflight" => preflight(root, &plan),
        "qualify" => qualify(root, &plan),
        other => panic!("unknown mode {other}"),
    }
}
