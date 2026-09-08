//! Short Spacewars terrain benchmark. For endurance runs, use spacewars_terrain_soak.
use engine_common::Scenario;
use engine_core::{SpacewarsConfig, Vec2};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit};
use scenario_spacewars::{SpacewarsAction, SpacewarsScenario, SpacewarsState};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn count(state: &SpacewarsState) -> usize {
    (0..state.planets.len())
        .filter_map(|i| state.planet_terrain(i))
        .chain(state.terrain_fragments().map(|f| f.terrain()))
        .map(|t| {
            t.cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count()
        })
        .sum()
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).ceil() as usize]
}

fn main() {
    println!(
        "case,planets,initial_cells,cut_ms,step_p95_ms,step_max_ms,draw_p95_ms,draw_max_ms,peak_fragments,peak_contacts,cannon_hits,removed_cells"
    );
    let dt = Duration::from_secs_f64(1.0 / 60.0);
    for case in ["fixture", "tunnel", "blocks", "multi_planet"] {
        let mut state = if case == "multi_planet" {
            let mut state = SpacewarsScenario::init(
                SpacewarsConfig {
                    universe_radius: 1500,
                    asteroid_probability_per_sec: 0.0,
                    use_starfield: false,
                    ..SpacewarsConfig::default()
                },
                42,
            );
            for i in 0..state.planets.len() {
                state.enable_planet_terrain(i).unwrap();
            }
            state
        } else {
            SpacewarsScenario::init_terrain_fixture(42)
        };
        let initial = count(&state);
        for _ in 0..120 {
            SpacewarsScenario::step(&mut state, &[], dt);
        }
        if case == "tunnel" {
            let field = state.planet_terrain(0).unwrap();
            let side = field.width() as i32;
            state
                .queue_planet_edit(
                    0,
                    TerrainEdit {
                        brush: Brush::Capsule {
                            start: CellCoord::new(side / 2, 0),
                            end: CellCoord::new(side / 2, side),
                            radius: 14,
                        },
                        mode: EditMode::Remove,
                    },
                )
                .unwrap();
        } else if case == "blocks" {
            let side = state.planet_terrain(0).unwrap().width() as i32;
            for line in (12..side).step_by(12) {
                for (start, end) in [
                    (CellCoord::new(line, 0), CellCoord::new(line, side)),
                    (CellCoord::new(0, line), CellCoord::new(side, line)),
                ] {
                    state
                        .queue_planet_edit(
                            0,
                            TerrainEdit {
                                brush: Brush::Capsule {
                                    start,
                                    end,
                                    radius: 0,
                                },
                                mode: EditMode::Remove,
                            },
                        )
                        .unwrap();
                }
            }
        }
        let start = Instant::now();
        SpacewarsScenario::step(&mut state, &[], dt);
        let cut = start.elapsed().as_secs_f64() * 1000.0;
        let mut steps = Vec::new();
        let mut draws = Vec::new();
        let mut peak_fragments = state.terrain_fragments().count();
        let mut contacts = 0;
        for tick in 0..1200 {
            // Normal ship weapons run alongside the terrain workload. A death
            // or completed encounter may naturally stop producing new craters.
            let actions = [
                SpacewarsAction::set_cannon(0, tick < 600),
                SpacewarsAction::set_laser(1, tick < 600),
            ];
            let start = Instant::now();
            SpacewarsScenario::step(&mut state, &actions, dt);
            steps.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            black_box(SpacewarsScenario::render_raster_local_play_frames(
                &state, 1.4,
            ));
            draws.push(start.elapsed().as_secs_f64() * 1000.0);
            peak_fragments = peak_fragments.max(state.terrain_fragments().count());
            contacts = contacts.max(state.last_step_metrics.rapier.contact_pairs);
            assert!(
                state
                    .ships
                    .iter()
                    .all(|s| s.position.distance_to(Vec2::ZERO).is_finite())
            );
        }
        assert_eq!(
            state.tick, 1321,
            "benchmark must run every requested physics tick"
        );
        assert_eq!(
            initial,
            count(&state) + state.terrain_removed_cells() as usize
        );
        println!(
            "{case},{},{initial},{cut:.3},{:.3},{:.3},{:.3},{:.3},{peak_fragments},{contacts},{},{}",
            state.planets.len(),
            percentile(&mut steps, 0.95),
            percentile(&mut steps, 1.0),
            percentile(&mut draws, 0.95),
            percentile(&mut draws, 1.0),
            state.terrain_cannon_hits(),
            state.terrain_removed_cells()
        );
    }
}
