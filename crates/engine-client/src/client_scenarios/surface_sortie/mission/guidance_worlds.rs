//! Milestone 5 of the match hybrid plan: lab worlds with and without a sun and
//! with two to eight planets, and native approach starts in them. See
//! docs/learned-guidance-varied-worlds.md.
use super::guidance_match::*;
use super::*;
use engine_agent::training_workers::EpisodePool;
use engine_common::{DirtMode, SpacewarsController};
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    free_flight::{CelestialBody, CelestialKind},
    mission::{ArenaLayout, MissionObservationV1},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spacewars_ai::{
    guidance::{Features, Movement, Policy, perception::Perception},
    mission_pilot::MissionGoal,
    model::DenseModel,
};
use std::{
    collections::BTreeMap,
    fs,
    ops::RangeInclusive,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

/// Planet counts of the cells, each with and without a sun.
const PLANET_COUNTS: [usize; 4] = [2, 3, 5, 8];
/// The edge cell's margins, one drawn per world.
const EDGE_MARGINS: RangeInclusive<u32> = 120..=320;
/// Native control's entry point lies this far above the destination, and its
/// route check keeps this far beyond every other body's radius.
const ENTRY_ALTITUDE: f32 = 85.0;
const ROUTE_BUFFER: f32 = 65.0;
/// Eligible starts lie this close to native control's entry point.
const ENTRY_DISTANCE: f32 = 35.0;
/// An approach also contributes its eligible state closest to a route buffer,
/// below this margin, among states at least `CAPTURE_CLEARANCE` outside native
/// control's capture distance. Every approach nears the climb boundary as it
/// descends to the entry, so climb margins are recorded but not used here.
const LOW_MARGIN: f32 = 30.0;
const CAPTURE_DISTANCE: f32 = 105.0;
const CAPTURE_CLEARANCE: f32 = 10.0;
const PREFIX_TICKS: u64 = 180 * 60;
const CENSUS_SEEDS: u64 = 96_100_001;
const CENSUS_WORLDS: u64 = 4;

/// One group of layouts: a sun or none, a planet count, and whether the edge
/// varies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Cell {
    pub(super) sun: bool,
    pub(super) planets: usize,
    pub(super) varied_edge: bool,
}

impl Cell {
    /// Sun or none times each planet count, then the edge cell.
    pub(super) fn all() -> Vec<Self> {
        let mut cells: Vec<_> = [true, false]
            .into_iter()
            .flat_map(|sun| {
                PLANET_COUNTS.map(|planets| Self {
                    sun,
                    planets,
                    varied_edge: false,
                })
            })
            .collect();
        cells.push(Self {
            sun: true,
            planets: 3,
            varied_edge: true,
        });
        cells
    }

    /// Today's match arena.
    pub(super) fn is_standard(&self) -> bool {
        self.sun && self.planets == 3 && !self.varied_edge
    }

    pub(super) fn name(&self) -> String {
        format!(
            "{}{}{}",
            if self.sun { "sun" } else { "nosun" },
            self.planets,
            if self.varied_edge { "-edge" } else { "" }
        )
    }

