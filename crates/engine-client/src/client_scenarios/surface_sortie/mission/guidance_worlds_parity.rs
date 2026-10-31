//! A development check for milestone 5, outside its frozen package: children
//! warm-started as in milestone 4 fly a sample of the varied-world training
//! starts exactly as their dense parents, while reading every body there.
use super::guidance_match::*;
use super::guidance_worlds::*;
use engine_agent::training_workers::EpisodePool;
use serde_json::{Value, json};
use spacewars_ai::{
    guidance::{Policy, perception},
    model::{DenseModel, shared::SharedModel},
};
use std::{collections::BTreeMap, fs, path::PathBuf};

/// Training starts per cell, the first in the frozen file's order.
const PER_CELL: usize = 2;

fn bits(outputs: Option<&Vec<f32>>) -> Option<Vec<u32>> {
    outputs.map(|v| v.iter().map(|x| x.to_bits()).collect())
}

#[test]
#[ignore = "development check; set SPACEWARS_WORLDS_ROOT to the varied-world package"]
fn warm_started_children_fly_varied_worlds_like_their_parents() {
    let root = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_ROOT").unwrap());
    let plan = read(&root, "plan.json");
    let recorded = read(&root.join("discover"), "training-starts.json");
    let mut picked: BTreeMap<String, Vec<Start>> = BTreeMap::new();
    for record in recorded.as_array().unwrap() {
        let start: Start = serde_json::from_value(record["start"].clone()).unwrap();
        let cell = picked.entry(start.cell.clone()).or_default();
        if cell.len() < PER_CELL {
            cell.push(start);
        }
    }
    let sample: Vec<Start> = picked.into_values().flatten().collect();
    // Milestone 4's warm start: width 4, scale 0.5, and seeds 1 and 2.
    let pairs: Vec<(DenseModel, SharedModel)> = [("e0f25b68", 1), ("e03d3e29", 2)]
        .into_iter()
        .map(|(name, seed)| {
            let model = &plan["models"][name];
            let bytes = fs::read(root.join(model["file"].as_str().unwrap())).unwrap();
            assert_eq!(hash(&bytes), model["sha256"]);
            let parent = DenseModel::from_json(&bytes).unwrap();
            let genome = perception::warm_start(parent.genome(), 4, 0.5, seed).unwrap();
            (parent, SharedModel::new(genome).unwrap())
        })
        .collect();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let rows: Vec<Value> = pool
        .map(&sample, |start| {
            let state = prepare_start(start);
            pairs
                .iter()
                .map(|(parent, child)| {
                    let policy = Policy::Model(parent.clone());
                    let dense = run(&state, &start.case, &Driver::guidance(policy), None, true);
                    let policy = Policy::Perception(child.clone());
                    let shared = run(&state, &start.case, &Driver::guidance(policy), None, true);
                    let same_decisions = dense.decisions.len() == shared.decisions.len()
                        && dense
                            .decisions
                            .iter()
                            .zip(&shared.decisions)
                            .all(|(d, s)| bits(d.raw_outputs.as_ref()) == bits(s.raw_outputs.as_ref()));
                    let bounded = shared.decisions.iter().all(|s| {
                        let sensed = &s.perception.as_ref().unwrap().inputs;
                        sensed.iter().all(|v| v.is_finite() && v.abs() <= 1.0)
                    });
                    json!({"case":start.case.name,"cell":start.cell,"parent":parent.id(),
                        "child":child.id(),"decisions":shared.decisions.len(),
                        "outcome":shared.episode.outcome,
                        "same_episode":parsed(json!(dense.episode)) == parsed(json!(shared.episode)),
                        "same_decisions":same_decisions,"bounded":bounded})
                })
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect();
    let decisions: u64 = rows.iter().map(|r| r["decisions"].as_u64().unwrap()).sum();
    println!(
        "{} episodes over {} starts and {decisions} decisions",
        rows.len(),
        sample.len()
    );
    for row in &rows {
        assert!(
            row["same_episode"] == true && row["same_decisions"] == true && row["bounded"] == true,
            "{row}"
        );
    }
}
