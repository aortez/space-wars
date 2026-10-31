//! Learned transfer legs against the native transfer law in whole matches:
//! the recovery bot in both seats, then the learned flight bot in one seat
//! against the recovery bot in the other, in both seat orders, on seeded
//! arenas under the match rules. The host is the client's own match step, so
//! the legs are flown exactly as a player sees them. The learned flight bot
//! is the recovery bot with its transfer legs flown by the stack, so the
//! seat's other numbers isolate the flying. Run in release:
//! `SPACEWARS_MATCH_ARTIFACTS=<dir> cargo test --release -p engine-client --
//! --ignored --nocapture learned_transfer_matches`. With
//! `SPACEWARS_LEARNED_NAVIGATION=<export>` or `SPACEWARS_LEARNED_MOTOR=<export>`
//! the learned seats fly a candidate navigation or motor export with the
//! retained other network instead of the retained stack, so a fit can be
//! measured in matches before it is retained. The matches are three-minute
//! prefixes under the default settings; `SPACEWARS_MATCH_SECONDS=<seconds>`
//! plays longer, `SPACEWARS_MATCH_ASTEROIDS=<seconds>` adds an asteroid
//! stream at that mean interval and `SPACEWARS_MATCH_DIRT=<mode>` turns on
//! loose dirt of that mode, such as `round`.
use super::*;
use engine_common::SpacewarsController::{self, LearnedFlightBot, RecoveryBot};
use serde::Serialize;
use spacewars_ai::mission_learned_transfer::LearnedTransferTelemetry;

const STEP: Duration = Duration::from_nanos(16_666_667);
const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 6];

/// The seeds to run: `SPACEWARS_MATCH_SEEDS=first..last` or the default six.
fn seeds() -> Vec<u64> {
    match std::env::var("SPACEWARS_MATCH_SEEDS") {
        Ok(range) => {
            let (first, last) = range.split_once("..").expect("first..last");
            (first.parse().unwrap()..=last.parse().unwrap()).collect()
        }
        Err(_) => SEEDS.to_vec(),
    }
}
const SECONDS: u64 = 180;
const BUDGET_REASON: &str = "transfer exhausted its progress budget";

/// The seconds to play: `SPACEWARS_MATCH_SECONDS` or a three-minute prefix.
fn seconds() -> u64 {
    std::env::var("SPACEWARS_MATCH_SECONDS").map_or(SECONDS, |seconds| seconds.parse().unwrap())
}

/// The default settings with these controllers, plus the asteroid stream and
/// loose dirt the environment asks for.
fn settings(controllers: [SpacewarsController; 2]) -> Settings {
    let mut settings = Settings::default();
    settings.spacewars.player_1_controller = controllers[0];
    settings.spacewars.player_2_controller = controllers[1];
    if let Ok(seconds) = std::env::var("SPACEWARS_MATCH_ASTEROIDS") {
        settings.material_combat.asteroids.interval_seconds = seconds.parse().unwrap();
    }
    if let Ok(mode) = std::env::var("SPACEWARS_MATCH_DIRT") {
        settings.material_combat.loose_dirt = true;
        settings.material_combat.dirt_mode = serde_json::from_value(mode.into()).unwrap();
    }
    settings
}

#[derive(Debug, Clone, Serialize)]
struct SeatReport {
    controller: &'static str,
    sorties: u32,
    /// Mission handoffs at a destination's entry.
    handoffs: u32,
    /// Ticks from each destination selection to its handoff, launch included.
    leg_ticks: Vec<u64>,
    replans: u32,
    budget_replans: u32,
    recoveries: u32,
    learned: Option<LearnedTransferTelemetry>,
}

#[derive(Debug, Clone, Serialize)]
struct MatchReport {
    seed: u64,
    controllers: [&'static str; 2],
    ticks: u64,
    result: Option<String>,
    owned_planets: [usize; 2],
    seats: [SeatReport; 2],
}

fn label(controller: SpacewarsController) -> &'static str {
    match controller {
        LearnedFlightBot => "learned_flight_bot",
        RecoveryBot => "recovery_bot",
        _ => unreachable!(),
    }
}