    /// The seed fixes the world, including the edge cell's margin.
    pub(super) fn world(&self, seed: u64) -> World {
        let edge_margin = if self.varied_edge {
            let digest = Sha256::new()
                .chain_update(b"spacewars.varied_worlds.edge.v1\0")
                .chain_update(seed.to_le_bytes())
                .finalize();
            let draw = u32::from_le_bytes(digest[..4].try_into().expect("four bytes"));
            EDGE_MARGINS.start() + draw % (EDGE_MARGINS.end() - EDGE_MARGINS.start() + 1)
        } else {
            ArenaLayout::STANDARD.edge_margin
        };
        World {
            seed,
            layout: ArenaLayout {
                sun: self.sun,
                planets: self.planets,
                edge_margin,
            },
            // The two shapes are near-duplicates here, so each world uses one.
            shape: if seed % 2 == 0 {
                DirtMode::WeakAngular
            } else {
                DirtMode::WeakRound
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct World {
    pub(super) seed: u64,
    pub(super) layout: ArenaLayout,
    pub(super) shape: DirtMode,
}

/// The match of `guidance_match::start_match` in a lab layout.
pub(super) fn start_world(world: &World) -> MaterialMissionClientScenario {
    let mut settings = Settings::default();
    settings.spacewars.player_1_controller = SpacewarsController::RecoveryBot;
    settings.spacewars.player_2_controller = SpacewarsController::RecoveryBot;
    settings.spacewars_match.time_limit_seconds = 600;
    settings.material_combat.loose_dirt = true;
    settings.material_combat.dirt_mode = world.shape;
    settings.material_combat.asteroids.interval_seconds = 0;
    create_layout_match(world.seed, &settings, world.layout)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    /// An approach's first eligible state.
    First,
    /// Its eligible state closest to a route buffer.
    LowMargin,
}

/// Clearances from native control's rules, as it measures them. Route margins
/// are the line from the ship to the current-bearing entry point, less each
/// body's radius and the route buffer. The climb margin is the altitude above
/// native control's climb boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(super) struct Margins {
    pub(super) sun: Option<f32>,
    /// The nearest other planet's.
    pub(super) planet: Option<f32>,
    pub(super) climb: f32,
    /// From the ship to the world edge.
    pub(super) edge: f32,
    /// Beyond native control's capture distance from the destination's center.
    pub(super) capture: f32,
}

impl Margins {
    pub(super) fn read(o: &MissionObservationV1, destination: usize) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        let ship = p.ship.position;
        let target = o
            .planets
            .iter()
            .find(|planet| planet.index == destination)
            .expect("destination observed");
        let up = (ship - target.motion.position).normalized();
        let entry = target.motion.position + up * (target.radius + ENTRY_ALTITUDE);
        let route = |center: Vec2, radius: f32| {
            center.distance_to(Vec2::closest_point_on_segment(center, ship, entry))
                - radius
                - ROUTE_BUFFER
        };
        let frame_up = (ship - p.planet.motion.position).normalized();
        let falling = (-(p.ship.velocity - p.planet.motion.velocity).dot(frame_up)).max(0.0);
        let altitude = ship.distance_to(p.planet.motion.position) - p.planet.radius;
        Self {
            sun: o.sun.map(|sun| route(sun.position, sun.radius)),
            planet: o
                .planets
                .iter()
                .filter(|planet| planet.index != destination)
                .map(|planet| route(planet.motion.position, planet.radius))
                .reduce(f32::min),
            climb: altitude - (70.0 + falling * falling / 50.0),
            edge: o.boundary.radius - ship.distance_to(o.boundary.center),
            capture: ship.distance_to(target.motion.position) - target.radius - CAPTURE_DISTANCE,
        }
    }

    /// The nearer of the sun's and the other planets' route margins.
    pub(super) fn route(&self) -> Option<f32> {
        [self.sun, self.planet]
            .into_iter()
            .flatten()
            .reduce(f32::min)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Start {
    pub(super) case: Case,
    pub(super) world: World,
    pub(super) cell: String,
    pub(super) kind: Kind,
    /// The tick native control selected this approach's destination.
    pub(super) selected_tick: u64,
    pub(super) margins: Margins,
}

pub(super) type WorldSource = (Start, Mutex<MaterialMissionClientScenario>);

/// Copied from `time_ranking`: the case for a seat's current state, if every
/// input is valid and unsaturated.
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
    let offset = (f.ship.position - f.planet_motion.position).normalized()
        * (f.planet_radius + ENTRY_ALTITUDE);
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

/// The state after the neutral tick every arm receives, if the start is still
/// a direct transfer within reach of the entry, as in `time_ranking`.
fn eligible(
    run: &MaterialMissionClientScenario,
    case: &Case,
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
    let after_case = source_case(&neutral, case.name.clone(), case.shape, case.seat);
    if failure(&after, &before).is_some()
        || after_case.is_none_or(|c| c.source_entry_distance > ENTRY_DISTANCE)
        || !direct_transfer(&probe(&neutral, case.seat), case)
    {
        return None;
    }
    assert_eq!(after.applied, [0.0; 3]);
    Some(neutral)
}

/// One native approach: a destination since its selection.
struct Approach {
    selected_tick: u64,
    planet: usize,
    transfer_ticks: u64,
    near_ticks: u64,
    first: Option<(Start, MaterialMissionClientScenario)>,
    low: Option<(Start, MaterialMissionClientScenario)>,
    /// The smallest route margin among its starts so far.
    route: f32,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct WorldCounts {
    /// Native approaches with a transfer, and those that came within reach of
    /// the entry in the destination frame.
    pub(super) approaches: u64,
    pub(super) near_approaches: u64,
    pub(super) first_starts: u64,
    pub(super) low_margin_starts: u64,
    pub(super) arrivals: [u64; 2],
    pub(super) ticks: u64,
    pub(super) round_finished: bool,
    pub(super) round_end: Option<String>,
    pub(super) seconds: f64,
}

/// Every approach's first eligible state and, below `LOW_MARGIN`, its
/// eligible state closest to a native rule, over a native match prefix.
pub(super) fn discover(world: &World, cell: &Cell, prefix: u64) -> (Vec<WorldSource>, WorldCounts) {
    let started = Instant::now();
    let mut run = start_world(world);
    let mut counts = WorldCounts::default();
    let mut current: [Option<Approach>; 2] = [None, None];
    let mut found = Vec::new();
    let mut finish = |approach: Approach, counts: &mut WorldCounts| {
        counts.approaches += u64::from(approach.transfer_ticks > 0);
        counts.near_approaches += u64::from(approach.near_ticks > 0);
        counts.first_starts += u64::from(approach.first.is_some());
        counts.low_margin_starts += u64::from(approach.low.is_some());
        for (start, state) in approach.first.into_iter().chain(approach.low) {
            found.push((start, Mutex::new(state)));
        }
    };
    while run.sortie.state.tick() < prefix && !run.is_game_over() {
        for (seat, slot) in current.iter_mut().enumerate() {
            let mission = run.pilots[seat].telemetry();
            let selected = mission
                .events
                .iter()
                .rev()
                .find(|e| e.kind == "selected")
                .map(|e| e.tick);
            let key = selected.zip(mission.target);
            if slot.as_ref().map(|a| (a.selected_tick, a.planet)) != key {
                if let Some(done) = slot.take() {
                    finish(done, &mut counts);
                }
                *slot = key.map(|(selected_tick, planet)| Approach {
                    selected_tick,
                    planet,
                    transfer_ticks: 0,
                    near_ticks: 0,
                    first: None,
                    low: None,
                    route: LOW_MARGIN,
                });
            }
            let Some(approach) = slot.as_mut() else {
                continue;
            };
            if mission.goal != MissionGoal::Transfer {
                continue;
            }
            approach.transfer_ticks += 1;
            let name = |kind: Kind| {
                format!(
                    "{}-{}-p{}-s{}-{}",
                    cell.name(),
                    world.seed,
                    seat + 1,
                    approach.selected_tick,
                    match kind {
                        Kind::First => "first",
                        Kind::LowMargin => "low",
                    }
                )
            };
            let Some(case) = source_case(&run, name(Kind::First), world.shape, seat)
                .filter(|c| c.planet == approach.planet)
                .filter(|c| c.source_entry_distance <= ENTRY_DISTANCE)
            else {
                continue;
            };
            approach.near_ticks += 1;
            let margins = Margins::read(
                &run.sortie.state.mission_observation(seat, None),
                case.planet,
            );
            let start = |case: Case, kind| Start {
                case,
                world: *world,
                cell: cell.name(),
                kind,
                selected_tick: approach.selected_tick,
                margins,
            };
            if approach.first.is_none() {
                if let Some(state) = eligible(&run, &case) {
                    approach.route = margins
                        .route()
                        .map_or(approach.route, |r| r.min(approach.route));
                    approach.first = Some((start(case, Kind::First), state));
                }
                continue;
            }
            if margins.capture >= CAPTURE_CLEARANCE
                && margins.route().is_some_and(|r| r < approach.route)
            {
                let case = Case {
                    name: name(Kind::LowMargin),
                    ..case
                };
                if let Some(state) = eligible(&run, &case) {
                    approach.route = margins.route().expect("a route margin");
                    approach.low = Some((start(case, Kind::LowMargin), state));
                }
            }
        }
        run.step(&[], STEP);
    }
    for done in current.into_iter().flatten() {
        finish(done, &mut counts);
    }
    for (seat, arrivals) in counts.arrivals.iter_mut().enumerate() {
        *arrivals = run.pilots[seat]
            .telemetry()
            .events
            .iter()
            .filter(|e| e.kind == "arrived")
            .count() as u64;
    }
    counts.ticks = run.sortie.state.tick();
    counts.round_finished = run.is_game_over();
    counts.round_end = run
        .sortie
        .state
        .match_observation()
        .and_then(|m| m.reason)
        .map(|reason| format!("{reason:?}"));
    counts.seconds = started.elapsed().as_secs_f64();
    (found, counts)
}

/// Rebuild one world's starts from its seed, with the checks of
/// `guidance_match::prepare`. One replay serves every start, in tick order.
pub(super) fn replay(
    world: &World,
    starts: &[&Start],
    mut visit: impl FnMut(&Start, MaterialMissionClientScenario),
) {
    let mut run = start_world(world);
    let mut ordered = starts.to_vec();
    ordered.sort_by_key(|start| start.case.source_tick);
    for start in ordered {
        let case = &start.case;
        assert_eq!(start.world, *world, "{}", case.name);
        while run.sortie.state.tick() < case.source_tick {
            assert!(!run.is_game_over(), "source unavailable: {}", case.name);
            run.step(&[], STEP);
        }
        let before = flight(&run, case.seat);
        assert_eq!(before.planet, case.planet);
        assert_eq!(failure(&before, &before), None);
        let distance = (before
            .ship
            .position
            .distance_to(before.planet_motion.position)
            - before.planet_radius
            - ENTRY_ALTITUDE)
            .abs();
        let speed = (before.ship.velocity - before.planet_motion.velocity).length();
        assert!((distance - case.source_entry_distance).abs() < 0.001);
        assert!((speed - case.source_relative_speed).abs() < 0.001);
        let state = eligible(&run, case)
            .unwrap_or_else(|| panic!("start no longer eligible: {}", case.name));
        visit(start, state);
    }
}

pub(super) fn prepare_start(start: &Start) -> MaterialMissionClientScenario {
    let mut prepared = None;
    replay(&start.world, &[start], |_, state| prepared = Some(state));
    prepared.unwrap()
}

/// A parent model and the file it came from.
fn parent(root: &Path, file: &str, sha256: &str) -> DenseModel {
    let bytes = fs::read(root.join(file)).unwrap();
    assert_eq!(hash(&bytes), sha256, "{file}");
    DenseModel::from_json(&bytes).unwrap()
}

/// The bodies the sensor would read at a start, and how many it leaves out.
fn sensed(source: &MaterialMissionClientScenario, case: &Case) -> Value {
    let f = flight(source, case.seat);
    let bodies = source.sortie.state.celestial_bodies();
    let perception = Perception::read(&f, &bodies, Some(case.planet));
    let encoded = perception.encode().unwrap();
    assert!(
        encoded
            .inputs
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1.0),
        "perception bounds: {}",
        case.name
    );
    // Records carry no body index, so match the bodies left out by clearance.
    let clearance = |body: &CelestialBody| {
        let distance = f.ship.position.distance_to(body.position);
        let gap = if body.kind == CelestialKind::Edge {
            body.radius - distance
        } else {
            distance - body.radius
        };
        gap.max(0.0)
    };
    let omitted: Vec<_> = bodies
        .iter()
        .filter(|body| {
            !perception
                .bodies
                .iter()
                .any(|r| (r.clearance - clearance(body)).abs() < 1e-3)
        })
        .map(|body| json!({"kind":format!("{:?}", body.kind),"clearance":clearance(body)}))
        .collect();
    assert_eq!(omitted.len(), perception.omitted, "{}", case.name);
    let kept: Vec<_> = perception.bodies.iter().map(|b| b.clearance).collect();
    json!({"bodies":bodies.len(),"omitted":omitted,"kept_clearances":kept})
}

#[test]
#[ignore = "development census; requires SPACEWARS_WORLDS_CENSUS_OUT and SPACEWARS_WORLDS_MODELS"]
fn learned_guidance_varied_worlds_census() {
    let out = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_CENSUS_OUT").unwrap());
    fs::create_dir(&out).expect("a fresh census directory");
    let models = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_MODELS").unwrap());
    let parents = [
        (
            "e0f25b68",
            parent(
                &models,
                "candidate.json",
                "5088139d6c09f0787bd0785865eb1f76dd3fdd3eebe869adf4a0f87ce5ab8097",
            ),
        ),
        (
            "e03d3e29",
            parent(
                &models,
                "time-final.json",
                "7951cc9a0505d06a0b69e8620476a589fa87e493a5290194c1fac4553e6422e4",
            ),
        ),
    ];
    // Smaller runs for trying the census out; the recorded census uses neither.
    let setting = |name: &str, default: u64| {
        std::env::var(name).map_or(default, |s| s.parse().expect("a whole number"))
    };
    let prefix = setting("SPACEWARS_WORLDS_CENSUS_PREFIX", PREFIX_TICKS);
    let per_cell = setting("SPACEWARS_WORLDS_CENSUS_WORLDS", CENSUS_WORLDS);
    let pool = EpisodePool::new(worker_config()).unwrap();
    let worlds: Vec<(Cell, World)> = Cell::all()
        .into_iter()
        .enumerate()
        .flat_map(|(index, cell)| {
            (0..per_cell).map(move |n| {
                (
                    cell,
                    cell.world(CENSUS_SEEDS + index as u64 * CENSUS_WORLDS + n),
                )
            })
        })
        .collect();
    let started = Instant::now();
    // A panic in an untried layout is a finding, not the end of the census.
    let discovered = pool.map(&worlds, |(cell, world)| {
        panic::catch_unwind(AssertUnwindSafe(|| discover(world, cell, prefix))).map_err(|e| {
            e.downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default()
        })
    });
    let discovery_seconds = started.elapsed().as_secs_f64();
    let mut sources = Vec::new();
    let mut world_rows = Vec::new();
    for ((cell, world), result) in worlds.iter().zip(discovered) {
        match result {
            Ok((found, counts)) => {
                world_rows.push(json!({"cell":cell.name(),"world":world,"counts":counts}));
                sources.extend(found);
            }
            Err(message) => {
                world_rows.push(json!({"cell":cell.name(),"world":world,"panic":message}));
            }
        }
    }
    // Starts must rebuild from their seeds. Each world's latest covers the
    // most native play.
    let mut latest = BTreeMap::new();
    for (s, (start, _)) in sources.iter().enumerate() {
        let tick = start.case.source_tick;
        let slot = latest.entry(start.world.seed).or_insert(s);
        if sources[*slot].0.case.source_tick < tick {
            *slot = s;
        }
    }
    let latest: Vec<usize> = latest.into_values().collect();
    let started = Instant::now();
    let rebuilt = pool.map(&latest, |&s| {
        let (start, state) = &sources[s];
        initial_record(&prepare_start(start), &start.case)
            == initial_record(&state.lock().unwrap(), &start.case)
    });
    let rebuild_seconds = started.elapsed().as_secs_f64();
    assert!(rebuilt.iter().all(|same| *same), "a start did not rebuild");
    let drivers: Vec<(String, Driver)> = std::iter::once(("native".to_string(), Driver::Native))
        .chain(parents.iter().map(|(name, model)| {
            (
                name.to_string(),
                Driver::guidance(Policy::Model(model.clone())),
            )
        }))
        .collect();
    let jobs: Vec<(usize, usize)> = (0..sources.len())
        .flat_map(|s| (0..drivers.len()).map(move |d| (s, d)))
        .collect();
    let started = Instant::now();
    let episodes = pool.map(&jobs, |&(s, d)| {
        let (start, state) = &sources[s];
        let source = fork(&state.lock().unwrap());
        let episode = run(&source, &start.case, &drivers[d].1, None, false).episode;
        json!({"outcome":episode.outcome,"ticks":episode.elapsed_ticks,
            "host_reason":episode.host_reason.as_ref().map(|r| json!({
                "goal":r["goal"],"avoidance":r["avoidance"],"reason":r["reason"]}))})
    });
    let episode_seconds = started.elapsed().as_secs_f64();
    let mut starts = Vec::new();
    let mut cells: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for (s, (start, state)) in sources.iter().enumerate() {
        let source = state.lock().unwrap();
        let mut outcomes = serde_json::Map::new();
        for (d, (name, _)) in drivers.iter().enumerate() {
            outcomes.insert(name.clone(), episodes[s * drivers.len() + d].clone());
        }
        let row = cells.entry(start.cell.clone()).or_default();
        for (name, outcome) in &outcomes {
            let arrived = outcome["outcome"] == "arrived";
            let entry = row
                .entry(name.clone())
                .or_insert_with(|| json!({"starts":0,"arrived":0,"arrival_ticks":0}));
            entry["starts"] = json!(entry["starts"].as_u64().unwrap() + 1);
            entry["arrived"] = json!(entry["arrived"].as_u64().unwrap() + u64::from(arrived));
            if arrived {
                entry["arrival_ticks"] = json!(
                    entry["arrival_ticks"].as_u64().unwrap() + outcome["ticks"].as_u64().unwrap()
                );
            }
        }
        starts.push(json!({
            "start":start,"record":initial_record(&source, &start.case),
            "sensed":sensed(&source, &start.case),"outcomes":outcomes
        }));
    }
    let census = json!({
        "cells":Cell::all().iter().map(|c| json!({"name":c.name(),"cell":c})).collect::<Vec<_>>(),
        "settings":{"prefix_ticks":prefix,"entry_distance":ENTRY_DISTANCE,
            "low_margin":LOW_MARGIN,"edge_margins":[EDGE_MARGINS.start(),EDGE_MARGINS.end()],
            "first_seed":CENSUS_SEEDS,"worlds_per_cell":per_cell},
        "workers":format!("{:?}", worker_config()),
        "discovery_seconds":discovery_seconds,"episode_seconds":episode_seconds,
        "rebuilt_starts":latest.len(),"rebuild_seconds":rebuild_seconds,
        "worlds":world_rows,"by_cell":cells,"starts":starts
    });
    atomic(&out, "census.json", &census);
    println!(
        "census: {} worlds, {} starts, discovery {discovery_seconds:.1} s, episodes {episode_seconds:.1} s",
        worlds.len(),
        sources.len()
    );
}

/// One set of the frozen plan. Each cell takes consecutive seeds from its own
/// range until it has its starts, keeping whole worlds.
#[derive(Debug, Deserialize)]
struct SetPlan {
    first_seed: u64,
    cell_stride: u64,
    starts_per_cell: usize,
    standard_starts: usize,
}

/// Worlds run in batches of this many. Results are kept in seed order, so the
/// sets do not depend on it.
const BATCH: u64 = 16;

/// A start, its rebuilt record and what the sensor reads there.
fn start_record(start: &Start, state: &MaterialMissionClientScenario) -> Value {
    json!({"start":start,"record":initial_record(state, &start.case),
        "sensed":sensed(state, &start.case)})
}

/// The compiled rules must be the frozen plan's.
fn check_plan(plan: &Value) {
    let cells: Vec<_> = Cell::all()
        .iter()
        .map(|c| json!({"name":c.name(),"sun":c.sun,"planets":c.planets,"varied_edge":c.varied_edge}))
        .collect();
    assert_eq!(plan["cells"], json!(cells));
    assert_eq!(
        plan["layout"],
        json!({"standard_edge_margin":ArenaLayout::STANDARD.edge_margin,
            "edge_margins":[EDGE_MARGINS.start(),EDGE_MARGINS.end()],
            "edge_draw":"spacewars.varied_worlds.edge.v1"})
    );
    let rule = &plan["discovery"];
    assert_eq!(rule["prefix_ticks"], PREFIX_TICKS);
    assert_eq!(rule["batch"], BATCH);
    for (name, value) in [
        ("entry_altitude", ENTRY_ALTITUDE),
        ("route_buffer", ROUTE_BUFFER),
        ("entry_distance", ENTRY_DISTANCE),
        ("low_margin", LOW_MARGIN),
        ("capture_distance", CAPTURE_DISTANCE),
        ("capture_clearance", CAPTURE_CLEARANCE),
    ] {
        assert_eq!(rule[name].as_f64(), Some(f64::from(value)), "{name}");
    }
}

fn discover_cell(
    pool: &EpisodePool,
    index: usize,
    cell: &Cell,
    set: &SetPlan,
) -> (Vec<Value>, Vec<Value>) {
    let target = if cell.is_standard() {
        set.standard_starts
    } else {
        set.starts_per_cell
    };
    let mut starts = Vec::new();
    let mut worlds = Vec::new();
    let mut next = 0;
    while starts.len() < target {
        assert!(
            next + BATCH <= set.cell_stride,
            "{} ran out of seeds",
            cell.name()
        );
        let seeds: Vec<u64> = (next..next + BATCH)
            .map(|n| set.first_seed + index as u64 * set.cell_stride + n)
            .collect();
        let found = pool.map(&seeds, |&seed| {
            let world = cell.world(seed);
            let (sources, counts) = discover(&world, cell, PREFIX_TICKS);
            let records: Vec<_> = sources
                .into_iter()
                .map(|(start, state)| start_record(&start, &state.into_inner().unwrap()))
                .collect();
            (world, records, counts)
        });
        for (world, records, counts) in found {
            if starts.len() >= target {
                break;
            }
            worlds.push(json!({"cell":cell.name(),"world":world,"counts":counts,
                "starts":records.len()}));
            starts.extend(records);
        }
        next += BATCH;
    }
    (starts, worlds)
}

fn count(starts: &[Value], kind: &str) -> usize {
    starts.iter().filter(|s| s["start"]["kind"] == kind).count()
}

fn discover_stage(root: &Path, plan: &Value) {
    let out = root.join("discover");
    fs::create_dir(&out).unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let started = Instant::now();
    // The arena builder now serves every layout. It must still rebuild the
    // lineage's known starts, and the standard layout must start its match.
    let known = read(root, "inputs.json");
    let restored = restore(&pool, &known["sources"]).len();
    assert_eq!(
        Some(restored as u64),
        plan["preflight"]["known_match_sources"].as_u64()
    );
    let training: SetPlan = serde_json::from_value(plan["sets"]["training"].clone()).unwrap();
    let standard = Cell::all().iter().position(Cell::is_standard).unwrap();
    let checks = plan["preflight"]["standard_check_seeds"].as_u64().unwrap();
    let ticks = plan["preflight"]["standard_check_ticks"].as_u64().unwrap();
    let seeds: Vec<u64> = (0..checks)
        .map(|n| training.first_seed + standard as u64 * training.cell_stride + n)
        .collect();
    let same = pool.map(&seeds, |&seed| {
        let world = Cell::all()[standard].world(seed);
        let mut lab = start_world(&world);
        let mut lineage = start_match(seed, world.shape);
        let case = Case {
            seed,
            shape: world.shape,
            ..probe_case()
        };
        (0..=ticks).all(|tick| {
            let same =
                tick % 100 != 0 || initial_record(&lab, &case) == initial_record(&lineage, &case);
            lab.step(&[], STEP);
            lineage.step(&[], STEP);
            same
        })
    });
    assert!(
        same.iter().all(|s| *s),
        "the standard layout left the lineage's match"
    );
    let preflight_seconds = started.elapsed().as_secs_f64();
    println!(
        "preflight: {restored} known starts rebuilt and {checks} standard worlds matched in {preflight_seconds:.1} s"
    );
    let mut sets = serde_json::Map::new();
    for name in ["training", "holdout"] {
        let set: SetPlan = serde_json::from_value(plan["sets"][name].clone()).unwrap();
        let started = Instant::now();
        let mut starts = Vec::new();
        let mut worlds = Vec::new();
        let mut cells = serde_json::Map::new();
        for (index, cell) in Cell::all().iter().enumerate() {
            let (found, used) = discover_cell(&pool, index, cell, &set);
            cells.insert(
                cell.name(),
                json!({"starts":found.len(),"worlds":used.len(),"first":count(&found, "first"),
                    "low_margin":count(&found, "low_margin")}),
            );
            starts.extend(found);
            worlds.extend(used);
        }
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "{name}: {} starts from {} worlds in {seconds:.1} s",
            starts.len(),
            worlds.len()
        );
        atomic(&out, &format!("{name}-starts.json"), &starts);
        atomic(&out, &format!("{name}-worlds.json"), &worlds);
        sets.insert(
            name.into(),
            json!({"starts":starts.len(),"worlds":worlds.len(),"cells":cells,"seconds":seconds}),
        );
    }
    atomic(
        &out,
        "summary.json",
        &json!({"plan_sha256":hash(&fs::read(root.join("plan.json")).unwrap()),
            "known_starts_rebuilt":restored,"standard_worlds_matched":checks,
            "preflight_seconds":preflight_seconds,"sets":sets}),
    );
}

/// Completions and time against native control, for every cell and overall.
/// A failed start counts as the 300-tick attempt bound.
fn summarize(episodes: &[Value], drivers: &[String]) -> Value {
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for episode in episodes {
        groups
            .entry(episode["cell"].as_str().unwrap().into())
            .or_default()
            .push(episode);
        groups.entry("all".into()).or_default().push(episode);
    }
    let arrived = |e: &Value, d: &str| e["outcomes"][d]["outcome"] == "arrived";
    let ticks = |e: &Value, d: &str| e["outcomes"][d]["ticks"].as_u64().unwrap();
    let total = |group: &[&Value], d: &str| -> u64 {
        group
            .iter()
            .map(|e| {
                if arrived(e, d) {
                    ticks(e, d)
                } else {
                    MAX_TICKS
                }
            })
            .sum()
    };
    let median = |mut v: Vec<u64>| -> Option<f64> {
        v.sort_unstable();
        (!v.is_empty()).then(|| {
            let n = v.len();
            if n % 2 == 1 {
                v[n / 2] as f64
            } else {
                (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0
            }
        })
    };
    let rows: serde_json::Map<String, Value> = groups
        .into_iter()
        .map(|(name, group)| {
            let row: serde_json::Map<String, Value> = drivers
                .iter()
                .map(|d| {
                    let completed = group.iter().filter(|e| arrived(e, d)).count();
                    let mut row = json!({"starts":group.len(),"completed":completed,
                        "total_ticks":total(&group, d),
                        "median_ticks":median(group.iter().filter(|e| arrived(e, d)).map(|e| ticks(e, d)).collect())});
                    if d != "native" {
                        let shared: Vec<_> = group
                            .iter()
                            .filter(|e| arrived(e, d) && arrived(e, "native"))
                            .collect();
                        row["native_successes_lost"] = json!(
                            group.iter().filter(|e| arrived(e, "native") && !arrived(e, d)).count()
                        );
                        row["native_failures_completed"] = json!(
                            group.iter().filter(|e| !arrived(e, "native") && arrived(e, d)).count()
                        );
                        row["shared"] = json!({
                            "starts":shared.len(),
                            "median_ticks":median(shared.iter().map(|e| ticks(e, d)).collect()),
                            "native_median_ticks":median(shared.iter().map(|e| ticks(e, "native")).collect()),
                            "faster":shared.iter().filter(|e| ticks(e, d) < ticks(e, "native")).count(),
                            "slower":shared.iter().filter(|e| ticks(e, d) > ticks(e, "native")).count(),
                        });
                        row["time_saved_against_native"] =
                            json!(total(&group, "native") as i64 - total(&group, d) as i64);
                    }
                    (d.clone(), row)
                })
                .collect();
            (name, Value::Object(row))
        })
        .collect();
    Value::Object(rows)
}

fn baseline_stage(root: &Path, plan: &Value) {
    let out = root.join("baseline");
    fs::create_dir(&out).unwrap();
    let pool = EpisodePool::new(worker_config()).unwrap();
    let models: BTreeMap<String, DenseModel> = plan["models"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, m)| {
            let model = parent(
                root,
                m["file"].as_str().unwrap(),
                m["sha256"].as_str().unwrap(),
            );
            assert_eq!(model.id(), m["genome"].as_str().unwrap());
            (name.clone(), model)
        })
        .collect();
    let mut results = serde_json::Map::new();
    for name in ["training", "holdout"] {
        let recorded = read(&root.join("discover"), &format!("{name}-starts.json"));
        let starts: Vec<(Start, &Value)> = recorded
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (serde_json::from_value(r["start"].clone()).unwrap(), r))
            .collect();
        let drivers: Vec<(String, Driver)> = plan["baselines"][name]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let d = d.as_str().unwrap();
                let driver = match d {
                    "native" => Driver::Native,
                    model => Driver::guidance(Policy::Model(models[model].clone())),
                };
                (d.to_string(), driver)
            })
            .collect();
        let mut worlds: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        for (index, (start, _)) in starts.iter().enumerate() {
            worlds.entry(start.world.seed).or_default().push(index);
        }
        let worlds: Vec<Vec<usize>> = worlds.into_values().collect();
        let started = Instant::now();
        let rows = pool.map(&worlds, |indices| {
            let world = starts[indices[0]].0.world;
            let refs: Vec<&Start> = indices.iter().map(|&i| &starts[i].0).collect();
            let mut rows = Vec::new();
            replay(&world, &refs, |start, state| {
                let index = *indices
                    .iter()
                    .find(|&&i| starts[i].0.case.name == start.case.name)
                    .unwrap();
                assert_eq!(
                    &parsed(start_record(start, &state)),
                    starts[index].1,
                    "start changed: {}",
                    start.case.name
                );
                let outcomes: serde_json::Map<String, Value> = drivers
                    .iter()
                    .map(|(d, driver)| {
                        let e = run(&state, &start.case, driver, None, false).episode;
                        (d.clone(), json!({"outcome":e.outcome,"ticks":e.elapsed_ticks,
                            "owned_ticks":e.owned_component_ticks,
                            "host_reason":e.host_reason.as_ref().map(|r| json!({
                                "goal":r["goal"],"avoidance":r["avoidance"],"reason":r["reason"]}))}))
                    })
                    .collect();
                rows.push((index, json!({"case":start.case.name,"cell":start.cell,
                    "kind":start.kind,"outcomes":outcomes})));
            });
            rows
        });
        let mut episodes: Vec<(usize, Value)> = rows.into_iter().flatten().collect();
        episodes.sort_by_key(|(index, _)| *index);
        assert_eq!(episodes.len(), starts.len());
        let episodes: Vec<Value> = episodes.into_iter().map(|(_, e)| e).collect();
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "{name}: {} starts rebuilt and flown in {seconds:.1} s",
            starts.len()
        );
        atomic(&out, &format!("{name}-episodes.json"), &episodes);
        let names: Vec<String> = drivers.into_iter().map(|(d, _)| d).collect();
        results.insert(
            name.into(),
            json!({"seconds":seconds,"drivers":names,"cells":summarize(&episodes, &names)}),
        );
    }
    atomic(
        &out,
        "results.json",
        &json!({"plan_sha256":hash(&fs::read(root.join("plan.json")).unwrap()),"sets":results}),
    );
}

