//! Milestone 1 of the match hybrid plan: rank candidates by match arrival time
//! right after completions, against a control arm with the earlier order. See
//! docs/learned-guidance-time-ranking.md. The training helpers are copies from
//! the frozen lineage under `training`, so its pinned sources stay unchanged.
use super::*;
use engine_agent::{
    evolution::{EvaluationTask, EvolutionConfig, EvolutionError, EvolutionRun, TaskManifest},
    ship_arrival, ship_curriculum,
    training_workers::{EpisodePool, WorkerConfig},
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Mutex,
    time::Instant,
};

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-time-ranking-v1-plan.json"
));

type Source = (Case, Mutex<MaterialMissionClientScenario>);

/// Each arm ranks these fields in its declared order; see `key`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeFitness<const TIME: bool> {
    completed: u32,
    precision_completed: u32,
    match_completed: u32,
    match_spare_ticks: u32,
    precision_position_points: u32,
    precision_velocity_points: u32,
    approach_speed_points: u32,
    approach_position_points: u32,
    precision_spare_ticks: u32,
}

impl<const TIME: bool> TimeFitness<TIME> {
    fn precision(&mut self, score: ship_arrival::ArrivalFitness) {
        self.completed += score.completed;
        self.precision_completed += score.completed;
        self.precision_position_points += score.position_points;
        self.precision_velocity_points += score.velocity_points;
        self.precision_spare_ticks += score.spare_ticks;
    }

    /// The same quality formulas as the lineage's approach fitness.
    fn approach(&mut self, result: &Episode) {
        if result.outcome == Outcome::Arrived {
            self.completed += 1;
            self.match_completed += 1;
            self.match_spare_ticks += (MAX_TICKS - result.elapsed_ticks) as u32;
        }
        if !matches!(
            result.outcome,
            Outcome::Arrived | Outcome::Timeout | Outcome::HostPreempted
        ) {
            return;
        }
        if let Some(f) = &result.final_flight {
            let speed = (f.ship.velocity - f.planet_motion.velocity).length();
            let distance = f
                .ship
                .position
                .distance_to(target(f, result.frozen_target_offset).position);
            self.approach_speed_points += ((1.0 - speed / 70.0).clamp(0.0, 1.0) * 1000.0) as u32;
            self.approach_position_points +=
                ((1.0 - distance / 100.0).clamp(0.0, 1.0) * 1000.0) as u32;
        }
    }

    /// The plan's `fitness_orders`, field for field.
    fn key(&self) -> Vec<u32> {
        let mut key = vec![
            self.completed,
            self.precision_completed,
            self.match_completed,
        ];
        let gentle = [
            self.precision_position_points,
            self.precision_velocity_points,
            self.approach_speed_points,
            self.approach_position_points,
        ];
        if TIME {
            key.push(self.match_spare_ticks);
            key.extend(gentle);
            key.push(self.precision_spare_ticks);
        } else {
            key.extend(gentle);
            // Exact ties on the combined count fall to match time, which keeps the
            // order consistent with equality.
            key.push(self.match_spare_ticks + self.precision_spare_ticks);
            key.push(self.match_spare_ticks);
        }
        key
    }
}

impl<const TIME: bool> Ord for TimeFitness<TIME> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl<const TIME: bool> PartialOrd for TimeFitness<TIME> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct TimeTask<'a, const TIME: bool> {
    sources: &'a [Source],
    precision: &'a [ship_arrival::ArrivalCase],
    pool: EpisodePool,
    manifest: TaskManifest,
}

impl<const TIME: bool> EvaluationTask for TimeTask<'_, TIME> {
    type Fitness = TimeFitness<TIME>;

    fn manifest(&self) -> TaskManifest {
        self.manifest.clone()
    }

    fn evaluate(&self, model: &DenseModel) -> Result<Self::Fitness, EvolutionError> {
        let mut score = TimeFitness::default();
        for result in self.pool.map(self.precision, |case| {
            ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false)
        }) {
            score.precision(result?.fitness);
        }
        for result in self.pool.map(self.sources, |(case, source)| {
            episode(&source.lock().unwrap(), case, "model", model, None)
        }) {
            score.approach(&result);
        }
        Ok(score)
    }
}

