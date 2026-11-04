//! Milestone 4 of the match hybrid plan: networks that read the perception
//! blocks, warm-started from dense parents, must fly every known case exactly
//! as their parents do. See docs/learned-guidance-perception-warm-start.md.
use super::guidance_match::*;
use engine_agent::{
    ship_arrival::{self, ArrivalOutcome},
    ship_perception,
    training_workers::EpisodePool,
};
use serde::Serialize;
use serde_json::{Value, json};
use spacewars_ai::{
    guidance::{
        Decision, Features, Policy, Target,
        perception::{self, Perception},
    },
    model::{DenseModel, shared::SharedModel},
};
use std::{
    collections::BTreeMap,
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-perception-warm-start-v1-plan.json"
));

/// Count, range and sum of one value.
#[derive(Debug, Clone, Copy)]
struct Stat {
    count: u64,
    min: f32,
    max: f32,
    sum: f64,
}

impl Stat {
    const EMPTY: Self = Self {
        count: 0,
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
        sum: 0.0,
    };

    fn add(&mut self, value: f32) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += f64::from(value);
    }

    fn merge(&mut self, other: &Self) {
        self.count += other.count;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
    }

    fn json(&self) -> Value {
        if self.count == 0 {
            return Value::Null;
        }
        json!({"min":self.min,"max":self.max,"mean":self.sum / self.count as f64})
    }
}

const BODY_FIELDS: [&str; perception::BODY_VALUES] = [
    "presence",
    "sun",
    "planet",
    "edge",
    "destination",
    "direction_x",
    "direction_y",
    "clearance",
    "closing_speed",
    "sideways_speed",
    "size",
];
const ARC_FIELDS: [&str; perception::ARC_VALUES] = [
    "distance",
    "fill",
    "sun",
    "planet",
    "edge",
    "closing_speed",
    "direction_x",
    "direction_y",
];
const KINDS: [&str; 3] = ["sun", "planet", "edge"];

/// The encoded perception a child read at every decision of a set.
#[derive(Debug, Clone)]
struct Ranges {
    decisions: u64,
    channels: Vec<Stat>,
    /// Decisions by the number of occupied body slots.
    occupied: BTreeMap<usize, u64>,
    /// Record fields by the record's kind.
    records: [[Stat; perception::BODY_VALUES]; 3],
    destination: [Stat; perception::BODY_VALUES],
    /// Arc fields by the kind of the arc's nearest surface.
    arcs: [[Stat; perception::ARC_VALUES]; 3],
}

/// The set one-hot field among three, if any.
fn kind(one_hot: &[f32]) -> Option<usize> {
    one_hot.iter().position(|v| *v > 0.0)
}

impl Ranges {
    fn new() -> Self {
        Self {
            decisions: 0,
            channels: vec![Stat::EMPTY; perception::INPUTS],
            occupied: BTreeMap::new(),
            records: [[Stat::EMPTY; perception::BODY_VALUES]; 3],
            destination: [Stat::EMPTY; perception::BODY_VALUES],
            arcs: [[Stat::EMPTY; perception::ARC_VALUES]; 3],
        }
    }

    fn add(&mut self, inputs: &[f32]) {
        assert_eq!(inputs.len(), perception::INPUTS);
        self.decisions += 1;
        for (stat, &value) in self.channels.iter_mut().zip(inputs) {
            stat.add(value);
        }
        let (slots, arcs) = inputs.split_at(perception::BODY_SLOTS * perception::BODY_VALUES);
        let mut occupied = 0;
        for record in slots.chunks_exact(perception::BODY_VALUES) {
            if record[0] <= 0.0 {
                continue;
            }
            occupied += 1;
            let kind = kind(&record[1..4]).expect("a present record has a kind");
            for (f, &value) in record.iter().enumerate() {
                self.records[kind][f].add(value);
                if record[4] > 0.0 {
                    self.destination[f].add(value);
                }
            }
        }
        *self.occupied.entry(occupied).or_default() += 1;
        for arc in arcs.chunks_exact(perception::ARC_VALUES) {
            if let Some(kind) = kind(&arc[2..5]) {
                for (f, &value) in arc.iter().enumerate() {
                    self.arcs[kind][f].add(value);
                }
            }
        }
    }

