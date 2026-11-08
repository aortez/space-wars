//! Development measurements for milestone 6's plan, outside any frozen
//! package: the memory every training start takes once rebuilt and held, the
//! time one candidate takes to fly every case, how native control flies the
//! approaches the component could take over, and a pilot that hands those
//! approaches over farther from the entry.
use super::guidance_match::*;
use super::guidance_worlds::*;
use super::*;
use engine_agent::{ship_arrival::ArrivalOutcome, ship_perception, training_workers::EpisodePool};
use engine_common::DirtMode;
use serde_json::{Value, json};
use spacewars_ai::{
    guidance::{Features, Movement, Policy, perception},
    mission_pilot::MissionGoal,
    model::{DenseModel, shared::SharedModel},
};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex, time::Instant};

/// Resident and peak memory of this process, in MiB.
fn memory() -> (u64, u64) {
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let field = |name: &str| {
        let kib: u64 = status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap()
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .unwrap();
        kib / 1024
    };
    (field("VmRSS:"), field("VmHWM:"))
}

#[test]
#[ignore = "development measurement; set SPACEWARS_WORLDS_ROOT and SPACEWARS_KNOWN_ROOT"]
fn perception_training_cost() {
    let worlds_root = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_ROOT").unwrap());
    let known_root = PathBuf::from(std::env::var_os("SPACEWARS_KNOWN_ROOT").unwrap());
    let pool = EpisodePool::new(worker_config()).unwrap();
    println!("before loading: {:?} MiB resident and peak", memory());

    // Every varied training start, rebuilt once and held, as training would.
    let recorded = read(&worlds_root.join("discover"), "training-starts.json");
    let starts: Vec<Start> = recorded
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["start"].clone()).unwrap())
        .collect();
    let mut worlds: BTreeMap<u64, Vec<&Start>> = BTreeMap::new();
    for start in &starts {
        worlds.entry(start.world.seed).or_default().push(start);
    }
    let worlds: Vec<Vec<&Start>> = worlds.into_values().collect();
    let started = Instant::now();
    let varied: Vec<Source> = pool
        .map(&worlds, |group| {
            let mut found = Vec::new();
            replay(&group[0].world, group, |start, state| {
                found.push((start.case.clone(), Mutex::new(state)));
            });
            found
        })
        .into_iter()
        .flatten()
        .collect();
    println!(
        "varied: {} starts rebuilt in {:.1} s; {:?} MiB",
        varied.len(),
        started.elapsed().as_secs_f64(),
        memory()
    );

    let base = read(&known_root, "base-plan.json");
    let started = Instant::now();
    let (known, _) = split(
        restore(&pool, &read(&known_root, "inputs.json")["sources"]),
        &base,
    );
    println!(
        "known: {} starts rebuilt in {:.1} s; {:?} MiB",
        known.len(),
        started.elapsed().as_secs_f64(),
        memory()
    );
    let cases = retention_cases(&base);

    let parent =
        DenseModel::from_json(&fs::read(worlds_root.join("time-final.json")).unwrap()).unwrap();
    let child =
        SharedModel::new(perception::warm_start(parent.genome(), 4, 0.5, 2).unwrap()).unwrap();
    for (label, policy) in [
        ("dense parent", Policy::Model(parent.clone())),
        ("perception child", Policy::Perception(child)),
    ] {
        let driver = Driver::guidance(policy.clone());
        for (set, sources) in [("varied", &varied), ("known", &known)] {
            let started = Instant::now();
            let episodes = pool.map(sources, |(case, state)| {
                run(&state.lock().unwrap(), case, &driver, None, false).episode
            });
            let arrived = episodes
                .iter()
                .filter(|e| e.outcome == Outcome::Arrived)
                .count();
            let ticks: u64 = episodes.iter().map(|e| e.elapsed_ticks).sum();
            println!(
                "{label}, {set}: {arrived}/{} arrived over {ticks} ticks in {:.1} s",
                episodes.len(),
                started.elapsed().as_secs_f64()
            );
        }
        let started = Instant::now();
        let arrivals = pool.map(&cases, |case| {
            ship_perception::run_episode(case.clone(), policy.clone(), false).unwrap()
        });
        let arrived = arrivals
            .iter()
            .filter(|e| e.episode.outcome == ArrivalOutcome::Arrived)
            .count();
        let ticks: u64 = arrivals
            .iter()
            .map(|e| u64::from(e.episode.elapsed_ticks))
            .sum();
        println!(
            "{label}, precision: {arrived}/{} arrived over {ticks} ticks in {:.1} s",
            arrivals.len(),
            started.elapsed().as_secs_f64()
        );
    }
    println!(
        "workers: {:?}; finally {:?} MiB",
        pool.control().snapshot(),
        memory()
    );

    // How native control and the parent fly the varied starts both complete,
    // with and without a sun.
    let cells: BTreeMap<&str, &str> = starts
        .iter()
        .map(|s| (s.case.name.as_str(), s.cell.as_str()))
        .collect();
    let fly = |driver: &Driver| {
        pool.map(&varied, |(case, state)| {
            run(&state.lock().unwrap(), case, driver, None, false).episode
        })
    };
    let native = fly(&Driver::Native);
    let model = fly(&Driver::guidance(Policy::Model(parent)));
    let speed = |e: &Episode| {
        let f = e.final_flight.as_ref().unwrap();
        (f.ship.velocity - f.planet_motion.velocity).length() as f64
    };
    let start_speed =
        |e: &Episode| (e.initial.ship.velocity - e.initial.planet_motion.velocity).length() as f64;
    for sun in [true, false] {
        let shared: Vec<(&Episode, &Episode)> = native
            .iter()
            .zip(&model)
            .filter(|(n, m)| {
                cells[n.case.as_str()].starts_with("sun") == sun
                    && n.outcome == Outcome::Arrived
                    && m.outcome == Outcome::Arrived
            })
            .collect();
        let count = shared.len() as f64;
        let mean = |f: &dyn Fn(&Episode) -> f64, native: bool| {
            shared
                .iter()
                .map(|(n, m)| f(if native { n } else { m }))
                .sum::<f64>()
                / count
        };
        for (label, is_native) in [("native", true), ("parent", false)] {
            println!(
                "sun {sun}, {label}: {count} shared; ticks {:.1}, start speed {:.1}, \
                 arrival speed {:.1}, thrust ticks {:.1}, brake ticks {:.1}, turn {:.1}",
                mean(&|e| e.elapsed_ticks as f64, is_native),
                mean(&start_speed, is_native),
                mean(&speed, is_native),
                mean(&|e| e.thrust_ticks as f64, is_native),
                mean(&|e| e.brake_ticks as f64, is_native),
                mean(&|e| e.absolute_turn, is_native),
            );
        }
    }
}

