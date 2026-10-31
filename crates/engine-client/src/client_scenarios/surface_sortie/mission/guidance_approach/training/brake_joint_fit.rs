//! Export actual hidden activations, then replay a bounded joint brake fit.
//! The external linear fit is diagnostic; native f32 inference is authoritative.
use super::*;
use std::{collections::BTreeSet, io::BufRead};

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-brake-joint-fit-v1-plan.json"
));

#[derive(Debug, Deserialize, Serialize)]
struct Row {
    domain: String,
    case: String,
    tick: u64,
    inputs: [f32; 9],
    hidden: [f32; 8],
    outputs: [f32; 3],
    brake: bool,
}

fn hidden(model: &DenseModel, inputs: &[f32; 9]) -> [f32; 8] {
    let genome = model.genome();
    assert_eq!(genome.layers.len(), 2);
    let layer = &genome.layers[0];
    assert_eq!(layer.outputs, 8);
    assert_eq!(layer.activation, spacewars_ai::model::Activation::Tanh);
    let result: Vec<_> = layer
        .weights
        .chunks_exact(inputs.len())
        .zip(&layer.biases)
        .map(|(weights, bias)| {
            let mut sum = *bias;
            for (weight, value) in weights.iter().zip(inputs) {
                sum += weight * value;
            }
            sum.tanh()
        })
        .collect();
    result.try_into().unwrap()
}

fn record(
    model: &DenseModel,
    domain: &str,
    case: &str,
    tick: u64,
    inputs: [f32; 9],
    outputs: [f32; 3],
    brake: bool,
) -> Row {
    let hidden = hidden(model, &inputs);
    let layer = &model.genome().layers[1];
    assert_eq!(layer.activation, spacewars_ai::model::Activation::Linear);
    let inferred = model.infer(&inputs).unwrap();
    for (i, weights) in layer.weights.chunks_exact(hidden.len()).enumerate() {
        let mut sum = layer.biases[i];
        for (weight, value) in weights.iter().zip(hidden) {
            sum += weight * value;
        }
        assert_eq!(sum.to_bits(), inferred[i].to_bits());
        assert_eq!(sum.to_bits(), outputs[i].to_bits());
    }
    assert_eq!(brake, outputs[2] > 0.0);
    Row {
        domain: domain.into(),
        case: case.into(),
        tick,
        inputs,
        hidden,
        outputs,
        brake,
    }
}

fn write_rows(path: impl AsRef<Path>, rows: &[Row]) {
    let mut writer = BufWriter::new(File::create(path).unwrap());
    for row in rows {
        serde_json::to_writer(&mut writer, row).unwrap();
        writeln!(writer).unwrap();
    }
    writer.flush().unwrap();
}

