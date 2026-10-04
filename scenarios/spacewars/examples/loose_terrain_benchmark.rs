//! Ordinary material scenarios, real asteroid contacts, canonical actor physics.
//! Timing excludes diagnostic scans and render submission/rasterization.
use engine_common::{MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario};
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).ceil() as usize]
}

fn main() {
    println!(
        "scene,loose,steps,grains,fragments,blocked,removed,step_p95_ms,step_p99_ms,step_max_ms,frame_p95_ms"
    );
    for scene in ["combat", "match"] {
        for loose in [false, true] {
            let mut state = if scene == "combat" {
                SurfaceSortieScenario::init_material_combat(42)
            } else {
                SurfaceSortieScenario::init_material_match(42)
            };
            if loose {
                state.enable_loose_terrain();
            }
            state.set_asteroid_pressure(MaterialAsteroidSettings {
                interval_seconds: 1,
                severity: MaterialAsteroidSeverity::Heavy,
            });
            let initial = state.terrain_diagnostics().occupied_cells;
            let mut times = Vec::new();
            let mut frames = Vec::new();
            for tick in 0..3600 {
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
            if loose {
                assert!(audit.loose_cells > 0 && audit.loose_cells <= audit.loose_limit);
            }
            println!(
                "{scene},{loose},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3}",
                times.len(),
                audit.loose_cells,
                audit.fragments,
                audit.rejected_releases,
                audit.removed_cells,
                percentile(&mut times, 0.95),
                percentile(&mut times, 0.99),
                percentile(&mut times, 1.0),
                percentile(&mut frames, 0.95)
            );
        }
    }
}