#[test]
#[ignore = "frozen varied-world sets; set SPACEWARS_WORLDS_ROOT and SPACEWARS_WORLDS_STAGE"]
fn learned_guidance_varied_worlds() {
    let root = PathBuf::from(std::env::var_os("SPACEWARS_WORLDS_ROOT").unwrap());
    let plan = read(&root, "plan.json");
    check_plan(&plan);
    let started = Instant::now();
    match std::env::var("SPACEWARS_WORLDS_STAGE").unwrap().as_str() {
        "discover" => discover_stage(&root, &plan),
        "baseline" => baseline_stage(&root, &plan),
        stage => panic!("unknown stage {stage}"),
    }
    println!("stage finished in {:.1} s", started.elapsed().as_secs_f64());
}

#[test]
fn the_standard_layout_starts_the_match_of_the_lineage() {
    let world = World {
        seed: 42,
        layout: ArenaLayout::STANDARD,
        shape: DirtMode::WeakAngular,
    };
    let mut lab = start_world(&world);
    let mut lineage = start_match(42, DirtMode::WeakAngular);
    for tick in 0..60 {
        if tick % 20 == 0 {
            assert_eq!(
                initial_record(&lab, &probe_case()),
                initial_record(&lineage, &probe_case())
            );
        }
        lab.step(&[], STEP);
        lineage.step(&[], STEP);
    }
}

