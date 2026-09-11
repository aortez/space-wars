//! Action-driven Expedition journey and endurance audit on material ground.
//! Example: --seconds 180 --players 2 --seat 1 --seed 42 --out /tmp/surface-soak
use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceMiningAction, SurfaceSortieAction,
        SurfaceSortieScenario,
    },
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn argument(name: &str, default: &str) -> String {
    let args: Vec<_> = std::env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map_or(default.to_owned(), |pair| pair[1].clone())
}

fn main() {
    let seconds: u32 = argument("--seconds", "180").parse().expect("seconds");
    let seed: u64 = argument("--seed", "42").parse().expect("seed");
    let players: usize = argument("--players", "1").parse().expect("players");
    let seat: usize = argument("--seat", "0").parse().expect("seat");
    assert!((1..=180).contains(&seconds) && (1..=2).contains(&players) && seat < players);
    let output = PathBuf::from(argument("--out", "/tmp/spacewars-surface-soak"));
    fs::create_dir_all(&output).unwrap();
    let owner = PlayerId::from_index(seat).unwrap();
    let mut state = SurfaceSortieScenario::init_material(seed, players);
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut phase = 0_u8;
    let mut previous_phase = 0;
    let mut phase_ticks = 0_u32;
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut times = Vec::<f64>::new();
    let mut peak_speed = 0.0_f32;
    let mut max_fragments = 0;
    let dt = Duration::from_nanos(16_666_667);
    for tick in 0..seconds * 60 {
        let observation = state.observation(seat);
        let snapshot = state.spaceling_snapshot(seat);
        let mut control = SurfaceSortieAction::default();
        let mut mine = SurfaceMiningAction::default();
        match phase {
            0 => {
                if observation.landing.phase == LandingPhase::Landed && observation.controls_armed {
                    control.interact_held = true;
                    phase = 1;
                }
            }
            1 => {
                if observation.location == PilotLocation::OnFoot
                    && observation
                        .planet_claim
                        .as_ref()
                        .is_some_and(|c| c.owner == Some(owner))
                {
                    phase = 2;
                }
            }
            2 => {
                if observation
                    .planet_claim
                    .as_ref()
                    .is_some_and(|c| c.owner != Some(owner))
                {
                    phase = 3;
                }
                // Use the real aimed excavation input to remove our flag's footing.
                if let Some(flag) = observation
                    .planet_claim
                    .as_ref()
                    .filter(|c| c.owner == Some(owner))
                    .and_then(|c| c.flag)
                {
                    mine.aim = flag.position - observation.position;
                    mine.cycle = phase_ticks == 0;
                    mine.held = phase_ticks > 1;
                } else {
                    phase = 3;
                }
            }
            3 => {
                if observation
                    .planet_claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(owner))
                {
                    phase = 4;
                }
            }
            4 => {
                if !observation.ship_available {
                    phase = 5;
                } else if observation.controls_armed {
                    control.primary_held = true;
                    control.interact_held = true;
                    control.brake_held = true;
                }
            }
            5 => {
                if observation.ship_available {
                    phase = 6;
                }
            }
            6 => {
                if observation.location != PilotLocation::OnFoot {
                    phase = 7;
                } else if let Some(snapshot) = snapshot {
                    let offset = observation.access_position - snapshot.motion.position;
                    let right = engine_core::Vec2::new(snapshot.up.y, -snapshot.up.x);
                    let distance = offset.dot(right);
                    if distance.abs() > 0.8 {
                        control.horizontal =
                            distance.signum() * if distance.abs() > 2.0 { 1.0 } else { 0.45 };
                        // A mined cell leaves a ledge taller than the capsule's
                        // walking clearance. Jump over it using the same input
                        // a player needs, with a release between attempts.
                        control.primary_held = snapshot.grounded()
                            && snapshot.relative_speed.abs() < 0.5
                            && phase_ticks.is_multiple_of(60);
                    } else if observation.landing.phase == LandingPhase::Landed
                        && observation.controls_armed
                        && phase_ticks.is_multiple_of(15)
                    {
                        control.interact_held = true;
                    }
                }
            }
            7 => {
                control.primary_held = observation.controls_armed;
                if observation.landing.altitude > 25.0 {
                    phase = 8;
                }
            }
            _ => {
                control.brake_held = true;
            }
        }
        if matches!(phase, 1 | 3 | 5 | 6)
            && observation.balance == "KNOCKED DOWN"
            && phase_ticks.is_multiple_of(60)
        {
            control.primary_held = true;
        }
        if phase != previous_phase {
            events.push(json!({"tick": tick, "phase": phase, "observation": observation}));
            eprintln!("{:.2}s phase {phase}", f64::from(tick) / 60.0);
            phase_ticks = 0;
            previous_phase = phase;
        } else {
            phase_ticks += 1;
        }
        let actions = [control.encode(owner), mine.encode(owner)];
        let start = Instant::now();
        SurfaceSortieScenario::step(&mut state, &actions, dt);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        if tick % 60 == 59 {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "tick {tick}: {audit:?}");
            assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
            assert!(
                audit.max_speed < 60_000.0,
                "world-crossing motion at {tick}: {audit:?}"
            );
            peak_speed = peak_speed.max(audit.max_speed);
            max_fragments = max_fragments.max(audit.fragments);
            samples.push(
                json!({"second": (tick+1)/60, "phase": phase, "audit": audit,
                "pilot": state.observation(seat)}),
            );
        }
    }
    times.sort_by(f64::total_cmp);
    let final_state = state.observation(seat);
    let summary = json!({
        "schema": 1, "seconds": seconds, "seed": seed, "players": players, "seat": seat,
        "journey_complete": phase == 8, "phase": phase,
        "step_p95_ms": times[(times.len()-1)*95/100], "step_max_ms": times.last(),
        "sampled_peak_speed": peak_speed, "peak_fragments": max_fragments,
        "final": final_state, "events": events, "samples": samples,
    });
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"report": output.join("report.json"), "complete": phase == 8, "step_p95_ms": summary["step_p95_ms"], "step_max_ms": summary["step_max_ms"]})
    );
    assert_eq!(phase, 8, "journey incomplete; inspect report.json");
}
