//! Shared two-planet policy in a bounded, reproducible physical trial.
use engine_common::{
    CombatBreakSettings, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
use serde_json::json;
use spacewars_ai::{BrainReset, combat_pilot::RulePilotV4, mission_pilot::MaterialMissionPilot};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or_else(|| default.to_owned(), |p| p[1].clone())
}
fn timing(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    json!({"p95_ms": values[(values.len() * 95 / 100).min(values.len()-1)], "max_ms":values.last()})
}
fn main() {
    let seed = arg("--seed", "42").parse().unwrap();
    let seat: usize = arg("--seat", "1").parse().unwrap();
    let mirror = arg("--mirror", "false").parse().unwrap();
    let seconds: u64 = arg("--seconds", "180").parse().unwrap();
    let mode = arg("--mode", "quiet");
    let interval = arg("--asteroid-interval", "0").parse().unwrap();
    let frames = arg("--frames", "false") == "true";
    let require_route = arg("--require-route", "false") == "true";
    let strike = arg("--strike-after-departure", "false") == "true";
    let bearing: f32 = arg("--bearing", "0").parse().unwrap();
    assert!(seat < 2 && (1..=180).contains(&seconds));
    assert!(["quiet", "intercept", "duel"].contains(&mode.as_str()));
    let out = PathBuf::from(arg("--out", "/tmp/surface-mission"));
    fs::create_dir_all(&out).unwrap();
    let mut state = SurfaceSortieScenario::init_material_travel_trial(seed, mirror, bearing);
    state.set_asteroid_pressure(MaterialAsteroidSettings {
        interval_seconds: interval,
        severity: MaterialAsteroidSeverity::Mixed,
    });
    let mut pilots = std::array::from_fn::<_, 2, _>(|i| {
        MaterialMissionPilot::new(
            BrainReset {
                actor: PlayerId::from_index(i).unwrap(),
                episode_seed: seed,
            },
            CombatBreakSettings::default(),
        )
    });
    let mut interceptor = RulePilotV4::with_combat_breaks(
        BrainReset {
            actor: PlayerId::from_index(1 - seat).unwrap(),
            episode_seed: seed,
        },
        CombatBreakSettings::default(),
    );
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut sensors = Vec::new();
    let mut policies = Vec::new();
    let mut steps = Vec::new();
    let mut samples = Vec::new();
    let mut events = Vec::new();
    let mut asteroid_events = Vec::new();
    let mut failures = Vec::new();
    let mut last = [String::new(), String::new()];
    let mut strike_tick = None;
    for tick in 0..seconds * 60 {
        if strike && strike_tick.is_none() && pilots[seat].telemetry().completed_sorties > 0 {
            assert!(state.spawn_recovery_hazard(
                seat,
                scenario_spacewars::surface_sortie::impact::RecoveryHazard::HeavyAsteroid,
                false
            ));
            strike_tick = Some(tick);
        }
        let mut actions = Vec::new();
        for i in 0..2 {
            let owner = PlayerId::from_index(i).unwrap();
            if i == seat || mode == "duel" {
                let clock = Instant::now();
                let o = state.mission_observation(i, pilots[i].site_request());
                sensors.push(clock.elapsed().as_secs_f64() * 1000.0);
                let clock = Instant::now();
                let mut intent = pilots[i].intent(&o);
                if mode == "quiet" {
                    intent.weapons = Default::default();
                }
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                actions.extend(intent.encode(owner));
                let label = pilots[i].label();
                if label != last[i] {
                    events.push(json!({"tick":tick,"seat":i,"label":label,"telemetry":pilots[i].telemetry()}));
                    eprintln!("{:.2}s P{} {label}", tick as f64 / 60.0, i + 1);
                    last[i] = label;
                }
            } else if mode == "intercept" {
                let clock = Instant::now();
                let o = state.combat_observation(i, interceptor.site_request());
                sensors.push(clock.elapsed().as_secs_f64() * 1000.0);
                let clock = Instant::now();
                let intent = interceptor.intent(&o);
                policies.push(clock.elapsed().as_secs_f64() * 1000.0);
                actions.extend(intent.encode(owner));
            }
        }
        let clock = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        steps.push(clock.elapsed().as_secs_f64() * 1000.0);
        let asteroids = state.asteroid_pressure();
        if !asteroids.arrivals.is_empty() || !asteroids.impacts.is_empty() {
            asteroid_events.push(
                json!({"tick":tick+1,"arrivals":asteroids.arrivals,"impacts":asteroids.impacts}),
            );
        }
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            if !audit.issues.is_empty()
                || audit.occupied_cells + audit.removed_cells != initial
                || audit.max_speed >= 500.0
            {
                failures.push(json!({"tick":tick+1,"audit":audit}));
            }
            let observations = std::array::from_fn::<_, 2, _>(|i| {
                state.pilot_observation(i, pilots[i].site_request())
            });
            samples.push(json!({"second":(tick+1)/60,"pilots":observations,"missions":pilots.each_ref().map(|p|p.telemetry()),"audit":audit}));
            if frames && ((tick + 1) / 60 == 1 || (tick + 1) % 1800 == 0) {
                for i in 0..2 {
                    fs::write(
                        out.join(format!("frame-{}-p{}.json", (tick + 1) / 60, i + 1)),
                        serde_json::to_vec(&SurfaceSortieScenario::player_frame(&state, i))
                            .unwrap(),
                    )
                    .unwrap();
                }
            }
        }
    }
    let completed: BTreeSet<_> = pilots[seat]
        .telemetry()
        .events
        .iter()
        .filter(|e| e.kind == "departed")
        .filter_map(|e| e.planet)
        .collect();
    let report = json!({"version":1,"seed":seed,"seat":seat,"mirror":mirror,"mode":mode,"seconds":seconds,"bearing":bearing,
        "physics_ok":failures.is_empty(),"audit_failures":failures,"distinct_departures":completed,
        "missions":pilots.each_ref().map(|p|p.telemetry()),"interceptor":interceptor.telemetry(),"strike_tick":strike_tick,
        "sensors":timing(sensors),"policy":timing(policies),"steps":timing(steps),"events":events,"samples":samples,
        "asteroids":state.asteroid_pressure(),"asteroid_events":asteroid_events});
    fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"physics_ok":report["physics_ok"],"distinct_departures":completed,"steps":report["steps"],"sensors":report["sensors"]})
    );
    assert!(report["physics_ok"] == true, "physical audit failed");
    if require_route {
        assert_eq!(completed.len(), 2, "both planet sorties must complete");
    }
}
