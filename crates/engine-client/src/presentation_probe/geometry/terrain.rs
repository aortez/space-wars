//! Production terrain artwork, prescribed poses/edits, no simulation or scanout.
//! Source construction and revision-to-definition replacement are timed. Field
//! editing/collision reconstruction and validation/readback are deliberately not.
use super::{Context, Error, Stamp, save_png};
use crate::{
    gpu_scene::{
        TrianglePresenter, plan,
        retained::{Definition, Placement},
    },
    presentation_probe::{Options, geometry},
    raster::{RasterOptions, RasterRenderer},
    render::{FrameLayout, Viewport},
};
use engine_common::{
    Camera2, RenderCircle, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
};
use engine_core::Vec2;
use engine_terrain::{
    Brush, CellCoord, EditMode, Terrain, TerrainEdit, TerrainGeometry, TerrainSurface,
};
use glow::HasContext;
use scenario_spacewars::{SpacewarsScenario, terrain_art};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "churn.rs"]
mod churn;
// Reuse the exact prescribed terrain histories in the queued stream lab.
pub(super) use churn::StreamSource;
pub(super) fn run_churn(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    churn::run(context, options, width, height)
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    bodies: usize,
    contour: bool,
    zoom: bool,
    edits: bool,
    visibility: bool,
}
const CASES: [Case; 4] = [
    Case {
        name: "terrain-blocks",
        bodies: 1,
        contour: false,
        zoom: false,
        edits: false,
        visibility: false,
    },
    Case {
        name: "terrain-contour",
        bodies: 4,
        contour: true,
        zoom: false,
        edits: false,
        visibility: false,
    },
    Case {
        name: "terrain-zoom",
        bodies: 2,
        contour: true,
        zoom: true,
        edits: false,
        visibility: false,
    },
    Case {
        name: "terrain-edits",
        bodies: 2,
        contour: true,
        zoom: false,
        edits: true,
        visibility: false,
    },
];
#[derive(Clone, Copy)]
enum Mode {
    Ordinary,
    Cold,
    Retained,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Ordinary => "triangles",
            Self::Cold => "chunks-cold",
            Self::Retained => "chunks-retained",
        }
    }
    fn reuse(self) -> bool {
        !matches!(self, Self::Ordinary)
    }
}
struct Snapshot {
    epoch: u64,
    field: Terrain,
    geometry: TerrainGeometry,
}
#[derive(Default)]
struct Art {
    epoch: Option<u64>,
    chunks: BTreeMap<u32, (u64, Arc<Definition>)>,
}
impl Art {
    fn definition(
        &mut self,
        snapshot: &Snapshot,
        index: usize,
        rebuilt: &mut u64,
    ) -> Result<Arc<Definition>, String> {
        if self.epoch != Some(snapshot.epoch) {
            self.chunks.clear();
            self.epoch = Some(snapshot.epoch);
        }
        let chunk = &snapshot.geometry.chunks()[index];
        // generation, not revision: neighboring edits can change this chunk's
        // derived contour even when its own cells did not change.
        if self
            .chunks
            .get(&chunk.id.0)
            .is_none_or(|(g, _)| *g != chunk.generation)
        {
            let polygons = terrain_art::polygons(&snapshot.field, chunk, Vec2::ZERO, 0.).collect();
            self.chunks.insert(
                chunk.id.0,
                (chunk.generation, Arc::new(Definition::new(polygons)?)),
            );
            *rebuilt += 1;
        }
        Ok(self.chunks[&chunk.id.0].1.clone())
    }
}
struct Producer {
    case: Case,
    bodies: Vec<Vec<Snapshot>>,
    art: Vec<Art>,
}
impl Producer {
    fn new(case: Case) -> Result<Self, Error> {
        let mut bodies = Vec::new();
        for body in 0..case.bodies {
            let surface = if case.contour {
                TerrainSurface::Contour
            } else {
                TerrainSurface::Blocks
            };
            let base = SpacewarsScenario::init_terrain_fixture(42 + body as u64)
                .planet_terrain(0)
                .unwrap()
                .clone();
            let mut field = base.clone();
            let mut geometry = TerrainGeometry::with_surface(&field, surface);
            let mut sequence = vec![Snapshot {
                epoch: 1,
                field: field.clone(),
                geometry: geometry.clone(),
            }];
            for center in [CellCoord::new(64, 64), CellCoord::new(31, 60)] {
                field.apply(TerrainEdit {
                    brush: Brush::Circle { center, radius: 6 },
                    mode: EditMode::Remove,
                })?;
                geometry.refresh(&field);
                sequence.push(Snapshot {
                    epoch: 1,
                    field: field.clone(),
                    geometry: geometry.clone(),
                });
            }
            // A different world resets all revision counters; equal chunk IDs
            // and revision zero MUST NOT reuse the first world's material art.
            let replacement = SpacewarsScenario::init_terrain_fixture(8192 + body as u64)
                .planet_terrain(0)
                .unwrap()
                .clone();
            sequence.push(Snapshot {
                epoch: 2,
                geometry: TerrainGeometry::with_surface(&replacement, surface),
                field: replacement,
            });
            bodies.push(sequence);
        }
        Ok(Self {
            case,
            bodies,
            art: (0..case.bodies).map(|_| Art::default()).collect(),
        })
    }