/// One tick of a native approach in its destination frame.
struct Sample {
    tick: u64,
    altitude: f32,
    speed: f32,
    /// No route detour or avoidance.
    direct: bool,
    /// Every nine-input value within its encoder's range.
    unsaturated: bool,
    /// Inside the harness's flight checks, including altitude 40-220.
    supported: bool,
}

#[derive(Default)]
struct Track {
    samples: Vec<Sample>,
    capture: Option<u64>,
}

fn quantiles(mut values: Vec<f64>) -> String {
    if values.is_empty() {
        return "none".into();
    }
    values.sort_by(f64::total_cmp);
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    format!("{:.0} / {:.0} / {:.0}", at(0.05), at(0.5), at(0.95))
}

#[test]
#[ignore = "development measurement over milestone 5's census worlds"]
fn native_approach_profile() {
    // Milestone 5's census worlds, four per cell, which are for development only.
    const CENSUS_SEEDS: u64 = 96_100_001;
    const PER_CELL: u64 = 4;
    // Three minutes as in discovery, unless SPACEWARS_PROFILE_SECONDS says otherwise.
    let prefix =
        60 * std::env::var("SPACEWARS_PROFILE_SECONDS").map_or(180, |s| s.parse::<u64>().unwrap());
    let worlds: Vec<World> = Cell::all()
        .into_iter()
        .enumerate()
        .flat_map(|(index, cell)| {
            (0..PER_CELL).map(move |n| cell.world(CENSUS_SEEDS + index as u64 * PER_CELL + n))
        })
        .collect();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let started = Instant::now();
    let runs: Vec<(Vec<Track>, u64, bool)> = pool.map(&worlds, |world| {
        let mut run = start_world(world);
        let mut tracks: BTreeMap<(usize, u64, usize), Track> = BTreeMap::new();
        while run.sortie.state.tick() < prefix && !run.is_game_over() {
            for seat in 0..2 {
                let mission = run.pilots[seat].telemetry();
                let Some(planet) = mission.target else {
                    continue;
                };
                let Some(selected) = mission
                    .events
                    .iter()
                    .rev()
                    .find(|e| e.kind == "selected")
                    .map(|e| e.tick)
                else {
                    continue;
                };
                let key = (seat, selected, planet);
                if mission.goal == MissionGoal::Capture {
                    if let Some(track) = tracks.get_mut(&key)
                        && track.capture.is_none()
                    {
                        track.capture = mission
                            .events
                            .iter()
                            .find(|e| {
                                e.kind == "arrived"
                                    && e.planet == Some(planet)
                                    && e.tick >= selected
                            })
                            .map(|e| e.tick);
                    }
                    continue;
                }
                if mission.goal != MissionGoal::Transfer {
                    continue;
                }
                let Some(f) = run.sortie.state.free_flight_observation(seat) else {
                    continue;
                };
                if f.planet != planet {
                    continue;
                }
                let up = (f.ship.position - f.planet_motion.position).normalized();
                let marker = target(&f, up * (f.planet_radius + 85.0));
                let unsaturated = Features::read(&f, marker)
                    .encode()
                    .is_ok_and(|e| !e.saturated.iter().any(|s| *s));
                tracks.entry(key).or_default().samples.push(Sample {
                    tick: f.tick,
                    altitude: f.ship.position.distance_to(f.planet_motion.position)
                        - f.planet_radius,
                    speed: (f.ship.velocity - f.planet_motion.velocity).length(),
                    direct: mission.avoidance.is_none(),
                    unsaturated,
                    supported: failure(&f, &f).is_none(),
                });
            }
            run.step(&[], STEP);
        }
        let tracks = tracks.into_values().collect::<Vec<_>>();
        (tracks, run.sortie.state.tick(), run.is_game_over())
    });
    // Captures per minute of match time, and the matches still running then.
    let minutes = (prefix / 3600) as usize;
    let mut captures = vec![0; minutes];
    let mut running = vec![0; minutes];
    for (tracks, end, _) in &runs {
        for minute in 0..minutes {
            running[minute] += u64::from(*end > minute as u64 * 3600);
        }
        for capture in tracks.iter().filter_map(|t| t.capture) {
            captures[(capture / 3600).min(minutes as u64 - 1) as usize] += 1;
        }
    }
    println!(
        "matches ended early: {}; captures per minute {captures:?}; matches running {running:?}",
        runs.iter().filter(|(_, _, finished)| *finished).count()
    );
    let tracks: Vec<Track> = runs.into_iter().flat_map(|(tracks, _, _)| tracks).collect();
    let arrived: Vec<&Track> = tracks
        .iter()
        .filter(|t| t.capture.is_some() && !t.samples.is_empty())
        .collect();
    println!(
        "{} worlds in {:.1} s: {} approaches in a destination frame, {} reaching capture",
        worlds.len(),
        started.elapsed().as_secs_f64(),
        tracks.iter().filter(|t| !t.samples.is_empty()).count(),
        arrived.len()
    );
    let entry: Vec<&Sample> = arrived.iter().map(|t| &t.samples[0]).collect();
    println!(
        "entering the destination frame: altitude {}, speed {}",
        quantiles(entry.iter().map(|s| f64::from(s.altitude)).collect()),
        quantiles(entry.iter().map(|s| f64::from(s.speed)).collect())
    );
    // Altitude 85 is the entry point, so each threshold lies that far above it.
    for (label, limit) in [
        ("frame entry", f32::INFINITY),
        ("135 above the entry", 220.0),
        ("100 above", 185.0),
        ("70 above", 155.0),
        ("35 above", 120.0),
    ] {
        let mut window = Vec::new();
        let mut speed = Vec::new();
        let (mut clean, mut reached) = (0, 0);
        for track in &arrived {
            let capture = track.capture.unwrap();
            let Some(index) = track.samples.iter().position(|s| s.altitude <= limit) else {
                continue;
            };
            reached += 1;
            let start = &track.samples[index];
            window.push((capture - start.tick) as f64);
            speed.push(f64::from(start.speed));
            if track.samples[index..]
                .iter()
                .all(|s| s.direct && s.unsaturated && s.supported)
            {
                clean += 1;
            }
        }
        println!(
            "from {label}: {reached} approaches; ticks to capture {}; speed {}; \
             {clean} direct, unsaturated and supported to capture",
            quantiles(window),
            quantiles(speed)
        );
    }
}