fn arm<const TIME: bool>() -> &'static str {
    if TIME { "time" } else { "control" }
}

fn task<'a, const TIME: bool>(
    sources: &'a [Source],
    precision: &'a [ship_arrival::ArrivalCase],
    plan: &Value,
) -> TimeTask<'a, TIME> {
    TimeTask {
        sources,
        precision,
        pool: EpisodePool::new(worker_config()).unwrap(),
        manifest: TaskManifest {
            id: format!("recovery-guidance-time-ranking-v1-{}", arm::<TIME>()),
            contract: spacewars_ai::guidance::contract(),
            configuration: json!({"arm":arm::<TIME>(),
                "fitness_order":plan["fitness_orders"][arm::<TIME>()],
                "plan_utf8":std::str::from_utf8(PLAN).unwrap()}),
        },
    }
}

// Copied from `training`.
fn worker_config() -> WorkerConfig {
    let mode = std::env::var("SPACEWARS_TRAINING_WORKERS").unwrap_or_else(|_| "auto".into());
    if mode == "auto" {
        WorkerConfig::Auto {
            target_cpu_percent: std::env::var("SPACEWARS_TRAINING_CPU_PERCENT")
                .map_or(75, |s| s.parse().expect("CPU target percentage")),
            max_workers: std::env::var("SPACEWARS_TRAINING_MAX_WORKERS")
                .map_or(0, |s| s.parse().expect("maximum workers")),
        }
    } else {
        WorkerConfig::Fixed {
            workers: mode.parse().expect("workers: auto or an integer"),
        }
    }
}

// Copied from `training`.
fn retention_cases(plan: &Value) -> Vec<ship_arrival::ArrivalCase> {
    let partitions = plan
        .get("retention_partitions")
        .cloned()
        .unwrap_or_else(|| json!(["training"]));
    let cases: Vec<_> = partitions
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|label| {
            let partition = ship_curriculum::PARTITIONS
                .into_iter()
                .find(|p| serde_json::to_value(p).unwrap() == *label)
                .expect("declared retention partition");
            ship_curriculum::cases(partition)
        })
        .collect();
    if plan["retention_deduplicate"] != true {
        return cases;
    }
    // The original partitions reuse physical starts under different names.
    // Compare every other field and retain the first occurrence's name/order.
    let mut seen = Vec::new();
    cases
        .into_iter()
        .filter(|case| {
            let mut identity = case.clone();
            identity.name.clear();
            if seen.contains(&identity) {
                false
            } else {
                seen.push(identity);
                true
            }
        })
        .collect()
}

// Copied from `training`.
fn source_case(
    run: &MaterialMissionClientScenario,
    name: String,
    shape: DirtMode,
    seat: usize,
) -> Option<Case> {
    let f = run.sortie.state.free_flight_observation(seat)?;
    if failure(&f, &f).is_some() {
        return None;
    }
    let offset =
        (f.ship.position - f.planet_motion.position).normalized() * (f.planet_radius + 85.0);
    let features = Features::read(&f, target(&f, offset));
    let encoded = features.encode().ok()?;
    if encoded.saturated.iter().any(|s| *s) {
        return None;
    }
    Some(Case {
        name,
        seed: run.seed,
        shape,
        seat,
        source_tick: f.tick,
        planet: f.planet,
        source_entry_distance: features.target_offset.length(),
        source_relative_speed: features.target_velocity.length(),
    })
}

