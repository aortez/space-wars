//! Reconstruct the retained Picade case with frozen controls and inspect CCD.
//! Run with the original static ARM toolchain when comparing retained hashes.
use engine_common::{
    Action, DirtMode, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario,
};
use engine_rapier::world::ccd_diagnostics;
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Input {
    tick: u64,
    actions: Vec<Action>,
}

fn argument(name: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == name)
        .unwrap_or_else(|| panic!("{name} required"))[1]
        .clone()
}

fn fnv64(bytes: &[u8]) -> String {
    format!(
        "{:016x}",
        bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
    )
}

fn main() {
    let seed: u64 = argument("--seed").parse().unwrap();
    let first: u64 = argument("--first").parse().unwrap();
    let last: u64 = argument("--last").parse().unwrap();
    let count_queries = std::env::args().any(|arg| arg == "--counts");
    assert!(first > 0 && last >= first && last <= 54_000);
    let inputs: Vec<Input> = BufReader::new(File::open(argument("--actions")).unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    let expected: BTreeMap<u64, Value> = BufReader::new(File::open(argument("--samples")).unwrap())
        .lines()
        .map(|line| {
            let sample: Value = serde_json::from_str(&line.unwrap()).unwrap();
            (sample["tick"].as_u64().unwrap(), sample)
        })
        .collect();
    let output = PathBuf::from(argument("--out"));
    fs::create_dir(&output).expect("use a fresh output directory");
    let mut costs = BufWriter::new(File::create(output.join("costs.jsonl")).unwrap());
    let mut audits = BufWriter::new(File::create(output.join("audits.jsonl")).unwrap());
    let mut state = SurfaceSortieScenario::init_material_match(seed);
    state.set_match_time_limit(Some(Duration::from_secs(900)));
    state.set_asteroid_pressure(MaterialAsteroidSettings {
        interval_seconds: 1,
        severity: MaterialAsteroidSeverity::Heavy,
    });
    state.enable_dirt_mode(DirtMode::Round);
    let initial_cells = state.terrain_diagnostics().occupied_cells;
    let mut checked = 0;
    let mut mismatches = 0;
    for input in inputs.iter().take(last as usize) {
        assert_eq!(input.tick, state.tick());
        if input.tick == first - 1 {
            fs::write(
                output.join("start-physics.bin"),
                state.ccd_physics_snapshot(),
            )
            .unwrap();
            fs::write(
                output.join("start-observation.json"),
                SurfaceSortieScenario::observe(&state).payload,
            )
            .unwrap();
        }
        let capture_counts = input.tick >= first - 1 && count_queries;
        if capture_counts {
            engine_rapier::profile::set_enabled(true);
        }
        let started = Instant::now();
        let mut step = || {
            SurfaceSortieScenario::step(
                &mut state,
                &input.actions,
                Duration::from_nanos(16_666_667),
            )
        };
        let (profile, lifecycle) = if capture_counts {
            let ((_, ccd), lifecycle) =
                scenario_spacewars::step_diagnostics::measure(|| ccd_diagnostics::measure(step));
            (Some(ccd), Some(lifecycle))
        } else {
            step();
            (None, None)
        };
        let elapsed = started.elapsed();
        let material_profile = capture_counts.then(|| {
            let timings = engine_rapier::profile::take();
            engine_rapier::profile::set_enabled(false);
            timings
        });
        if input.tick >= first - 1 {
            let metrics = state.last_step_metrics();
            serde_json::to_writer(
                &mut costs,
                &json!({
                    "tick":state.tick(), "step_ms":elapsed.as_secs_f64()*1000.0,
                    "physics_ms":metrics.physics_time.as_secs_f64()*1000.0,
                    "lifecycle_ms":metrics.lifecycle_time.as_secs_f64()*1000.0,
                    "narrow_phase_ms":metrics.rapier.narrow_phase_time.as_secs_f64()*1000.0,
                    "solver_ms":metrics.rapier.solver_time.as_secs_f64()*1000.0,
                    "ccd_ms":metrics.rapier.ccd_time.map(|d| d.as_secs_f64()*1000.0),
                    "substeps":metrics.rapier.ccd_substeps, "profile":profile, "lifecycle":lifecycle,
                    "material_profile":material_profile,
                    "physics_fnv64":fnv64(&state.ccd_physics_snapshot()),
                    "scenario_fnv64":fnv64(&SurfaceSortieScenario::observe(&state).payload),
                }),
            )
            .unwrap();
            writeln!(costs).unwrap();
        }
        if state.tick().is_multiple_of(60) || state.tick() == last {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "terrain audit: {:?}", audit.issues);
            assert_eq!(
                audit.occupied_cells + audit.loose_cells as u64 + audit.removed_cells,
                initial_cells
            );
            assert!(audit.max_speed < 500.0);
            let signature = fnv64(&SurfaceSortieScenario::observe(&state).payload);
            let retained = expected.get(&state.tick()).expect("retained audit missing");
            let matches = signature == retained["scenario_fnv64"].as_str().unwrap();
            checked += 1;
            mismatches += usize::from(!matches);
            serde_json::to_writer(
                &mut audits,
                &json!({
                    "tick":state.tick(),"scenario_fnv64":signature,"retained_match":matches,
                    "expected_scenario_fnv64":retained["scenario_fnv64"], "terrain":audit,
                }),
            )
            .unwrap();
            writeln!(audits).unwrap();
        }
        if state.tick().is_multiple_of(3000) {
            eprintln!(
                "tick={} checked={checked} mismatches={mismatches}",
                state.tick()
            );
        }
    }
    assert_eq!(state.tick(), last, "not enough input ticks");
    fs::write(
        output.join("final-physics.bin"),
        state.ccd_physics_snapshot(),
    )
    .unwrap();
    fs::write(
        output.join("final-observation.json"),
        SurfaceSortieScenario::observe(&state).payload,
    )
    .unwrap();
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "seed":seed,"first":first,"last":last,"checked":checked,"mismatches":mismatches,
        "count_queries":count_queries,
        "timing_scope":"opt-in diagnostic binary; query clocks and counters active only with --counts",
            "completed":true,
        }))
        .unwrap(),
    )
    .unwrap();
    costs.flush().unwrap();
    audits.flush().unwrap();
    assert_eq!(
        mismatches, 0,
        "fixed inputs did not reconstruct the retained physical case"
    );
}
