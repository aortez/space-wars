//! Matched moving Spacewars replays, through the actual camera adapter and
//! compositor. Simulation runs outside the measured render intervals; source
//! frames, hint validation, projection, tessellation, uploads and completion are in.
use super::{Context, Error, Stamp, save_png};
use crate::{
    client_scenarios::{self, ClientScenario, RenderBackend, ScenarioAsset, ScenarioStartMode},
    gpu_scene::{CompositePresenter, Mode, presentation},
    presentation_probe::{Options, geometry},
    render::Viewport,
};
use engine_common::{Settings, SpacewarsController};
use glow::HasContext;
use std::{
    fs::File,
    io::Write,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(super) struct Fixture {
    pub(super) name: &'static str,
    seed: u64,
    start: u32,
    human: bool,
}

pub(super) const FIXTURES: [Fixture; 3] = [
    Fixture {
        name: "bots-early",
        seed: 42,
        start: 120,
        human: false,
    },
    Fixture {
        name: "bots-late",
        seed: 42,
        start: 1800,
        human: false,
    },
    // Reproduces the Pi live trial: P1 parked at a close-up scale while the
    // approaching P2 bot changes zoom continuously. Simulation is still untimed.
    Fixture {
        name: "close-up-zoom",
        seed: 0,
        start: 1500,
        human: true,
    },
];

const MODES: [Mode; 16] = [
    Mode::Triangles,
    Mode::RetainedCold,
    Mode::Retained,
    Mode::RetainedAdaptive,
    Mode::RetainedZoom,
    Mode::RetainedHybrid,
    Mode::RetainedOutlines,
    Mode::RetainedPaced,
    Mode::RetainedMemoized,
    Mode::RetainedScan,
    Mode::RetainedRanges,
    Mode::RetainedCulled,
    Mode::RetainedResident,
    Mode::RetainedResidentPasses,
    Mode::RetainedResidentBuffers,
    Mode::RetainedResidentUniforms,
];

pub(super) fn scenario(v: Viewport, fixture: Fixture) -> Result<Box<dyn ClientScenario>, Error> {
    let mut settings = Settings::default();
    settings.spacewars.player_1_controller = if fixture.human {
        SpacewarsController::Human
    } else {
        SpacewarsController::RuleBot
    };
    settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
    let mut s = client_scenarios::registration("spacewars")
        .unwrap()
        .create_with_asset(
            fixture.seed,
            &settings,
            v,
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )?;
    for _ in 0..fixture.start {
        s.step(&[], Duration::from_nanos(16_666_667));
    }
    Ok(s)
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let viewport = Viewport::new(width as f32, height as f32);
    let supported: Vec<_> = MODES
        .into_iter()
        .filter(|m| m.supported(&context.gl))
        .collect();
    for name in &options.geometry_live_modes {
        if !supported.iter().any(|m| m.name() == name) {
            return Err(format!(
                "requested geometry mode {name} is not supported by {}",
                context.identity[1]
            )
            .into());
        }
    }
    let modes: Vec<_> = supported
        .iter()
        .copied()
        .filter(|mode| {
            options.geometry_live_modes.is_empty()
                || options
                    .geometry_live_modes
                    .iter()
                    .any(|name| name == mode.name())
        })
        .collect();
    let metadata = serde_json::json!({"version":11,"kind":"live-retained","gl":context.identity,
        "fixtures": FIXTURES.map(|f| serde_json::json!({"name":f.name,"seed":f.seed,"start_tick":f.start,"human_p1":f.human})),
        "width":width,"height":height,"frames":options.presentation_frames,
        "repeats":options.presentation_repeats,"in_flight":1,"simulation_timed":false,
        "source_timed":true,"text_ui_scanout_timed":false,"timing":"summed per-frame source through glFinish wall and thread CPU",
        "startup_frames":8,"first_use":"geometry cache explicitly empty at each block; GPU targets prewarmed; new scenario; not process startup",
        "modes": modes.iter().map(|m|m.name()).collect::<Vec<_>>(), "validated_modes": supported.iter().map(|m|m.name()).collect::<Vec<_>>(),
        "unsupported_modes": MODES.into_iter().filter(|m| !m.supported(&context.gl)).map(|m|m.name()).collect::<Vec<_>>(),
        "admission_work_limit": crate::gpu_scene::retained::ADMISSION_WORK_LIMIT});
    eprintln!("live_retained_probe={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    validate_resident_bindings(context, viewport)?;
    super::zoom::validate(context, options)?;
    let mut cached = [
        (Mode::Retained, CompositePresenter::new(context.gl.clone())?),
        (
            Mode::RetainedAdaptive,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedZoom,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedHybrid,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedOutlines,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedPaced,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedMemoized,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedScan,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedRanges,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedCulled,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedResident,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedResidentPasses,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedResidentBuffers,
            CompositePresenter::new(context.gl.clone())?,
        ),
        (
            Mode::RetainedResidentUniforms,
            CompositePresenter::new(context.gl.clone())?,
        ),
    ]
    .into_iter()
    .filter(|(m, _)| m.supported(&context.gl))
    .collect::<Vec<_>>();
    let mut reference = CompositePresenter::new(context.gl.clone())?;
    let mut checked = 0;
    let mut reused = 0;
    let mut hybrid = crate::gpu_scene::retained::Stats::default();
    let mut outlines = crate::gpu_scene::retained::Stats::default();
    let mut memoized = crate::gpu_scene::retained::Stats::default();
    let mut max_mae = 0.0_f64;
    let mut max_different = 0.0_f64;
    // A fresh scenario repeats numeric revisions. Keep the renderer/cache alive
    // across replacements, changed views, moving bodies and camera smoothing.
    for fixture in FIXTURES {
        for v in [viewport, Viewport::new(641., 361.), viewport] {
            let mut s = scenario(v, fixture)?;
            for tick in 0..24 {
                s.step(&[], Duration::from_nanos(16_666_667));
                let frames = s.render_frames(RenderBackend::Raster, v);
                let plan =
                    presentation::build_with_retained(&frames, v, s.frame_layout(), 1.0, true)?;
                reused += plan
                    .passes
                    .iter()
                    .map(|p| p.scene().retained.len())
                    .sum::<usize>();
                reference.draw_with_mode(&plan, Mode::Triangles)?;
                let ordinary = reference.readback()?;
                let rgb: Vec<_> = ordinary
                    .chunks_exact(4)
                    .flat_map(|p| p[..3].iter().copied())
                    .collect();
                let mut ranges_pixels = None;
                for (mode, renderer) in &mut cached {
                    let output = renderer.draw_with_mode(&plan, *mode)?;
                    if *mode == Mode::RetainedHybrid {
                        hybrid.accumulate(output.mesh.retained);
                    }
                    if *mode == Mode::RetainedOutlines {
                        outlines.accumulate(output.mesh.retained);
                    }
                    if *mode == Mode::RetainedMemoized {
                        memoized.accumulate(output.mesh.retained);
                    }
                    let pixels = renderer.readback()?;
                    if *mode == Mode::RetainedCulled && ranges_pixels.as_ref() != Some(&pixels) {
                        return Err(format!(
                            "culling changed visible pixels: {} tick={tick}",
                            fixture.name
                        )
                        .into());
                    }
                    if *mode == Mode::RetainedRanges {
                        ranges_pixels = Some(pixels.clone());
                    }
                    let (mae, different) = geometry::compare(&rgb, &pixels)?;
                    max_mae = max_mae.max(mae);
                    max_different = max_different.max(different);
                    if mae > 0.05 || different > 0.001 {
                        if let Some(path) = &options.geometry_output {
                            for (label, pixels) in
                                [(mode.name(), &pixels), ("reference", &ordinary)]
                            {
                                save_png(
                                    &path.join(format!(
                                        "failed-{}-{checked}-{label}.png",
                                        fixture.name
                                    )),
                                    v.width as u32,
                                    v.height as u32,
                                    png::ColorType::Rgba,
                                    pixels,
                                )?;
                            }
                        }
                        return Err(format!(
                            "live retained parity failed fixture={} mode={} tick={tick}: {mae} / {different}", fixture.name, mode.name()
                        ).into());
                    }
                    checked += 1;
                    renderer.collect_retired();
                    if matches!(tick, 0 | 23)
                        && v == viewport
                        && let Some(path) = &options.geometry_output
                    {
                        // Include check number: replacement histories cannot overwrite.
                        save_png(
                            &path.join(format!("{}-{checked}-{}.png", fixture.name, mode.name())),
                            width,
                            height,
                            png::ColorType::Rgba,
                            &pixels,
                        )?;
                        save_png(
                            &path.join(format!("{}-{checked}-triangles.png", fixture.name)),
                            width,
                            height,
                            png::ColorType::Rgba,
                            &ordinary,
                        )?;
                    }
                }
                reference.collect_retired();
            }
        }
    }
    if reused == 0 {
        return Err("live replay did not exercise any geometry reuse".into());
    }
    if memoized.build_reuses == 0 {
        return Err("live replay did not exercise construction reuse".into());
    }
    eprintln!(
        "validated construction reuse polygons={}",
        memoized.build_reuses
    );
    if hybrid.hybrid_exact_groups == 0 || hybrid.hybrid_zoom_groups == 0 || hybrid.promotions == 0 {
        return Err("live replay did not exercise both hybrid paths and promotion".into());
    }
    if outlines.hybrid_exact_groups == 0
        || outlines.hybrid_zoom_groups == 0
        || outlines.promotions == 0
        || outlines.zoom_reused_strokes <= hybrid.zoom_reused_strokes
    {
        return Err(
            "outline replay did not exercise promotion and additional outline reuse".into(),
        );
    }
    eprintln!(
        "validated outline strokes={} previous_strokes={} fallback_polygons={} previous_fallback_polygons={}",
        outlines.zoom_reused_strokes,
        hybrid.zoom_reused_strokes,
        outlines.zoom_fallback_polygons,
        hybrid.zoom_fallback_polygons
    );
    eprintln!(
        "validated hybrid exact_groups={} zoom_groups={} promotions={}",
        hybrid.hybrid_exact_groups, hybrid.hybrid_zoom_groups, hybrid.promotions
    );
    eprintln!(
        "validated live frames={checked} groups={reused} max_mae={max_mae:.8} max_different={max_different:.8}"
    );
    drop(cached);
    drop(reference);
    let mut renderer = CompositePresenter::new(context.gl.clone())?;
    // Keep target allocation out of first-use cache costs, including block one.
    // This disposable source is not shared with any measured scenario.
    {
        let s = scenario(viewport, FIXTURES[0])?;
        let frames = s.render_frames(RenderBackend::Raster, viewport);
        let plan =
            presentation::build_with_retained(&frames, viewport, s.frame_layout(), 1.0, false)?;
        renderer.draw_with_mode(&plan, Mode::Triangles)?;
        unsafe {
            context.gl.finish();
        }
    }
    println!(
        "fixture,start_tick,mode,repeat,frames,render_wall_ms,render_cpu_ms,source_ms,project_ms,encode_ms,submit_ms,draws,upload_bytes,hits,misses,scale_misses,evictions,fallbacks,admissions,bypasses,entries,cache_bytes,probation_entries,groups,source_groups,zoom_reused_polygons,zoom_fallback_polygons,zoom_reused_fills,zoom_reused_strokes,hybrid_exact_groups,hybrid_zoom_groups,hybrid_fallback_groups,promotions,exact_entries,zoom_entries,first_wall_ms,first_cpu_ms,first_encode_ms,first_admissions,first_promotions,startup_wall_total_ms,startup_cpu_total_ms,startup_encode_total_ms,startup_wall_max_ms,startup_admissions,startup_promotions,wall_p95_ms,wall_max_ms,build_ms,build_max_ms,build_work,deferred,oversized,first_build_ms,first_deferred,startup_build_ms,startup_deferred,build_reuses,first_build_reuses,startup_build_reuses,vertices,indices,cached_vertices,streamed_vertices,placements,atlas_entries,atlas_payload_bytes,atlas_upload_bytes,resident_fallback_groups,culled_pieces,culled_indices"
    );
    for fixture in FIXTURES {
        for repeat in 0..options.presentation_repeats {
            for offset in 0..modes.len() {
                let mode = modes[(repeat as usize + offset) % modes.len()];
                let mut s = scenario(viewport, fixture)?;
                renderer.clear_retained();
                let mut sums = [0.0_f64; 6];
                let mut first = [0.0_f64; 6];
                let mut first_cache = crate::gpu_scene::retained::Stats::default();
                let mut startup = [0.0_f64; 6];
                let mut startup_max = 0.0_f64;
                let mut startup_cache = crate::gpu_scene::retained::Stats::default();
                let mut walls = Vec::new();
                let mut draws = 0;
                let mut upload = 0;
                let mut groups = 0;
                let mut source_groups = 0;
                let mut cache = crate::gpu_scene::retained::Stats::default();
                let mut resident = crate::gpu_scene::resident::Stats::default();
                let (mut vertices, mut indices) = (0, 0);
                for tick in 0..8 + options.presentation_frames {
                    s.step(&[], Duration::from_nanos(16_666_667));
                    let total = Stamp::now()?;
                    let began = Instant::now();
                    let frames = s.render_frames(RenderBackend::Raster, viewport);
                    let source = began.elapsed();
                    let began = Instant::now();
                    let plan = presentation::build_with_retained(
                        &frames,
                        viewport,
                        s.frame_layout(),
                        1.0,
                        mode.retention().is_some(),
                    )?;
                    let project = began.elapsed();
                    let output = renderer.draw_with_mode(&plan, mode)?;
                    unsafe {
                        context.gl.finish();
                    }
                    let count = plan
                        .passes
                        .iter()
                        .map(|p| p.scene().retained.len())
                        .sum::<usize>();
                    let source_count = frames.iter().map(|f| f.geometry_hints.len()).sum::<usize>();
                    drop(plan);
                    drop(frames);
                    let (wall, cpu) = total.elapsed()?;
                    renderer.collect_retired();
                    let times = [
                        wall,
                        cpu,
                        source.as_secs_f64() * 1000.0,
                        project.as_secs_f64() * 1000.0,
                        output.encode.as_secs_f64() * 1000.0,
                        output.submit.as_secs_f64() * 1000.0,
                    ];
                    if tick == 0 {
                        first = times;
                        first_cache = output.mesh.retained;
                    }
                    if tick < 8 {
                        for (a, b) in startup.iter_mut().zip(times) {
                            *a += b;
                        }
                        startup_max = startup_max.max(wall);
                        startup_cache.accumulate(output.mesh.retained);
                    } else {
                        walls.push(wall);
                        for (a, b) in sums.iter_mut().zip(times) {
                            *a += b;
                        }
                        cache.accumulate(output.mesh.retained);
                        resident.accumulate(output.mesh.resident);
                        vertices += output.mesh.vertices;
                        indices += output.mesh.indices;
                        draws += output.mesh.draws;
                        upload += output.mesh.upload_bytes;
                        groups += count;
                        source_groups += source_count;
                    }
                }
                let n = f64::from(options.presentation_frames);
                let timings = sums.map(|v| format!("{:.6}", v / n)).join(",");
                println!(
                    "{},{},{},{repeat},{},{timings},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    fixture.name,
                    fixture.start,
                    mode.name(),
                    options.presentation_frames,
                    draws as f64 / n,
                    upload as f64 / n,
                    cache.hits,
                    cache.misses,
                    cache.scale_misses,
                    cache.evictions,
                    cache.fallbacks,
                    cache.admissions,
                    cache.bypasses,
                    cache.entries,
                    cache.bytes,
                    cache.probation_entries,
                    groups,
                    source_groups,
                    cache.zoom_reused_polygons,
                    cache.zoom_fallback_polygons,
                    cache.zoom_reused_fills,
                    cache.zoom_reused_strokes,
                    {
                        walls.sort_by(f64::total_cmp);
                        [
                            cache.hybrid_exact_groups.to_string(),
                            cache.hybrid_zoom_groups.to_string(),
                            cache.hybrid_fallback_groups.to_string(),
                            cache.promotions.to_string(),
                            cache.exact_entries.to_string(),
                            cache.zoom_entries.to_string(),
                            format!("{:.6}", first[0]),
                            format!("{:.6}", first[1]),
                            format!("{:.6}", first[4]),
                            first_cache.admissions.to_string(),
                            first_cache.promotions.to_string(),
                            format!("{:.6}", startup[0]),
                            format!("{:.6}", startup[1]),
                            format!("{:.6}", startup[4]),
                            format!("{startup_max:.6}"),
                            startup_cache.admissions.to_string(),
                            startup_cache.promotions.to_string(),
                            format!(
                                "{:.6}",
                                walls[(walls.len() * 95).div_ceil(100).saturating_sub(1)]
                            ),
                            format!("{:.6}", walls.last().unwrap()),
                            format!("{:.6}", cache.build_ns as f64 / 1e6 / n),
                            format!("{:.6}", cache.max_build_ns as f64 / 1e6),
                            cache.build_work.to_string(),
                            cache.deferred.to_string(),
                            cache.oversized.to_string(),
                            format!("{:.6}", first_cache.build_ns as f64 / 1e6),
                            first_cache.deferred.to_string(),
                            format!("{:.6}", startup_cache.build_ns as f64 / 1e6),
                            startup_cache.deferred.to_string(),
                            cache.build_reuses.to_string(),
                            first_cache.build_reuses.to_string(),
                            startup_cache.build_reuses.to_string(),
                            (vertices as f64 / n).to_string(),
                            (indices as f64 / n).to_string(),
                            (resident.cached_vertices as f64 / n).to_string(),
                            (resident.streamed_vertices as f64 / n).to_string(),
                            (resident.placements as f64 / n).to_string(),
                            resident.atlas_entries.to_string(),
                            resident.atlas_payload_bytes.to_string(),
                            (resident.atlas_upload_bytes as f64 / n).to_string(),
                            resident.fallback_groups.to_string(),
                            cache.culled_pieces.to_string(),
                            cache.culled_indices.to_string(),
                        ]
                        .join(",")
                    }
                );
            }
        }
    }
    Ok(())
}

fn validate_resident_bindings(context: &Context, viewport: Viewport) -> Result<(), Error> {
    let s = scenario(viewport, FIXTURES[0])?;
    let frames = s.render_frames(RenderBackend::Raster, viewport);
    let plan = presentation::build_with_retained(&frames, viewport, s.frame_layout(), 1., true)?;
    let g = &context.gl;
    let mut samplers = Vec::new();
    let mut textures = Vec::new();
    let mut buffers = Vec::new();
    // Isolated offscreen context. Save and restore the caller bindings, and
    // always delete the deliberately foreign objects even when the check fails.
    unsafe {
        let active = g.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
        let before: [_; 2] = std::array::from_fn(|i| {
            g.active_texture(glow::TEXTURE0 + i as u32);
            (
                g.get_parameter_sampler(glow::SAMPLER_BINDING),
                g.get_parameter_texture(glow::TEXTURE_BINDING_2D),
            )
        });
        let storage = glow::SHADER_STORAGE_BUFFER;
        let binding = glow::SHADER_STORAGE_BUFFER_BINDING;
        let generic = g.get_parameter_buffer(binding);
        let read_ranges = || -> [[i64; 3]; 2] {
            std::array::from_fn(|i| {
                [
                    i64::from(g.get_parameter_indexed_i32(binding, i as u32)),
                    g.get_parameter_indexed_i64(glow::SHADER_STORAGE_BUFFER_START, i as u32),
                    g.get_parameter_indexed_i64(glow::SHADER_STORAGE_BUFFER_SIZE, i as u32),
                ]
            })
        };
        let old_ranges = read_ranges();
        let uniform_saved =
            crate::gpu_scene::resident_storage::Bindings::save(g.clone(), glow::UNIFORM_BUFFER)?;
        let mut uniform_buffers = Vec::new();
        let read_uniform = || {
            [
                i64::from(g.get_parameter_indexed_i32(glow::UNIFORM_BUFFER_BINDING, 0)),
                g.get_parameter_indexed_i64(glow::UNIFORM_BUFFER_START, 0),
                g.get_parameter_indexed_i64(glow::UNIFORM_BUFFER_SIZE, 0),
            ]
        };
        let result = (|| -> Result<(), Error> {
            let alignment = g.get_parameter_i32(glow::UNIFORM_BUFFER_OFFSET_ALIGNMENT);
            for i in 0..2 {
                let buffer = g.create_buffer()?;
                uniform_buffers.push(buffer);
                g.bind_buffer(glow::UNIFORM_BUFFER, Some(buffer));
                g.buffer_data_size(glow::UNIFORM_BUFFER, alignment * 4, glow::STATIC_DRAW);
                if i == 0 {
                    g.bind_buffer_range(
                        glow::UNIFORM_BUFFER,
                        0,
                        Some(buffer),
                        alignment,
                        alignment * 2,
                    );
                }
            }
            let expected_uniform = read_uniform();
            // Three distinct buffers catch indexed-vs-generic binding leaks.
            let alignment = g.get_parameter_i32(glow::SHADER_STORAGE_BUFFER_OFFSET_ALIGNMENT);
            for i in 0..3 {
                let buffer = g.create_buffer()?;
                buffers.push(buffer);
                g.bind_buffer(storage, Some(buffer));
                g.buffer_data_size(storage, alignment * 4, glow::STATIC_DRAW);
                if i < 2 {
                    g.bind_buffer_range(storage, i, Some(buffer), alignment, alignment * 2);
                }
            }
            let expected_ranges = read_ranges();
            for i in 0..2 {
                samplers.push(g.create_sampler()?);
                textures.push(g.create_texture()?);
                g.active_texture(glow::TEXTURE0 + i as u32);
                g.bind_sampler(i as u32, Some(samplers[i]));
                g.bind_texture(glow::TEXTURE_2D, Some(textures[i]));
            }
            let mut renderer = CompositePresenter::new(context.gl.clone())?;
            for mode in [
                Mode::RetainedResident,
                Mode::RetainedResidentPasses,
                Mode::RetainedResidentBuffers,
                Mode::RetainedResidentUniforms,
                Mode::RetainedRanges,
                Mode::RetainedCulled,
                Mode::Triangles,
                Mode::RetainedResident,
                Mode::RetainedResidentBuffers,
                Mode::RetainedResidentUniforms,
            ]
            .into_iter()
            .filter(|m| m.supported(g))
            {
                renderer.draw_with_mode(&plan, mode)?;
                if g.get_parameter_i32(glow::ACTIVE_TEXTURE) != glow::TEXTURE1 as i32 {
                    return Err("resident path changed active texture unit".into());
                }
                for i in 0..2 {
                    g.active_texture(glow::TEXTURE0 + i as u32);
                    if g.get_parameter_sampler(glow::SAMPLER_BINDING) != Some(samplers[i])
                        || g.get_parameter_texture(glow::TEXTURE_BINDING_2D) != Some(textures[i])
                    {
                        return Err("resident path leaked texture/sampler bindings".into());
                    }
                }
                renderer.collect_retired();
                if g.get_parameter_buffer(glow::UNIFORM_BUFFER_BINDING) != Some(uniform_buffers[1])
                    || read_uniform() != expected_uniform
                {
                    return Err(
                        "resident path leaked uniform buffer name/offset/size or generic binding"
                            .into(),
                    );
                }
                if g.get_parameter_buffer(binding) != Some(buffers[2])
                    || read_ranges() != expected_ranges
                {
                    return Err(
                        "resident path leaked storage buffer name/offset/size or generic binding"
                            .into(),
                    );
                }
            }
            drop(renderer);
            if g.get_parameter_buffer(glow::UNIFORM_BUFFER_BINDING) != Some(uniform_buffers[1])
                || read_uniform() != expected_uniform
            {
                return Err("resident teardown leaked uniform bindings".into());
            }
            if g.get_parameter_buffer(binding) != Some(buffers[2])
                || read_ranges() != expected_ranges
            {
                return Err("resident teardown leaked storage bindings".into());
            }
            Ok(())
        })();
        for (i, [name, start, size]) in old_ranges.into_iter().enumerate() {
            let buffer = std::num::NonZeroU32::new(name as u32).map(glow::NativeBuffer);
            if buffer.is_some() && size > 0 {
                g.bind_buffer_range(
                    storage,
                    i as u32,
                    buffer,
                    start.try_into()?,
                    size.try_into()?,
                );
            } else {
                g.bind_buffer_base(storage, i as u32, buffer);
            }
        }
        g.bind_buffer(storage, generic);
        drop(uniform_saved);
        for buffer in uniform_buffers {
            g.delete_buffer(buffer);
        }
        for (i, (sampler, texture)) in before.into_iter().enumerate() {
            g.active_texture(glow::TEXTURE0 + i as u32);
            g.bind_sampler(i as u32, sampler);
            g.bind_texture(glow::TEXTURE_2D, texture);
        }
        g.active_texture(active);
        for sampler in samplers {
            g.delete_sampler(sampler);
        }
        for texture in textures {
            g.delete_texture(texture);
        }
        for buffer in buffers {
            g.delete_buffer(buffer);
        }
        result?;
    }
    eprintln!(
        "validated resident texture/sampler, storage and uniform buffer ranges across repeated mode transitions and teardown"
    );
    Ok(())
}
