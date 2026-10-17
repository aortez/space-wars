//! Controlled brake intervention and precision traces on inspected cases only.
use super::*;

const DIAGNOSTIC_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-failure-diagnostic-v1-plan.json"
));

fn brake_on(model: &DenseModel) -> DenseModel {
    let mut genome = model.genome().clone();
    let output = genome.layers.last_mut().unwrap();
    assert_eq!(output.outputs, 3);
    assert_eq!(output.activation, spacewars_ai::model::Activation::Linear);
    let width = output.weights.len() / output.outputs;
    output.weights[2 * width..].fill(0.0);
    output.biases[2] = 1.0;
    DenseModel::new(genome).unwrap()
}

#[test]
fn full_retention_covers_every_original_physical_start_once() {
    let plan: Value = serde_json::from_slice(DIAGNOSTIC_PLAN).unwrap();
    let cases = retention_cases(&plan);
    assert_eq!(cases.len(), 157);
    let identity = |mut case: ship_arrival::ArrivalCase| {
        case.name.clear();
        case
    };
    let starts: Vec<_> = cases.iter().cloned().map(identity).collect();
    let original: Vec<_> = ship_curriculum::PARTITIONS
        .into_iter()
        .flat_map(ship_curriculum::cases)
        .collect();
    assert_eq!(original.len(), 167);
    for case in original {
        assert_eq!(
            starts
                .iter()
                .filter(|s| **s == identity(case.clone()))
                .count(),
            1
        );
    }
    for name in plan["precision_traces"].as_array().unwrap() {
        assert!(cases.iter().any(|c| c.name == name.as_str().unwrap()));
    }
}

#[test]
fn brake_intervention_preserves_turn_and_thrust_on_identical_inputs() {
    let parent = DenseModel::from_json(GENOME).unwrap();
    let changed = brake_on(&parent);
    assert!(changed.genome().same_architecture(parent.genome()));
    assert_eq!(
        changed.genome().parameter_count(),
        parent.genome().parameter_count()
    );
    for value in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        let mut inputs = [0.0; 9];
        for index in 0..9 {
            inputs[index] = value;
            let before = parent.infer(&inputs).unwrap();
            let after = changed.infer(&inputs).unwrap();
            assert_eq!(before[..2], after[..2]);
            assert_eq!(after[2], 1.0);
            assert!(Movement::decode(&after).unwrap().brake);
        }
    }
}

#[test]
#[ignore = "explicit retained model and fresh evidence directory required"]
fn learned_guidance_failure_diagnostic() {
    let plan: Value = serde_json::from_slice(DIAGNOSTIC_PLAN).unwrap();
    let model_path = std::env::var_os("SPACEWARS_GUIDANCE_DIAG_MODEL").unwrap();
    let model_bytes = fs::read(model_path).unwrap();
    assert_eq!(hash(&model_bytes), plan["finalist_file_sha256"]);
    let finalist = DenseModel::from_json(&model_bytes).unwrap();
    assert_eq!(finalist.id(), plan["finalist_genome"].as_str().unwrap());
    let parent = DenseModel::from_json(GENOME).unwrap();
    assert_eq!(parent.id(), plan["parent_genome"].as_str().unwrap());
    assert_eq!(hash(GENOME), plan["parent_file_sha256"]);
    let intervention = brake_on(&finalist);
    let out = std::env::var_os("SPACEWARS_GUIDANCE_DIAG_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier diagnostic evidence");
    fs::write(out.join("plan.json"), DIAGNOSTIC_PLAN).unwrap();
    save(out.join("parent.json"), parent.genome());
    save(out.join("finalist.json"), finalist.genome());
    save(out.join("brake-on.json"), intervention.genome());

    let cases = retention_cases(&plan);
    assert_eq!(cases.len(), 157);
    let approaches = sources(serde_json::from_value(plan["approach_cases"].clone()).unwrap());
    assert_eq!(approaches.len(), 43);
    save(
        out.join("initial-conditions.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
            "plan_sha256":hash(DIAGNOSTIC_PLAN),"retention_cases":cases,
            "approaches":records(&approaches),"parent":parent.id(),
            "finalist":finalist.id(),"brake_on":intervention.id()}),
    );

    let mut reports = Vec::new();
    for (label, driver, model) in [
        ("reference", "reference", &parent),
        ("parent", "model", &parent),
        ("finalist", "model", &finalist),
        ("brake_on", "model", &intervention),
    ] {
        let precision_out = out.join(format!("{label}-precision"));
        let approach_out = out.join(format!("{label}-approaches"));
        fs::create_dir(&precision_out).unwrap();
        fs::create_dir(&approach_out).unwrap();
        let mut retained = Vec::new();
        let mut score = Fitness::default();
        for case in cases.iter().cloned() {
            let trace = plan["precision_traces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name.as_str().unwrap() == case.name);
            let mut result = ship_arrival::run_episode(
                case,
                if driver == "reference" {
                    Policy::Reference
                } else {
                    Policy::Model(model.clone())
                },
                trace,
            )
            .unwrap();
            if trace {
                save(
                    precision_out.join(format!("{}.json", result.case.name)),
                    &json!(result),
                );
            }
            result.trace.clear();
            score.retained_completed += result.fitness.completed;
            score.retained_position_points += result.fitness.position_points;
            score.retained_velocity_points += result.fitness.velocity_points;
            score.spare_ticks += result.fitness.spare_ticks;
            retained.push(result);
        }
        let match_episodes: Vec<_> = approaches
            .iter()
            .map(|(case, source)| episode(source, case, driver, model, Some(&approach_out)))
            .collect();
        for episode in &match_episodes {
            score.record_approach(episode);
        }
        eprintln!(
            "{label}: {}/157 practice, {}/43 approaches",
            score.retained_completed, score.approach_completed
        );
        reports.push(
            json!({"controller":label,"genome":if driver=="model" {Some(model.id())} else {None},
            "fitness":score,"retention":retained,"approaches":match_episodes}),
        );
        save(
            out.join("results.json"),
            &json!({"experiment":plan["experiment"],"plan_sha256":hash(DIAGNOSTIC_PLAN),
                "runtime":engine_agent::evolution::runtime_id().unwrap(),
                "controllers":reports,"training_or_production_controls_changed":false}),
        );
    }
}
