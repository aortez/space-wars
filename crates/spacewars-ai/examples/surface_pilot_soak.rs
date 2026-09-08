//! The real interactive pilot policy in a bounded, action-only endurance run.
//! cargo run -p spacewars-ai --example surface_pilot_soak --release --
//!   --seconds 180 --case 0 --seat 1 --seed 42 --out /tmp/pilot-soak
use engine_common::Scenario;
use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
use serde_json::json;
use spacewars_ai::{
    BrainReset,
    pilot::{PilotBrain, PilotGoal, RulePilotV1},
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[path = "../tests/support/surface_cases.rs"]
mod cases;

fn argument(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or(default.to_owned(), |p| p[1].clone())
}

fn main() {
    let seconds: u32 = argument("--seconds", "180").parse().unwrap();
    let seed: u64 = argument("--seed", "42").parse().unwrap();
    let seat: usize = argument("--seat", "0").parse().unwrap();
    let players: usize = argument("--players", "2").parse().unwrap();
    let case: usize = argument("--case", "0").parse().unwrap();
    assert!(
        (1..=180).contains(&seconds) && (1..=2).contains(&players) && seat < players && case < 4
    );
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state =
        SurfaceSortieScenario::init_material_flight(seed, players, &[(owner, cases::STARTS[case])]);
    let mut brain = RulePilotV1::new(BrainReset {
        actor: owner,
        episode_seed: seed,
    });
    let initial = state.terrain_diagnostics().occupied_cells;
    let output = PathBuf::from(argument("--out", "/tmp/surface-pilot-soak"));
    fs::create_dir_all(&output).unwrap();
    let mut ai_times = Vec::new();
    let mut step_times = Vec::new();
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut previous_goal = None;
    let mut peak_speed = 0.0_f32;
    let mut max_stall_ticks = 0;
    let mut blocked_ticks = 0;
    for tick in 0..seconds * 60 {
        let start = Instant::now();
        let observation = state.pilot_observation(seat, brain.site_request());
        let action = brain.intent(&observation);
        ai_times.push(start.elapsed().as_secs_f64() * 1000.0);
        if brain.telemetry().goal != PilotGoal::Complete {
            max_stall_ticks = max_stall_ticks.max(
                observation
                    .tick
                    .saturating_sub(brain.telemetry().last_progress_tick),
            );
        }
        blocked_ticks += u32::from(brain.telemetry().blocked_reason.is_some());
        if previous_goal != Some(brain.telemetry().goal) {
            eprintln!(
                "{:.2}s {:?} site {:?} {:?}",
                tick as f32 / 60.0,
                brain.telemetry().goal,
                brain.site_request(),
                brain.telemetry().blocked_reason
            );
            events.push(
                json!({"tick": tick, "brain": brain.telemetry(), "observation": observation}),
            );
            previous_goal = Some(brain.telemetry().goal);
        }
        let start = Instant::now();
        SurfaceSortieScenario::step(
            &mut state,
            &[action.encode(owner)],
            Duration::from_nanos(16_666_667),
        );
        step_times.push(start.elapsed().as_secs_f64() * 1000.0);
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "tick {tick}: {audit:?}");
            assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
            assert!(audit.max_speed < 500.0, "unbounded motion: {audit:?}");
            peak_speed = peak_speed.max(audit.max_speed);
            samples.push(json!({"second": (tick+1)/60, "audit": audit, "brain": brain.telemetry(), "pilot": observation, "action": format!("{action:?}")}));
        }
    }
    ai_times.sort_by(f64::total_cmp);
    step_times.sort_by(f64::total_cmp);
    let complete = brain.telemetry().completed_tick.is_some();
    let summary = json!({
        "schema": 1, "seed": seed, "players": players, "seat": seat, "case": case, "start": cases::STARTS[case], "seconds": seconds,
        "complete": complete, "brain": brain.telemetry(), "sampled_peak_speed": peak_speed,
        "max_stall_ticks": max_stall_ticks, "blocked_ticks": blocked_ticks,
        "ai_p95_ms": ai_times[(ai_times.len()-1)*95/100], "ai_max_ms": ai_times.last(),
        "step_p95_ms": step_times[(step_times.len()-1)*95/100], "step_max_ms": step_times.last(),
        "events": events, "samples": samples,
    });
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"report": output.join("report.json"), "complete": complete, "brain": brain.telemetry(), "step_p95_ms": summary["step_p95_ms"], "ai_p95_ms": summary["ai_p95_ms"]})
    );
    assert!(complete, "sortie incomplete; inspect report.json");
}
