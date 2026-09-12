//! Fixed edits on copies of the generated match's fields. Measures geometry
//! independently of bot trajectories, contacts and fragment motion.
use engine_core::Vec2;
use engine_terrain::{
    Brush, EditMode, MaterialId, Terrain, TerrainEdit, TerrainGeometry, TerrainSurface,
};
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

fn stats(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    let p = |n: usize| values[(values.len() * n / 100).min(values.len() - 1)];
    json!({"count":values.len(), "mean_ms":values.iter().sum::<f64>() / values.len() as f64,
        "p50_ms":p(50), "p95_ms":p(95), "p99_ms":p(99), "max_ms":values.last()})
}

fn metrics(field: &Terrain, geometry: &TerrainGeometry) -> serde_json::Value {
    let mut hash = 0xcbf29ce484222325_u64;
    for cell in field.cells() {
        for byte in [cell.material.0, cell.durability] {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    json!({"cell_bytes":field.cell_bytes(), "sample_bytes":field.surface_sample_bytes(),
        "occupied_cells":field.cells().iter().filter(|c|c.material != MaterialId::VOID).count(),
        "material_hash":format!("{hash:016x}"), "shapes":geometry.shape_count(),
        "rectangles":geometry.rectangle_count(),
        "polygon_vertices":geometry.chunks().iter().flat_map(|c|&c.polygons).map(|p|p.vertices.len()).sum::<usize>()})
}

fn main() {
    let seed: u64 = arg("--seed", "42").parse().unwrap();
    let repeats: usize = arg("--repeats", "3").parse().unwrap();
    assert!((1..=20).contains(&repeats));
    let out = PathBuf::from(arg("--out", "/tmp/surface-geometry"));
    fs::create_dir(&out).expect("new output directory");
    let state = SurfaceSortieScenario::init_material_arena(seed);
    let world = state.mission_observation(0, None);
    let mut reports = Vec::new();
    for (planet, observation) in world.planets.iter().enumerate() {
        let original = state.planet_terrain(planet).unwrap();
        let surface = if original.surface_sample_bytes() == 0 {
            TerrainSurface::Blocks
        } else {
            TerrainSurface::Interpolated
        };
        for repeat in 0..repeats {
            let mut field = original.clone();
            let clock = Instant::now();
            let mut geometry = TerrainGeometry::with_surface(&field, surface);
            let initial_build_ms = clock.elapsed().as_secs_f64() * 1000.0;
            let initial = metrics(&field, &geometry);
            let mut edit_times = Vec::new();
            let mut refresh_times = Vec::new();
            let mut samples = Vec::new();
            for cut in 0..60 {
                let angle = cut as f32 * 2.399_963_1;
                let radius = observation.radius * 0.99;
                let center = field
                    .local_to_cell(
                        Vec2::new(angle.cos(), angle.sin())
                            * radius
                            * if cut < 40 { 0.97 } else { 0.55 },
                    )
                    .unwrap();
                let brush = if cut < 40 {
                    Brush::Circle {
                        center,
                        radius: [0, 1, 3, 6][cut % 4],
                    }
                } else {
                    let end = field.local_to_cell(-field.cell_center(center)).unwrap();
                    Brush::Capsule {
                        start: center,
                        end,
                        radius: 1,
                    }
                };
                let clock = Instant::now();
                let edit = field
                    .apply(TerrainEdit {
                        brush,
                        mode: EditMode::Remove,
                    })
                    .unwrap();
                edit_times.push(clock.elapsed().as_secs_f64() * 1000.0);
                let clock = Instant::now();
                let rebuilt = geometry.refresh(&field);
                refresh_times.push(clock.elapsed().as_secs_f64() * 1000.0);
                assert!(geometry.is_current(&field));
                let covered: u64 = geometry.chunks().iter().map(|c| c.material_cells()).sum();
                assert_eq!(
                    covered,
                    field
                        .cells()
                        .iter()
                        .filter(|c| c.material != MaterialId::VOID)
                        .count() as u64
                );
                samples.push(
                    json!({"cut":cut, "changed_cells":edit.changed_cells, "rebuilt_chunks":rebuilt.len(),
                    "geometry":metrics(&field,&geometry)}),
                );
            }
            reports.push(
                json!({"planet":planet,"repeat":repeat,"surface":format!("{surface:?}"),
                "radius":observation.radius,"initial_build_ms":initial_build_ms,"initial":initial,
                "edits":stats(edit_times),"refresh":stats(refresh_times),"samples":samples}),
            );
        }
    }
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&json!({
        "version":1,"seed":seed,"repeats":repeats,"reports":reports,
        "scope":"field edits and geometry refresh only; no physics, connectivity splitting or rendering"
    })).unwrap()).unwrap();
}
