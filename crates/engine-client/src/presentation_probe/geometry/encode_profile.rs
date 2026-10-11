//! Warm CPU encoding of moving production replays, with optional detailed clocks.
//! Simulation/source/projection run outside timing; no GL drawing or uploads.
use super::{Context, Error, Stamp, live_retained};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        encode_profile::EncodeProfile,
        mesh::MeshBuilder,
        presentation,
        retained::{Cache, Policy, Stats},
    },
    presentation_probe::Options,
    render::Viewport,
};
use std::{fs::File, io::Write, time::Duration};

const STARTUP: u32 = 8;

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let metadata = serde_json::json!({"version":4,"kind":"warm-encode-profile",
        "gl":context.identity,"width":width,"height":height,"frames":options.presentation_frames,
        "repeats":options.presentation_repeats,"startup_frames":STARTUP,
        "policies":["retained-memoized", "retained-scan", "retained-ranges", "retained-culled"],"timing":"CPU mesh encoding only; simulation, source, projection, GPU uploads/drawing, UI and scanout excluded",
        "instrumentation":"rotated plain/profiled/scan/scan-profiled/ranges/ranges-profiled/culled/culled-profiled blocks; per-piece clocks compiled out of unprofiled modes; compare unprofiled totals",
        "stroke_attribution":"guarded-stroke preparation, tessellation and final checks are nested spans; offscreen time is a subset of parent stroke time; post-tessellation visibility inspection is diagnostic only and timed separately"});
    eprintln!("encode_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    validate(v)?;
    println!(
        "fixture,mode,repeat,frames,wall_ms,cpu_ms,resolve_ms,exact_expand_ms,scalable_expand_ms,fallback_fill_ms,fallback_stroke_ms,ordinary_ms,validate_ms,exact_vertices,scalable_vertices,scalable_ranges,fallback_fills,fallback_strokes,ordinary_draws,vertices,indices,upload_bytes,admissions,build_ms,entries,cache_bytes,stroke_prepare_ms,stroke_tessellate_ms,stroke_finish_ms,stroke_visibility_ms,offscreen_stroke_ms,stroke_points,stroke_vertices,stroke_indices,empty_strokes,offscreen_strokes"
    );
    for fixture in live_retained::FIXTURES {
        for repeat in 0..options.presentation_repeats {
            for offset in 0..8 {
                let mode = (repeat + offset) % 8;
                let detailed = mode % 2 != 0;
                let name = [
                    "plain",
                    "profiled",
                    "scan",
                    "scan-profiled",
                    "ranges",
                    "ranges-profiled",
                    "culled",
                    "culled-profiled",
                ][mode as usize];
                let policy = if mode >= 6 {
                    Policy::HybridCulled
                } else if mode >= 4 {
                    Policy::HybridRanges
                } else if mode >= 2 {
                    Policy::HybridScan
                } else {
                    Policy::HybridMemoized
                };
                let mut s = live_retained::scenario(v, fixture)?;
                let mut cache = Cache::default();
                let mut mesh = MeshBuilder::default();
                let mut profile = EncodeProfile::default();
                let mut totals = Stats::default();
                let (mut wall, mut cpu) = (0., 0.);
                let (mut vertices, mut indices, mut bytes) = (0_u64, 0_u64, 0_u64);
                for tick in 0..STARTUP + options.presentation_frames {
                    s.step(&[], Duration::from_nanos(16_666_667));
                    let frames = s.render_frames(RenderBackend::Raster, v);
                    let plan =
                        presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
                    cache.begin_frame();
                    let stamp = Stamp::now()?;
                    for pass in &plan.passes {
                        let scene = pass.scene();
                        let part = if detailed {
                            cache.rebuild_profiled_clipped(
                                scene,
                                &scene.retained,
                                &mut mesh,
                                policy,
                                pass.clip(),
                            )?
                        } else {
                            cache.rebuild_clipped(
                                scene,
                                &scene.retained,
                                &mut mesh,
                                policy,
                                pass.clip(),
                            )?;
                            EncodeProfile::default()
                        };
                        std::hint::black_box(&mesh.buffers);
                        if tick >= STARTUP {
                            profile.accumulate(part);
                            totals.accumulate(cache.stats);
                            vertices += mesh.buffers.vertices.len() as u64;
                            indices += mesh.buffers.indices.len() as u64;
                            bytes += mesh.upload_bytes() as u64;
                        }
                    }
                    let (w, c) = stamp.elapsed()?;
                    if tick >= STARTUP {
                        wall += w;
                        cpu += c;
                    }
                }
                let n = f64::from(options.presentation_frames);
                let times = [
                    profile.resolve,
                    profile.exact_expand,
                    profile.scalable_expand,
                    profile.fallback_fill,
                    profile.fallback_stroke,
                    profile.ordinary,
                    profile.validate,
                ]
                .map(|d| format!("{:.6}", d.as_secs_f64() * 1000. / n))
                .join(",");
                let counts = [
                    profile.exact_vertices,
                    profile.scalable_vertices,
                    profile.scalable_ranges,
                    profile.fallback_fills,
                    profile.fallback_strokes,
                    profile.ordinary_draws,
                    vertices,
                    indices,
                    bytes,
                ]
                .map(|v| format!("{:.6}", v as f64 / n))
                .join(",");
                let stroke_times = [
                    profile.guarded_stroke.prepare,
                    profile.guarded_stroke.stroke,
                    profile.guarded_stroke.finish,
                    profile.stroke_visibility,
                    profile.offscreen_stroke,
                ]
                .map(|d| format!("{:.6}", d.as_secs_f64() * 1000. / n))
                .join(",");
                let stroke_counts = [
                    profile.guarded_stroke.points,
                    profile.guarded_stroke.vertices,
                    profile.guarded_stroke.indices,
                    profile.guarded_stroke.empty,
                    profile.offscreen_strokes,
                ]
                .map(|v| format!("{:.6}", v as f64 / n))
                .join(",");
                println!(
                    "{},{name},{repeat},{},{:.6},{:.6},{times},{counts},{},{:.6},{},{},{stroke_times},{stroke_counts}",
                    fixture.name,
                    options.presentation_frames,
                    wall / n,
                    cpu / n,
                    totals.admissions,
                    totals.build_ns as f64 / 1e6 / n,
                    totals.entries,
                    totals.bytes
                );
            }
        }
    }
    Ok(())
}