/// The first eligible direct-transfer state per seat and shape of one world,
/// copied from `training::validation`.
fn discover_world(seed: u64, prefix: u64, distance: f32) -> (Vec<Source>, Vec<Value>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for shape in [DirtMode::WeakAngular, DirtMode::WeakRound] {
        let mut run = start_match(seed, shape);
        let mut selected = [false; 2];
        while run.sortie.state.tick() < prefix && !run.is_game_over() {
            for (seat, selected) in selected.iter_mut().enumerate() {
                if *selected {
                    continue;
                }
                let name = format!("validation-{seed}-{shape:?}-p{}", seat + 1);
                let Some(case) = source_case(&run, name, shape, seat) else {
                    continue;
                };
                if case.source_entry_distance > distance
                    || !direct_transfer(&probe(&run, seat), &case)
                {
                    continue;
                }
                let before = flight(&run, seat);
                let mut neutral = fork(&run);
                neutral.bots[seat] = false;
                neutral.step(&Movement::default().encode(before.owner), STEP);
                neutral.bots[seat] = true;
                let after = flight(&neutral, seat);
                let after_case = source_case(&neutral, case.name.clone(), shape, seat);
                if failure(&after, &before).is_some()
                    || after_case.is_none_or(|c| c.source_entry_distance > distance)
                    || !direct_transfer(&probe(&neutral, seat), &case)
                {
                    continue;
                }
                assert_eq!(after.applied, [0.0; 3]);
                found.push((case, Mutex::new(neutral)));
                *selected = true;
            }
            if selected.iter().all(|s| *s) {
                break;
            }
            run.step(&[], STEP);
        }
        for (seat, selected) in selected.into_iter().enumerate() {
            if !selected {
                missing.push(json!({"seed":seed,"shape":shape,"seat":seat,
                    "end_tick":run.sortie.state.tick(),"round_finished":run.is_game_over()}));
            }
        }
    }
    (found, missing)
}

fn discover(pool: &EpisodePool, settings: &Value) -> (Vec<Source>, Vec<Value>) {
    let seeds: Vec<u64> = serde_json::from_value(settings["validation_seeds"].clone()).unwrap();
    let prefix = settings["validation_prefix_ticks"].as_u64().unwrap();
    let distance = settings["validation_entry_distance"].as_f64().unwrap() as f32;
    let mut sources = Vec::new();
    let mut missing = Vec::new();
    for (found, absent) in pool.map(&seeds, |&seed| discover_world(seed, prefix, distance)) {
        sources.extend(found);
        missing.extend(absent);
    }
    assert_eq!(sources.len() + missing.len(), seeds.len() * 4);
    (sources, missing)
}

fn records(sources: &[Source]) -> Vec<Value> {
    sources
        .iter()
        .map(|(c, s)| initial_record(&s.lock().unwrap(), c))
        .collect()
}

fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

/// Rebuild recorded match states and check them against their records.
fn restore(pool: &EpisodePool, recorded: &Value) -> Vec<Source> {
    let cases: Vec<Case> = recorded
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["case"].clone()).unwrap())
        .collect();
    let runs = pool.map(&cases, prepare);
    let restored: Vec<Source> = cases
        .into_iter()
        .zip(runs)
        .map(|(case, run)| (case, Mutex::new(run)))
        .collect();
    assert_eq!(
        parsed(json!(records(&restored))),
        parsed(recorded.clone()),
        "physical sources changed"
    );
    restored
}

/// The base plan's 464 training approaches and the 18 known ones left out.
fn split(all: Vec<Source>, base: &Value) -> (Vec<Source>, Vec<Source>) {
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
    (training, excluded)
}

fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

fn atomic(out: &Path, name: &str, value: &impl Serialize) {
    let temporary = out.join(format!(".{name}"));
    save(&temporary, value);
    fs::rename(temporary, out.join(name)).unwrap();
}

fn load_model(root: &Path, plan: &Value, name: &str) -> DenseModel {
    let specification = &plan["models"][name];
    let bytes = fs::read(root.join(specification["file"].as_str().unwrap())).unwrap();
    assert_eq!(hash(&bytes), specification["sha256"]);
    let model = DenseModel::from_json(&bytes).unwrap();
    assert_eq!(model.id(), specification["genome"].as_str().unwrap());
    model
}

