//! Controlled brake-threshold sweep over inspected match and precision starts.
use super::*;
use std::collections::BTreeSet;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-brake-bias-v1-plan.json"
));

fn shifted(model: &DenseModel, offset: f32) -> DenseModel {
    assert!(offset.is_finite());
    let mut genome = model.genome().clone();
    let layer = genome.layers.last_mut().unwrap();
    assert_eq!(layer.outputs, 3);
    assert_eq!(layer.activation, spacewars_ai::model::Activation::Linear);
    layer.biases[2] += offset;
    DenseModel::new(genome).unwrap()
}

fn precision_traces(
    baseline: &DenseModel,
    candidate: &DenseModel,
    prior: &[ship_arrival::ArrivalEpisode],
    current: &[ship_arrival::ArrivalEpisode],
    out: &Path,
) -> Value {
    let mut names = Vec::new();
    for result in current {
        if result.outcome == ship_arrival::ArrivalOutcome::Arrived {
            continue;
        }
        let original = prior.iter().find(|p| p.case == result.case).unwrap();
        for (label, model, expected) in [
            ("baseline", baseline, original),
            ("changed", candidate, result),
        ] {
            let mut traced =
                ship_arrival::run_episode(result.case.clone(), Policy::Model(model.clone()), true)
                    .unwrap();
            save(
                out.join(format!("{label}-{}.json", result.case.name)),
                &traced,
            );
            traced.trace.clear();
            assert_eq!(&traced, expected, "tracing changed a precision episode");
        }
        names.push(result.case.name.clone());
    }
    json!({"cases":names,"endpoint_parity":true})
}

#[test]
#[ignore = "frozen brake-bias diagnostic; explicit mode and fresh output required"]
fn learned_guidance_brake_bias() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_BIAS_MODE").unwrap();
    assert!(["preflight", "sweep"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_BIAS_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let bytes = fs::read(std::env::var_os("SPACEWARS_BIAS_MODEL").unwrap()).unwrap();
    assert_eq!(hash(&bytes), plan["baseline_file_sha256"]);
    let baseline = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"].as_str().unwrap());
    let cases = sources(serde_json::from_value(plan["approach_cases"].clone()).unwrap());
    let retained = retention_cases(&plan);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let prior = fs::read(std::env::var_os("SPACEWARS_BIAS_PRIOR").unwrap()).unwrap();
    assert_eq!(hash(&prior), plan["prior_preflight_sha256"]);
    let prior: Value = serde_json::from_slice(&prior).unwrap();
    let starts = records(&cases);
    let expected: Vec<_> = prior["training"]
        .as_array()
        .unwrap()
        .iter()
        .chain(prior["validation"].as_array().unwrap())
        .cloned()
        .collect();
    // Compare through the same JSON reader. In-memory f64 projections can
    // differ by a final decimal digit from the parsed historical receipt.
    // The complete world and mission hashes are also part of these records.
    let readback = |value: &Value| {
        serde_json::from_slice::<Value>(&serde_json::to_vec(value).unwrap()).unwrap()
    };
    assert_eq!(
        readback(&json!(starts)),
        json!(expected),
        "inspected physical starts changed"
    );
    assert_eq!(readback(&json!(retained)), prior["retention_cases"]);
    let receipt = serde_json::to_vec_pretty(&json!({
        "runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"baseline":baseline.id(),
        "approaches":starts,"retention_cases":retained,"protected_cases":protected
    }))
    .unwrap();
    if mode == "sweep" {
        let frozen = fs::read(std::env::var_os("SPACEWARS_BIAS_PREFLIGHT").unwrap()).unwrap();
        assert_eq!(receipt, frozen, "frozen source or executable changed");
    }
    fs::write(out.join("plan.json"), PLAN).unwrap();
    fs::write(out.join("preflight.json"), receipt).unwrap();
    if mode == "preflight" {
        return;
    }

    let pool = EpisodePool::new(worker_config()).unwrap();
    let mut variants: Vec<_> = plan["bias_offsets"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let offset = value.as_f64().unwrap() as f32;
            (
                format!("bias-{index:02}"),
                Some(offset),
                shifted(&baseline, offset),
            )
        })
        .collect();
    variants.push(("brake-on".into(), None, diagnostic::brake_on(&baseline)));
    let mut reports = Vec::new();
    let mut baseline_precision = Vec::new();
    let mut first_precision_loss = None;
    let started = std::time::Instant::now();
    for (label, offset, model) in variants {
        let directory = out.join(&label);
        let traces = directory.join("approaches");
        fs::create_dir(&directory).unwrap();
        fs::create_dir(&traces).unwrap();
        save(directory.join("genome.json"), model.genome());
        let approaches = pool.map(&cases, |(case, source)| {
            episode(
                &source.lock().unwrap(),
                case,
                "model",
                &model,
                Some(&traces),
            )
        });
        let precision: Vec<_> = pool.map(&retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false).unwrap()
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
        if offset == Some(0.0) {
            assert_eq!(
                (
                    precision_completed,
                    protected_completed,
                    additional_completed
                ),
                (157, 90, 0)
            );
            baseline_precision = precision.clone();
        } else if offset.is_some()
            && precision_completed < retained.len()
            && first_precision_loss.is_none()
        {
            // Select using the declared rule; trace only after the fixed sweep.
            first_precision_loss = Some((label.clone(), model.clone(), precision.clone()));
        }
        eprintln!(
            "{label} {offset:?}: {precision_completed}/157 precision, {protected_completed}/90 protected, {additional_completed}/8 additional"
        );
        reports.push(json!({"label":label,"offset":offset,"genome":model.id(),
            "precision_completed":precision_completed,"protected_completed":protected_completed,
            "additional_completed":additional_completed,"approaches":approaches,"precision":precision}));
        save(
            out.join("results.json"),
            &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(PLAN),"variants":reports,"complete":false}),
        );
        save(out.join("workers.json"), &pool.control().snapshot());
    }
    let sweep_seconds = started.elapsed().as_secs_f64();
    let precision_trace = first_precision_loss.map(|(label, model, episodes)| {
        let traces = out.join("first-precision-loss");
        fs::create_dir(&traces).unwrap();
        json!({"variant":label,"trace":precision_traces(&baseline, &model, &baseline_precision, &episodes, &traces)})
    });
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"variants":reports,"complete":true,"precision_trace":precision_trace,
        "sweep_seconds":sweep_seconds,"total_seconds":started.elapsed().as_secs_f64(),
        "training_or_production_controls_changed":false}),
    );
}

