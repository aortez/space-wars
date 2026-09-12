//! Identical moving terrain and cuts, without bots, gravity, actors or splitting.
use engine_core::Vec2;
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainColliders, TerrainSpec},
    world::{BodyKind, BodySpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig},
};
use engine_terrain::{Brush, CellCoord, EditMode, TerrainEdit, TerrainGeometry, TerrainSurface};
use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};

fn arg(name: &str, default: &str) -> String {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|p| p[0] == name)
        .map_or_else(|| default.to_owned(), |p| p[1].clone())
}
fn timing(mut samples: Vec<f64>) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    json!({"count":samples.len(), "mean_ms":samples.iter().sum::<f64>() / samples.len() as f64,
        "p95_ms":samples[samples.len()*95/100], "max_ms":samples.last()})
}
fn main() {
    let seed: u64 = arg("--seed", "3121799525250095703").parse().unwrap();
    let seconds: usize = arg("--seconds", "60").parse().unwrap();
    assert!((10..=180).contains(&seconds));
    let out = PathBuf::from(arg("--out", "/tmp/terrain-colliders"));
    fs::create_dir(&out).expect("fresh output directory");
    let mut reports = Vec::new();
    for surface in [TerrainSurface::Blocks, TerrainSurface::Interpolated] {
        let source =
            SurfaceSortieScenario::init_material_arena_surface_trial(seed, false, 0.0, surface);
        let fields: Vec<_> = (0..3)
            .map(|i| source.planet_terrain(i).unwrap().clone())
            .collect();
        drop(source);
        let mut reference = None;
        for layout in [TerrainColliders::Separate, TerrainColliders::ChunkCompound] {
            let mut world = PhysicsWorld::new(PhysicsWorldConfig {
                gravity: Vec2::ZERO,
                length_unit: 10.0,
                solver_iterations: 8,
                internal_stabilization_iterations: 2,
                max_ccd_substeps: 4,
                collect_events: true,
            });
            let mut terrain: Vec<_> = fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let geometry = TerrainGeometry::with_surface(field, surface);
                    let assembly = TerrainAssembly::insert(
                        &mut world,
                        PhysicsId::new(i as u64 + 1),
                        BodySpec {
                            kind: BodyKind::KinematicPosition,
                            position: Vec2::new(i as f32 * 1000.0, 20.0),
                            can_sleep: false,
                            ..Default::default()
                        },
                        field,
                        &geometry,
                        TerrainSpec {
                            surface,
                            colliders: layout,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    (field.clone(), geometry, assembly)
                })
                .collect();
            let initial_colliders = world.collider_count();
            let mut steps = Vec::new();
            let mut broad = Vec::new();
            let mut sync = Vec::new();
            let mut edits = Vec::new();
            let mut candidates = 0usize;
            let mut max_pose_error = 0.0_f32;
            for tick in 0..seconds * 60 {
                if tick > 0 && tick % 90 == 0 {
                    let cut = tick / 90 - 1;
                    let index = cut % terrain.len();
                    let (field, geometry, assembly) = &mut terrain[index];
                    let angle = cut as f32 * 2.399_963_1;
                    let radius =
                        field.width().min(field.height()) as f32 * field.cell_size() * 0.44;
                    let center = field
                        .local_to_cell(Vec2::from_radians(angle) * radius)
                        .unwrap();
                    field
                        .apply(TerrainEdit {
                            brush: if cut % 4 == 3 {
                                Brush::Capsule {
                                    start: center,
                                    end: CellCoord::new(
                                        field.width() as i32 / 2,
                                        field.height() as i32 / 2,
                                    ),
                                    radius: 1,
                                }
                            } else {
                                Brush::Circle {
                                    center,
                                    radius: [0, 1, 3][cut % 4],
                                }
                            },
                            mode: EditMode::Remove,
                        })
                        .unwrap();
                    geometry.refresh(field);
                    let clock = Instant::now();
                    assembly.synchronize(&mut world, field, geometry).unwrap();
                    sync.push(clock.elapsed().as_secs_f64() * 1000.0);
                    edits.push(json!({"tick":tick, "planet":index, "hash":field.hash(),
                        "material_cells":geometry.chunks().iter().map(|c|c.material_cells()).sum::<u64>(),
                        "geometry_shapes":geometry.shape_count()}));
                }
                for (i, (_, _, assembly)) in terrain.iter().enumerate() {
                    let t = (tick + 1) as f32 / 60.0;
                    world.set_next_kinematic_pose(
                        assembly.body(),
                        Vec2::new(i as f32 * 1000.0 + 20.0 * t.sin(), 20.0 * t.cos()),
                        t * (0.03 + i as f32 * 0.02),
                    );
                }
                let clock = Instant::now();
                let metrics = world.step(1.0 / 60.0);
                steps.push(clock.elapsed().as_secs_f64() * 1000.0);
                broad.push(
                    (metrics.broad_phase_time + metrics.final_broad_phase_time).as_secs_f64()
                        * 1000.0,
                );
                candidates += metrics.candidate_pairs;
                assert_eq!(metrics.contact_pairs, 0);
                for (i, (_, _, assembly)) in terrain.iter().enumerate() {
                    let t = (tick + 1) as f32 / 60.0;
                    let expected = Vec2::new(i as f32 * 1000.0 + 20.0 * t.sin(), 20.0 * t.cos());
                    let motion = world.motion(assembly.body()).unwrap();
                    max_pose_error = max_pose_error.max(motion.position.distance_to(expected));
                    assert!(max_pose_error < 0.002, "prescribed pose drift");
                }
            }
            let final_state: Vec<_> = terrain
                .iter()
                .map(|(field, geometry, _)| {
                    assert!(geometry.is_current(field));
                    json!({"hash":field.hash()})
                })
                .collect();
            let equal_work = json!({"edits":edits, "final":final_state});
            if let Some(reference) = &reference {
                assert_eq!(reference, &equal_work);
            } else {
                reference = Some(equal_work.clone());
            }
            reports.push(
                json!({"surface":format!("{surface:?}"), "layout":format!("{layout:?}"),
                "max_prescribed_pose_error":max_pose_error,
                "initial_colliders":initial_colliders, "final_colliders":world.collider_count(),
                "mean_candidate_pairs":candidates as f64 / steps.len() as f64,
                "steps":timing(steps), "broad_phase":timing(broad), "synchronize":timing(sync),
                "work":equal_work}),
            );
        }
    }
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&json!({"version":1,
        "seed":seed, "seconds":seconds, "equal_work":true, "cases":reports,
        "scope":"identical prescribed moving planets and material cuts; no actors, gravity, AI, connectivity splitting or rendering; steps include backend metrics and event collection; synchronize excludes material edit and geometry refresh"})).unwrap()).unwrap();
}