fn final_model(root: &Path, plan: &Value, arm: &str) -> DenseModel {
    let selection = read(&root.join(arm), "selection.json");
    assert_eq!(selection["generation"], plan["evolution"]["generations"]);
    let model =
        DenseModel::from_json(&fs::read(root.join(arm).join("final.json")).unwrap()).unwrap();
    assert_eq!(model.id(), selection["genome"].as_str().unwrap());
    model
}

fn matches(
    pool: &EpisodePool,
    sources: &[Source],
    driver: &str,
    model: &DenseModel,
    traces: Option<&Path>,
) -> Vec<Episode> {
    pool.map(sources, |(case, source)| {
        episode(&source.lock().unwrap(), case, driver, model, traces)
    })
}

fn precision(
    pool: &EpisodePool,
    cases: &[ship_arrival::ArrivalCase],
    model: &DenseModel,
) -> Vec<ship_arrival::ArrivalEpisode> {
    pool.map(cases, |case| {
        ship_arrival::run_episode(case.clone(), Policy::Model(model.clone()), false).unwrap()
    })
}

fn median(mut values: Vec<u64>) -> Option<f64> {
    values.sort_unstable();
    let n = values.len();
    (n > 0).then(|| {
        if n % 2 == 1 {
            values[n / 2] as f64
        } else {
            (values[n / 2 - 1] + values[n / 2]) as f64 / 2.0
        }
    })
}

/// Completions and arrival time against native control from identical starts.
fn versus(native: &[Episode], model: &[Episode]) -> Value {
    assert_eq!(native.len(), model.len());
    let ticks = |e: &Episode| {
        if e.outcome == Outcome::Arrived {
            e.elapsed_ticks
        } else {
            MAX_TICKS
        }
    };
    let (mut shared, mut native_shared) = (Vec::new(), Vec::new());
    let (mut faster, mut slower, mut tied, mut gained, mut lost) = (0, 0, 0, 0, 0);
    let mut saved = 0_i64;
    for (n, m) in native.iter().zip(model) {
        assert_eq!(n.case, m.case);
        saved += ticks(n) as i64 - ticks(m) as i64;
        match (n.outcome == Outcome::Arrived, m.outcome == Outcome::Arrived) {
            (true, true) => {
                shared.push(m.elapsed_ticks);
                native_shared.push(n.elapsed_ticks);
                match m.elapsed_ticks.cmp(&n.elapsed_ticks) {
                    Ordering::Less => faster += 1,
                    Ordering::Greater => slower += 1,
                    Ordering::Equal => tied += 1,
                }
            }
            (false, true) => gained += 1,
            (true, false) => lost += 1,
            (false, false) => {}
        }
    }
    let completed = |episodes: &[Episode]| {
        episodes
            .iter()
            .filter(|e| e.outcome == Outcome::Arrived)
            .count()
    };
    let shared_successes = shared.len();
    json!({"starts":native.len(),"completed":completed(model),"native_completed":completed(native),
        "shared_successes":shared_successes,"gained":gained,"native_successes_lost":lost,
        "median_ticks":median(shared),"native_median_ticks":median(native_shared),
        "faster":faster,"slower":slower,"tied":tied,"time_saved_ticks":saved})
}

fn by_case(episodes: impl IntoIterator<Item = Value>) -> BTreeMap<String, Value> {
    episodes
        .into_iter()
        .map(|e| (e["case"].as_str().unwrap().to_owned(), e))
        .collect()
}

fn assert_parity(episodes: &[Episode], expected: &BTreeMap<String, Value>, label: &str) -> usize {
    for e in episodes {
        assert_eq!(
            parsed(json!(e)),
            expected[&e.case],
            "{label} parity: {}",
            e.case
        );
    }
    episodes.len()
}

fn controller<'a>(results: &'a Value, label: &str) -> &'a Value {
    results["controllers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["controller"] == label)
        .unwrap()
}