/// Copied from `guidance_worlds`: the case for a seat's current state, if
/// every input is valid and unsaturated.
fn pilot_case(
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

/// Copied from `guidance_worlds::eligible`, with the distance from the entry
/// as a parameter.
fn pilot_eligible(
    run: &MaterialMissionClientScenario,
    case: &Case,
    distance: f32,
) -> Option<MaterialMissionClientScenario> {
    if !direct_transfer(&probe(run, case.seat), case) {
        return None;
    }
    let before = flight(run, case.seat);
    let mut neutral = fork(run);
    neutral.bots[case.seat] = false;
    neutral.step(&Movement::default().encode(before.owner), STEP);
    neutral.bots[case.seat] = true;
    let after = flight(&neutral, case.seat);
    let after_case = pilot_case(&neutral, case.name.clone(), case.shape, case.seat);
    if failure(&after, &before).is_some()
        || after_case.is_none_or(|c| c.source_entry_distance > distance)
        || !direct_transfer(&probe(&neutral, case.seat), case)
    {
        return None;
    }
    Some(neutral)
}

/// A pilot start: one approach handed over at one distance from the entry.
struct PilotStart {
    cell: String,
    sun: bool,
    /// World seed, seat and the tick native control selected the destination.
    approach: (u64, usize, u64),
    distance: u32,
    case: Case,
    state: Mutex<MaterialMissionClientScenario>,
}

#[test]
#[ignore = "development pilot over milestone 5's census worlds; set SPACEWARS_WORLDS_ROOT and SPACEWARS_PILOT_OUT"]
fn takeover_distance_pilot() {
    // Milestone 5's census worlds, four per cell, which are for development only.
    const CENSUS_SEEDS: u64 = 96_100_001;
    const PER_CELL: u64 = 4;
    const PREFIX: u64 = 180 * 60;
    // Each approach's first eligible state within each distance of the entry.
    const DISTANCES: [u32; 2] = [100, 35];
    let root = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_ROOT").unwrap());
    let out = PathBuf::from(std::env::var_os("SPACEWARS_PILOT_OUT").unwrap());
    let worlds: Vec<(Cell, World)> = Cell::all()
        .into_iter()
        .enumerate()
        .flat_map(|(index, cell)| {
            (0..PER_CELL)
                .map(move |n| (cell, cell.world(CENSUS_SEEDS + index as u64 * PER_CELL + n)))
        })
        .collect();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let started = Instant::now();
    let starts: Vec<PilotStart> = pool
        .map(&worlds, |(cell, world)| {
            let mut run = start_world(world);
            let mut current: [Option<(u64, usize)>; 2] = [None, None];
            let mut taken = [[false; DISTANCES.len()]; 2];
            let mut found = Vec::new();
            while run.sortie.state.tick() < PREFIX && !run.is_game_over() {
                for seat in 0..2 {
                    let mission = run.pilots[seat].telemetry();
                    let key = mission
                        .events
                        .iter()
                        .rev()
                        .find(|e| e.kind == "selected")
                        .map(|e| e.tick)
                        .zip(mission.target);
                    if current[seat] != key {
                        current[seat] = key;
                        taken[seat] = [false; DISTANCES.len()];
                    }
                    let Some((selected, planet)) = key else {
                        continue;
                    };
                    if mission.goal != MissionGoal::Transfer {
                        continue;
                    }
                    for (index, &distance) in DISTANCES.iter().enumerate() {
                        if taken[seat][index] {
                            continue;
                        }
                        let name = format!(
                            "{}-{}-p{}-s{selected}-d{distance}",
                            cell.name(),
                            world.seed,
                            seat + 1
                        );
                        let Some(case) = pilot_case(&run, name, world.shape, seat).filter(|c| {
                            c.planet == planet && c.source_entry_distance <= distance as f32
                        }) else {
                            continue;
                        };
                        if let Some(state) = pilot_eligible(&run, &case, distance as f32) {
                            taken[seat][index] = true;
                            found.push(PilotStart {
                                cell: cell.name(),
                                sun: cell.sun,
                                approach: (world.seed, seat, selected),
                                distance,
                                case,
                                state: Mutex::new(state),
                            });
                        }
                    }
                }
                run.step(&[], STEP);
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
    println!(
        "{} starts from {} worlds in {:.1} s",
        starts.len(),
        worlds.len(),
        started.elapsed().as_secs_f64()
    );
    let models: Vec<(&str, DenseModel)> = [
        ("e0f25b68", "candidate.json"),
        ("e03d3e29", "time-final.json"),
    ]
    .into_iter()
    .map(|(name, file)| {
        let bytes = fs::read(root.join(file)).unwrap();
        (name, DenseModel::from_json(&bytes).unwrap())
    })
    .collect();
    let drivers: Vec<(&str, Driver)> = std::iter::once(("native", Driver::Native))
        .chain(
            models
                .iter()
                .map(|(name, model)| (*name, Driver::guidance(Policy::Model(model.clone())))),
        )
        .collect();
    let started = Instant::now();
    let rows: Vec<Value> = pool.map(&starts, |start| {
        let state = start.state.lock().unwrap();
        let outcomes: serde_json::Map<String, Value> = drivers
            .iter()
            .map(|(label, driver)| {
                let e = run(&state, &start.case, driver, None, false).episode;
                let speed = e
                    .final_flight
                    .as_ref()
                    .map(|f| (f.ship.velocity - f.planet_motion.velocity).length());
                let host = e.host_reason.as_ref().map(
                    |r| json!({"goal":r["goal"],"avoidance":r["avoidance"],"reason":r["reason"]}),
                );
                (
                    label.to_string(),
                    json!({"outcome":e.outcome,"ticks":e.elapsed_ticks,"speed":speed,
                        "thrust":e.thrust_ticks,"brake":e.brake_ticks,"host":host}),
                )
            })
            .collect();
        json!({"cell":start.cell,"sun":start.sun,"seed":start.approach.0,"seat":start.approach.1,
            "selected":start.approach.2,"distance":start.distance,"case":start.case,
            "outcomes":outcomes})
    });
    println!(
        "{} starts flown by {} controllers in {:.1} s",
        rows.len(),
        drivers.len(),
        started.elapsed().as_secs_f64()
    );
    save(out.join("pilot.json"), &rows);
}