    fn merge(&mut self, other: &Self) {
        self.decisions += other.decisions;
        for (a, b) in self.channels.iter_mut().zip(&other.channels) {
            a.merge(b);
        }
        for (count, decisions) in &other.occupied {
            *self.occupied.entry(*count).or_default() += decisions;
        }
        for kind in 0..3 {
            for (a, b) in self.records[kind].iter_mut().zip(&other.records[kind]) {
                a.merge(b);
            }
            for (a, b) in self.arcs[kind].iter_mut().zip(&other.arcs[kind]) {
                a.merge(b);
            }
        }
        for (a, b) in self.destination.iter_mut().zip(&other.destination) {
            a.merge(b);
        }
    }

    fn json(&self) -> Value {
        let fields = |names: &[&str], stats: &[Stat]| {
            names
                .iter()
                .zip(stats)
                .map(|(name, stat)| (name.to_string(), stat.json()))
                .collect::<serde_json::Map<_, _>>()
        };
        let min = self
            .channels
            .iter()
            .map(|s| s.min)
            .fold(f32::INFINITY, f32::min);
        let max = self
            .channels
            .iter()
            .map(|s| s.max)
            .fold(f32::NEG_INFINITY, f32::max);
        json!({
            "decisions":self.decisions,
            "channel_bounds":{"min":min,"max":max},
            "occupied_slots":self.occupied.iter().map(|(k,v)|(k.to_string(),json!(v))).collect::<serde_json::Map<_,_>>(),
            "records":KINDS.iter().zip(&self.records).map(|(kind,stats)|
                (kind.to_string(),json!({"records":stats[0].count,"fields":fields(&BODY_FIELDS,stats)}))).collect::<serde_json::Map<_,_>>(),
            "destination":fields(&BODY_FIELDS,&self.destination),
            "arcs_by_nearest":KINDS.iter().zip(&self.arcs).map(|(kind,stats)|
                (kind.to_string(),json!({"arcs":stats[0].count,"fields":fields(&ARC_FIELDS,stats)}))).collect::<serde_json::Map<_,_>>(),
        })
    }
}

/// What a comparison keeps from a run: the episode record, every decision's
/// raw output bits and the perception ranges.
struct Summary {
    record: Value,
    outcome: String,
    raw: Vec<Vec<u32>>,
    ranges: Ranges,
}

fn bits(raw: Option<&Vec<f32>>) -> Vec<u32> {
    raw.into_iter().flatten().map(|v| v.to_bits()).collect()
}

fn summarize_match(run: Run) -> Summary {
    let mut ranges = Ranges::new();
    for decision in &run.decisions {
        if let Some(sensed) = &decision.perception {
            ranges.add(&sensed.inputs);
        }
    }
    Summary {
        outcome: format!("{:?}", run.episode.outcome),
        raw: run
            .decisions
            .iter()
            .map(|d: &Decision| bits(d.raw_outputs.as_ref()))
            .collect(),
        record: parsed(json!(run.episode)),
        ranges,
    }
}

/// A typed record as its file reads back. Going through `json!` instead would
/// widen each f32 before printing it, so its numbers would read back
/// differently from those of a file the lineage wrote from the typed record.
fn stored(record: &impl Serialize) -> Value {
    serde_json::from_slice(&serde_json::to_vec(record).unwrap()).unwrap()
}

/// The record without its trace and with the current runtime, as the lineage
/// stored it.
fn summarize_precision(run: ship_perception::PerceivingEpisode, runtime: &str) -> Summary {
    let mut ranges = Ranges::new();
    for sensed in &run.perception {
        ranges.add(&sensed.inputs);
    }
    let mut episode = run.episode;
    let raw = episode
        .trace
        .iter()
        .map(|tick| bits(tick.raw_outputs.as_ref()))
        .collect();
    episode.trace.clear();
    assert_eq!(episode.runtime, runtime);
    Summary {
        outcome: format!("{:?}", episode.outcome),
        record: stored(&episode),
        raw,
        ranges,
    }
}

fn controller<'a>(recorded: &'a Value, label: &str) -> &'a Value {
    recorded
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["controller"] == label)
        .unwrap()
}

