//! Reusable pieces of the match-approach harness: frozen starts and one
//! episode runner for native control and every guidance policy.
//! Copied from `guidance_approach` and its `time_ranking` module, whose pinned
//! sources stay unchanged. For the drivers those accept, episodes and traces
//! are identical; a perception policy also reads the match world's bodies.
use super::*;
use engine_agent::{
    ship_curriculum,
    training_workers::{EpisodePool, WorkerConfig},
};
use engine_common::{DirtMode, SpacewarsController};
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, flight::FlightControlLimits,
        free_flight::FreeFlightObservation,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    guidance::{
        Binding, Controller, Decision, Features, Movement, Policy, Request, Target,
        perception::Perception,
    },
    mission_pilot::MissionGoal,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    sync::Mutex,
};

pub(super) const STEP: Duration = Duration::from_nanos(16_666_667);
pub(super) const MAX_TICKS: u64 = 300;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct Case {
    pub(super) name: String,
    pub(super) seed: u64,
    pub(super) shape: DirtMode,
    pub(super) seat: usize,
    pub(super) source_tick: u64,
    pub(super) planet: usize,
    pub(super) source_entry_distance: f32,
    pub(super) source_relative_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Arrived,
    Timeout,
    LostControl,
    FrameChanged,
    Contact,
    Assisted,
    Damaged,
    OutsideRegion,
    Saturated,
    InvalidOutput,
    HostPreempted,
    RoundFinished,
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Episode {
    pub(super) case: String,
    pub(super) controller: String,
    pub(super) outcome: Outcome,
    pub(super) elapsed_ticks: u64,
    pub(super) owned_component_ticks: u64,
    pub(super) thrust_ticks: u64,
    pub(super) brake_ticks: u64,
    pub(super) absolute_turn: f64,
    pub(super) initial: FreeFlightObservation,
    #[serde(rename = "final")]
    pub(super) final_flight: Option<FreeFlightObservation>,
    pub(super) host_reason: Option<Value>,
    pub(super) error_detail: Option<String>,
    pub(super) final_mission: Value,
    pub(super) frozen_target_offset: Vec2,
}

/// Who flies an episode: native control throughout, or a guidance policy
/// until native control preempts it or hands off to capture.
#[derive(Debug, Clone)]
pub(super) enum Driver {
    Native,
    Guidance(Box<Policy>),
}

impl Driver {
    pub(super) fn guidance(policy: Policy) -> Self {
        Self::Guidance(Box::new(policy))
    }

    /// The lineage's controller names; every model is "model".
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Native => "mission",
            Self::Guidance(policy) => match **policy {
                Policy::Reference => "reference",
                Policy::Idle => "idle",
                Policy::Model(_) | Policy::Perception(_) => "model",
            },
        }
    }
}

/// An episode and, when requested, every decision the policy made.
pub(super) struct Run {
    pub(super) episode: Episode,
    pub(super) decisions: Vec<Decision>,
}

