//! Physical combat, optionally followed by P1 attempting to land under fire.
//! No scripted hits or health edits; both missions use ordinary controls.
use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{PilotLocation, SurfaceSortieScenario, pilot::MaterialFlightStart},
};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    combat_pilot::{CombatIntent, RulePilotV4},
    flight_pilot::FlightIntent,
    pilot::{PilotBrain, RulePilotV1},
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
    let mut state = SurfaceSortieScenario::init_material_combat_flight(
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
    );
    let mut brains = [0, 1].map(|seat| {
        // Keep the landing subject's policy fixed across pressure comparisons;
        // only its opponent receives the experimental combat pacing.
        let config = if seat == 0 && land_after.is_some() {
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
    let mut landing = RulePilotV1::new(BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: seed,
    });
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
    let mut previous_breaks = [(0, 0, 0, None); 2];
    let mut previous = ["", ""];
    let mut steps = Vec::new();
    let mut ai = Vec::new();
    let mut failure = None;
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let mut actions = Vec::new();
        for seat in 0..2 {
            let landing_now = seat == 0 && land_after.is_some_and(|s| tick >= s * 60);
            let site = if landing_now {
                landing.site_request()
            } else {
                brains[seat].site_request()
            };
            let o = state.combat_observation(seat, site);
            if landing_now {
                landing_start.get_or_insert_with(|| o.clone());
                if o.recovery.flight.pilot.location == PilotLocation::OnFoot {
                    exited_tick.get_or_insert(tick);
                }
                if o.recovery.flight.pilot.ship_form != scenario_spacewars::ShipForm::Ship
                    || !o.recovery.flight.pilot.ship_available
                {
                    lost_tick.get_or_insert(tick);
                }
            }
            let intent = if landing_now {
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
            let label = if landing_now {
                landing.telemetry().goal.label()
            } else {
                brains[seat].label()
            };
            if label != previous[seat] {
                if label != "engage ship" && previous[seat] != "engage ship" || tick % 60 == 0 {
                    eprintln!("{:.2}s P{} {label}", tick as f32 / 60.0, seat + 1);
                }
                events.push(json!({"tick":tick,"seat":seat,"goal":label,"brain":brains[seat].telemetry(),"observation":o}));
                previous[seat] = label;
            }
        }
        ai.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
        for seat in 0..2 {
            let damage = state.damage_observation(seat);
            if damage.last_damage_tick == Some(u64::from(tick) + 1) {
                damage_events.push(json!({"tick":tick + 1,"seat":seat,"damage":damage}));
            }
        }
        if land_after.is_some_and(|s| tick >= s * 60) {
            let p = state.pilot_observation(0, landing.site_request());
            if p.location == PilotLocation::OnFoot {
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
                failure = Some(format!("audit failed at {}s: {audit:?}", (tick + 1) / 60));
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
            samples.push(json!({"second":(tick+1)/60,"brains":brains.each_ref().map(|b| b.telemetry()),"pilots":[state.combat_observation(0,brains[0].site_request()),state.combat_observation(1,brains[1].site_request())],"audit":audit,"landing":land_after.filter(|s| tick >= s * 60).map(|_| landing.telemetry())}));
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
            if failure.is_some() {
                break;
            }
        }
    }
    steps.sort_by(f64::total_cmp);
    ai.sort_by(f64::total_cmp);
    let report = json!({"version":3,"combat_breaks":break_config,"break_events":break_events,"damage_events":damage_events,"seed":seed,"seconds":seconds,"completed_seconds":steps.len()/60,"failure":failure,"mirror":mirror,"separation":separation,"brains":brains.each_ref().map(|b| b.telemetry()),
        "landing_under_fire":land_after.map(|s| json!({"land_after_seconds":s,"start":landing_start,"telemetry":landing.telemetry(),"exited_tick":exited_tick,"lost_tick":lost_tick})),
        "weapons":[state.combat_telemetry(0),state.combat_telemetry(1)],"step_p95_ms":steps[steps.len()*95/100],"step_max_ms":steps.last(),"ai_p95_ms":ai[ai.len()*95/100],
        "samples":samples,"events":events});
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