fn validate(v: Viewport) -> Result<(), Error> {
    let mut checked = 0;
    for fixture in live_retained::FIXTURES {
        let mut s = live_retained::scenario(v, fixture)?;
        let (mut plain, mut detailed) = (Cache::default(), Cache::default());
        let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
        let mut scans = [
            (Cache::default(), MeshBuilder::default()),
            (Cache::default(), MeshBuilder::default()),
            (Cache::default(), MeshBuilder::default()),
            (Cache::default(), MeshBuilder::default()),
        ];
        let (mut culled, mut culled_profiled) = (Cache::default(), Cache::default());
        let (mut culled_mesh, mut culled_profiled_mesh) =
            (MeshBuilder::default(), MeshBuilder::default());
        let (mut resident_cache, mut resident_mesh, mut resident_frame) = (
            Cache::default(),
            MeshBuilder::default(),
            crate::gpu_scene::resident::Frame::default(),
        );
        for _ in 0..72 {
            s.step(&[], Duration::from_nanos(16_666_667));
            let frames = s.render_frames(RenderBackend::Raster, v);
            let plan = presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
            plain.begin_frame();
            detailed.begin_frame();
            culled.begin_frame();
            culled_profiled.begin_frame();
            resident_cache.begin_frame();
            resident_frame.begin_frame();
            for (cache, _) in &mut scans {
                cache.begin_frame();
            }
            for pass in &plan.passes {
                let scene = pass.scene();
                plain.rebuild(scene, &scene.retained, &mut a, Policy::HybridMemoized)?;
                resident_cache.rebuild_resident(
                    scene,
                    &scene.retained,
                    &mut resident_mesh,
                    &mut resident_frame,
                )?;
                let reference = resident_frame.expand_reference()?;
                if !a
                    .buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .eq(reference
                        .buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits()))
                    || a.buffers.indices != reference.buffers.indices
                    || plain.stats.hits != resident_cache.stats.hits
                    || plain.stats.admissions != resident_cache.stats.admissions
                    || plain.stats.zoom_fallback_polygons
                        != resident_cache.stats.zoom_fallback_polygons
                {
                    return Err("resident references changed geometry/order/cache decisions".into());
                }
                let r = resident_frame.stats();
                if r.atlas_entries > 128
                    || r.atlas_payload_bytes > 16 * 1024 * 1024
                    || r.fallback_groups != 0
                {
                    return Err("production replay exceeded resident atlas budget".into());
                }
                resident_frame.uploaded(); // Emulate completed submission.
                checked += 1;
                detailed.rebuild_profiled(
                    scene,
                    &scene.retained,
                    &mut b,
                    Policy::HybridMemoized,
                )?;
                if a.buffers.vertices != b.buffers.vertices
                    || a.buffers.indices != b.buffers.indices
                    || a.batches != b.batches
                {
                    return Err("encoding instrumentation changed mesh geometry/order".into());
                }
                for c in [plain.stats, detailed.stats] {
                    if c.entries > 128 || c.bytes > 16 * 1024 * 1024 {
                        return Err("encoding profile exceeded cache budget".into());
                    }
                }
                if (
                    plain.stats.hits,
                    plain.stats.admissions,
                    plain.stats.bytes,
                    plain.stats.zoom_fallback_polygons,
                ) != (
                    detailed.stats.hits,
                    detailed.stats.admissions,
                    detailed.stats.bytes,
                    detailed.stats.zoom_fallback_polygons,
                ) {
                    return Err("encoding instrumentation changed cache decisions".into());
                }
                checked += 1;
                for (i, (cache, mesh)) in scans.iter_mut().enumerate() {
                    let policy = if i < 2 {
                        Policy::HybridScan
                    } else {
                        Policy::HybridRanges
                    };
                    if i % 2 == 0 {
                        cache.rebuild(scene, &scene.retained, mesh, policy)?;
                    } else {
                        cache.rebuild_profiled(scene, &scene.retained, mesh, policy)?;
                    }
                    if !a
                        .buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits())
                        .eq(mesh.buffers.vertices.iter().flatten().map(|v| v.to_bits()))
                        || a.buffers.indices != mesh.buffers.indices
                        || a.batches != mesh.batches
                        || plain.stats.hits != cache.stats.hits
                        || plain.stats.admissions != cache.stats.admissions
                        || plain.stats.bytes != cache.stats.bytes
                        || plain.stats.zoom_fallback_polygons != cache.stats.zoom_fallback_polygons
                    {
                        return Err("encoding variant changed geometry or cache decisions".into());
                    }
                    checked += 1;
                }
                culled.rebuild_clipped(
                    scene,
                    &scene.retained,
                    &mut culled_mesh,
                    Policy::HybridCulled,
                    pass.clip(),
                )?;
                culled_profiled.rebuild_profiled_clipped(
                    scene,
                    &scene.retained,
                    &mut culled_profiled_mesh,
                    Policy::HybridCulled,
                    pass.clip(),
                )?;
                if !culled_mesh
                    .buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .eq(culled_profiled_mesh
                        .buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits()))
                    || culled_mesh.buffers.indices != culled_profiled_mesh.buffers.indices
                    || culled_mesh.batches != culled_profiled_mesh.batches
                    || culled.stats.hits != culled_profiled.stats.hits
                    || culled.stats.admissions != culled_profiled.stats.admissions
                    || culled.stats.bytes != culled_profiled.stats.bytes
                    || culled.stats.zoom_fallback_polygons
                        != culled_profiled.stats.zoom_fallback_polygons
                    || culled.stats.culled_pieces != culled_profiled.stats.culled_pieces
                    || culled.stats.culled_indices != culled_profiled.stats.culled_indices
                {
                    return Err(
                        "stroke attribution changed clipped geometry or cache decisions".into(),
                    );
                }
                checked += 1;
            }
        }
    }
    eprintln!(
        "validated encoding meshes={checked} including startup, stable views and changing zoom"
    );
    Ok(())
}