fn read_rows(directory: &Path) -> Vec<Row> {
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths
        .iter()
        .flat_map(|p| {
            std::io::BufReader::new(File::open(p).unwrap())
                .lines()
                .map(|line| serde_json::from_str::<Row>(&line.unwrap()).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn readback(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn assert_brake_only(baseline: &DenseModel, candidate: &DenseModel, limit: f32) {
    let mut restored = candidate.genome().clone();
    let original = &baseline.genome().layers[1];
    let layer = &mut restored.layers[1];
    assert!(
        layer.weights[16..24]
            .iter()
            .chain(&layer.biases[2..3])
            .all(|x| x.is_finite() && x.abs() <= limit)
    );
    layer.weights[16..24].copy_from_slice(&original.weights[16..24]);
    layer.biases[2] = original.biases[2];
    assert_eq!(DenseModel::new(restored).unwrap().id(), baseline.id());
}

fn static_check(
    model: &DenseModel,
    rows: &[Row],
    protected: &BTreeSet<String>,
    targets: &BTreeSet<String>,
) -> Value {
    let mut preserved = 0;
    let mut flips = Vec::new();
    let mut starts = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        let outputs = model.infer(&row.inputs).unwrap();
        assert_eq!(outputs[..2], row.outputs[..2]);
        assert_eq!(hidden(model, &row.inputs), row.hidden);
        if row.domain == "precision" || protected.contains(&row.case) {
            preserved += 1;
            if (outputs[2] > 0.0) != row.brake {
                flips.push(json!({"domain":row.domain,"case":row.case,"tick":row.tick,"baseline":row.outputs[2],"changed":outputs[2]}));
            }
        } else if seen.insert(row.case.clone()) {
            starts.push(json!({"case":row.case,"requested":targets.contains(&row.case),"baseline":row.outputs[2],"changed":outputs[2],"brake":outputs[2]>0.0}));
        }
    }
    let requested_on = starts
        .iter()
        .filter(|r| r["requested"] == true && r["brake"] == true)
        .count();
    json!({"preserved_decisions":preserved,"flip_count":flips.len(),"flips":flips,"starts":starts,
        "requested_on":requested_on,"requested_count":targets.len(),"passes":flips.is_empty() && requested_on == targets.len()})
}

#[test]
#[ignore = "frozen local diagnostic; explicit mode and fresh output required"]
fn learned_guidance_brake_joint_fit() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let mode = std::env::var("SPACEWARS_JOINT_MODE").unwrap();
    assert!(["export", "evaluate"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_JOINT_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain earlier evidence");
    let load = |variable: &str, field: &str| {
        let bytes = fs::read(std::env::var_os(variable).unwrap()).unwrap();
        assert_eq!(hash(&bytes), plan[field]);
        bytes
    };
    let baseline =
        DenseModel::from_json(&load("SPACEWARS_JOINT_MODEL", "baseline_file_sha256")).unwrap();
    assert_eq!(baseline.id(), plan["baseline_genome"]);
    let prior: Value =
        serde_json::from_slice(&load("SPACEWARS_JOINT_PRIOR", "prior_preflight_sha256")).unwrap();
    let cases = sources(serde_json::from_value(plan["approach_cases"].clone()).unwrap());
    let retained = retention_cases(&plan);
    let protected: BTreeSet<String> =
        serde_json::from_value(plan["protected_cases"].clone()).unwrap();
    let starts = records(&cases);
    assert_eq!(readback(json!(starts)), prior["approaches"]);
    assert_eq!(readback(json!(retained)), prior["retention_cases"]);
    let receipt = serde_json::to_vec_pretty(&json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),
        "plan_sha256":hash(PLAN),"baseline":baseline.id(),"approaches":starts,"retention_cases":retained})).unwrap();
    if mode == "evaluate" {
        assert_eq!(
            receipt,
            fs::read(std::env::var_os("SPACEWARS_JOINT_PREFLIGHT").unwrap()).unwrap()
        );
    }
    fs::write(out.join("preflight.json"), receipt).unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    if mode == "export" {
        let traces = out.join("traces");
        let rows_directory = out.join("rows");
        fs::create_dir(&traces).unwrap();
        fs::create_dir(&rows_directory).unwrap();
        let approaches = pool.map(&cases, |(case, source)| {
            let source = source.lock().unwrap();
            let result = episode(&source, case, "model", &baseline, Some(&traces));
            assert_eq!(result, episode(&source, case, "model", &baseline, None));
            let trace =
                fs::read_to_string(traces.join(format!("{}-model.jsonl", case.name))).unwrap();
            let rows: Vec<_> = trace
                .lines()
                .filter_map(|line| {
                    let line: Value = serde_json::from_str(line).unwrap();
                    let d = &line["decision"];
                    if d.is_null() {
                        return None;
                    }
                    Some(record(
                        &baseline,
                        "match",
                        &case.name,
                        line["tick"].as_u64().unwrap(),
                        serde_json::from_value(d["encoded"]["inputs"].clone()).unwrap(),
                        serde_json::from_value(d["raw_outputs"].clone()).unwrap(),
                        d["movement"]["brake"].as_bool().unwrap(),
                    ))
                })
                .collect();
            assert_eq!(rows.len() as u64, result.owned_component_ticks);
            write_rows(
                rows_directory.join(format!("match--{}.jsonl", case.name)),
                &rows,
            );
            result
        });
        let precision = pool.map(&retained, |case| {
            let mut result =
                ship_arrival::run_episode(case.clone(), Policy::Model(baseline.clone()), true)
                    .unwrap();
            let rows: Vec<_> = result
                .trace
                .iter()
                .map(|tick| {
                    record(
                        &baseline,
                        "precision",
                        &case.name,
                        tick.before.tick,
                        tick.encoded.inputs,
                        tick.raw_outputs.clone().unwrap().try_into().unwrap(),
                        tick.movement.unwrap().brake,
                    )
                })
                .collect();
            write_rows(
                rows_directory.join(format!("precision--{}.jsonl", case.name)),
                &rows,
            );
            result.trace.clear();
            assert_eq!(
                result,
                ship_arrival::run_episode(case.clone(), Policy::Model(baseline.clone()), false)
                    .unwrap()
            );
            assert_eq!(result.outcome, ship_arrival::ArrivalOutcome::Arrived);
            result
        });
        for e in &approaches {
            assert_eq!(protected.contains(&e.case), e.outcome == Outcome::Arrived);
        }
        save(
            out.join("endpoints.json"),
            &json!({"approaches":approaches,"precision":precision}),
        );
        save(
            out.join("summary.json"),
            &json!({"matches":approaches.len(),"precision":precision.len(),"native_trace_endpoint_parity":true,
            "rows":read_rows(&rows_directory).len(),"complete":true}),
        );
        return;
    }

    let export = std::env::var_os("SPACEWARS_JOINT_EXPORT").unwrap();
    let export = Path::new(&export);
    let rows = read_rows(&export.join("rows"));
    let reference: Value =
        serde_json::from_slice(&fs::read(export.join("endpoints.json")).unwrap()).unwrap();
    let fit = std::env::var_os("SPACEWARS_JOINT_FIT").unwrap();
    let fit = Path::new(&fit);
    let manifest: Value =
        serde_json::from_slice(&fs::read(fit.join("candidates.json")).unwrap()).unwrap();
    let candidates = manifest.as_array().unwrap();
    assert!(candidates.len() <= 5);
    let mut variants = vec![("baseline".to_string(), baseline.clone(), BTreeSet::new())];
    for candidate in candidates {
        let file = candidate["file"].as_str().unwrap();
        let bytes = fs::read(fit.join(file)).unwrap();
        assert_eq!(hash(&bytes), candidate["file_sha256"]);
        let model = DenseModel::from_json(&bytes).unwrap();
        assert_brake_only(
            &baseline,
            &model,
            plan["parameter_limit"].as_f64().unwrap() as f32,
        );
        variants.push((
            candidate["label"].as_str().unwrap().into(),
            model,
            serde_json::from_value(candidate["targets"].clone()).unwrap(),
        ));
    }
    let mut reports = Vec::new();
    for (label, model, targets) in variants {
        let directory = out.join(&label);
        fs::create_dir(&directory).unwrap();
        let check = static_check(&model, &rows, &protected, &targets);
        save(directory.join("static.json"), &check);
        save(directory.join("genome.json"), model.genome());
        let approaches = pool.map(&cases, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", &model, None)
        });
        let precision = pool.map(&retained, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false).unwrap()
        });
        let endpoints = readback(json!({"approaches":approaches,"precision":precision}));
        if label == "baseline" {
            assert_eq!(endpoints, reference);
            assert_eq!(check["passes"], true);
        }
        save(directory.join("endpoints.json"), &endpoints);
        let score = [
            precision
                .iter()
                .filter(|e| e.outcome == ship_arrival::ArrivalOutcome::Arrived)
                .count(),
            approaches
                .iter()
                .filter(|e| protected.contains(&e.case) && e.outcome == Outcome::Arrived)
                .count(),
            approaches
                .iter()
                .filter(|e| !protected.contains(&e.case) && e.outcome == Outcome::Arrived)
                .count(),
        ];
        let mut trace_cases = Vec::new();
        if label != "baseline" {
            let traces = directory.join("traces");
            fs::create_dir(&traces).unwrap();
            for ((case, source), expected) in cases.iter().zip(&approaches) {
                if !protected.contains(&case.name) || expected.outcome != Outcome::Arrived {
                    assert_eq!(
                        &episode(
                            &source.lock().unwrap(),
                            case,
                            "model",
                            &model,
                            Some(&traces)
                        ),
                        expected
                    );
                    trace_cases.push(case.name.clone());
                }
            }
            for expected in precision
                .iter()
                .filter(|e| e.outcome != ship_arrival::ArrivalOutcome::Arrived)
                .take(3)
            {
                for (name, m) in [("baseline", &baseline), ("changed", &model)] {
                    let mut traced = ship_arrival::run_episode(
                        expected.case.clone(),
                        Policy::Model(m.clone()),
                        true,
                    )
                    .unwrap();
                    save(
                        traces.join(format!("{name}-{}.json", expected.case.name)),
                        &traced,
                    );
                    traced.trace.clear();
                    if name == "changed" {
                        assert_eq!(&traced, expected);
                    } else {
                        assert_eq!(
                            readback(json!(traced)),
                            *reference["precision"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .find(|e| e["case"]["name"] == expected.case.name)
                                .unwrap()
                        );
                    }
                }
            }
        }
        eprintln!(
            "{label}: {score:?}; {} preserved-action flips",
            check["flip_count"]
        );
        reports.push(json!({"label":label,"genome":model.id(),"targets":targets,"static_pass":check["passes"],"static_flips":check["flip_count"],"score":score,
            "gained":approaches.iter().filter(|e| !protected.contains(&e.case) && e.outcome == Outcome::Arrived).map(|e| &e.case).collect::<Vec<_>>(),
            "lost_matches":approaches.iter().filter(|e| protected.contains(&e.case) && e.outcome != Outcome::Arrived).map(|e| &e.case).collect::<Vec<_>>(),
            "lost_precision":precision.iter().filter(|e| e.outcome != ship_arrival::ArrivalOutcome::Arrived).map(|e| &e.case.name).collect::<Vec<_>>(),"match_traces":trace_cases}));
    }
    save(
        out.join("results.json"),
        &json!({"runtime":engine_agent::evolution::runtime_id().unwrap(),"plan_sha256":hash(PLAN),"variants":reports,"complete":true}),
    );
}

#[cfg(test)]
mod tests;