    fn generate(
        &mut self,
        index: usize,
        viewport: Viewport,
        reuse: bool,
    ) -> Result<(RenderFrame, Vec<Placement>, u64), Error> {
        let phase = index as f32 * 0.07;
        let height = if self.case.bodies == 1 { 140. } else { 280. }
            * if self.case.zoom && self.case.visibility {
                1. + 0.12 * (index as f32 * 0.04).sin()
            } else if self.case.zoom && index % 32 >= 16 {
                0.8
            } else {
                1.
            };
        let camera = Camera2::new(RenderPoint::new(phase.sin() * 4., phase.cos() * 3.), height);
        let mut frame = RenderFrame::new(camera);
        let mut groups = Vec::new();
        let mut rebuilt = 0;
        frame.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(-60., 0.),
                4.,
                RenderColor::RED,
            )),
        );
        let mut first = 1;
        for body in 0..self.case.bodies {
            let stride = if self.case.visibility { 16 } else { 8 };
            let stage = if self.case.edits {
                index / stride % 4
            } else {
                0
            };
            let snapshot = &self.bodies[body][stage];
            let position = if self.case.bodies == 1 {
                Vec2::ZERO
            } else {
                Vec2::new(
                    if body % 2 == 0 { -64. } else { 64. },
                    if self.case.bodies <= 2 {
                        0.
                    } else if body < 2 {
                        -64.
                    } else {
                        64.
                    },
                )
            };
            let angle = phase * 0.17 + body as f32 * 0.4;
            for (i, chunk) in snapshot.geometry.chunks().iter().enumerate() {
                if chunk.shape_count() == 0 {
                    continue;
                }
                // Prescribed visibility, not a culling optimization: reveal
                // both halves again after hiding alternating chunk sets.
                if self.case.visibility && matches!((index / 16 % 4, i % 2), (1, 0) | (2, 1)) {
                    continue;
                }
                if reuse {
                    let definition = self.art[body].definition(snapshot, i, &mut rebuilt)?;
                    let p = camera.world_to_viewport(
                        RenderPoint::new(position.x, position.y),
                        viewport.aspect_ratio(),
                    );
                    groups.push(Placement {
                        first,
                        definition,
                        scale: viewport.height / height,
                        angle,
                        translation: RenderPoint::new(p.x * viewport.width, p.y * viewport.height),
                    });
                }
                // This calls the actual producer in BOTH paths and remains
                // inside timing. No free world-point construction for the cache.
                for polygon in terrain_art::polygons(&snapshot.field, chunk, position, angle) {
                    frame.push_primitive(0, RenderPrimitive::Polygon(polygon));
                    first += 1;
                }
                if i % 4 == 0 {
                    frame.push_primitive(
                        0,
                        RenderPrimitive::Circle(RenderCircle::filled(
                            RenderPoint::new(
                                position.x + phase.cos() * 18.,
                                position.y + phase.sin() * 18.,
                            ),
                            1.,
                            RenderColor::rgba(0., 1., 0., 0.25),
                        )),
                    );
                    first += 1;
                }
            }
        }
        frame.push_primitive(
            1,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(-20., -2.),
                    RenderPoint::new(20., -2.),
                    RenderPoint::new(20., 2.),
                    RenderPoint::new(-20., 2.),
                ],
                RenderColor::rgba(0.2, 0.5, 1., 0.25),
            )),
        );
        Ok((frame, groups, rebuilt))
    }
}