/// A placeholder case for comparing whole-world records.
fn probe_case() -> Case {
    Case {
        name: String::new(),
        seed: 42,
        shape: DirtMode::WeakAngular,
        seat: 0,
        source_tick: 0,
        planet: 0,
        source_entry_distance: 0.0,
        source_relative_speed: 0.0,
    }
}

#[test]
fn cells_name_distinct_worlds_and_keep_the_standard_edge() {
    let cells = Cell::all();
    assert_eq!(cells.len(), 9);
    let names: std::collections::BTreeSet<_> = cells.iter().map(Cell::name).collect();
    assert_eq!(names.len(), 9);
    for cell in &cells {
        for seed in 96_100_001..96_100_009 {
            let world = cell.world(seed);
            assert_eq!(world, cell.world(seed));
            assert_eq!(world.layout.sun, cell.sun);
            assert_eq!(world.layout.planets, cell.planets);
            if cell.varied_edge {
                assert!(EDGE_MARGINS.contains(&world.layout.edge_margin));
            } else {
                assert_eq!(world.layout.edge_margin, ArenaLayout::STANDARD.edge_margin);
            }
        }
    }
    let standard = Cell {
        sun: true,
        planets: 3,
        varied_edge: false,
    };
    assert_eq!(standard.world(7).layout, ArenaLayout::STANDARD);
    let margins: std::collections::BTreeSet<_> = (96_100_001..96_100_041)
        .map(|seed| cells[8].world(seed).layout.edge_margin)
        .collect();
    assert!(margins.len() > 20, "the edge margin varies: {margins:?}");
}