fn load_parent(root: &Path, specification: &Value) -> DenseModel {
    let bytes = fs::read(root.join(specification["file"].as_str().unwrap())).unwrap();
    assert_eq!(hash(&bytes), specification["sha256"]);
    let model = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(model.id(), specification["genome"].as_str().unwrap());
    model
}

fn child(parent: &DenseModel, plan: &Value, seed: u64) -> SharedModel {
    let warm = &plan["warm_start"];
    let genome = perception::warm_start(
        parent.genome(),
        warm["encoder_width"].as_u64().unwrap() as usize,
        warm["initial_scale"].as_f64().unwrap() as f32,
        seed,
    )
    .unwrap();
    assert_eq!(genome.parameter_count() as u64, warm["parameters"]);
    SharedModel::new(genome).unwrap()
}

/// Counts of records and decisions that agree, for one set and parent.
#[derive(Default)]
struct Agreement {
    episodes: usize,
    arrived: usize,
    runner: usize,
    child: usize,
    decisions_identical: usize,
    decisions: usize,
}

impl Agreement {
    fn passed(&self) -> bool {
        [self.runner, self.child, self.decisions_identical] == [self.episodes; 3]
    }

    fn json(&self) -> Value {
        json!({"episodes":self.episodes,"arrived":self.arrived,
            "runner_reproduces_record":self.runner,"child_reproduces_record":self.child,
            "child_decisions_identical":self.decisions_identical,"decisions":self.decisions,
            "passed":self.passed()})
    }
}

fn compare(
    names: &[String],
    expected: &BTreeMap<String, Value>,
    parent: &[Summary],
    child: &[Summary],
    ranges: &mut Ranges,
) -> Agreement {
    let mut agreement = Agreement::default();
    for ((name, p), c) in names.iter().zip(parent).zip(child) {
        agreement.episodes += 1;
        agreement.arrived += usize::from(c.outcome == "Arrived");
        agreement.runner += usize::from(p.record == expected[name]);
        agreement.child += usize::from(c.record == expected[name]);
        agreement.decisions_identical += usize::from(c.raw == p.raw);
        agreement.decisions += c.raw.len();
        if c.record != expected[name] || c.raw != p.raw {
            eprintln!("{name}: child differs from its parent");
        }
        ranges.merge(&c.ranges);
    }
    agreement
}

/// Per-decision cost of sensing and inference on real match states: the
/// first `sources` training starts, each timed over `repetitions` calls.
fn cost(sources: &[Source], parent: &DenseModel, child: &SharedModel, plan: &Value) -> Value {
    let sample = plan["cost"]["sources"].as_u64().unwrap() as usize;
    let repetitions = plan["cost"]["repetitions"].as_u64().unwrap() as u32;
    let timed = |work: &mut dyn FnMut()| {
        let started = Instant::now();
        for _ in 0..repetitions {
            work();
        }
        started.elapsed().as_secs_f64() * 1e6 / f64::from(repetitions)
    };
    let mut rows = Vec::new();
    for (case, source) in sources.iter().take(sample) {
        let run = source.lock().unwrap();
        let f = flight(&run, case.seat);
        let offset =
            (f.ship.position - f.planet_motion.position).normalized() * (f.planet_radius + 85.0);
        let marker: Target = target(&f, offset);
        let nine = Features::read(&f, marker).encode().unwrap().inputs;
        let bodies = run.sortie.state.celestial_bodies();
        let sensed = Perception::read(&f, &bodies, Some(case.planet));
        let inputs = [nine.as_slice(), &sensed.encode().unwrap().inputs].concat();
        rows.push(json!({
            "case":case.name,
            "bodies_us":timed(&mut || { black_box(run.sortie.state.celestial_bodies()); }),
            "sensing_us":timed(&mut || {
                black_box(Perception::read(black_box(&f), black_box(&bodies), Some(case.planet)).encode().unwrap());
            }),
            "child_inference_us":timed(&mut || { black_box(child.infer(black_box(&inputs)).unwrap()); }),
            "parent_inference_us":timed(&mut || { black_box(parent.infer(black_box(&nine)).unwrap()); }),
        }));
    }
    let mean =
        |key: &str| rows.iter().map(|r| r[key].as_f64().unwrap()).sum::<f64>() / rows.len() as f64;
    json!({"sources":rows.len(),"repetitions":repetitions,
        "mean_us":{"bodies":mean("bodies_us"),"sensing":mean("sensing_us"),
            "child_inference":mean("child_inference_us"),"parent_inference":mean("parent_inference_us")},
        "rows":rows})
}