fn prepare(
    renderer: &mut TrianglePresenter,
    scene: &plan::Scene,
    groups: &[Placement],
    mode: Mode,
) -> Result<(), String> {
    match mode {
        Mode::Ordinary => renderer.prepare(scene),
        Mode::Cold | Mode::Retained => {
            renderer.prepare_retained(scene, groups, matches!(mode, Mode::Retained))
        }
    }
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let viewport = Viewport::new(width as f32, height as f32);
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let metadata = serde_json::json!({"version":1,"kind":"terrain-chunks","gl":context.identity,"width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,"in_flight":1,"scanout":false,"source_production_timed":true,"terrain_edit_reconstruction_timed":false,"cache_limit_bytes":16777216,"cache_limit_entries":128,"timing":"synchronous completed render frame (glFinish), excluding simulation, UI and scanout"});
    eprintln!("terrain_probe={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    validate(context, options, viewport)?;
    println!(
        "case,mode,repeat,frames,completed_wall_ms,cpu_ms,source_wall_ms,project_wall_ms,encode_wall_ms,submit_wall_ms,draws,upload_bytes,definition_builds,hits,misses,evictions,fallbacks,entries,cache_bytes"
    );
    for case in CASES {
        for repeat in 0..options.presentation_repeats {
            for offset in 0..3 {
                let mode =
                    [Mode::Ordinary, Mode::Cold, Mode::Retained][((repeat + offset) % 3) as usize];
                let mut producer = Producer::new(case)?;
                let mut renderer = TrianglePresenter::new(context.gl.clone())?;
                for i in 0..8 {
                    let (frame, groups, _) = producer.generate(i, viewport, mode.reuse())?;
                    let scene = plan::build(&[frame], viewport, FrameLayout::EqualHorizontal, 1.)?;
                    prepare(&mut renderer, &scene, &groups, mode)?;
                    renderer.draw_prepared()?;
                }
                unsafe {
                    context.gl.finish();
                }
                let (mut source, mut project, mut encode, mut submit) = (
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                );
                let (
                    mut draws,
                    mut bytes,
                    mut builds,
                    mut hits,
                    mut misses,
                    mut evictions,
                    mut fallbacks,
                ) = (0, 0, 0, 0, 0, 0, 0);
                let (mut entries, mut cache_bytes) = (0, 0);
                let total = Stamp::now()?;
                for i in 0..options.presentation_frames {
                    let start = Instant::now();
                    let (frame, groups, rebuilt) =
                        producer.generate(i as usize, viewport, mode.reuse())?;
                    source += start.elapsed();
                    builds += rebuilt;
                    let start = Instant::now();
                    let scene = plan::build(&[frame], viewport, FrameLayout::EqualHorizontal, 1.)?;
                    project += start.elapsed();
                    prepare(&mut renderer, &scene, &groups, mode)?;
                    let output = renderer.draw_prepared()?;
                    encode += output.encode;
                    submit += output.submit;
                    draws += output.mesh.draws;
                    bytes += output.mesh.upload_bytes;
                    let s = output.mesh.retained;
                    hits += s.hits;
                    misses += s.misses;
                    evictions += s.evictions;
                    fallbacks += s.fallbacks;
                    entries = entries.max(s.entries);
                    cache_bytes = cache_bytes.max(s.bytes);
                    unsafe {
                        context.gl.finish();
                    }
                }
                let (wall, cpu) = total.elapsed()?;
                let n = f64::from(options.presentation_frames);
                let ms = |d: Duration| d.as_secs_f64() * 1000. / n;
                println!(
                    "{},{},{repeat},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{builds},{hits},{misses},{evictions},{fallbacks},{entries},{cache_bytes}",
                    case.name,
                    mode.name(),
                    options.presentation_frames,
                    wall / n,
                    cpu / n,
                    ms(source),
                    ms(project),
                    ms(encode),
                    ms(submit),
                    draws as f64 / n,
                    bytes as f64 / n
                );
            }
        }
    }
    Ok(())
}

fn validate(context: &Context, options: &Options, viewport: Viewport) -> Result<(), Error> {
    for case in CASES {
        let mut source = Producer::new(case)?;
        let mut ordinary = TrianglePresenter::new(context.gl.clone())?;
        let mut cached = TrianglePresenter::new(context.gl.clone())?;
        let mut worst = (0.0_f64, 0.0_f64);
        for step in 0..48 {
            let v = if (32..40).contains(&step) {
                Viewport::new(viewport.width / 2., viewport.height / 2.)
            } else {
                viewport
            };
            let (frame, groups, _) = source.generate(step, v, true)?;
            let scene = plan::build(
                std::slice::from_ref(&frame),
                v,
                FrameLayout::EqualHorizontal,
                1.,
            )?;
            if !groups.iter().all(|p| p.matches(&scene)) {
                return Err(format!(
                    "{} step {step}: metadata differs from production geometry",
                    case.name
                )
                .into());
            }
            ordinary.draw(&scene)?;
            let expected = ordinary.readback()?;
            cached.prepare_retained(&scene, &groups, true)?;
            let output = cached.draw_prepared()?;
            let actual = cached.readback()?;
            let rgb: Vec<_> = expected
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            let error = geometry::difference(&rgb, &actual)?;
            worst.0 = worst.0.max(error.0);
            worst.1 = worst.1.max(error.1);
            if error.0 > 0.05 || error.1 > 0.001 {
                if let Some(path) = &options.geometry_output {
                    save_png(
                        &path.join(format!("failed-{}-{step}-cached.png", case.name)),
                        v.width as u32,
                        v.height as u32,
                        png::ColorType::Rgba,
                        &actual,
                    )?;
                    save_png(
                        &path.join(format!("failed-{}-{step}-reference.png", case.name)),
                        v.width as u32,
                        v.height as u32,
                        png::ColorType::Rgba,
                        &expected,
                    )?;
                }
                return Err(format!(
                    "{} step {step}: tight retained/reference gate failed {error:?}",
                    case.name
                )
                .into());
            }
            if output.mesh.retained.fallbacks != 0 {
                return Err("unexpected terrain cache budget fallback".into());
            }
            if [0, 8, 16, 24, 47].contains(&step) {
                let raster = RasterRenderer::new()
                    .image_from_frames_with_layout(
                        &[frame],
                        v,
                        FrameLayout::EqualHorizontal,
                        RasterOptions::default(),
                    )
                    .to_rgb8()
                    .unwrap();
                geometry::compare(raster.as_bytes(), &actual)?;
                if let Some(path) = &options.geometry_output {
                    for (label, pixels) in [("cached", &actual), ("reference", &expected)] {
                        save_png(
                            &path.join(format!("{}-{step}-{label}.png", case.name)),
                            v.width as u32,
                            v.height as u32,
                            png::ColorType::Rgba,
                            pixels,
                        )?;
                    }
                }
            }
            ordinary.collect_retired();
            cached.collect_retired();
        }
        eprintln!(
            "validated {}: 48 moving/edit/resize history frames; worst RGB MAE={:.6} pixel-error fraction={:.6}",
            case.name, worst.0, worst.1
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_identity_tracks_neighbor_generation_and_world_replacement() {
        let mut producer = Producer::new(CASES[3]).unwrap();
        let v = Viewport::new(800., 600.);
        let (_, a, builds) = producer.generate(0, v, true).unwrap();
        assert!(builds > 0);
        let (_, same, builds) = producer.generate(1, v, true).unwrap();
        assert_eq!(builds, 0);
        assert!(
            a.iter()
                .zip(same)
                .all(|(a, b)| Arc::ptr_eq(&a.definition, &b.definition))
        );
        let before: BTreeMap<_, _> = producer.art[0]
            .chunks
            .iter()
            .map(|(k, (_, v))| (*k, v.clone()))
            .collect();
        producer.generate(8, v, true).unwrap();
        let base = &producer.bodies[0][0].geometry;
        let changed = &producer.bodies[0][1].geometry;
        let mut neighbor_only = 0;
        for (old, new) in base.chunks().iter().zip(changed.chunks()) {
            if old.shape_count() == 0 || new.shape_count() == 0 {
                continue;
            }
            let a = &before[&old.id.0];
            let b = &producer.art[0].chunks[&new.id.0].1;
            assert_eq!(Arc::ptr_eq(a, b), old.generation == new.generation);
            neighbor_only +=
                usize::from(old.revision == new.revision && old.generation != new.generation);
        }
        assert!(neighbor_only > 0, "fixture must exercise halo invalidation");
        let (_, replacement, builds) = producer.generate(24, v, true).unwrap();
        assert!(builds > 0);
        assert!(
            !replacement
                .iter()
                .any(|p| a.iter().any(|a| Arc::ptr_eq(&p.definition, &a.definition)))
        );
    }
}
