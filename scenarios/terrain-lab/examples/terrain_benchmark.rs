//! Headless repeated-edit fixture. Run in release on desktop or the Pi.
use std::time::Instant;

use engine_core::Vec2;
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainSpec},
    world::{BodyKind, BodySpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig},
};
use engine_terrain::{Brush, CellCoord, EditMode, TerrainEdit, TerrainGeometry};
use scenario_terrain_lab::{TerrainLabConfig, generate_planet};

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).ceil() as usize]
}

fn main() {
    println!(
        "cell_size,workload,cells,cell_kib,initial_quads,generation_ms,initial_geometry_ms,initial_physics_ms,edit_p95_ms,geometry_p95_ms,collider_p95_ms,step_p95_ms,commit_max_ms,final_quads,hash"
    );
    for cell_size in [1.0, 0.5] {
        for workload in ["craters", "tunnels"] {
            let config = TerrainLabConfig {
                radius: 150.0,
                cell_size,
                ..Default::default()
            };
            let started = Instant::now();
            let mut terrain = generate_planet(config, 42).unwrap();
            let generation = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let mut geometry = TerrainGeometry::new(&terrain);
            let generation_geometry = started.elapsed().as_secs_f64() * 1000.0;
            let initial_quads = geometry.rectangle_count();
            let mut world = PhysicsWorld::new(PhysicsWorldConfig {
                gravity: Vec2::ZERO,
                ..Default::default()
            });
            let started = Instant::now();
            let mut assembly = TerrainAssembly::insert(
                &mut world,
                PhysicsId::new(1),
                BodySpec {
                    kind: BodyKind::KinematicPosition,
                    ..Default::default()
                },
                &terrain,
                &geometry,
                TerrainSpec::default(),
            )
            .unwrap();
            let initial_physics = started.elapsed().as_secs_f64() * 1000.0;
            world.step(1.0 / 60.0);
            let mut edits = Vec::new();
            let mut meshes = Vec::new();
            let mut colliders = Vec::new();
            let mut steps = Vec::new();
            let mut commits = Vec::new();
            let side = terrain.width() as i32;
            for index in 0..80 {
                let brush = if workload == "craters" {
                    Brush::Circle {
                        center: CellCoord::new(
                            (index * 97 + side / 3) % side,
                            (index * 71 + side / 2) % side,
                        ),
                        radius: (5.0 / cell_size) as u32,
                    }
                } else {
                    let y = (side / 5 + index * 7) % side;
                    Brush::Capsule {
                        start: CellCoord::new(-1, y),
                        end: CellCoord::new(side, y + 10),
                        radius: (1.5 / cell_size).ceil() as u32,
                    }
                };
                let commit_started = Instant::now();
                let started = Instant::now();
                terrain
                    .apply(TerrainEdit {
                        brush,
                        mode: EditMode::Remove,
                    })
                    .unwrap();
                edits.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                geometry.refresh(&terrain);
                meshes.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                assembly
                    .synchronize(&mut world, &terrain, &geometry)
                    .unwrap();
                colliders.push(started.elapsed().as_secs_f64() * 1000.0);
                commits.push(commit_started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                world.set_next_kinematic_pose(
                    assembly.body(),
                    Vec2::new(index as f32 * 0.01, 0.0),
                    index as f32 * 0.001,
                );
                world.step(1.0 / 60.0);
                steps.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            println!(
                "{cell_size},{workload},{},{:.1},{initial_quads},{generation:.3},{generation_geometry:.3},{initial_physics:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{:016x}",
                terrain.cells().len(),
                terrain.cell_bytes() as f64 / 1024.0,
                percentile(&mut edits, 0.95),
                percentile(&mut meshes, 0.95),
                percentile(&mut colliders, 0.95),
                percentile(&mut steps, 0.95),
                percentile(&mut commits, 1.0),
                geometry.rectangle_count(),
                terrain.hash()
            );
        }
    }
}
