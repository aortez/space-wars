//! Full separation, fragment collisions, and rendering workload, without a display.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use engine_common::Scenario;
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit};
use scenario_terrain_lab::{
    FIXED_HZ, TerrainLabAction, TerrainLabConfig, TerrainLabScenario, TerrainLabState,
};

fn occupied(state: &TerrainLabState) -> usize {
    std::iter::once(state.terrain())
        .chain(state.fragments().iter().map(|f| f.terrain()))
        .map(|terrain| {
            terrain
                .cells()
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
        "case,radius,fragments,cut_step_ms,connectivity_ms,step_p95_ms,step_max_ms,frame_p95_ms,frame_max_ms,colliders,cell_kib,hash"
    );
    let dt = Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ));
    for (case, radius) in [("equator", 20.0), ("blocks", 20.0), ("equator", 150.0)] {
        let mut state = TerrainLabScenario::init(
            TerrainLabConfig {
                radius,
                ..Default::default()
            },
            42,
        );
        // Preserve the separation-only baseline; impact_benchmark measures wear
        // and subsequent chain reactions with the same starting cuts.
        state.config.impacts.enabled = false;
        let initial_cells = occupied(&state);
        for _ in 0..120 {
            TerrainLabScenario::step(&mut state, &[], dt);
        }
        let side = state.terrain().width() as i32;
        let mut edits = Vec::new();
        let mut cut = |start, end, radius| {
            edits.push(
                TerrainLabAction::Edit(TerrainEdit {
                    brush: Brush::Capsule { start, end, radius },
                    mode: EditMode::Remove,
                })
                .encode(),
            )
        };
        if case == "equator" {
            cut(
                CellCoord::new(0, side / 2),
                CellCoord::new(side, side / 2),
                4,
            );
        } else {
            for line in (8..side).step_by(8) {
                cut(CellCoord::new(line, 0), CellCoord::new(line, side), 0);
                cut(CellCoord::new(0, line), CellCoord::new(side, line), 0);
            }
        }
        let started = Instant::now();
        TerrainLabScenario::step(&mut state, &edits, dt);
        let cut_step = started.elapsed().as_secs_f64() * 1000.0;
        let connectivity = state.last_edit.connectivity_time.as_secs_f64() * 1000.0;
        assert!(!state.fragments().is_empty());
        assert_eq!(
            initial_cells,
            occupied(&state) + state.removed_cells as usize
        );
        assert_eq!(state.recovered.rock_cells + state.recovered.ore_cells, 0);
        let mut steps = Vec::new();
        let mut frames = Vec::new();
        for _ in 0..600 {
            let started = Instant::now();
            TerrainLabScenario::step(&mut state, &[], dt);
            steps.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            black_box(TerrainLabScenario::render_frame(&state));
            frames.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "{case},{radius},{},{cut_step:.3},{connectivity:.3},{:.3},{:.3},{:.3},{:.3},{},{:.1},{:016x}",
            state.fragments().len(),
            percentile(&mut steps, 0.95),
            percentile(&mut steps, 1.0),
            percentile(&mut frames, 0.95),
            percentile(&mut frames, 1.0),
            state.collider_count(),
            state.terrain_cell_bytes() as f64 / 1024.0,
            state.terrain_hash()
        );
    }
}
