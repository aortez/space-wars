//! Fixed-model coverage and a separately retained solar-route diagnosis.
use super::*;

const BROAD_PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-broad-v1-plan.json"
));

fn saved(value: Value) -> Value {
    // Match the JSON reader's f64 representation when comparing a freshly
    // serialized f32 field with retained evidence; world hashes stay exact.
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn geometry(run: &MaterialMissionClientScenario, case: &Case) -> Value {
    // Query a clone so diagnostic caches cannot alter the replayed world.
    let mut observed = fork(run);
    let observation = observed.sortie.state.mission_observation_with_cadence(
        case.seat,
        observed.pilots[case.seat].sensor_request(),
        Default::default(),
    );
    let pilot = &observation.local.combat.recovery.flight.pilot;
    let checked = probe(run, case.seat);
    json!({"tick":pilot.tick,"sun":observation.sun,
        "planets":observation.planets.iter().map(|p|json!({"index":p.index,"motion":p.motion,"radius":p.radius})).collect::<Vec<_>>(),
        "queries_ready":pilot.queries_ready,"host":checked.pilots[case.seat].telemetry(),
        "flight":flight(run,case.seat)})
}

fn replay_geometry(
    source: &MaterialMissionClientScenario,
    case: &Case,
    driver: &str,
    result: &Episode,
    out: &Path,
) {
    let input = fs::read_to_string(out.join(format!("{}-{driver}.jsonl", case.name))).unwrap();
    let rows: Vec<Value> = input
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len() as u64, result.elapsed_ticks);
    let mut run = fork(source);
    run.bots[case.seat] = driver == "mission";
    let mut observations = Vec::new();
    for row in &rows {
        assert_eq!(saved(json!(flight(&run, case.seat))), row["before"]);
        observations.push(geometry(&run, case));
        if row["native_handoff"] == true || driver == "mission" {
            run = probe(&run, case.seat);
        } else {
            let action = &row["decision"]["movement"];
            let movement = Movement {
                turn: action["turn"].as_f64().unwrap() as f32,
                thrust: action["thrust"].as_bool().unwrap(),
                brake: action["brake"].as_bool().unwrap(),
            };
            run.step(&movement.encode(result.initial.owner), STEP);
        }
        assert_eq!(
            saved(json!(run.sortie.state.free_flight_observation(case.seat))),
            row["after"]
        );
        assert_eq!(
            hash(&SurfaceSortieScenario::observe(&run.sortie.state).payload),
            row["world_sha256"]
        );
    }
    assert_eq!(
        json!(run.sortie.state.free_flight_observation(case.seat)),
        json!(result.final_flight)
    );
    observations.push(geometry(&run, case));
    if let Some(reason) = &result.host_reason {
        assert_eq!(&observations.last().unwrap()["host"], reason);
    }
    save(
        out.join(format!("{}-geometry.json", case.name)),
        &observations,
    );
}

