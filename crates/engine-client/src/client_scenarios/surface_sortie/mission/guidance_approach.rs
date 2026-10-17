//! Frozen development continuations from real match approaches. Component
//! trials hold the selected mission memory; this is not a live hybrid adapter.
use super::*;
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
    guidance::{Binding, Controller, Features, Movement, Policy, Request, Target},
    mission_pilot::MissionGoal,
    model::DenseModel,
};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

mod training;

const PLAN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/data/learned-guidance-approach-v1-plan.json"
));
const GENOME: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../engine-agent/fixtures/ship-arrival-mixed-v1/seed-1.json"
));
const STEP: Duration = Duration::from_nanos(16_666_667);
const MAX_TICKS: u64 = 300;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Case {
    name: String,
    seed: u64,
    shape: DirtMode,
    seat: usize,
    source_tick: u64,
    planet: usize,
    source_entry_distance: f32,
    source_relative_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
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
struct Episode {
    case: String,
    controller: String,
    outcome: Outcome,
    elapsed_ticks: u64,
    owned_component_ticks: u64,
    thrust_ticks: u64,
    brake_ticks: u64,
    absolute_turn: f64,
    initial: FreeFlightObservation,
    #[serde(rename = "final")]
    final_flight: Option<FreeFlightObservation>,
    host_reason: Option<Value>,
    error_detail: Option<String>,
    final_mission: Value,
    frozen_target_offset: Vec2,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn save(path: impl AsRef<Path>, value: &impl Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn fork(source: &MaterialMissionClientScenario) -> MaterialMissionClientScenario {
    use training::performance::timed;
    timed("match_clone", || MaterialMissionClientScenario {
        sortie: {
            let state = timed("clone.state", || source.sortie.state.clone());
            timed("clone.cameras", || SurfaceSortieClientScenario::new(state))
        },
        pilots: timed("clone.pilots", || source.pilots.clone()),
        recovery_profiles: source.recovery_profiles,
        bots: source.bots,
        registration: source.registration,
        seed: source.seed,
        profile: profiling::Profile::default(),
        evaluation: timed("clone.evaluation", || source.evaluation.clone()),
        surveys: timed("clone.surveys", || source.surveys.clone()),
        flag_surveys: timed("clone.flag_surveys", || source.flag_surveys.clone()),
        flag_value_shadow: timed("clone.flag_value_shadow", || {
            source.flag_value_shadow.clone()
        }),
    })
}

fn flight(run: &MaterialMissionClientScenario, seat: usize) -> FreeFlightObservation {
    run.sortie.state.free_flight_observation(seat).unwrap()
}

fn failure(f: &FreeFlightObservation, initial: &FreeFlightObservation) -> Option<Outcome> {
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

fn target(f: &FreeFlightObservation, offset: Vec2) -> Target {
    Target {
        id: f.planet as u64,
        position: f.planet_motion.position + offset,
        velocity: f.planet_motion.velocity,
    }
}

fn probe(run: &MaterialMissionClientScenario, seat: usize) -> MaterialMissionClientScenario {
    training::performance::timed("host_probe", || {
        let mut checked = fork(run);
        checked.bots[seat] = true;
        training::performance::timed("host_step", || checked.step(&[], STEP));
        training::performance::record_match_step("host", &checked);
        checked
    })
}

fn arrived(run: &MaterialMissionClientScenario, case: &Case, tick: u64) -> bool {
    let mission = run.pilots[case.seat].telemetry();
    mission.goal == MissionGoal::Capture
        && mission.target == Some(case.planet)
        && mission.events.iter().any(|event| {
            event.tick == tick && event.planet == Some(case.planet) && event.kind == "arrived"
        })
}

fn direct_transfer(run: &MaterialMissionClientScenario, case: &Case) -> bool {
    let mission = run.pilots[case.seat].telemetry();
    mission.goal == MissionGoal::Transfer
        && mission.target == Some(case.planet)
        && mission.avoidance.is_none()
}

fn start_match(seed: u64, shape: DirtMode) -> MaterialMissionClientScenario {
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

fn prepare(case: &Case) -> MaterialMissionClientScenario {
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

fn initial_record(run: &MaterialMissionClientScenario, case: &Case) -> Value {
    json!({
        "case":case, "flight":flight(run,case.seat),
        "world_sha256":hash(&SurfaceSortieScenario::observe(&run.sortie.state).payload),
        "mission_sha256":hash(format!("{:?}",run.pilots).as_bytes()),
        "mission":run.pilots.each_ref().map(|p|p.telemetry())
    })
}

fn episode(
    source: &MaterialMissionClientScenario,
    case: &Case,
    name: &str,
    model: &DenseModel,
    out: Option<&Path>,
) -> Episode {
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
    let mut controller = match name {
        "mission" => None,
        "reference" => Some(Controller::new(Policy::Reference, binding).unwrap()),
        "model" => Some(Controller::new(Policy::Model(model.clone()), binding).unwrap()),
        _ => panic!("unknown controller"),
    };
    run.bots[case.seat] = controller.is_none();
    let mut trace = out.map(|out| {
        BufWriter::new(File::create(out.join(format!("{}-{name}.jsonl", case.name))).unwrap())
    });
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
            training::performance::timed("match_drop", || drop(checked));
            break Outcome::HostPreempted;
        }
        let decision = if handoff || controller.is_none() {
            training::performance::timed("match_drop", || {
                drop(std::mem::replace(&mut run, checked))
            });
            None
        } else {
            let result = training::performance::timed("match_network_decision", || {
                controller.as_mut().unwrap().decide(Request {
                    binding,
                    tick: current.tick,
                    target_id: marker.id,
                    features,
                })
            });
            let decision = match result {
                Ok(decision) => decision,
                Err(error) => {
                    error_detail = Some(error.to_string());
                    training::performance::timed("match_drop", || drop(checked));
                    break Outcome::InvalidOutput;
                }
            };
            training::performance::timed("component_step", || {
                run.step(&decision.movement.encode(initial.owner), STEP)
            });
            training::performance::record_match_step("component", &run);
            owned_ticks += 1;
            training::performance::timed("match_drop", || drop(checked));
            Some(decision)
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
    };
    if let Some(trace) = &mut trace {
        trace.flush().unwrap();
    }
    let result = Episode {
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
    training::performance::timed("match_drop", || drop(run));
    result
}

#[test]
fn approach_probe_preserves_source_and_predicts_native_continuation() {
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    let case: Case = serde_json::from_value(plan["cases"][0].clone()).unwrap();
    let source = prepare(&case);
    let before = initial_record(&source, &case);
    let mut checked = probe(&source, case.seat);
    assert_eq!(initial_record(&source, &case), before);
    let mut ordinary = fork(&source);
    ordinary.step(&[], STEP);
    for _ in 0..30 {
        assert_eq!(
            initial_record(&checked, &case),
            initial_record(&ordinary, &case)
        );
        checked.step(&[], STEP);
        ordinary.step(&[], STEP);
    }
}

#[test]
#[ignore = "frozen development comparison; explicit preflight/evaluate mode and fresh output required"]
fn retained_guidance_match_approaches() {
    let mode = std::env::var("SPACEWARS_APPROACH_MODE").unwrap();
    assert!(["preflight", "evaluate"].contains(&mode.as_str()));
    let out = std::env::var_os("SPACEWARS_APPROACH_OUT").unwrap();
    let out = Path::new(&out);
    fs::create_dir(out).expect("retain previous evidence");
    let plan: Value = serde_json::from_slice(PLAN).unwrap();
    assert_eq!(plan["max_ticks"], MAX_TICKS);
    let model = DenseModel::from_json(GENOME).unwrap();
    assert_eq!(model.id(), plan["candidate"].as_str().unwrap());
    assert_eq!(hash(GENOME), plan["candidate_file_sha256"]);
    let runtime = engine_agent::evolution::runtime_id().unwrap();
    let cases: Vec<Case> = serde_json::from_value(plan["cases"].clone()).unwrap();
    assert_eq!(cases.len(), 8);
    let sources: Vec<_> = cases.iter().map(prepare).collect();
    let preflight = json!({
        "plan_sha256":hash(PLAN),"runtime":runtime,
        "cases":sources.iter().zip(&cases).map(|(s,c)|initial_record(s,c)).collect::<Vec<_>>()
    });
    save(out.join("plan.json"), &plan);
    save(out.join("preflight.json"), &preflight);
    if mode == "preflight" {
        eprintln!("Eight approach starts verified; no controller evaluations executed.");
        return;
    }
    let frozen = std::env::var_os("SPACEWARS_APPROACH_PREFLIGHT").unwrap();
    // Compare the exact emitted receipt. Parsing a JSON f64 can round a promoted
    // f32 differently even when both files are byte-for-byte identical.
    let expected = fs::read(frozen).unwrap();
    let actual = serde_json::to_vec_pretty(&preflight).unwrap();
    assert_eq!(
        hash(&actual),
        hash(&expected),
        "source/build or physical starts changed"
    );
    let mut results = Vec::new();
    for name in ["mission", "reference", "model"] {
        for (source, case) in sources.iter().zip(&cases) {
            let result = episode(source, case, name, &model, Some(out));
            eprintln!(
                "{} {name}: {:?} in {} ticks",
                case.name, result.outcome, result.elapsed_ticks
            );
            results.push(result);
        }
    }
    let counts: Vec<_> = ["mission", "reference", "model"]
        .into_iter()
        .map(|name| {
            let selected: Vec<_> = results.iter().filter(|r| r.controller == name).collect();
            json!({"controller":name,"cases":selected.len(),
            "arrived":selected.iter().filter(|r|r.outcome==Outcome::Arrived).count()})
        })
        .collect();
    let candidate_passed = counts[2]["arrived"] == 8;
    save(
        out.join("results.json"),
        &json!({
            "protocol":plan["protocol"],"plan_sha256":hash(PLAN),"runtime":runtime,
            "counts":counts,"candidate_passed":candidate_passed,"episodes":results,
            "qualification_evidence":false,"production_controls_changed":false
        }),
    );
}
