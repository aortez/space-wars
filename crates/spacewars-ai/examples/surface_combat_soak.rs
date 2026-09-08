//! Two physical combat pilots, no scripted hits or health edits.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieScenario, pilot::MaterialFlightStart},
};
use serde_json::json;
use spacewars_ai::{BrainReset, combat_pilot::RulePilotV4};
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
        RulePilotV4::new(BrainReset {
            actor: PlayerId::from_index(seat).unwrap(),
            episode_seed: seed,
        })
    });
    let out = PathBuf::from(arg("--out", "/tmp/combat-soak"));
    fs::create_dir_all(&out).unwrap();
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut previous = ["", ""];
    let mut steps = Vec::new();
    let mut ai = Vec::new();
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let mut actions = Vec::new();
        for seat in 0..2 {
            let o = state.combat_observation(seat, brains[seat].site_request());
            actions.extend(
                brains[seat]
                    .intent(&o)
                    .encode(PlayerId::from_index(seat).unwrap()),
            );
            let label = brains[seat].label();
            if label != previous[seat] {
                if label != "engage ship" && previous[seat] != "engage ship" || tick % 60 == 0 {
                    eprintln!("{:.2}s P{} {label}", tick as f32 / 60.0, seat + 1);
                }
                events.push(json!({"tick":tick,"seat":seat,"brain":brains[seat].telemetry(),"observation":o}));
                previous[seat] = label;
            }
        }
        ai.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(start.elapsed().as_secs_f64() * 1000.0);
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "{audit:?}");
            assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
            assert!(audit.max_speed < 500.0);
            samples.push(json!({"second":(tick+1)/60,"brains":brains.each_ref().map(|b| b.telemetry()),"pilots":[state.combat_observation(0,brains[0].site_request()),state.combat_observation(1,brains[1].site_request())],"audit":audit}));
        }
    }
    steps.sort_by(f64::total_cmp);
    ai.sort_by(f64::total_cmp);
    let report = json!({"version":1,"seed":seed,"seconds":seconds,"mirror":mirror,"separation":separation,"brains":brains.each_ref().map(|b| b.telemetry()),
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
}
