//! Real cannon -> bank failure -> clump impact on a rotating material planet.
//! Audits and replay checkpoints are outside the whole-scenario step timer.
use clap::{Parser, ValueEnum};
use engine_common::{DirtMode, Scenario};
use scenario_spacewars::{SpacewarsAction, SpacewarsScenario, SpacewarsState};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, ValueEnum)]
enum Shape {
    Round,
    Angular,
}
#[derive(Clone, Copy, ValueEnum)]
enum Strength {
    Off,
    Weak,
    Strong,
}
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=180))]
    seconds: u32,
    #[arg(long, value_enum, default_value = "angular")]
    shape: Shape,
    #[arg(long, value_enum, default_value = "weak")]
    strength: Strength,
    #[arg(long)]
    replay: bool,
    #[arg(long)]
    diagnostics: bool,
    /// Emit inclusive shared-stage timings for each tick, outside the timer.
    #[arg(long)]
    profile: bool,
    /// Save quiet packing groups outside the step timer for the packing inspector.
    #[arg(long)]
    packing_dir: Option<std::path::PathBuf>,
    /// Capture after this many ticks (defaults to the end of the run).
    #[arg(long, requires = "packing_dir", value_parser = clap::value_parser!(u32).range(1..))]
    packing_tick: Option<u32>,
    /// Fail rather than report a timing run that did not exercise the lifecycle.
    #[arg(long)]
    require_lifecycle: bool,
    /// Fire at 0, 8 and 16 seconds into an initially filled, uneven curved planet.
    #[arg(long)]
    curved: bool,
    #[arg(long, conflicts_with = "curved")]
    roof: bool,
    #[arg(long, requires = "roof")]
    require_roof: bool,
    /// Require repeated cannon hits and an inspected undercut (which may hold).
    #[arg(long, requires = "curved")]
    require_undercut: bool,
}
fn advance(state: &mut SpacewarsState, tick: u32, curved: bool) {
    let firing = tick == 0 || (curved && [480, 960].contains(&tick));
    let releasing = tick == 1 || (curved && [481, 961].contains(&tick));
    let actions = if firing {
        vec![SpacewarsAction::set_cannon(0, true)]
    } else if releasing {
        vec![SpacewarsAction::set_cannon(0, false)]
    } else {
        vec![]
    };
    SpacewarsScenario::step(state, &actions, Duration::from_secs_f64(1.0 / 60.0));
}
fn hash(state: &SpacewarsState) -> u64 {
    SpacewarsScenario::observe(state)
        .payload
        .iter()
        .fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
}
fn main() {
    let args = Args::parse();
    let capture_tick = args.packing_tick.unwrap_or(args.seconds * 60);
    assert!(
        capture_tick <= args.seconds * 60,
        "capture is past the end of the run"
    );
    let mode = match (args.strength, args.shape) {
        (Strength::Off, Shape::Round) => DirtMode::Round,
        (Strength::Off, Shape::Angular) => DirtMode::Angular,
        (Strength::Weak, Shape::Round) => DirtMode::WeakRound,
        (Strength::Weak, Shape::Angular) => DirtMode::WeakAngular,
        (Strength::Strong, Shape::Round) => DirtMode::StrongRound,
        (Strength::Strong, Shape::Angular) => DirtMode::StrongAngular,
    };
    let init = || {
        if args.roof {
            SpacewarsScenario::init_roof_fixture(args.seed, mode)
        } else if args.curved {
            SpacewarsScenario::init_curved_destruction_fixture(args.seed, mode)
        } else {
            SpacewarsScenario::init_destruction_fixture(args.seed, mode)
        }
    };
    let mut state = init();
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut checkpoints = Vec::new();
    let mut times = Vec::new();
    let mut peak_loose = 0;
    let mut peak_fragments = 0;
    engine_rapier::profile::set_enabled(args.profile);
    for tick in 0..args.seconds * 60 {
        let start = Instant::now();
        advance(&mut state, tick, args.curved);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        if args.profile {
            let pool = state.loose_terrain().unwrap();
            let physics = state.last_step_metrics.rapier;
            eprintln!(
                "{}",
                serde_json::json!({
                    "tick":tick+1, "total_ms":times.last(),
                    "profile":engine_rapier::profile::take(),
                    "loose":pool.len(), "cohesion":pool.cohesion_diagnostics(),
                    "impact_fracture":pool.impact_diagnostics(),
                    "physics":{
                        "wall_ms":physics.wall_time.as_secs_f64()*1000.0,
                        "rapier_ms":physics.rapier_step_time.as_secs_f64()*1000.0,
                        "update_ms":physics.update_time.as_secs_f64()*1000.0,
                        "user_changes_ms":physics.user_changes_time.as_secs_f64()*1000.0,
                        "collision_ms":physics.collision_detection_time.as_secs_f64()*1000.0,
                        "broad_phase_ms":physics.broad_phase_time.as_secs_f64()*1000.0,
                        "narrow_phase_ms":physics.narrow_phase_time.as_secs_f64()*1000.0,
                        "solver_ms":physics.solver_time.as_secs_f64()*1000.0,
                        "ccd_ms":physics.ccd_time.map(|time|time.as_secs_f64()*1000.0),
                        "ccd_first_impact_ms":physics.ccd_first_impact_time.as_secs_f64()*1000.0,
                        "ccd_clamping_ms":physics.ccd_clamping_time.as_secs_f64()*1000.0,
                        "ccd_substeps":physics.ccd_substeps,
                        "contacts":physics.contacts,"candidate_pairs":physics.candidate_pairs,
                    },
                })
            );
        }
        if tick + 1 == capture_tick
            && let Some(directory) = &args.packing_dir
        {
            // Capture builds a support graph of its own. Keep its spans out
            // of the following tick's profile as well as the step timer.
            engine_rapier::profile::set_enabled(false);
            let before = hash(&state);
            let motion = state.terrain_diagnostics().motion_hash;
            std::fs::create_dir_all(directory).unwrap();
            for (i, snapshot) in state.capture_loose_packing().unwrap().iter().enumerate() {
                std::fs::write(
                    directory.join(format!("group-{i}.packing")),
                    snapshot.to_bytes().unwrap(),
                )
                .unwrap();
            }
            assert_eq!(hash(&state), before, "capture changed the observation");
            assert_eq!(
                state.terrain_diagnostics().motion_hash,
                motion,
                "capture changed motion"
            );
            engine_rapier::profile::set_enabled(args.profile);
        }
        // Full audits are deliberately sampled outside timing.
        if (tick + 1) % 60 == 0 {
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "{:?}", audit.issues);
            assert_eq!(initial, audit.occupied_cells + audit.loose_cells as u64);
            assert_eq!(audit.removed_cells, 0);
            peak_loose = peak_loose.max(audit.loose_cells);
            peak_fragments = peak_fragments.max(audit.fragments);
            checkpoints.push((hash(&state), audit.motion_hash.clone()));
            if args.diagnostics {
                eprintln!("{}", serde_json::json!({"tick":tick+1,"audit":audit}));
            }
        }
    }
    engine_rapier::profile::set_enabled(false);
    let audit = state.terrain_diagnostics();
    if args.require_roof {
        assert!(audit.cannon_hits > 0, "ordinary cannon must hit");
        assert!(audit.cohesion.roof_failures > 0, "roof must crack");
        assert!(
            audit.cohesion.failures > audit.cohesion.roof_failures,
            "remaining ledge must fail"
        );
        assert!(audit.impact_fracture.failures > 0, "clump must chip");
    }
    if args.require_lifecycle {
        assert!(audit.cannon_hits > 0, "ordinary cannon must hit");
        assert!(audit.cohesion.failures > 0, "bank must fail");
        assert!(audit.impact_fracture.failures > 0, "clump must chip");
        assert!(audit.deposited_cells > 0, "some dirt must return");
    }
    if args.require_undercut {
        assert!(
            audit.cannon_hits >= 3,
            "ordinary cannon must hit repeatedly"
        );
        assert!(
            audit.cohesion.irregular_held_checks + audit.cohesion.irregular_failures > 0,
            "an irregular undercut must be inspected"
        );
    }
    if args.replay {
        let mut replay = init();
        for tick in 0..args.seconds * 60 {
            advance(&mut replay, tick, args.curved);
            if (tick + 1) % 60 == 0 {
                assert_eq!(
                    (hash(&replay), replay.terrain_diagnostics().motion_hash),
                    checkpoints[(tick / 60) as usize],
                    "replay at {}",
                    tick + 1
                );
            }
        }
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "workload":if args.roof { "spacewars-cannon-roof-v1" } else if args.curved { "spacewars-curved-cannon-v1" } else { "spacewars-cannon-quarry-v2" }, "mode":mode,
            "seed":args.seed,"ticks":args.seconds*60,"initial_cells":initial,
            "mean_ms":times.iter().sum::<f64>()/times.len() as f64,
            "p95_ms":times[times.len()*95/100],"max_ms":times.last(),
            "sampled_peak_loose":peak_loose,"sampled_peak_fragments":peak_fragments,
            "observation_hash":format!("{:016x}",hash(&state)), "replay_checked":args.replay,
            "audit":audit,
        })
    );
}