pub(super) type Source = (Case, Mutex<MaterialMissionClientScenario>);

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn save(path: impl AsRef<Path>, value: &impl Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub(super) fn atomic(out: &Path, name: &str, value: &impl Serialize) {
    let temporary = out.join(format!(".{name}"));
    save(&temporary, value);
    fs::rename(temporary, out.join(name)).unwrap();
}

pub(super) fn read(root: &Path, name: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
}

pub(super) fn parsed(value: Value) -> Value {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

pub(super) fn fork(source: &MaterialMissionClientScenario) -> MaterialMissionClientScenario {
    MaterialMissionClientScenario {
        sortie: SurfaceSortieClientScenario::new(source.sortie.state.clone()),
        pilots: source.pilots.clone(),
        recovery_profiles: source.recovery_profiles,
        bots: source.bots,
        registration: source.registration,
        seed: source.seed,
        profile: profiling::Profile::default(),
        evaluation: source.evaluation.clone(),
        surveys: source.surveys.clone(),
        flag_surveys: source.flag_surveys.clone(),
        flag_value_shadow: source.flag_value_shadow.clone(),
        // A fresh host: the forked seat resumes its leg with a new attempt.
        learned: source
            .learned
            .as_ref()
            .map(|_| LearnedTransferHost::new(2).expect("the retained learned flight stack")),
    }
}

pub(super) fn flight(run: &MaterialMissionClientScenario, seat: usize) -> FreeFlightObservation {
    run.sortie.state.free_flight_observation(seat).unwrap()
}

pub(super) fn failure(
    f: &FreeFlightObservation,
    initial: &FreeFlightObservation,
) -> Option<Outcome> {
    if f.version != 1
        || !f.alive
        || !f.available
        || f.form != ShipForm::Ship
        || f.owner != initial.owner
        || f.spaceling != initial.spaceling
        || f.vehicle != initial.vehicle
        || f.location != PilotLocation::Aboard(initial.vehicle)
        || !f.controls_armed
        || !f.flight.enabled
        || f.flight.wings_closed
        || f.flight.sweep != 0.0
        || f.flight.limits != FlightControlLimits::for_sweep(0.0)
    {
        return Some(Outcome::LostControl);
    }
    if f.planet != initial.planet {
        return Some(Outcome::FrameChanged);
    }
    if f.contact || f.landing.supported_feet > 0 {
        return Some(Outcome::Contact);
    }
    if f.landing.assist_strength > 0.0 || f.landing.phase != LandingPhase::Flying {
        return Some(Outcome::Assisted);
    }
    if f.health < initial.health {
        return Some(Outcome::Damaged);
    }
    let altitude = f.ship.position.distance_to(f.planet_motion.position) - f.planet_radius;
    if !altitude.is_finite() || !(40.0..=220.0).contains(&altitude) {
        return Some(Outcome::OutsideRegion);
    }
    None
}

pub(super) fn target(f: &FreeFlightObservation, offset: Vec2) -> Target {
    Target {
        id: f.planet as u64,
        position: f.planet_motion.position + offset,
        velocity: f.planet_motion.velocity,
    }
}

pub(super) fn probe(
    run: &MaterialMissionClientScenario,
    seat: usize,
) -> MaterialMissionClientScenario {
    let mut checked = fork(run);
    checked.bots[seat] = true;
    checked.step(&[], STEP);
    checked
}

pub(super) fn arrived(run: &MaterialMissionClientScenario, case: &Case, tick: u64) -> bool {
    let mission = run.pilots[case.seat].telemetry();
    mission.goal == MissionGoal::Capture
        && mission.target == Some(case.planet)
        && mission.events.iter().any(|event| {
            event.tick == tick && event.planet == Some(case.planet) && event.kind == "arrived"
        })
}

pub(super) fn direct_transfer(run: &MaterialMissionClientScenario, case: &Case) -> bool {
    let mission = run.pilots[case.seat].telemetry();
    mission.goal == MissionGoal::Transfer
        && mission.target == Some(case.planet)
        && mission.avoidance.is_none()
}

pub(super) fn start_match(seed: u64, shape: DirtMode) -> MaterialMissionClientScenario {
    let mut settings = Settings::default();
    settings.spacewars.player_1_controller = SpacewarsController::RecoveryBot;
    settings.spacewars.player_2_controller = SpacewarsController::RecoveryBot;
    settings.spacewars_match.time_limit_seconds = 600;
    settings.material_combat.loose_dirt = true;
    settings.material_combat.dirt_mode = shape;
    settings.material_combat.asteroids.interval_seconds = 0;
    let mut boxed = create_match(
        seed,
        &settings,
        Viewport::new(800.0, 480.0),
        ScenarioStartMode::Normal,
        &ScenarioAsset::None,
    )
    .unwrap();
    let run = boxed
        .as_any_mut()
        .downcast_mut::<MaterialMissionClientScenario>()
        .unwrap();
    fork(run)
}

pub(super) fn prepare(case: &Case) -> MaterialMissionClientScenario {
    assert!(case.seat < 2);
    let mut source = start_match(case.seed, case.shape);
    let run = &mut source;
    while run.sortie.state.tick() < case.source_tick {
        assert!(!run.is_game_over(), "source unavailable: {}", case.name);
        run.step(&[], STEP);
    }
    let before = flight(run, case.seat);
    assert_eq!(before.planet, case.planet);
    assert_eq!(failure(&before, &before), None);
    let distance = (before
        .ship
        .position
        .distance_to(before.planet_motion.position)
        - before.planet_radius
        - 85.0)
        .abs();
    let speed = (before.ship.velocity - before.planet_motion.velocity).length();
    assert!((distance - case.source_entry_distance).abs() < 0.001);
    assert!((speed - case.source_relative_speed).abs() < 0.001);
    assert!(direct_transfer(&probe(run, case.seat), case));
    // Every arm receives this same physical neutral tick. The actor's mission
    // memory and absolute clocks are retained, including the original PWM state.
    run.bots[case.seat] = false;
    run.step(&Movement::default().encode(before.owner), STEP);
    run.bots[case.seat] = true;
    let initial = flight(run, case.seat);
    assert_eq!(
        failure(&initial, &before),
        None,
        "invalid setup: {}",
        case.name
    );
    assert_eq!(initial.applied, [0.0; 3]);
    assert!(direct_transfer(&probe(run, case.seat), case));
    fork(run)
}

pub(super) fn initial_record(run: &MaterialMissionClientScenario, case: &Case) -> Value {
    json!({
        "case":case, "flight":flight(run,case.seat),
        "world_sha256":hash(&SurfaceSortieScenario::observe(&run.sortie.state).payload),
        "mission_sha256":hash(format!("{:?}",run.pilots).as_bytes()),
        "mission":run.pilots.each_ref().map(|p|p.telemetry())
    })
}

/// One approach from a frozen start. A perception policy reads the bodies of
/// the same completed step as its nine features, with the case's planet as
/// the destination. `keep` retains every decision.
pub(super) fn run(
    source: &MaterialMissionClientScenario,
    case: &Case,
    driver: &Driver,
    traces: Option<&Path>,
    keep: bool,
) -> Run {
    let mut run = fork(source);
    let initial = flight(&run, case.seat);
    let offset = (initial.ship.position - initial.planet_motion.position).normalized()
        * (initial.planet_radius + 85.0);
    let binding = Binding {
        owner: initial.owner,
        spaceling: initial.spaceling,
        vehicle: initial.vehicle,
        planet: initial.planet,
        epoch: 1,
    };
    let name = driver.name();
    let (mut controller, perceives) = match driver {
        Driver::Native => (None, false),
        Driver::Guidance(policy) => (
            Some(Controller::new((**policy).clone(), binding).unwrap()),
            policy.perceives(),
        ),
    };
    run.bots[case.seat] = controller.is_none();
    let mut trace = traces.map(|out| {
        BufWriter::new(File::create(out.join(format!("{}-{name}.jsonl", case.name))).unwrap())
    });
    let mut decisions = Vec::new();
    let mut thrust_ticks = 0_u64;
    let mut brake_ticks = 0_u64;
    let mut absolute_turn = 0.0_f64;
    let mut owned_ticks = 0_u64;
    let mut host_reason = None;
    let mut error_detail = None;
    let outcome = loop {
        let Some(current) = run.sortie.state.free_flight_observation(case.seat) else {
            break Outcome::LostControl;
        };
        if let Some(failed) = failure(&current, &initial) {
            break failed;
        }
        if arrived(&run, case, current.tick.saturating_sub(1)) {
            break Outcome::Arrived;
        }
        if run.is_game_over() {
            break Outcome::RoundFinished;
        }
        if current.tick - initial.tick >= MAX_TICKS {
            break Outcome::Timeout;
        }
        let marker = target(&current, offset);
        let features = Features::read(&current, marker);
        let encoded = match features.encode() {
            Ok(encoded) => encoded,
            Err(error) => {
                error_detail = Some(error.to_string());
                break Outcome::InvalidOutput;
            }
        };
        if encoded.saturated.iter().any(|s| *s) {
            break Outcome::Saturated;
        }
        // The clone supplies the native priority decision and a real neutral
        // capture handoff. Discarded guide/PWM updates never reach a component arm.
        let checked = probe(&run, case.seat);
        let handoff = arrived(&checked, case, current.tick);
        if !handoff && !direct_transfer(&checked, case) {
            host_reason =
                Some(serde_json::to_value(checked.pilots[case.seat].telemetry()).unwrap());
            break Outcome::HostPreempted;
        }
        let decision = if let Some(controller) = controller.as_mut().filter(|_| !handoff) {
            let request = Request {
                binding,
                tick: current.tick,
                target_id: marker.id,
                features,
            };
            let result = if perceives {
                let bodies = run.sortie.state.celestial_bodies();
                controller.decide_perceiving(
                    request,
                    &Perception::read(&current, &bodies, Some(case.planet)),
                )
            } else {
                controller.decide(request)
            };
            let decision = match result {
                Ok(decision) => decision,
                Err(error) => {
                    error_detail = Some(error.to_string());
                    break Outcome::InvalidOutput;
                }
            };
            run.step(&decision.movement.encode(initial.owner), STEP);
            owned_ticks += 1;
            Some(decision)
        } else {
            run = checked;
            None
        };
        let after = run.sortie.state.free_flight_observation(case.seat);
        if let Some(f) = &after {
            thrust_ticks += u64::from(f.applied[1] > 0.0);
            brake_ticks += u64::from(f.applied[2] > 0.0);
            absolute_turn += f64::from(f.applied[0].abs());
        }
        if let Some(trace) = &mut trace {
            serde_json::to_writer(
                &mut *trace,
                &json!({
                    "tick":current.tick,"before":current,"target":marker,"encoded":encoded,
                    "decision":decision,"native_handoff":handoff,"after":after,
                    "world_sha256":hash(&SurfaceSortieScenario::observe(&run.sortie.state).payload)
                }),
            )
            .unwrap();
            writeln!(trace).unwrap();
        }
        if keep && let Some(decision) = decision {
            decisions.push(decision);
        }
    };
    if let Some(trace) = &mut trace {
        trace.flush().unwrap();
    }
    let episode = Episode {
        case: case.name.clone(),
        controller: name.into(),
        outcome,
        elapsed_ticks: run.sortie.state.tick() - initial.tick,
        owned_component_ticks: owned_ticks,
        thrust_ticks,
        brake_ticks,
        absolute_turn,
        initial,
        final_flight: run.sortie.state.free_flight_observation(case.seat),
        host_reason,
        error_detail,
        final_mission: serde_json::to_value(run.pilots[case.seat].telemetry()).unwrap(),
        frozen_target_offset: offset,
    };
    Run { episode, decisions }
}

pub(super) fn worker_config() -> WorkerConfig {
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

/// The precision cases a lineage plan retains.
pub(super) fn retention_cases(plan: &Value) -> Vec<engine_agent::ship_arrival::ArrivalCase> {
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

pub(super) fn records(sources: &[Source]) -> Vec<Value> {
    sources
        .iter()
        .map(|(c, s)| initial_record(&s.lock().unwrap(), c))
        .collect()
}

/// Rebuild recorded match states and check them against their records.
pub(super) fn restore(pool: &EpisodePool, recorded: &Value) -> Vec<Source> {
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

/// A lineage plan's training approaches and the other known ones.
pub(super) fn split(all: Vec<Source>, base: &Value) -> (Vec<Source>, Vec<Source>) {
    let included: BTreeSet<String> = base["retained_match_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(base["focal_cases"].as_array().unwrap())
        .map(|n| n.as_str().unwrap().into())
        .collect();
    all.into_iter()
        .partition(|(c, _)| included.contains(&c.name))
}

pub(super) fn by_case(episodes: impl IntoIterator<Item = Value>) -> BTreeMap<String, Value> {
    episodes
        .into_iter()
        .map(|e| (e["case"].as_str().unwrap().to_owned(), e))
        .collect()
}