/// The retained stack, or a candidate navigation or motor export over the
/// retained other network when `SPACEWARS_LEARNED_NAVIGATION` or
/// `SPACEWARS_LEARNED_MOTOR` names one.
fn candidate_host() -> Option<spacewars_ai::mission_learned_transfer::LearnedTransferHost> {
    use spacewars_ai::flight_stack::FlightStack;
    let navigation_path = std::env::var_os("SPACEWARS_LEARNED_NAVIGATION");
    let motor_path = std::env::var_os("SPACEWARS_LEARNED_MOTOR");
    if navigation_path.is_none() && motor_path.is_none() {
        return None;
    }
    let navigation = serde_json::from_slice(&match navigation_path {
        Some(path) => std::fs::read(path).unwrap(),
        None => include_bytes!(
            "../../../../../spacewars-ai/fixtures/learned-flight-stack-v2/navigation.json"
        )
        .to_vec(),
    })
    .unwrap();
    let motor = serde_json::from_slice(&match motor_path {
        Some(path) => std::fs::read(path).unwrap(),
        None => include_bytes!(
            "../../../../../spacewars-ai/fixtures/learned-flight-stack-v2/motor.json"
        )
        .to_vec(),
    })
    .unwrap();
    let stack = FlightStack::from_files(navigation, motor).unwrap();
    Some(spacewars_ai::mission_learned_transfer::LearnedTransferHost::with_stack(stack, 2).unwrap())
}

fn run(seed: u64, controllers: [SpacewarsController; 2]) -> MatchReport {
    let mut client = create_match(
        seed,
        &settings(controllers),
        Viewport::new(800.0, 480.0),
        ScenarioStartMode::Normal,
        &ScenarioAsset::None,
    )
    .unwrap();
    if let Some(candidate) = candidate_host() {
        let host = client
            .as_any_mut()
            .downcast_mut::<MaterialMissionClientScenario>()
            .unwrap();
        if host.learned.is_some() {
            host.learned = Some(candidate);
        }
    }
    for _ in 0..seconds() * 60 {
        client.step(&[], STEP);
        if client.is_game_over() {
            break;
        }
    }
    let host = client
        .as_any()
        .downcast_ref::<MaterialMissionClientScenario>()
        .unwrap();
    let seats = std::array::from_fn(|seat| {
        let telemetry = host.pilots[seat].telemetry();
        let mut selected = None;
        let mut leg_ticks = Vec::new();
        for event in &telemetry.events {
            match event.kind {
                "selected" => selected = Some((event.planet, event.tick)),
                "arrived" => {
                    if let Some((planet, tick)) = selected.take()
                        && planet == event.planet
                    {
                        leg_ticks.push(event.tick - tick);
                    }
                }
                _ => {}
            }
        }
        SeatReport {
            controller: label(controllers[seat]),
            sorties: telemetry.completed_sorties,
            handoffs: telemetry
                .events
                .iter()
                .filter(|e| e.kind == "arrived")
                .count() as u32,
            leg_ticks,
            replans: telemetry.replans,
            budget_replans: telemetry
                .events
                .iter()
                .filter(|e| e.kind == "replan" && e.reason == Some(BUDGET_REASON))
                .count() as u32,
            recoveries: telemetry.completed_recoveries,
            learned: host
                .learned
                .as_ref()
                .filter(|_| host.pilots[seat].learned_transfer())
                .map(|learned| learned.telemetry(seat).clone()),
        }
    });
    let round = host.sortie.state.match_observation().unwrap();
    MatchReport {
        seed,
        controllers: controllers.map(label),
        ticks: host.sortie.state.tick(),
        result: host.game_over_message(),
        owned_planets: round.owned_planets,
        seats,
    }
}

#[test]
#[ignore = "a release-mode comparison of about ten minutes; see the module doc"]
fn learned_transfer_matches_against_the_native_transfer_law() {
    let mut reports = Vec::new();
    for seed in seeds() {
        for controllers in [
            [RecoveryBot, RecoveryBot],
            [LearnedFlightBot, RecoveryBot],
            [RecoveryBot, LearnedFlightBot],
        ] {
            let report = run(seed, controllers);
            for seat in &report.seats {
                let legs = seat.leg_ticks.len();
                let mean = seat.leg_ticks.iter().sum::<u64>() as f64 / legs.max(1) as f64;
                println!(
                    "seed {seed} {:?}: {} sorties {} handoffs {} (mean {:.0} ticks) replans {} budget {} recoveries {} learned {}",
                    report.controllers,
                    seat.controller,
                    seat.sorties,
                    seat.handoffs,
                    mean,
                    seat.replans,
                    seat.budget_replans,
                    seat.recoveries,
                    seat.learned.as_ref().map_or("off".into(), |l| format!(
                        "legs {} attempts {} flown {} native {} refusals {} arrivals {} deadlines {}",
                        l.legs, l.attempts, l.flown_ticks, l.native_ticks, l.refusals, l.arrivals, l.deadlines
                    )),
                );
            }
            println!(
                "seed {seed} {:?}: {} ticks, planets {:?}, {}",
                report.controllers,
                report.ticks,
                report.owned_planets,
                report.result.as_deref().unwrap_or("in progress")
            );
            reports.push(report);
        }
    }
    if let Some(directory) = std::env::var_os("SPACEWARS_MATCH_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("learned-transfer-matches.json"),
            serde_json::to_vec_pretty(&reports).unwrap(),
        )
        .unwrap();
    }
}