fn reproduce(root: &Path, plan: &Value) {
    let out = root.join("reproduce");
    fs::create_dir(&out).unwrap();
    let started = Instant::now();
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let base = read(root, "base-plan.json");
    let (training, excluded) = split(restore(&pool, &read(root, "inputs.json")["sources"]), &base);
    let sets = &plan["sets"];
    assert_eq!(json!(training.len()), sets["training"]);
    assert_eq!(json!(excluded.len()), sets["excluded"]);
    let cases = retention_cases(&base);
    assert_eq!(json!(cases.len()), sets["precision"]);
    let recorded = [
        ("training", read(root, "training-episodes.json"), &training),
        ("excluded", read(root, "excluded-episodes.json"), &excluded),
    ];
    let mut results = json!({"runtime":runtime,"plan_sha256":hash(PLAN),"parents":{},
        "complete":false});
    let mut passed = true;
    for (label, specification) in plan["parents"].as_object().unwrap() {
        let parent = load_parent(root, specification);
        let child = child(
            &parent,
            plan,
            specification["warm_start_seed"].as_u64().unwrap(),
        );
        save(out.join(format!("{label}-child.json")), child.genome());
        let mut entry = json!({"genome":parent.id(),"child":child.id(),
            "child_parameters":child.genome().parameter_count(),"sets":{}});
        let mut match_ranges = Ranges::new();
        let recorded_label = specification["recorded"].as_str().unwrap();
        for (set, record, sources) in &recorded {
            let expected = by_case(
                controller(record, recorded_label)["episodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
            let names: Vec<String> = sources.iter().map(|(c, _)| c.name.clone()).collect();
            let fly = |driver: Driver| {
                pool.map(sources, |(case, source)| {
                    summarize_match(run(&source.lock().unwrap(), case, &driver, None, true))
                })
            };
            let parent_runs = fly(Driver::guidance(Policy::Model(parent.clone())));
            let child_runs = fly(Driver::guidance(Policy::Perception(child.clone())));
            let agreement = compare(
                &names,
                &expected,
                &parent_runs,
                &child_runs,
                &mut match_ranges,
            );
            passed &= agreement.passed();
            entry["sets"][*set] = agreement.json();
            eprintln!("{label} {set}: {}", entry["sets"][*set]);
        }
        let record = read(root, specification["precision"].as_str().unwrap());
        let mut expected = BTreeMap::new();
        for mut e in record.as_array().unwrap().iter().cloned() {
            e["runtime"] = json!(runtime);
            expected.insert(e["case"]["name"].as_str().unwrap().to_owned(), e);
        }
        let names: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
        let fly = |policy: Policy| {
            pool.map(&cases, |case| {
                summarize_precision(
                    ship_perception::run_episode(case.clone(), policy.clone(), true).unwrap(),
                    &runtime,
                )
            })
        };
        let parent_runs = fly(Policy::Model(parent.clone()));
        let mut child_runs = fly(Policy::Perception(child.clone()));
        // The child names its own brain; everything else must match.
        let brain = ship_perception::brain(&Policy::Model(parent.clone()));
        for summary in &mut child_runs {
            summary.record["brain"] = stored(&brain);
            summary.record["brain_id"] = json!(brain.identity().unwrap());
        }
        let mut precision_ranges = Ranges::new();
        let agreement = compare(
            &names,
            &expected,
            &parent_runs,
            &child_runs,
            &mut precision_ranges,
        );
        // The copied runner also matches the pinned one on the record's cases.
        let pinned = pool.map(&cases[..8], |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(parent.clone()), false).unwrap()
        });
        assert!(pinned.iter().all(|e| expected[&e.case.name] == stored(e)));
        passed &= agreement.passed()
            && child_runs
                .iter()
                .all(|s| s.outcome == format!("{:?}", ArrivalOutcome::Arrived));
        entry["sets"]["precision"] = agreement.json();
        eprintln!("{label} precision: {}", entry["sets"]["precision"]);
        entry["perception"] =
            json!({"match":match_ranges.json(),"precision":precision_ranges.json()});
        if label == "candidate" {
            results["cost"] = cost(&training, &parent, &child, plan);
        }
        results["parents"][label] = entry;
        atomic(&out, "results.json", &results);
    }
    results["passed"] = json!(passed);
    results["complete"] = json!(true);
    results["seconds"] = json!(started.elapsed().as_secs_f64());
    atomic(&out, "results.json", &results);
    eprintln!(
        "reproduction {} in {:.1} s",
        if passed { "passed" } else { "failed" },
        started.elapsed().as_secs_f64()
    );
    assert!(passed, "a child differs from its parent");
}

