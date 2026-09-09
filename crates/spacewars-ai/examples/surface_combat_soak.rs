//! Physical combat, optionally followed by P1 attempting to land under fire.
//! Fixed initial health is optional; no runtime hits or recovery are scripted.
use engine_common::{
    CombatBreakSettings, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{PilotLocation, SurfaceSortieScenario, pilot::MaterialFlightStart},
};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize)]
struct PhaseMetrics {
    ticks: u64,
    exposed_ticks: u64,
    initial_health: Option<f32>,
    damage_by_recorded_source: BTreeMap<String, f32>,
    losses: u32,
}

use spacewars_ai::{
    BrainReset,
    combat_pilot::{CombatIntent, RulePilotV4},
    flight_pilot::FlightIntent,
    pilot::{PilotBrain, RulePilotV1},
    tactical_capture::TacticalCapturePilot,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or(default.to_owned(), |p| p[1].clone())
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seconds: u32 = arg("--seconds", "180").parse().unwrap();
    assert!((1..=180).contains(&seconds));
    let mirror = arg("--mirror", "false") == "true";
    let separation: f32 = arg("--separation", "0.5").parse().unwrap();
    assert!((0.3..=1.5).contains(&separation));
    let land_after_arg = arg("--land-after", "");
    let land_after: Option<u32> = (!land_after_arg.is_empty()).then(|| {
        land_after_arg
            .parse()
            .expect("--land-after must be a whole number of seconds")
    });
    assert!(land_after.is_none_or(|s| s < seconds));
    let landing_policy = arg("--landing-policy", "basic");
    assert!(matches!(landing_policy.as_str(), "basic" | "tactical"));
    let use_tactical = landing_policy == "tactical";
    let subject_seat: usize = arg("--subject-seat", "0").parse().unwrap();
    assert!(subject_seat < 2);
    let subject_health: f32 = arg("--subject-health", "100").parse().unwrap();
    assert!((1.0..=100.0).contains(&subject_health));
    let opponent_fire = arg("--opponent-fire", "true").parse::<bool>().unwrap();
    let break_config = CombatBreakSettings {
        interval_seconds: arg("--break-interval", "0")
            .parse()
            .expect("--break-interval is seconds; 0 disables breaks"),
        duration_seconds: arg("--break-seconds", "4")
            .parse()
            .expect("--break-seconds is a duration in seconds"),
    };
    assert_eq!(
        break_config,
        break_config.normalized(),
        "break interval must be 0..120 and duration 1..15"
    );
    let save_frames = arg("--frames", "false") == "true";
    let asteroid_settings = MaterialAsteroidSettings {
        interval_seconds: arg("--asteroid-interval", "0").parse().unwrap(),
        severity: match arg("--asteroid-severity", "mixed").as_str() {
            "light" => MaterialAsteroidSeverity::Light,
            "mixed" => MaterialAsteroidSeverity::Mixed,
            "heavy" => MaterialAsteroidSeverity::Heavy,
            _ => panic!("asteroid severity must be light, mixed or heavy"),
        },
    };
    assert_eq!(asteroid_settings, asteroid_settings.normalized());
    // Diagnostic continuation retains a failing exit status and the first alarm;
    // use it to distinguish a brief collision kick from sustained acceleration.
    let continue_after_failure = arg("--continue-after-failure", "false")
        .parse::<bool>()
        .unwrap();
    let mut state = SurfaceSortieScenario::init_material_combat_trial(
        seed,
        &[0, 1].map(|seat| {
            (
                PlayerId::from_index(seat).unwrap(),
                MaterialFlightStart {
                    bearing: (if (seat == 0) != mirror { -1.0 } else { 1.0 }) * separation,
                    altitude: 90.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )
        }),
        std::array::from_fn(|seat| {
            if seat == subject_seat {
                subject_health
            } else {
                100.0
            }
        }),
    );
    let mut brains = [0, 1].map(|seat| {
        // Keep the landing subject's policy fixed across pressure comparisons;
        // only its opponent receives the experimental combat pacing.
        let config = if seat == subject_seat && land_after.is_some() {
            CombatBreakSettings {
                interval_seconds: 0,
                ..break_config
            }
        } else {
            break_config
        };
        RulePilotV4::with_combat_breaks(
            BrainReset {
                actor: PlayerId::from_index(seat).unwrap(),
                episode_seed: seed,
            },
            config,
        )
    });
    state.set_asteroid_pressure(asteroid_settings);
    let mut landing = RulePilotV1::new(BrainReset {
        actor: PlayerId::from_index(subject_seat).unwrap(),
        episode_seed: seed,
    });
    let mut tactical = TacticalCapturePilot::new(
        BrainReset {
            actor: PlayerId::from_index(subject_seat).unwrap(),
            episode_seed: seed,
        },
        break_config,
    );
    let mut landing_start = None;
    let mut exited_tick = None;
    let mut lost_tick = None;
    let out = PathBuf::from(arg("--out", "/tmp/combat-soak"));
    fs::create_dir_all(&out).unwrap();
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut break_events = Vec::new();
    let mut damage_events = Vec::new();
    let mut audit_events = Vec::new();
    let mut asteroid_events = Vec::new();
    let mut previous_breaks = [(0, 0, 0, None); 2];
    let mut previous = ["", ""];
    let mut ground_maps = [None, None];
    let mut rebuild_surveys = [None, None];
    let mut ground_failures = Vec::new();
    let mut steps = Vec::new();
    let mut ai = Vec::new();
    let mut sensors = Vec::new();
    let mut policies = Vec::new();
    let mut failure = None;
    let mut phases = BTreeMap::<String, PhaseMetrics>::new();
    let initial_state = [
        state.combat_observation(0, None),
        state.combat_observation(1, None),
    ];
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let mut actions = Vec::new();
        let mut subject_phase = None;
        for seat in 0..2 {
            let sensor_start = Instant::now();
            let landing_now = seat == subject_seat && land_after.is_some_and(|s| tick >= s * 60);
            let site = if landing_now && use_tactical {
                tactical.site_request()
            } else if landing_now {
                landing.site_request()
            } else {
                brains[seat].site_request()
            };
            let tactical_o = (landing_now && use_tactical)
                .then(|| state.tactical_sortie_observation(seat, site));
            let o = tactical_o.as_ref().map_or_else(
                || state.combat_observation(seat, site),
                |o| o.combat.clone(),
            );
            sensors.push(sensor_start.elapsed().as_secs_f64() * 1000.0);
            if let Some(map) = &o.recovery.ground {
                ground_maps[seat] = Some(map.clone());
            }
            if let Some(survey) = &o.recovery.rebuild {
                rebuild_surveys[seat] = Some(survey.clone());
            }
            if landing_now {
                landing_start.get_or_insert_with(|| o.clone());
                if lost_tick.is_none()
                    && o.recovery.flight.pilot.ship_form == scenario_spacewars::ShipForm::Ship
                    && o.recovery.flight.pilot.location == PilotLocation::OnFoot
                {
                    exited_tick.get_or_insert(tick);
                }
                if o.recovery.flight.pilot.ship_form != scenario_spacewars::ShipForm::Ship
                    || !o.recovery.flight.pilot.ship_available
                {
                    lost_tick.get_or_insert(tick);
                }
            }
            let policy_start = Instant::now();
            let mut intent = if let Some(o) = &tactical_o {
                tactical.intent(o)
            } else if landing_now {
                CombatIntent {
                    flight: FlightIntent {
                        controls: landing.intent(&o.recovery.flight.pilot),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            } else {
                brains[seat].intent(&o)
            };
            policies.push(policy_start.elapsed().as_secs_f64() * 1000.0);
            if seat != subject_seat && !opponent_fire {
                intent.weapons = Default::default();
            }
            if !landing_now {
                let b = &brains[seat].telemetry().breaks;
                let milestone = (b.started, b.completed, b.interrupted, b.last_reengaged_tick);
                if milestone != previous_breaks[seat] {
                    break_events.push(json!({"tick":tick,"seat":seat,"breaks":b,"observation":o}));
                    previous_breaks[seat] = milestone;
                }
                if b.active_until_tick.is_some() && intent.weapons != Default::default() {
                    failure = Some(format!("P{} fired during a break at tick {tick}", seat + 1));
                }
            }
            actions.extend(intent.encode(PlayerId::from_index(seat).unwrap()));
            let label = if landing_now && use_tactical {
                tactical.label()
            } else if landing_now {
                landing.telemetry().goal.label()
            } else {
                brains[seat].label()
            };
            if landing_now {
                let phase = if use_tactical && tactical.telemetry().completed_tick.is_some()
                    || !use_tactical && landing.telemetry().completed_tick.is_some()
                {
                    "after sortie completion"
                } else if use_tactical && tactical.telemetry().failed_tick.is_some() {
                    "after sortie failure"
                } else if use_tactical
                    && tactical.telemetry().goal
                        != spacewars_ai::tactical_sortie::TacticalGoal::Surface
                {
                    tactical.telemetry().goal.label()
                } else {
                    label
                };
                subject_phase = Some(phase);
                let metrics = phases.entry(phase.to_owned()).or_default();
                metrics.ticks += 1;
                let p = &o.recovery.flight.pilot;
                metrics.initial_health.get_or_insert(p.ship_health);
                metrics.exposed_ticks += u64::from(
                    matches!(p.location, PilotLocation::Aboard(_))
                        && p.ship_form == scenario_spacewars::ShipForm::Ship
                        && o.target.is_some_and(|t| {
                            !t.ground_occluded
                                && t.motion.position.distance_to(p.ship.position) < 300.0
                        }),
                );
            }
            if label != previous[seat] {
                if label.contains("no measured")
                    || label.contains("ground traversal")
                    || label.contains("on ground route")
                    || label.contains("no reachable standing")
                    || label.contains("no grounded hatch")
                {
                    ground_failures.push(json!({"tick":tick,"seat":seat,"goal":label,"map":ground_maps[seat],"last_rebuild_survey":rebuild_surveys[seat],"observation":o,"brain":brains[seat].telemetry(),"capture_combat":tactical.combat_telemetry(),"tactical":tactical.telemetry()}));
                    if save_frames {
                        let mut frame = SurfaceSortieScenario::player_frame(&state, seat);
                        frame.layers.retain(|layer| layer.z < 15);
                        let p = &o.recovery.flight.pilot;
                        let point = p.actor.unwrap_or(p.ship).position;
                        frame.camera = engine_common::Camera2::new(
                            engine_common::RenderPoint::new(point.x, point.y),
                            36.0,
                        );
                        fs::write(
                            out.join(format!("failure-{tick}-p{}.json", seat + 1)),
                            serde_json::to_vec(&frame).unwrap(),
                        )
                        .unwrap();
                    }
                }
                if label != "engage ship" && previous[seat] != "engage ship" || tick % 60 == 0 {
                    eprintln!("{:.2}s P{} {label}", tick as f32 / 60.0, seat + 1);
                }
                events.push(json!({"tick":tick,"seat":seat,"goal":label,"brain":brains[seat].telemetry(),"tactical":if landing_now && use_tactical {Some(tactical.telemetry())} else {None},"cover":tactical_o.as_ref().map(|o| &o.cover),"observation":o}));
                previous[seat] = label;
            }
        }
        ai.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
        let environment = state.asteroid_pressure();
        if !environment.arrivals.is_empty() || !environment.impacts.is_empty() {
            asteroid_events.push(json!({"tick":tick + 1,"arrivals":environment.arrivals,"impacts":environment.impacts}));
        }
        for seat in 0..2 {
            let damage = state.damage_observation(seat);
            if damage.last_damage_tick == Some(u64::from(tick) + 1) {
                damage_events.push(json!({"tick":tick + 1,"seat":seat,"damage":damage}));
                if seat == subject_seat
                    && let Some(phase) = subject_phase
                {
                    let metrics = phases.get_mut(phase).unwrap();
                    *metrics
                        .damage_by_recorded_source
                        .entry(damage.last_source.unwrap_or("unknown").to_owned())
                        .or_default() += damage.last_damage_percent;
                    metrics.losses += u32::from(damage.last_ship_lost);
                }
            }
        }
        if land_after.is_some_and(|s| tick >= s * 60) {
            let p = state.pilot_observation(
                subject_seat,
                if use_tactical {
                    tactical.site_request()
                } else {
                    landing.site_request()
                },
            );
            if lost_tick.is_none()
                && p.ship_form == scenario_spacewars::ShipForm::Ship
                && p.location == PilotLocation::OnFoot
            {
                exited_tick.get_or_insert(tick + 1);
            }
            if p.ship_form != scenario_spacewars::ShipForm::Ship || !p.ship_available {
                lost_tick.get_or_insert(tick + 1);
            }
        }
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                let message = format!("audit failed at {}s: {audit:?}", (tick + 1) / 60);
                failure.get_or_insert(message.clone());
                audit_events.push(json!({"second":(tick+1)/60,"message":message,"audit":audit}));
            }
            for (seat, brain) in brains.iter().enumerate() {
                if let Some(supply) = state.combat_observation(seat, brain.site_request()).supply
                    && (!(0.0..=100.0).contains(&supply.energy_percent)
                        || supply.rounds_loaded > 2
                        || supply
                            .reload_progress
                            .is_some_and(|p| !(0.0..=1.0).contains(&p)))
                {
                    failure = Some(format!(
                        "P{} supply invalid at {}s: {supply:?}",
                        seat + 1,
                        (tick + 1) / 60
                    ));
                }
            }
            samples.push(json!({"second":(tick+1)/60,"capture_combat":tactical.combat_telemetry(),"brains":brains.each_ref().map(|b| b.telemetry()),"pilots":std::array::from_fn::<_,2,_>(|seat| state.combat_observation(seat, if seat == subject_seat && land_after.is_some_and(|s| tick>=s*60) {
                if use_tactical {tactical.site_request()} else {landing.site_request()}
            } else {brains[seat].site_request()})),"audit":audit,"asteroids":state.asteroid_pressure(),"landing":land_after.filter(|s| tick >= s * 60).map(|_| if use_tactical {json!(tactical.telemetry())} else {json!(landing.telemetry())})}));
            if failure.is_some()
                || save_frames
                    && [1, 2, 3, 10, 12, 13, 30, 60, 90, 110, 120, 180].contains(&((tick + 1) / 60))
            {
                for seat in 0..2 {
                    let mut frame = SurfaceSortieScenario::player_frame(&state, seat);
                    fs::write(
                        out.join(format!("frame-{}-p{}.json", (tick + 1) / 60, seat + 1)),
                        serde_json::to_vec(&frame).unwrap(),
                    )
                    .unwrap();
                    frame.layers.retain(|layer| layer.z < 15);
                    let p = state.pilot_observation(seat, None).ship.position;
                    frame.camera = engine_common::Camera2::new(
                        engine_common::RenderPoint::new(p.x, p.y),
                        28.0,
                    );
                    fs::write(
                        out.join(format!("ship-{}-p{}.json", (tick + 1) / 60, seat + 1)),
                        serde_json::to_vec(&frame).unwrap(),
                    )
                    .unwrap();
                }
            }
            if failure.is_some() && !continue_after_failure {
                break;
            }
        }
    }
    steps.sort_by(f64::total_cmp);
    ai.sort_by(f64::total_cmp);
    sensors.sort_by(f64::total_cmp);
    policies.sort_by(f64::total_cmp);
    let report = json!({"version":5,"capture_combat":tactical.combat_telemetry(),"asteroids":state.asteroid_pressure(),"asteroid_events":asteroid_events,"continue_after_failure":continue_after_failure,"audit_events":audit_events,"phases":phases,"landing_policy":landing_policy,"initial_state":initial_state,"subject_seat":subject_seat,"subject_health":subject_health,"opponent_fire":opponent_fire,"combat_breaks":break_config,"break_events":break_events,"damage_events":damage_events,"seed":seed,"seconds":seconds,"completed_seconds":steps.len()/60,"failure":failure,"mirror":mirror,"separation":separation,"brains":brains.each_ref().map(|b| b.telemetry()),
        "landing_under_fire":land_after.map(|s| json!({"land_after_seconds":s,"start":landing_start,"telemetry":if use_tactical {json!(tactical.telemetry())} else {json!(landing.telemetry())},"exited_tick":exited_tick,"lost_tick":lost_tick})),
        "weapons":[state.combat_telemetry(0),state.combat_telemetry(1)],"step_p95_ms":steps[steps.len()*95/100],"step_max_ms":steps.last(),"ai_p95_ms":ai[ai.len()*95/100],
        "ai_max_ms":ai.last(),"sensor_p95_ms":sensors[sensors.len()*95/100],"sensor_max_ms":sensors.last(),"policy_p95_ms":policies[policies.len()*95/100],"policy_max_ms":policies.last(),"samples":samples,"events":events,"ground_failures":ground_failures});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"brains":report["brains"],"weapons":report["weapons"],"step_p95_ms":report["step_p95_ms"],"ai_p95_ms":report["ai_p95_ms"]})
    );
    if let Some(failure) = failure {
        eprintln!("{failure}");
        std::process::exit(1);
    }
}