/// A per-tick trace of one learned seat for a lifecycle diagnosis: what
/// withdraws the leg, how the attempts run, and how the distance closes.
#[test]
#[ignore = "a release-mode trace written to SPACEWARS_MATCH_ARTIFACTS"]
fn trace_learned_transfer_seat() {
    use std::io::Write;
    let seed: u64 = std::env::var("SPACEWARS_TRACE_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let seat: usize = std::env::var("SPACEWARS_TRACE_SEAT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut controllers = [RecoveryBot; 2];
    controllers[seat] = LearnedFlightBot;
    let mut client = create_match(
        seed,
        &settings(controllers),
        Viewport::new(800.0, 480.0),
        ScenarioStartMode::Normal,
        &ScenarioAsset::None,
    )
    .unwrap();
    if let Some(candidate) = candidate_host() {
        let host = client
            .as_any_mut()
            .downcast_mut::<MaterialMissionClientScenario>()
            .unwrap();
        host.learned = Some(candidate);
    }
    let directory =
        std::path::PathBuf::from(std::env::var_os("SPACEWARS_MATCH_ARTIFACTS").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(directory.join(format!("trace-seed{seed}-seat{seat}.tsv"))).unwrap(),
    );
    // An optional window of ticks whose policy observations, stack commands
    // and physical ship state are dumped for an offline law diagnosis.
    let dump: Option<(u64, u64)> = std::env::var("SPACEWARS_TRACE_DUMP").ok().and_then(|s| {
        s.split_once(':')
            .map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap()))
    });
    let mut dumped = dump.map(|_| {
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(
                directory.join(format!("observations-seed{seed}-seat{seat}.jsonl")),
            )
            .unwrap(),
        );
        writeln!(
            file,
            "{}",
            serde_json::json!({"schema": scenario_spacewars::surface_sortie::flight_policy_observation::schema()})
        )
        .unwrap();
        file
    });
    writeln!(out, "tick\tgoal\treason\ttarget\tframe\taltitude\tleg\tflying\tattempt\telapsed\tdistance\tspeed_error\tship_speed\toutcome\tcommand_speed\tcommand_heading\theat_clearance\tsurface_clearance\tsteering\tthrust\tbrake\tspin\theading_error\tnavigation_tick\tlocation\tship_form\tship_health\tpilot_health\topponent_distance").unwrap();
    for _ in 0..seconds() * 60 {
        client.step(&[], STEP);
        let host = client
            .as_any()
            .downcast_ref::<MaterialMissionClientScenario>()
            .unwrap();
        let telemetry = host.pilots[seat].telemetry();
        let o = host.sortie.state.mission_observation(seat, None);
        let p = &o.local.combat.recovery.flight.pilot;
        let altitude = p.ship.position.distance_to(p.planet.motion.position) - p.planet.radius;
        let leg = host.pilots[seat].transfer_leg();
        let learned = host.learned.as_ref().unwrap();
        let status = learned.telemetry(seat).last;
        let decision = learned.last_decision(seat);
        let command = decision.map(|d| d.command);
        let action = decision.map(|d| d.action);
        let heading_error = command.map(|c| {
            let error = (c.heading - p.ship.angle).rem_euclid(std::f32::consts::TAU);
            if error > std::f32::consts::PI {
                error - std::f32::consts::TAU
            } else {
                error
            }
        });
        // Clearances from the mission observation: the sun's obstacle radius
        // and the planets' routing bounds.
        let heat = o.sun.map_or(f32::INFINITY, |sun| {
            p.ship.position.distance_to(sun.position) - sun.radius
        });
        let surface = o
            .planets
            .iter()
            .map(|planet| p.ship.position.distance_to(planet.motion.position) - planet.radius)
            .fold(f32::INFINITY, f32::min);
        writeln!(
            out,
            "{}\t{:?}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\t{:.0}\t{:.0}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{:?}\t{:?}\t{:.0}\t{:.0}\t{}",
            host.sortie.state.tick(),
            telemetry.goal,
            telemetry.reason.unwrap_or(""),
            telemetry.target.map_or(-1, |t| t as i64),
            p.planet.index,
            altitude,
            leg.map_or(String::from("-"), |l| format!("{:.0}", l.distance)),
            learned.flying(seat),
            status.map_or(0, |s| s.goal_id),
            status.map_or(0, |s| s.elapsed_ticks),
            status.map_or(String::from("-"), |s| format!("{:.1}", s.distance)),
            status.map_or(String::from("-"), |s| format!("{:.1}", s.speed_error)),
            p.ship.velocity.length(),
            status.and_then(|s| s.outcome).map_or(String::from("-"), |o| format!("{o:?}")),
            command.map_or(String::from("-"), |c| format!("{:.1}", c.speed)),
            command.map_or(String::from("-"), |c| format!("{:.2}", c.heading)),
            heat,
            surface,
            action.map_or(String::from("-"), |a| format!("{:.2}", a.steering)),
            action.map_or(String::from("-"), |a| format!("{:.2}", a.thrust)),
            action.map_or(String::from("-"), |a| format!("{:.2}", a.brake)),
            p.ship.spin,
            heading_error.map_or(String::from("-"), |e| format!("{e:.2}")),
            decision.is_some_and(|d| d.navigation.is_some()),
            p.location,
            p.ship_form,
            p.ship_health,
            o.match_context
                .as_ref()
                .map_or(f32::NAN, |context| context.pilot_health[seat]),
            o.opponent.map_or(String::from("-"), |opponent| format!(
                "{:.0}",
                p.ship.position.distance_to(opponent.motion.position)
            )),
        )
        .unwrap();
        if let Some(file) = &mut dumped
            && let (Some(leg), Some((from, to))) = (leg, dump)
            && (from..=to).contains(&host.sortie.state.tick())
        {
            let state = &host.sortie.state;
            let own = state.free_flight_observation(seat).unwrap();
            let observation = match learned.observation(seat, state, leg) {
                Some(Ok(o)) => serde_json::to_value(o.inputs).unwrap(),
                Some(Err(e)) => serde_json::json!({"error": e.to_string()}),
                None => serde_json::json!({"error": "not flying"}),
            };
            // The motor network's inputs for the held command, for a
            // comparison with the teacher it imitates.
            let motor_inputs = command.and_then(|command| {
                scenario_spacewars::surface_sortie::flight_environment::motor::command_inputs_for(
                    learned.stack().contract(),
                    state,
                    seat,
                    scenario_spacewars::surface_sortie::flight_environment::FlightAction {
                        steering: own.applied[0],
                        thrust: own.applied[1],
                        brake: own.applied[2],
                    },
                    command,
                )
                .ok()
            });
            writeln!(
                file,
                "{}",
                serde_json::json!({
                    "tick": state.tick(),
                    "inputs": observation,
                    "goal": {"position": leg.entry, "velocity": leg.entry_velocity},
                    "ship": {"position": own.ship.position, "velocity": own.ship.velocity, "angle": own.ship.angle, "spin": own.ship.spin,
                             "frame_velocity": own.frame_velocity, "planet": own.planet, "planet_velocity": own.planet_motion.velocity,
                             "gravity": own.gravity, "applied": own.applied, "forward_speed": own.flight.forward_speed,
                             "flight_enabled": own.flight.enabled, "wings_closed": own.flight.wings_closed, "sweep": own.flight.sweep,
                             "contact": own.contact, "supported_feet": own.landing.supported_feet, "alive": own.alive,
                             "controls_armed": own.controls_armed, "available": own.available, "location": format!("{:?}", own.location)},
                    "decision": decision.map(|d| serde_json::json!({"command": d.command, "navigation": d.navigation, "action": d.action, "progress": d.progress})),
                    "motor_inputs": motor_inputs,
                })
            )
            .unwrap();
        }
        if client.is_game_over() {
            break;
        }
    }
}