#[test]
fn brake_bias_preserves_other_outputs_and_zero_offset_identity() {
    let model = DenseModel::from_json(GENOME).unwrap();
    assert_eq!(shifted(&model, 0.0).id(), model.id());
    for offset in [0.03125, 0.5, 2.0] {
        let changed = shifted(&model, offset);
        let mut restored = changed.genome().clone();
        restored.layers.last_mut().unwrap().biases[2] =
            model.genome().layers.last().unwrap().biases[2];
        assert_eq!(&restored, model.genome());
        for inputs in [[0.0; 9], [0.25; 9], [-0.5; 9], [1.0; 9]] {
            let a = model.infer(&inputs).unwrap();
            let b = changed.infer(&inputs).unwrap();
            assert_eq!(a[..2], b[..2]);
            assert!((b[2] - a[2] - offset).abs() < 0.00001);
        }
    }
}

#[test]
fn brake_bias_recipe_covers_all_inspected_starts_and_fixed_interventions() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let cases: Vec<Case> = serde_json::from_value(plan["approach_cases"].clone()).unwrap();
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cases.len(), 98);
    assert_eq!(names.len(), 98);
    let physical: BTreeSet<_> = cases
        .iter()
        .map(|c| (c.seed, c.shape.label(), c.seat, c.source_tick, c.planet))
        .collect();
    assert_eq!(physical.len(), 98);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let additional: BTreeSet<String> =
        serde_json::from_value(plan["additional_cases"].clone()).unwrap();
    assert_eq!((protected.len(), additional.len()), (90, 8));
    assert!(protected.is_disjoint(&additional));
    assert_eq!(
        protected
            .union(&additional)
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        names
    );
    assert_eq!(retention_cases(&plan).len(), 157);
    let offsets: Vec<f32> = serde_json::from_value(plan["bias_offsets"].clone()).unwrap();
    assert_eq!(offsets.len(), 13);
    assert_eq!(offsets[0], 0.0);
    assert!(offsets.iter().all(|v| v.is_finite() && *v >= 0.0));
    assert!(offsets.windows(2).all(|w| w[0] < w[1]));
    assert_eq!((offsets.len() + 1) * (98 + 157), 3570);
    assert_eq!(plan["constant_brake_control"], true);
}
