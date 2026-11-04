//! Full-suite retention execution and post-selection settling measurements.
use super::*;

pub(super) fn episodes(
    cases: &[ship_arrival::ArrivalCase],
    model: &DenseModel,
    workers: usize,
) -> Result<Vec<ship_arrival::ArrivalEpisode>, EvolutionError> {
    assert!(!cases.is_empty() && workers > 0);
    let evaluate = |cases: &[ship_arrival::ArrivalCase]| {
        cases
            .iter()
            .cloned()
            .map(|case| ship_arrival::run_episode(case, Policy::Model(model.clone()), false))
            .collect::<Result<Vec<_>, _>>()
    };
    if workers == 1 {
        return evaluate(cases);
    }
    // Worlds and controllers are case-local. Joining in source order preserves
    // the exact aggregation order and the first error from the serial recipe.
    std::thread::scope(|scope| {
        let jobs: Vec<_> = cases
            .chunks(cases.len().div_ceil(workers))
            .map(|chunk| scope.spawn(move || evaluate(chunk)))
            .collect();
        let mut results = Vec::with_capacity(cases.len());
        for job in jobs {
            results.extend(job.join().expect("retention worker panicked")?);
        }
        Ok(results)
    })
}

pub(super) fn settling(
    original: &ship_curriculum::Evaluation,
    plan: &Value,
    model: &DenseModel,
    driver: &str,
    label: &str,
    out: &Path,
) -> Value {
    let traces = out.join(format!("{label}-precision"));
    fs::create_dir(&traces).unwrap();
    let reports: Vec<_> = retention_cases(plan)
        .into_iter()
        .map(|case| {
            let expected = original
                .partitions
                .iter()
                .flat_map(|p| &p.episodes)
                .find(|e| e.case == case)
                .unwrap();
            let mut result = ship_arrival::run_episode(
                case,
                if driver == "reference" {
                    Policy::Reference
                } else {
                    Policy::Model(model.clone())
                },
                true,
            )
            .unwrap();
            let keep = result.outcome != ship_arrival::ArrivalOutcome::Arrived
                || plan["precision_traces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n.as_str().unwrap() == result.case.name);
            if keep {
                save(traces.join(format!("{}.json", result.case.name)), &result);
            }
            let report = json!({"case":result.case.name,"outcome":result.outcome,
                "elapsed_ticks":result.elapsed_ticks,"measure":result.measure,
                "longest_dwell":result.trace.iter().map(|t|t.measure.dwell).max().unwrap_or(0),
                "ticks_within_both_tolerances":result.trace.iter().filter(|t|t.after.as_ref().is_some_and(|a|a.tick==t.before.tick+1)
                    && t.measure.distance<=ship_arrival::POSITION_TOLERANCE
                    && t.measure.relative_speed<=ship_arrival::SPEED_TOLERANCE).count(),
                "trace_retained":keep});
            result.trace.clear();
            assert_eq!(&result, expected, "settling traces changed the episode");
            report
        })
        .collect();
    json!({"unique_cases":reports.len(),"endpoint_parity":true,"cases":reports})
}

#[test]
fn full_curriculum_protects_every_start_and_reserves_different_worlds() {
    let previous: Value = serde_json::from_slice(LEARNING_PLAN_V2).unwrap();
    let plan: Value = serde_json::from_slice(LEARNING_PLAN_V3).unwrap();
    let diagnostic: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-failure-diagnostic-v1-plan.json"
    )))
    .unwrap();
    assert_eq!(retention_cases(&plan), retention_cases(&diagnostic));
    assert_eq!(retention_cases(&plan).len(), 157);
    for field in ["training_cases", "evolution", "fitness_order"] {
        assert_eq!(plan[field], previous[field]);
    }
    assert_eq!(plan["development_cases"].as_array().unwrap().len(), 40);
    let identity = |case: &Value| {
        ["seed", "shape", "seat", "source_tick", "planet"].map(|key| case[key].to_string())
    };
    let inspected: std::collections::BTreeSet<_> = plan["training_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(plan["development_cases"].as_array().unwrap())
        .map(identity)
        .collect();
    assert_eq!(inspected.len(), 43);
    for case in diagnostic["approach_cases"].as_array().unwrap() {
        assert!(inspected.contains(&identity(case)));
        assert!(
            !plan["validation_seeds"]
                .as_array()
                .unwrap()
                .contains(&case["seed"])
        );
    }
    assert_eq!(plan["validation_seeds"].as_array().unwrap().len(), 4);
    let config = configuration(&plan);
    assert_eq!(config.population * config.generations * (157 + 15), 66048);
}

#[test]
fn independent_retention_workers_preserve_complete_episode_results() {
    let model = DenseModel::from_json(GENOME).unwrap();
    let cases = ship_curriculum::cases(Partition::FreshMoving);
    assert_eq!(
        episodes(&cases[..4], &model, 1).unwrap(),
        episodes(&cases[..4], &model, 4).unwrap()
    );
    let mut invalid = cases[..4].to_vec();
    invalid[1].max_ticks = 1;
    assert_eq!(
        episodes(&invalid, &model, 1).unwrap_err().to_string(),
        episodes(&invalid, &model, 4).unwrap_err().to_string()
    );
}