#[test]
#[ignore = "fixed-model evaluation; frozen models and fresh output required"]
fn learned_guidance_broad_evaluation() {
    let plan: Value = serde_json::from_slice(BROAD_PLAN).unwrap();
    let root = std::env::var_os("SPACEWARS_BROAD_ROOT").unwrap();
    let root = Path::new(&root);
    assert_eq!(fs::read(root.join("plan.json")).unwrap(), BROAD_PLAN);
    let out = root.join("evaluation");
    fs::create_dir(&out).expect("preserve existing evidence");
    let models: Vec<_> = plan["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let bytes = fs::read(root.join(row["path"].as_str().unwrap())).unwrap();
            assert_eq!(hash(&bytes), row["sha256"]);
            let model = DenseModel::from_json(&bytes).unwrap();
            assert_eq!(model.id(), row["genome"].as_str().unwrap());
            (row["label"].as_str().unwrap().to_string(), model)
        })
        .collect();
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    save(
        out.join("models-freeze.json"),
        &json!({"runtime":runtime,"plan_sha256":hash(BROAD_PLAN),"models":plan["models"]}),
    );
    let pool = EpisodePool::new(worker_config()).unwrap();

    // Reproduce the inspected case before evaluating the new cohort.
    let bytes = fs::read(root.join("diagnostic-inputs.json")).unwrap();
    assert_eq!(hash(&bytes), plan["diagnostic_sha256"]);
    let diagnostic: Value = serde_json::from_slice(&bytes).unwrap();
    let retained = sources(
        diagnostic["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| serde_json::from_value(row["case"].clone()).unwrap())
            .collect(),
    );
    assert_eq!(saved(json!(records(&retained))), diagnostic["sources"]);
    let prior = Path::new(plan["prior_directory"].as_str().unwrap());
    let mut diagnosis = Vec::new();
    for (label, driver, model) in models
        .iter()
        .map(|(l, m)| (l.as_str(), "model", m))
        .chain(std::iter::once(("native", "mission", &models[0].1)))
    {
        let traces = out.join(format!("{label}-diagnostic"));
        fs::create_dir(&traces).unwrap();
        let mut episodes = Vec::new();
        for (case, source) in &retained {
            let source = source.lock().unwrap();
            let result = episode(&source, case, driver, model, Some(&traces));
            if driver == "model" {
                let expected = diagnostic["expected"][label]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["case"] == case.name)
                    .unwrap();
                assert_eq!(&saved(json!(result)), expected, "saved diagnostic changed");
                let name = format!("{}-model.jsonl", case.name);
                assert_eq!(
                    fs::read(traces.join(&name)).unwrap(),
                    fs::read(prior.join(format!("evaluate/{label}-validation/{name}"))).unwrap()
                );
            }
            replay_geometry(&source, case, driver, &result, &traces);
            episodes.push(result);
        }
        diagnosis.push(json!({"controller":label,"driver":driver,"episodes":episodes}));
    }
    save(
        out.join("diagnosis.json"),
        &json!({"controllers":diagnosis,"saved_trace_parity":true}),
    );
    eprintln!("Retained solar-route diagnosis and exact replay parity passed");

    // Availability alone chooses the cohort. No continuation outcome is read.
    let (cases, audit) = focused::discover(&plan);
    assert_eq!(
        cases.len(),
        4 * plan["validation_worlds"].as_u64().unwrap() as usize
    );
    let initial = records(&cases);
    save(
        out.join("sources.json"),
        &json!({"sources":initial,"discovery":audit}),
    );
    eprintln!(
        "Frozen {} starts from {} fresh worlds",
        cases.len(),
        cases.len() / 4
    );
    let mut reports = Vec::new();
    for (label, driver, model) in models
        .iter()
        .map(|(l, m)| (l.as_str(), "model", m))
        .chain(std::iter::once(("native", "mission", &models[0].1)))
    {
        let traces = out.join(format!("{label}-fresh"));
        fs::create_dir(&traces).unwrap();
        let episodes = pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, driver, model, Some(&traces))
        });
        assert_eq!(records(&cases), initial, "evaluation mutated a source");
        let arrived = episodes
            .iter()
            .filter(|e| e.outcome == Outcome::Arrived)
            .count();
        eprintln!("{label}: {arrived}/{} fresh arrivals", cases.len());
        reports.push(json!({"controller":label,"driver":driver,"genome":if driver=="model" {Some(model.id())}else{None},"arrived":arrived,"episodes":episodes}));
        save(
            out.join("results.json"),
            &json!({"runtime":runtime,"plan_sha256":hash(BROAD_PLAN),"controllers":reports,"production_controls_changed":false,"live_integration_qualified":false}),
        );
    }
    save(
        out.join("complete.json"),
        &json!({"source_parity":true,"controllers":6,"cases":cases.len(),"workers":pool.control().snapshot()}),
    );
}
