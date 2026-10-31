//! Inspect individual brake-readout changes without continuing evolution.
use super::*;
use std::collections::BTreeSet;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-brake-neighborhood-v1-plan.json"
));

fn shifted(model: &DenseModel, coordinate: usize, delta: f32) -> DenseModel {
    assert!(coordinate < 9 && delta.is_finite());
    let mut genome = model.genome().clone();
    let layer = genome.layers.last_mut().unwrap();
    assert_eq!(layer.outputs, 3);
    assert_eq!(layer.weights.len(), 24);
    assert_eq!(layer.activation, spacewars_ai::model::Activation::Linear);
    if coordinate < 8 {
        layer.weights[16 + coordinate] += delta;
    } else {
        layer.biases[2] += delta;
    }
    DenseModel::new(genome).unwrap()
}

struct Variant {
    label: String,
    family: &'static str,
    coordinate: Option<usize>,
    delta: Option<f32>,
    previous_fitness: Option<Value>,
    model: DenseModel,
}

fn variants(plan: &Value, baseline: &DenseModel, checkpoint: &Value) -> Vec<Variant> {
    let mut result = vec![Variant {
        label: "baseline".into(),
        family: "baseline",
        coordinate: None,
        delta: None,
        previous_fitness: None,
        model: baseline.clone(),
    }];
    for coordinate in 0..9 {
        for (i, delta) in plan["offsets"].as_array().unwrap().iter().enumerate() {
            let delta = delta.as_f64().unwrap() as f32;
            result.push(Variant {
                label: format!("coordinate-{coordinate}-step-{i:02}"),
                family: "coordinate",
                coordinate: Some(coordinate),
                delta: Some(delta),
                previous_fitness: None,
                model: shifted(baseline, coordinate, delta),
            });
        }
    }
    result.push(Variant {
        label: "brake-on".into(),
        family: "constant",
        coordinate: None,
        delta: None,
        previous_fitness: None,
        model: diagnostic::brake_on(baseline),
    });
    let run = &checkpoint["run"];
    let last = run["history"].as_array().unwrap().last().unwrap();
    assert_eq!(run["history"].as_array().unwrap().len(), 24);
    assert_eq!(last["best_id"], baseline.id());
    for slot in plan["population_slots"].as_array().unwrap() {
        let slot = slot.as_u64().unwrap() as usize;
        let genome = serde_json::from_value(run["population"][slot]["genome"].clone()).unwrap();
        let model = DenseModel::new(genome).unwrap();
        assert_eq!(model.id(), last["population_ids"][slot]);
        result.push(Variant {
            label: format!("population-{slot:02}"),
            family: "population",
            coordinate: None,
            delta: None,
            previous_fitness: Some(run["fitness"][slot].clone()),
            model,
        });
    }
    assert_eq!(
        result.len(),
        plan["variant_count"].as_u64().unwrap() as usize
    );
    result
}

fn trace_indices(reports: &[Value]) -> Vec<usize> {
    let mut selected = vec![
        0,
        reports
            .iter()
            .position(|r| r["label"] == "brake-on")
            .unwrap(),
    ];
    for family in 0..3 {
        let mut best = None;
        for (i, report) in reports.iter().enumerate() {
            let score: [u32; 5] = serde_json::from_value(report["score"].clone()).unwrap();
            let eligible = match family {
                0 => report["family"] == "coordinate" && score[..2] == [157, 108],
                1 => report["family"] == "coordinate" && score[2] > 0,
                _ => report["family"] == "population",
            };
            if eligible && best.is_none_or(|(_, previous)| score > previous) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            if !selected.contains(&i) {
                selected.push(i);
            }
        }
    }
    selected
}

