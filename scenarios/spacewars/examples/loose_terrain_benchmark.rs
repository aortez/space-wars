//! Ordinary material scenarios, real asteroid contacts, canonical actor physics.
//! Timing excludes diagnostic scans and render submission/rasterization.
use clap::{Parser, ValueEnum};
use engine_common::{MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario};
use engine_rapier::terrain::{GrainShape, LooseTerrainConfig};
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, ValueEnum)]
enum Scene {
    All,
    Combat,
    Match,
}
#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum Mode {
    All,
    Off,
    Round,
    Angular,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Off => "off",
            Self::Round => "round",
            Self::Angular => "angular",
        }
    }
}
#[derive(Parser)]
struct Args {
    #[arg(long, value_enum, default_value = "all")]
    scene: Scene,
    #[arg(long, value_enum, default_value = "all")]
    mode: Mode,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=180))]
    seconds: u32,
    #[arg(long, default_value_t = 192, value_parser = clap::value_parser!(u32).range(1..=4096))]
    limit: u32,
    /// Emit per-second material/settling diagnostics to stderr, outside timing.
    #[arg(long)]
    diagnostics: bool,
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).ceil() as usize]
}

fn main() {
    let args = Args::parse();
    println!(
        "scene,mode,seed,limit,steps,grains,fragments,blocked,removed,deposited,step_mean_ms,step_p50_ms,step_p95_ms,step_p99_ms,step_max_ms,frame_p95_ms,state_hash"
    );
    let scenes = match args.scene {
        Scene::All => vec!["combat", "match"],
        Scene::Combat => vec!["combat"],
        Scene::Match => vec!["match"],
    };
    let modes = match args.mode {
        Mode::All => vec![Mode::Off, Mode::Round, Mode::Angular],
        mode => vec![mode],
    };
    for scene in scenes {
        for &mode in &modes {
            let loose = mode != Mode::Off;
            let mut state = if scene == "combat" {
                SurfaceSortieScenario::init_material_combat(args.seed)
            } else {
                SurfaceSortieScenario::init_material_match(args.seed)
            };
            if loose {
                state
                    .enable_loose_terrain_with_config(LooseTerrainConfig {
                        shape: if mode == Mode::Angular {
                            GrainShape::Hexagon
                        } else {
                            GrainShape::Round
                        },
                        max_grains: args.limit as usize,
                        ..Default::default()
                    })
                    .unwrap();
            }
            state.set_asteroid_pressure(MaterialAsteroidSettings {
                interval_seconds: 1,
                severity: MaterialAsteroidSeverity::Heavy,
            });
            let initial = state.terrain_diagnostics().occupied_cells;
            let mut times = Vec::new();
            let mut frames = Vec::new();
            for tick in 0..u64::from(args.seconds) * 60 {
                if state.match_outcome().is_some() {
                    break;
                }
                let start = Instant::now();
                SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
                times.push(start.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(state.observation(0).tick, tick + 1);
                let start = Instant::now();
                black_box(SurfaceSortieScenario::render_frame(&state));
                frames.push(start.elapsed().as_secs_f64() * 1000.0);
                if tick % 60 == 0 {
                    let audit = state.terrain_diagnostics();
                    if args.diagnostics {
                        eprintln!(
                            "{}",
                            serde_json::json!({"scene":scene,"mode":mode.label(),
                            "tick":tick + 1,"grains":audit.loose_cells,"deposited":audit.deposited_cells,
                            "rejected":audit.rejected_releases,"settling":audit.settling})
                        );
                    }
                    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
                    assert_eq!(
                        initial,
                        audit.occupied_cells + audit.loose_cells as u64 + audit.removed_cells
                    );
                    if loose {
                        assert_eq!(audit.removed_cells, 0);
                    }
                }
            }
            let audit = state.terrain_diagnostics();
            assert!(audit.issues.is_empty(), "{:?}", audit.issues);
            assert_eq!(
                initial,
                audit.occupied_cells + audit.loose_cells as u64 + audit.removed_cells
            );
            if loose {
                assert_eq!(audit.loose_limit, args.limit as usize);
                assert!(audit.loose_cells <= audit.loose_limit);
                assert_eq!(audit.removed_cells, 0);
            }
            assert!(
                !times.is_empty(),
                "benchmark requires active simulation ticks"
            );
            let hash = SurfaceSortieScenario::observe(&state)
                .payload
                .iter()
                .fold(0xcbf29ce484222325_u64, |h, b| {
                    (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
                });
            println!(
                "{scene},{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{hash:016x}",
                mode.label(),
                args.seed,
                args.limit,
                times.len(),
                audit.loose_cells,
                audit.fragments,
                audit.rejected_releases,
                audit.removed_cells,
                audit.deposited_cells,
                times.iter().sum::<f64>() / times.len() as f64,
                percentile(&mut times, 0.5),
                percentile(&mut times, 0.95),
                percentile(&mut times, 0.99),
                percentile(&mut times, 1.0),
                percentile(&mut frames, 0.95)
            );
        }
    }
}