#[test]
#[ignore = "frozen perception warm-start check; set SPACEWARS_PERCEPTION_ROOT"]
fn learned_guidance_perception_warm_start() {
    let root = PathBuf::from(std::env::var_os("SPACEWARS_PERCEPTION_ROOT").unwrap());
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    assert_eq!(
        hash(&fs::read(root.join("plan.json")).unwrap()),
        hash(PLAN),
        "the package plan differs from the compiled plan"
    );
    for (name, digest) in plan["inputs"].as_object().unwrap() {
        assert_eq!(hash(&fs::read(root.join(name)).unwrap()), *digest, "{name}");
    }
    reproduce(&root, &plan);
}

#[test]
fn plan_declares_the_compiled_observation_and_warm_start() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let observation = &plan["observation"];
    let contract = perception::contract();
    assert_eq!(observation["name"], contract.observation);
    assert_eq!(observation["inputs"], json!(contract.inputs));
    for (key, value) in [
        ("body_slots", perception::BODY_SLOTS),
        ("body_values", perception::BODY_VALUES),
        ("arcs", perception::ARCS),
        ("arc_values", perception::ARC_VALUES),
    ] {
        assert_eq!(observation[key], json!(value), "{key}");
    }
    assert_eq!(observation["fill_range"], json!(perception::FILL_RANGE));
    assert_eq!(
        observation["threat_seconds"],
        json!(perception::THREAT_SECONDS)
    );
    let parent = DenseModel::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine-agent/fixtures/ship-arrival-mixed-v1/seed-1.json"
    )))
    .unwrap();
    assert_eq!(
        json!(child(&parent, &plan, 1).genome().parameter_count()),
        plan["warm_start"]["parameters"]
    );
}

/// The match runner flies one known approach identically for a dense parent
/// and its warm-started child, and as the lineage runner recorded it.
#[test]
#[ignore = "replays a match case recorded on the experiment machine; another C math library flies the match elsewhere"]
fn warm_started_child_flies_a_match_approach_like_its_parent() {
    let approach: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/data/learned-guidance-approach-v1-plan.json"
    )))
    .unwrap();
    let case: Case = serde_json::from_value(approach["cases"][0].clone()).unwrap();
    let source = prepare(&case);
    let parent = DenseModel::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine-agent/fixtures/ship-arrival-mixed-v1/seed-1.json"
    )))
    .unwrap();
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let child = child(&parent, &plan, 1);
    let dense = run(
        &source,
        &case,
        &Driver::guidance(Policy::Model(parent)),
        None,
        true,
    );
    let shared = run(
        &source,
        &case,
        &Driver::guidance(Policy::Perception(child)),
        None,
        true,
    );
    assert!(dense.episode.owned_component_ticks > 10);
    assert_eq!(parsed(json!(dense.episode)), parsed(json!(shared.episode)));
    assert_eq!(dense.decisions.len(), shared.decisions.len());
    for (d, s) in dense.decisions.iter().zip(&shared.decisions) {
        assert_eq!(bits(d.raw_outputs.as_ref()), bits(s.raw_outputs.as_ref()));
        assert!(d.perception.is_none());
        let sensed = &s.perception.as_ref().unwrap().inputs;
        // The match world has the sun, three planets and the edge.
        let present = (0..perception::BODY_SLOTS)
            .filter(|slot| sensed[slot * perception::BODY_VALUES] > 0.0)
            .count();
        assert_eq!(present, 5);
        assert_eq!(sensed[4], 1.0, "the destination keeps the first slot");
    }
    let native = run(&source, &case, &Driver::Native, None, false);
    assert_eq!(native.episode.controller, "mission");
    assert!(native.decisions.is_empty());
}