#[test]
#[ignore = "frozen local diagnostic; explicit mode and fresh output required"]
fn learned_guidance_brake_neighborhood() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_NEIGHBOR_MODE").unwrap();
    assert!(["preflight", "sweep"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_NEIGHBOR_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let load = |variable: &str, field: &str| {
        let bytes = fs::read(std::env::var_os(variable).unwrap()).unwrap();
        assert_eq!(hash(&bytes), plan[field]);
        bytes
    };
    let baseline =
        DenseModel::from_json(&load("SPACEWARS_NEIGHBOR_MODEL", "baseline_file_sha256")).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"]);
    let prior: Value =
        serde_json::from_slice(&load("SPACEWARS_NEIGHBOR_PRIOR", "prior_preflight_sha256"))
            .unwrap();
    let checkpoint: Value = serde_json::from_slice(&load(
        "SPACEWARS_NEIGHBOR_CHECKPOINT",
        "prior_checkpoint_sha256",
    ))
    .unwrap();
    let cases = sources(serde_json::from_value(plan["approach_cases"].clone()).unwrap());
    let retained = retention_cases(&plan);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let expected: Vec<_> = prior["training"]
        .as_array()
        .unwrap()
        .iter()
        .chain(prior["validation"].as_array().unwrap())
        .cloned()
        .collect();
    let starts = records(&cases);
    let readback =
        |v: &Value| serde_json::from_slice::<Value>(&serde_json::to_vec(v).unwrap()).unwrap();
    assert_eq!(readback(&json!(starts)), json!(expected));
    assert_eq!(readback(&json!(retained)), prior["retention_cases"]);
    let receipt = serde_json::to_vec_pretty(&json!({
        "runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),
        "baseline":baseline.id(),"approaches":starts,"retention_cases":retained,"protected_cases":protected
    })).unwrap();
    if mode == "sweep" {
        assert_eq!(
            receipt,
            fs::read(std::env::var_os("SPACEWARS_NEIGHBOR_PREFLIGHT").unwrap()).unwrap()
        );
    }
    fs::write(out.join("plan.json"), PLAN).unwrap();
    fs::write(out.join("preflight.json"), receipt).unwrap();
    if mode == "preflight" {
        return;
    }

    let variants = variants(&plan, &baseline, &checkpoint);
    let pool = EpisodePool::new(worker_config()).unwrap();
    let old_protected: BTreeSet<String> =
        serde_json::from_value(prior["protected_cases"].clone()).unwrap();
    let mut reports = Vec::new();
    let mut endpoints = Vec::new();
    let started = std::time::Instant::now();
    for variant in &variants {
        let directory = out.join(&variant.label);
        fs::create_dir(&directory).unwrap();
        save(directory.join("genome.json"), variant.model.genome());
        let approaches = pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", &variant.model, None)
        });
        let precision: Vec<_> = pool.map(&retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(variant.model.clone()), false)
                .unwrap()
        });
        let precision_completed = precision
            .iter()
            .filter(|e| e.outcome == ship_arrival::ArrivalOutcome::Arrived)
            .count();
        let protected_completed = approaches
            .iter()
            .filter(|e| protected.contains(&e.case) && e.outcome == Outcome::Arrived)
            .count();
        let additional_completed = approaches
            .iter()
            .filter(|e| !protected.contains(&e.case) && e.outcome == Outcome::Arrived)
            .count();
        let mut progress = Fitness::default();
        for e in approaches.iter().filter(|e| !protected.contains(&e.case)) {
            progress.record_approach(e);
        }
        if variant.label == "baseline" {
            assert_eq!(
                (
                    precision_completed,
                    protected_completed,
                    additional_completed
                ),
                (157, 108, 0)
            );
        }
        if let Some(previous) = &variant.previous_fitness {
            assert_eq!(
                precision_completed as u64,
                previous["precision_completed"].as_u64().unwrap()
            );
            let old_success = |member: bool| {
                approaches[..98]
                    .iter()
                    .filter(|e| {
                        old_protected.contains(&e.case) == member && e.outcome == Outcome::Arrived
                    })
                    .count() as u64
            };
            assert_eq!(old_success(true), previous["protected_completed"]);
            assert_eq!(old_success(false), previous["additional_completed"]);
        }
        save(
            directory.join("endpoints.json"),
            &json!({"approaches":approaches,"precision":precision}),
        );
        let report = json!({"label":variant.label,"family":variant.family,"coordinate":variant.coordinate,"delta":variant.delta,"genome":variant.model.id(),
            "previous_fitness":variant.previous_fitness,
            "score":[precision_completed,protected_completed,additional_completed,progress.approach_speed_points,progress.approach_position_points],
            "gained":approaches.iter().filter(|e| !protected.contains(&e.case) && e.outcome == Outcome::Arrived).map(|e| &e.case).collect::<Vec<_>>(),
            "lost_matches":approaches.iter().filter(|e| protected.contains(&e.case) && e.outcome != Outcome::Arrived).map(|e| &e.case).collect::<Vec<_>>(),
            "lost_precision":precision.iter().filter(|e| e.outcome != ship_arrival::ArrivalOutcome::Arrived).map(|e| &e.case.name).collect::<Vec<_>>()});
        eprintln!(
            "{}: {precision_completed}/157 precision, {protected_completed}/108 protected, {additional_completed}/6 additional",
            variant.label
        );
        reports.push(report);
        endpoints.push((approaches, precision));
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),"variants":reports,"complete":false}),
        );
    }
    let sweep_seconds = started.elapsed().as_secs_f64();
    let indices = trace_indices(&reports);
    let mut traces = Vec::new();
    for index in indices {
        let variant = &variants[index];
        let directory = out.join(&variant.label).join("traces");
        fs::create_dir(&directory).unwrap();
        let mut names = Vec::new();
        for ((case, source), expected) in cases.iter().zip(&endpoints[index].0) {
            if index == 0 || !protected.contains(&case.name) || expected.outcome != Outcome::Arrived
            {
                let traced = episode(
                    &source.lock().unwrap(),
                    case,
                    "model",
                    &variant.model,
                    Some(&directory),
                );
                assert_eq!(&traced, expected);
                names.push(case.name.clone());
            }
        }
        let mut precision_names = Vec::new();
        for (expected, original) in endpoints[index]
            .1
            .iter()
            .zip(&endpoints[0].1)
            .filter(|(e, _)| e.outcome != ship_arrival::ArrivalOutcome::Arrived)
            .take(3)
        {
            for (label, model, reference) in [
                ("baseline", &baseline, original),
                ("changed", &variant.model, expected),
            ] {
                let mut traced = ship_arrival::run_episode(
                    expected.case.clone(),
                    Policy::Model(model.clone()),
                    true,
                )
                .unwrap();
                save(
                    directory.join(format!("{label}-{}.json", expected.case.name)),
                    &traced,
                );
                traced.trace.clear();
                assert_eq!(&traced, reference);
            }
            precision_names.push(expected.case.name.clone());
        }
        traces.push(json!({"variant":variant.label,"match_cases":names,"precision_cases":precision_names,"endpoint_parity":true}));
    }
    save(out.join("workers.json"), &pool.control().snapshot());
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),
        "variants":reports,"complete":true,"traces":traces,"sweep_seconds":sweep_seconds,"total_seconds":started.elapsed().as_secs_f64(),
        "training_or_production_controls_changed":false}),
    );
}

#[cfg(test)]
mod tests;
