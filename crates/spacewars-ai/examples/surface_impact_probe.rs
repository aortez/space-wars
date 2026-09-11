//! Tick-level replay of ordinary combat or generated-world missions. Optional
//! pod controls start at a specified tick; no hits, motion or damage are injected.
use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{SurfaceSortieAction, SurfaceSortieScenario, pilot::MaterialFlightStart},
};
use serde_json::json;
use spacewars_ai::{BrainReset, combat_pilot::RulePilotV4, mission_pilot::MaterialMissionPilot};
use std::{
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Duration,
};

fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or_else(|| default.to_owned(), |p| p[1].clone())
}

fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let mirror = arg("--mirror", "true").parse::<bool>().unwrap();
    let world = arg("--world", "combat");
    assert!(["combat", "generated"].contains(&world.as_str()));
    let generated = world == "generated";
    let seconds: u64 = arg("--seconds", "180").parse().unwrap();
    let from: u64 = arg("--from-tick", "0").parse().unwrap();
    let until: u64 = arg("--until-tick", "10800").parse().unwrap();
    let control_from: u64 = arg("--control-from-tick", "0").parse().unwrap();
    let seat: usize = arg("--seat", "1").parse().unwrap();
    let control = arg("--pod-control", "bot");
    assert!(seat < 2 && (1..=180).contains(&seconds) && from <= until);
    assert!(["bot", "brake", "coast"].contains(&control.as_str()));
    let breaks = CombatBreakSettings {
        interval_seconds: arg("--break-interval", "8").parse().unwrap(),
        duration_seconds: 4,
    };
    assert_eq!(breaks, breaks.normalized());
    let mut state = if generated {
        SurfaceSortieScenario::init_material_arena_trial(seed, mirror, 0.0)
    } else {
        SurfaceSortieScenario::init_material_combat_trial(
            seed,
            &[0, 1].map(|i| {
                (
                    PlayerId::from_index(i).unwrap(),
                    MaterialFlightStart {
                        bearing: if (i == 0) != mirror { -0.5 } else { 0.5 },
                        altitude: 90.0,
                        radial_speed: 0.0,
                        lateral_speed: 0.0,
                        heading_offset: 0.0,
                    },
                )
            }),
            [100.0; 2],
        )
    };
    state.enable_match_rules();
    let resets = [0, 1].map(|i| BrainReset {
        actor: PlayerId::from_index(i).unwrap(),
        episode_seed: seed,
    });
    let mut combat = resets.map(|r| RulePilotV4::with_combat_breaks(r, breaks));
    let mut missions = resets.map(|r| MaterialMissionPilot::new(r, breaks));
    let out = PathBuf::from(arg("--out", "/tmp/surface-impact-probe"));
    fs::create_dir_all(&out).unwrap();
    let mut trace = BufWriter::new(fs::File::create(out.join("trace.jsonl")).unwrap());
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut alarms = Vec::new();
    let mut damage = Vec::new();
    let mut elapsed = 0;
    for tick in 0..seconds * 60 {
        if state.match_outcome().is_some() {
            break;
        }
        let mut actions = Vec::new();
        for i in 0..2 {
            let (o, mut intent, telemetry) = if generated {
                let mission = state.mission_observation(i, missions[i].site_request());
                let intent = missions[i].intent(&mission);
                (mission.local.combat, intent, json!(missions[i].telemetry()))
            } else {
                let o = state.combat_observation(i, combat[i].site_request());
                let intent = combat[i].intent(&o);
                (o, intent, json!(combat[i].telemetry()))
            };
            let p = &o.recovery.flight.pilot;
            let selected = intent.flight.controls;
            let overridden = i == seat
                && tick >= control_from
                && p.ship_form == ShipForm::EscapePod
                && p.actor.is_none()
                && control != "bot";
            if overridden {
                intent.flight.controls = SurfaceSortieAction {
                    brake_held: control == "brake",
                    ..Default::default()
                };
            }
            if (from..=until).contains(&tick) {
                let a = intent.flight.controls;
                serde_json::to_writer(&mut trace, &json!({
                    "tick":tick, "seat":i, "motion":state.impact_motion(i),
                    "form":p.ship_form, "location":p.location, "pilot":p,
                    "jetpack":o.recovery.jetpack.as_ref().map(|j| json!({"charge":j.charge,"burning":j.burning,"burn_seconds":j.burn_seconds,"gravity":j.gravity,"reference_velocity":j.reference_velocity})),
                    "vitals":state.observation(i).pilot_vitals, "damage":state.damage_observation(i),
                    "controls":{"horizontal":a.horizontal,"primary":a.primary_held,"brake":a.brake_held,"interact":a.interact_held},
                    "bot_controls":{"horizontal":selected.horizontal,"primary":selected.primary_held,"brake":selected.brake_held},
                    "overridden":overridden, "telemetry":telemetry,
                })).unwrap();
                writeln!(trace).unwrap();
            }
            actions.extend(intent.encode(PlayerId::from_index(i).unwrap()));
        }
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        elapsed = tick + 1;
        let round = state.match_observation().unwrap();
        for (i, vitals) in round.pilots.iter().enumerate() {
            if vitals.last_damage.is_some_and(|d| d.tick == elapsed) {
                damage.push(json!({"tick":elapsed,"seat":i,"vitals":vitals,"motion":state.impact_motion(i)}));
            }
        }
        if elapsed % 60 == 0 || round.outcome.is_some() {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                alarms.push(json!({"tick":elapsed,"audit":audit}));
            }
        }
    }
    trace.flush().unwrap();
    let report = json!({"seed":seed,"mirror":mirror,"generated":generated,"seconds":seconds,
        "control":control,"control_from_tick":control_from,"seat":seat,"breaks":breaks,
        "elapsed_ticks":elapsed,"round":state.match_observation(),"damage":damage,"alarms":alarms,
        "audit":state.terrain_diagnostics(),"final_motion":[state.impact_motion(0),state.impact_motion(1)]});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(alarms.is_empty(), "physical audit failed; see report");
}
