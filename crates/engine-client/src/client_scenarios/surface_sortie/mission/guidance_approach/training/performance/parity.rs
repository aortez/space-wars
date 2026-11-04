//! Replay previously inspected evaluations; never resume or modify training.
use super::*;

#[test]
#[ignore = "explicit historical v3 evaluation and fresh parity output required"]
fn learned_guidance_clone_trace_parity() {
    let input = std::env::var_os("SPACEWARS_TRAINING_PROFILE_INPUT").unwrap();
    let input = Path::new(&input);
    let out = std::env::var_os("SPACEWARS_TRAINING_PROFILE_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("preserve prior parity runs");
    let preflight: Value =
        serde_json::from_slice(&fs::read(input.join("preflight/preflight.json")).unwrap()).unwrap();
    let evaluation: Value =
        serde_json::from_slice(&fs::read(input.join("evaluation/results.json")).unwrap()).unwrap();
    let parent = DenseModel::from_json(GENOME).unwrap();
    let finalist =
        DenseModel::from_json(&fs::read(input.join("run/finalist.json")).unwrap()).unwrap();
    let mut groups = Vec::new();
    let mut total_ticks = 0_u64;
    let mut total_episodes = 0;
    let pool = std::env::var_os("SPACEWARS_TRAINING_WORKERS")
        .map(|_| EpisodePool::new(worker_config()).unwrap());
    for partition in ["training", "development", "validation"] {
        let expected_starts = preflight[partition].as_array().unwrap();
        // Use the retained starts, including the resolved reserved starts, so
        // this check does not search for new cases or change the curriculum.
        let starts = sources(
            expected_starts
                .iter()
                .map(|s| serde_json::from_value(s["case"].clone()).unwrap())
                .collect(),
        );
        for (actual, expected) in records(&starts).iter().zip(expected_starts) {
            for field in ["world_sha256", "mission_sha256"] {
                assert_eq!(actual[field], expected[field], "{partition}: {field}");
            }
        }
        for (label, driver, model) in [
            ("mission", "mission", &parent),
            ("reference", "reference", &parent),
            ("parent", "model", &parent),
            ("finalist", "model", &finalist),
        ] {
            let expected_controller = evaluation["controllers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["controller"] == label)
                .unwrap();
            let expected_group = expected_controller["approaches"]
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["partition"] == partition)
                .unwrap();
            let expected = expected_group["episodes"].as_array().unwrap();
            assert_eq!(starts.len(), expected.len());
            let directory = format!("{label}-{partition}");
            let traces = out.join(&directory);
            fs::create_dir(&traces).unwrap();
            let mut receipts = Vec::new();
            let evaluate = |(case, source): &Source| {
                episode(&source.lock().unwrap(), case, driver, model, Some(&traces))
            };
            let actual = match &pool {
                Some(pool) => pool.map(&starts, evaluate),
                None => starts.iter().map(evaluate).collect(),
            };
            for (((case, _), actual), expected) in starts.iter().zip(actual).zip(expected) {
                // The original report first converted episodes to Value,
                // promoting f32 fields to f64, then serialized them. Use the
                // same path before parsing both sides once.
                let actual_json: Value =
                    serde_json::from_slice(&serde_json::to_vec(&json!(&actual)).unwrap()).unwrap();
                assert_eq!(&actual_json, expected, "{directory}: {}", case.name);
                let file = format!("{}-{driver}.jsonl", case.name);
                let actual_trace = fs::read(traces.join(&file)).unwrap();
                let expected_trace =
                    fs::read(input.join("evaluation").join(&directory).join(file)).unwrap();
                assert!(
                    actual_trace == expected_trace,
                    "{directory}: {} controls, observations or world hashes changed",
                    case.name
                );
                let ticks = actual_trace.iter().filter(|byte| **byte == b'\n').count() as u64;
                assert_eq!(ticks, actual.elapsed_ticks);
                total_ticks += ticks;
                total_episodes += 1;
                receipts.push(
                    json!({"case":case.name,"ticks":ticks,"trace_sha256":hash(&actual_trace)}),
                );
            }
            eprintln!(
                "{directory}: {} episodes match historical traces",
                receipts.len()
            );
            groups.push(json!({"group":directory,"episodes":receipts}));
            save(
                out.join("parity.json"),
                &json!({"groups":groups,"episodes":total_episodes,
                "ticks":total_ticks,"complete":false}),
            );
        }
    }
    save(
        out.join("parity.json"),
        &json!({"groups":groups,"episodes":total_episodes,
        "ticks":total_ticks,"complete":true,"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "original_runtime":evaluation["runtime"],"trace_byte_parity":true,"episode_result_parity":true,
        "workers":pool.as_ref().map(|p|p.control().snapshot())}),
    );
}