/// Reproduce the candidate's recorded known results and native control's recorded
/// outcomes before any training.
fn preflight(root: &Path, plan: &Value) {
    let out = root.join("preflight");
    fs::create_dir(&out).unwrap();
    let started = Instant::now();
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let base = read(root, "base-plan.json");
    let inputs = read(root, "inputs.json");
    let (training, excluded) = split(restore(&pool, &inputs["sources"]), &base);
    let cases = retention_cases(&base);
    assert_eq!(cases.len(), 157);
    let candidate = load_model(root, plan, "candidate");

    let recorded_native = by_case(inputs["native"].as_array().unwrap().iter().cloned());
    let native_training = matches(&pool, &training, "mission", &candidate, None);
    let native_excluded = matches(&pool, &excluded, "mission", &candidate, None);
    let native_parity = assert_parity(&native_training, &recorded_native, "native")
        + assert_parity(&native_excluded, &recorded_native, "native");

    let expected = read(root, "expected-candidate.json");
    let selected = controller(&expected, "selected");
    assert_eq!(selected["genome"], candidate.id());
    let recorded_candidate = by_case(
        selected["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["episodes"].as_array().unwrap().clone()),
    );
    let candidate_training = matches(&pool, &training, "model", &candidate, None);
    let candidate_excluded = matches(&pool, &excluded, "model", &candidate, None);
    let candidate_parity = assert_parity(&candidate_training, &recorded_candidate, "candidate")
        + assert_parity(&candidate_excluded, &recorded_candidate, "candidate");
    let arrivals = precision(&pool, &cases, &candidate);
    let mut normalized = selected["precision_retention"].clone();
    for e in normalized.as_array_mut().unwrap() {
        e["runtime"] = json!(runtime);
    }
    assert_eq!(parsed(json!(arrivals)), normalized, "precision parity");

    // The copied formulas must reproduce the lineage's recorded fitness.
    let mut fitness = TimeFitness::<true>::default();
    for e in &arrivals {
        fitness.precision(e.fitness);
    }
    for e in &candidate_training {
        fitness.approach(e);
    }
    let old = &selected["fitness"];
    assert_eq!(
        (
            fitness.completed,
            fitness.precision_completed,
            fitness.match_completed
        ),
        (621, 157, 464)
    );
    for (new, name) in [
        (
            fitness.precision_position_points,
            "precision_position_points",
        ),
        (
            fitness.precision_velocity_points,
            "precision_velocity_points",
        ),
        (fitness.approach_speed_points, "approach_speed_points"),
        (fitness.approach_position_points, "approach_position_points"),
        (
            fitness.match_spare_ticks + fitness.precision_spare_ticks,
            "spare_ticks",
        ),
    ] {
        assert_eq!(json!(new), old[name], "fitness parity: {name}");
    }
    let time = task::<true>(&training, &cases, plan)
        .evaluate(&candidate)
        .unwrap();
    let control = task::<false>(&training, &cases, plan)
        .evaluate(&candidate)
        .unwrap();
    assert_eq!(time, fitness);
    assert_eq!(json!(control), json!(fitness));

    save(
        out.join("results.json"),
        &json!({"runtime":runtime,"plan_sha256":hash(PLAN),"passed":true,
            "native_parity":native_parity,"candidate_parity":candidate_parity,
            "precision_parity":arrivals.len(),"candidate_fitness":fitness,
            "training_versus_native":versus(&native_training, &candidate_training),
            "excluded_versus_native":versus(&native_excluded, &candidate_excluded),
            "seconds":started.elapsed().as_secs_f64()}),
    );
    eprintln!(
        "preflight passed in {:.1} s",
        started.elapsed().as_secs_f64()
    );
}

fn train<const TIME: bool>(root: &Path, plan: &Value) {
    let receipt = read(root, "preflight/results.json");
    assert_eq!(
        receipt["runtime"],
        engine_agent::evolution::runtime_id().unwrap()
    );
    assert_eq!(receipt["passed"], true);
    let pool = EpisodePool::new(worker_config()).unwrap();
    let base = read(root, "base-plan.json");
    let (training, _) = split(restore(&pool, &read(root, "inputs.json")["sources"]), &base);
    drop(pool);
    let cases = retention_cases(&base);
    let task = task::<TIME>(&training, &cases, plan);
    let seed = load_model(root, plan, "candidate");
    let out = root.join(arm::<TIME>());
    fs::create_dir_all(&out).unwrap();
    let path = out.join("checkpoint.json");
    let configuration: EvolutionConfig = serde_json::from_value(plan["evolution"].clone()).unwrap();
    let seeds = vec![seed.genome().clone(); configuration.population];
    let mut run = if path.exists() {
        EvolutionRun::load(&path, &task).unwrap()
    } else {
        EvolutionRun::new(
            &task,
            seed.genome().clone(),
            seeds.clone(),
            configuration.clone(),
        )
        .unwrap()
    };
    assert_eq!(run.config(), &configuration);
    assert_eq!(run.seeds(), seeds);
    run.save(&path).unwrap();
    let started = Instant::now();
    loop {
        let before = run.history().len();
        if !run.advance(&task).unwrap() {
            break;
        }
        run.save(&path).unwrap();
        if run.history().len() != before {
            let winner = run.history().last().unwrap();
            // Elites carry the seed's 621 successes, so no winner can have fewer.
            assert_eq!(winner.best_fitness.completed, 621);
            save(out.join("workers.json"), &task.pool.control().snapshot());
            let progress = json!({"arm":arm::<TIME>(),"generation":run.history().len(),
                "generations":configuration.generations,"best_fitness":winner.best_fitness,
                "seconds":started.elapsed().as_secs_f64()});
            atomic(&out, "progress.json", &progress);
            eprintln!("{progress}");
        }
    }
    assert_eq!(run, EvolutionRun::load(&path, &task).unwrap());
    let winner = run.history().last().unwrap();
    save(out.join("final.json"), &winner.best_genome);
    atomic(
        &out,
        "selection.json",
        &json!({"generation":winner.generation + 1,"genome":winner.best_id,
            "fitness":winner.best_fitness,"checkpoint_sha256":hash(&fs::read(&path).unwrap())}),
    );
    save(
        out.join("timing.json"),
        &json!({"seconds":started.elapsed().as_secs_f64(),"complete":run.complete()}),
    );
}

/// Every model and native control on one set of starts, plus the comparison.
fn evaluate_set(
    pool: &EpisodePool,
    sources: &[Source],
    models: &[(&str, DenseModel)],
    traces: Option<&Path>,
) -> (Value, Value) {
    let native = matches(pool, sources, "mission", &models[0].1, traces);
    let mut episodes = vec![json!({"controller":"native","episodes":native})];
    let mut summary = vec![
        json!({"controller":"native","completed":native.iter().filter(|e|e.outcome==Outcome::Arrived).count(),"starts":native.len()}),
    ];
    for (label, model) in models {
        let found = matches(pool, sources, "model", model, traces);
        summary.push(
            json!({"controller":label,"genome":model.id(),"versus_native":versus(&native, &found)}),
        );
        episodes.push(json!({"controller":label,"episodes":found}));
    }
    (json!(summary), json!(episodes))
}

fn evaluate(root: &Path, plan: &Value) {
    let out = root.join("evaluate");
    fs::create_dir(&out).unwrap();
    let started = Instant::now();
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let base = read(root, "base-plan.json");
    let models = [
        ("candidate", load_model(root, plan, "candidate")),
        ("time-final", final_model(root, plan, "time")),
        ("control-final", final_model(root, plan, "control")),
    ];
    let mut results = json!({"runtime":runtime,"plan_sha256":hash(PLAN),
        "models":models.iter().map(|(l,m)|(l.to_string(),json!(m.id()))).collect::<serde_json::Map<_,_>>(),
        "sets":{},"precision":{},"complete":false});

    // Known starts, rebuilt and checked against their records.
    let (training, excluded) = split(restore(&pool, &read(root, "inputs.json")["sources"]), &base);
    for (name, sources) in [("training", &training), ("excluded", &excluded)] {
        let (summary, episodes) = evaluate_set(&pool, sources, &models, None);
        results["sets"][name] = summary;
        save(out.join(format!("{name}-episodes.json")), &episodes);
        atomic(&out, "results.json", &results);
    }
    drop((training, excluded));

    // The fresh check's starts: inspected already, so not a holdout.
    let (inspected, missing) = discover(&pool, &plan["inspected"]);
    let recorded = read(root, "fresh-check-discovery.json");
    assert_eq!(
        parsed(json!(records(&inspected))),
        recorded["sources"],
        "inspected starts changed"
    );
    assert_eq!(parsed(json!(missing)), recorded["missing"]);
    let (summary, episodes) = evaluate_set(&pool, &inspected, &models, None);
    let recorded = read(root, "fresh-check-matches.json");
    for (label, old) in [("native", "native-mission"), ("candidate", "candidate")] {
        let found = episodes
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["controller"] == label)
            .unwrap();
        let expected = by_case(
            controller(&recorded, old)["episodes"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        for e in found["episodes"].as_array().unwrap() {
            assert_eq!(
                parsed(e.clone()),
                expected[e["case"].as_str().unwrap()],
                "inspected parity: {label}"
            );
        }
    }
    results["sets"]["inspected"] = summary;
    save(out.join("inspected-episodes.json"), &episodes);
    atomic(&out, "results.json", &results);
    drop(inspected);

    // New worlds, discovered with no controller-outcome filter.
    let (fresh, missing) = discover(&pool, &plan["fresh"]);
    save(
        out.join("fresh-discovery.json"),
        &json!({"runtime":runtime,"plan_sha256":hash(PLAN),"sources":records(&fresh),"missing":missing,
            "no_controller_outcome_filter":true}),
    );
    // Every controller writes its own trace directory, since the models share a
    // driver name and so a trace file name.
    let traces = out.join("fresh-traces");
    let directory = |label: &str| {
        let directory = traces.join(label);
        fs::create_dir_all(&directory).unwrap();
        directory
    };
    let native = matches(
        &pool,
        &fresh,
        "mission",
        &models[0].1,
        Some(directory("native").as_path()),
    );
    let mut summary = vec![json!({"controller":"native","starts":native.len(),
        "completed":native.iter().filter(|e|e.outcome==Outcome::Arrived).count()})];
    let mut episodes = vec![json!({"controller":"native","episodes":native})];
    for (label, model) in &models {
        let found = matches(
            &pool,
            &fresh,
            "model",
            model,
            Some(directory(label).as_path()),
        );
        summary.push(
            json!({"controller":label,"genome":model.id(),"versus_native":versus(&native, &found)}),
        );
        episodes.push(json!({"controller":label,"episodes":found}));
    }
    results["sets"]["fresh"] = json!(summary);
    save(out.join("fresh-episodes.json"), &episodes);
    atomic(&out, "results.json", &results);
    drop(fresh);

    // Precision: the 157 training cases and the fresh check's 96 inspected cases.
    let training_cases = retention_cases(&base);
    let fresh_check = read(root, "fresh-check-plan.json");
    let inspected_cases: Vec<ship_arrival::ArrivalCase> = fresh_check["precision_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| serde_json::from_value(c["case"].clone()).unwrap())
        .collect();
    let recorded = read(root, "fresh-check-precision.json");
    for (name, cases) in [
        ("training", &training_cases),
        ("inspected", &inspected_cases),
    ] {
        let mut rows = Vec::new();
        for (label, model) in &models {
            let found = precision(&pool, cases, model);
            if name == "inspected" && *label == "candidate" {
                let mut expected = controller(&recorded, "candidate")["episodes"].clone();
                for e in expected.as_array_mut().unwrap() {
                    e["runtime"] = json!(runtime);
                }
                assert_eq!(parsed(json!(found)), expected, "inspected precision parity");
            }
            let arrived = found
                .iter()
                .filter(|e| e.outcome == ship_arrival::ArrivalOutcome::Arrived)
                .count();
            rows.push(
                json!({"controller":label,"arrived":arrived,"cases":found.len(),
                "median_ticks":median(found.iter().map(|e|u64::from(e.elapsed_ticks)).collect())}),
            );
            save(out.join(format!("precision-{name}-{label}.json")), &found);
        }
        results["precision"][name] = json!(rows);
        atomic(&out, "results.json", &results);
    }
    results["complete"] = json!(true);
    results["seconds"] = json!(started.elapsed().as_secs_f64());
    atomic(&out, "results.json", &results);
    eprintln!(
        "evaluation finished in {:.1} s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
#[ignore = "frozen time-ranking experiment; set SPACEWARS_TIME_RANKING_ROOT and SPACEWARS_TIME_RANKING_MODE"]
fn learned_guidance_time_ranking() {
    let root = PathBuf::from(std::env::var_os("SPACEWARS_TIME_RANKING_ROOT").unwrap());
    let mode = std::env::var("SPACEWARS_TIME_RANKING_MODE").unwrap();
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    assert_eq!(
        hash(&fs::read(root.join("plan.json")).unwrap()),
        hash(PLAN),
        "the package plan differs from the compiled plan"
    );
    for (name, digest) in plan["inputs"].as_object().unwrap() {
        assert_eq!(hash(&fs::read(root.join(name)).unwrap()), *digest, "{name}");
    }
    assert_eq!(
        hash(&fs::read(root.join("novelty.json")).unwrap()),
        plan["novelty_sha256"]
    );
    match mode.as_str() {
        "preflight" => preflight(&root, &plan),
        "train-time" => train::<true>(&root, &plan),
        "train-control" => train::<false>(&root, &plan),
        "evaluate" => evaluate(&root, &plan),
        _ => panic!("unknown mode {mode}"),
    }
}

#[test]
fn time_arm_ranks_arrival_time_before_gentle_arrival() {
    fn sample<const TIME: bool>() -> TimeFitness<TIME> {
        TimeFitness {
            completed: 2,
            precision_completed: 3,
            match_completed: 5,
            match_spare_ticks: 7,
            precision_position_points: 11,
            precision_velocity_points: 13,
            approach_speed_points: 17,
            approach_position_points: 19,
            precision_spare_ticks: 23,
        }
    }
    fn declared<const TIME: bool>(plan: &Value) -> Vec<u32> {
        let fields = serde_json::to_value(sample::<TIME>()).unwrap();
        plan["fitness_orders"][arm::<TIME>()]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| match name.as_str().unwrap() {
                "spare_ticks" => 7 + 23,
                name => fields[name].as_u64().unwrap() as u32,
            })
            .collect()
    }
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    assert_eq!(sample::<true>().key(), declared::<true>(&plan));
    assert_eq!(sample::<false>().key(), declared::<false>(&plan));
    let config: EvolutionConfig = serde_json::from_value(plan["evolution"].clone()).unwrap();
    assert_eq!(config.generations, plan["budget"]["generations_per_arm"]);

    fn pair<const TIME: bool>() -> [TimeFitness<TIME>; 3] {
        let base = TimeFitness::<TIME> {
            completed: 621,
            precision_completed: 157,
            match_completed: 464,
            ..TimeFitness::default()
        };
        let slow_gentle = TimeFitness {
            match_spare_ticks: 100,
            approach_speed_points: 900,
            ..base
        };
        let fast_rough = TimeFitness {
            match_spare_ticks: 120,
            approach_speed_points: 800,
            ..base
        };
        let lost = TimeFitness {
            completed: 620,
            match_completed: 463,
            match_spare_ticks: 10_000,
            ..base
        };
        [slow_gentle, fast_rough, lost]
    }
    let [slow_gentle, fast_rough, lost] = pair::<true>();
    assert!(fast_rough > slow_gentle && lost < slow_gentle);
    let [slow_gentle, fast_rough, lost] = pair::<false>();
    assert!(slow_gentle > fast_rough && lost < fast_rough);
}
