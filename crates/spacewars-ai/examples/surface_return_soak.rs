//! A full return/replacement leg with real support, claims, losses and boarding.
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, return_trial::ReturnTrial,
    },
};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    flight_pilot::FlightIntent,
    recovery_task::{RecoverShipTask, TaskStatus},
};
use std::{fs, path::PathBuf, time::Duration};
fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or_else(|| default.to_string(), |p| p[1].clone())
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seat: usize = arg("--seat", "0").parse().unwrap();
    let mirror = arg("--mirror", "false").parse().unwrap();
    let bearing = arg("--bearing", "0").parse().unwrap();
    let seconds: u64 = arg("--seconds", "180").parse().unwrap();
    assert!(seat < 2 && (1..=180).contains(&seconds));
    let mode = arg("--case", "reachable");
    let trial = match mode.as_str() {
        "reachable" => ReturnTrial::Reachable,
        "tipped" => ReturnTrial::TippedShip,
        "other-planet" => ReturnTrial::OtherPlanet,
        _ => panic!("--case must be reachable, tipped or other-planet"),
    };
    let out = PathBuf::from(arg("--out", "/tmp/surface-return"));
    fs::create_dir_all(&out).unwrap();
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state =
        SurfaceSortieScenario::init_material_return_trial(seed, seat, mirror, bearing, trial);
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut task = RecoverShipTask::new(BrainReset {
        actor: owner,
        episode_seed: seed,
    });
    let mut started = None;
    let mut boarded = None;
    let mut departed = None;
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut failures = Vec::new();
    let mut last_label = "";
    for tick in 0..seconds * 60 {
        let o = state.recovery_task_observation(seat, task.site_request());
        let p = &o.flight.pilot;
        // Let the physically placed spaceling raise its initial flag first.
        // This makes the return leg independent of a preceding capture route.
        if p.planet
            .claim
            .as_ref()
            .is_some_and(|c| c.owner == Some(owner))
        {
            started.get_or_insert(tick);
        }
        let mut intent = if started.is_some() {
            task.step(&o)
        } else {
            FlightIntent::default()
        };
        if task.telemetry().status == TaskStatus::Succeeded {
            assert!(matches!(p.location, PilotLocation::Aboard(_)));
            boarded.get_or_insert(tick);
            if p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 70.0 {
                departed.get_or_insert(tick);
            }
            intent.controls = SurfaceSortieAction {
                primary_held: departed.is_none(),
                brake_held: departed.is_some(),
                ..Default::default()
            };
        }
        let label = task.telemetry().label();
        if label != last_label {
            events.push(
                json!({"tick": tick,"label": label,"task": task.telemetry(),"observation": o}),
            );
            eprintln!("{:.2}s {label}", tick as f64 / 60.0);
            last_label = label;
        }
        SurfaceSortieScenario::step(
            &mut state,
            &intent.encode(owner),
            Duration::from_nanos(16_666_667),
        );
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                failures.push(json!({"tick":tick+1,"audit":audit}));
            }
            samples.push(json!({"tick":tick+1,"task":task.telemetry(),"pilot":state.pilot_observation(seat,None),"audit":audit}));
        }
        if [29, 59, 119, 179].contains(&(tick / 60)) && tick % 60 == 59 {
            fs::write(
                out.join(format!("frame-{}.json", (tick + 1) / 60)),
                serde_json::to_vec(&SurfaceSortieScenario::player_frame(&state, seat)).unwrap(),
            )
            .unwrap();
        }
    }
    let report = json!({"version":1,"seed":seed,"seat":seat,"mirror":mirror,"bearing":bearing,"case":mode,
        "seconds":seconds,"physics_ok":failures.is_empty(),"failures":failures,"claimed_tick":started,
        "boarded_tick":boarded,"departed_tick":departed,"task":task.telemetry(),"events":events,"samples":samples});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"case":mode,"physics_ok":failures.is_empty(),"claimed":started,"boarded":boarded,"departed":departed,"scuttles":task.telemetry().scuttle_attempts})
    );
    assert!(failures.is_empty(), "physical audit failed");
    assert!(
        boarded.is_some() && departed.is_some(),
        "return leg must board and depart"
    );
    assert_eq!(
        task.telemetry().scuttled_tick.is_some(),
        trial != ReturnTrial::Reachable,
        "only unavailable return paths should require a replacement"
    );
}
