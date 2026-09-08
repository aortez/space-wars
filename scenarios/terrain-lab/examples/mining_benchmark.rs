//! Mining with a dynamic spaceling and draw-list construction, without a display.
use std::hint::black_box;
use std::time::{Duration, Instant};

use engine_common::Scenario;
use scenario_terrain_lab::{
    FIXED_HZ, MiningControls, MiningTool, TerrainLabAction, TerrainLabConfig, TerrainLabScenario,
    TerrainView,
};

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).ceil() as usize]
}

fn main() {
    println!(
        "tool,radius,ticks,edit_ticks,step_p95_ms,step_max_ms,frame_build_p95_ms,frame_build_max_ms,peak_primitives,rock_cells,ore_cells,hash"
    );
    let dt = Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ));
    for radius in [20.0, 150.0] {
        for tool in MiningTool::ALL {
            let mut state = TerrainLabScenario::init(
                TerrainLabConfig {
                    radius,
                    ..Default::default()
                },
                42,
            );
            state.select_tool(tool);
            state.set_view(TerrainView::Detail);
            for _ in 0..120 {
                TerrainLabScenario::step(&mut state, &[], dt);
            }
            let mut steps = Vec::new();
            let mut frames = Vec::new();
            let mut edit_ticks = 0;
            let mut peak_primitives = 0;
            for tick in 0..600 {
                let actions = [TerrainLabAction::Mining(MiningControls {
                    held: tick < 480,
                    turn: if tick % 120 < 60 { 0.1 } else { -0.1 },
                    ..Default::default()
                })
                .encode()];
                let revision = state.terrain().revision();
                let started = Instant::now();
                TerrainLabScenario::step(&mut state, &actions, dt);
                steps.push(started.elapsed().as_secs_f64() * 1000.0);
                edit_ticks += usize::from(state.terrain().revision() != revision);
                let started = Instant::now();
                let frame = TerrainLabScenario::render_frame(&state);
                peak_primitives = peak_primitives.max(
                    frame
                        .layers
                        .iter()
                        .map(|layer| layer.primitives.len())
                        .sum::<usize>(),
                );
                black_box(frame);
                frames.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            assert!(
                state.recovered.rock_cells + state.recovered.ore_cells > 0,
                "fixture must actually mine material"
            );
            println!(
                "{tool:?},{radius},600,{edit_ticks},{:.3},{:.3},{:.3},{:.3},{peak_primitives},{},{},{:016x}",
                percentile(&mut steps, 0.95),
                percentile(&mut steps, 1.0),
                percentile(&mut frames, 0.95),
                percentile(&mut frames, 1.0),
                state.recovered.rock_cells,
                state.recovered.ore_cells,
                state.terrain_hash(),
            );
        }
    }
}
