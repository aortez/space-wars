//! A physical, three-minute crossing trial with retained reports on failure.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{SurfaceSortieAction, SurfaceSortieScenario, pilot::MaterialFlightStart},
};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    jetpack_crossing::{CrossingGoal, JetpackCrossingPilot},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn arg(key: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|a| a[0] == key)
        .map_or(default.to_owned(), |a| a[1].clone())
}
fn timing(values: &mut [f64]) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    json!({"p95_ms":values[values.len()*95/100],"max_ms":values.last()})
}
fn main() {
    let seat: usize = arg("--seat", "0").parse().unwrap();
    let seed = arg("--seed", "42").parse().unwrap();
    let bearing = arg("--bearing", "parked");
    let seconds: usize = arg("--seconds", "180").parse().unwrap();
    assert!(seat < 2 && (1..=180).contains(&seconds));
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state = if bearing == "parked" {
        SurfaceSortieScenario::init_material_jetpack(seed, 2)
    } else {
        let mut s = SurfaceSortieScenario::init_material_flight(
            seed,
            2,
            &[(
                owner,
                MaterialFlightStart {
                    bearing: bearing.parse().unwrap(),
                    altitude: 15.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )],
        );
        s.enable_jetpacks();
        s
    };
    let mut bot = JetpackCrossingPilot::new(BrainReset {
        actor: owner,
        episode_seed: seed,
    });
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut audits = Vec::new();
    let mut sensor_times = Vec::new();
    let mut step_times = Vec::new();
    let mut previous = CrossingGoal::Exit;
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let o = state.jetpack_crossing_observation(seat, bot.direction());
        let a = bot.step(&o);
        sensor_times.push(start.elapsed().as_secs_f64() * 1000.0);
        if bot.telemetry().goal != previous {
            events.push(json!({"tick":tick,"telemetry":bot.telemetry(),"observation":o}));
            previous = bot.telemetry().goal;
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(
            &mut state,
            &[
                a.encode(owner),
                SurfaceSortieAction::default().encode(PlayerId::from_index(1 - seat).unwrap()),
            ],
            Duration::from_nanos(16_666_667),
        );
        step_times.push(start.elapsed().as_secs_f64() * 1000.0);
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty() {
                audits.push(json!({"tick":tick+1,"audit":audit}));
            }
            samples.push(json!({"second":(tick+1)/60,"telemetry":bot.telemetry(),"observation":o,"audit":audit}));
        }
    }
    let complete = bot.telemetry().goal == CrossingGoal::Complete
        && bot.telemetry().crossings == 2
        && bot.telemetry().claimed;
    let report = json!({"version":1,"seed":seed,"seat":seat,"bearing":bearing,"seconds":seconds,"complete":complete,"audit_passed":audits.is_empty(),"telemetry":bot.telemetry(),"events":events,"samples":samples,"audit_failures":audits,"sensor_policy":timing(&mut sensor_times),"physics_step":timing(&mut step_times)});
    let out = PathBuf::from(arg("--out", "/tmp/jetpack-crossing"));
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"complete":complete,"audit_passed":audits.is_empty(),"telemetry":bot.telemetry()})
    );
    assert!(
        complete && audits.is_empty(),
        "crossing failed; full report retained in {}",
        out.display()
    );
}
