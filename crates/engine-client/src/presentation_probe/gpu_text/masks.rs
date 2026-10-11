//! Compare atlas producers through public FemtoVG APIs, NOT a replacement for
//! Slint's live renderer. Identical font bytes, shaping, placement and final
//! glyph-quad API; analytical coverage is intentionally not pixel-identical
//! to FemtoVG's eight-sample vector coverage. Captures/quality checks untimed.
use super::{Context, Error, Options, STAGES, Stamp, Time, counters, rows, save};
use femtovg::{Baseline, Canvas, Color, FontId, Paint, TextContext};
use glow::HasContext;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, rc::Rc};
mod atlas;
use atlas::CpuAtlas;
type GlCanvas = Canvas<femtovg::renderer::OpenGl>;

struct Fonts {
    bytes: [Vec<u8>; 2],
    ids: [FontId; 2],
    text: TextContext,
}
impl Fonts {
    fn new(regular: &Path, bold: &Path) -> Result<Self, Error> {
        let text = TextContext::default();
        let load = |path: &Path| -> Result<(Vec<u8>, FontId), Error> {
            let bytes = std::fs::read(path)?;
            let face = ttf_parser::Face::parse(&bytes, 0)?;
            if face.is_variable() {
                return Err("variable fonts are outside this experiment".into());
            }
            eprintln!(
                "mask_font path={} sha256={:x} units_per_em={}",
                path.display(),
                Sha256::digest(&bytes),
                face.units_per_em()
            );
            let id = text.add_font_mem(&bytes)?;
            Ok((bytes, id))
        };
        let regular = load(regular)?;
        let bold = load(bold)?;
        Ok(Self {
            bytes: [regular.0, bold.0],
            ids: [regular.1, bold.1],
            text,
        })
    }
}
struct Row {
    label: String,
    paint: Paint,
    font: usize,
    size: f32,
    x: f32,
    origin: (f32, f32),
}
fn fixture(fonts: &Fonts, context: &Context, stage: &str) -> Result<Vec<Row>, Error> {
    let layout_delta = if matches!(stage, "layout-half-pixel" | "layout-repeat") {
        1.
    } else {
        0.
    };
    let shift = if matches!(stage, "item-half-pixel" | "item-repeat") {
        0.5
    } else {
        0.
    };
    let size_delta = if stage == "size-change" { 1. } else { 0. };
    rows(if matches!(stage, "score-change" | "score-repeat") {
        2
    } else {
        1
    })
    .into_iter()
    .enumerate()
    .map(|(index, row)| {
        let font = usize::from(row.weight >= 700);
        let size = row.size + size_delta;
        let mut paint = Paint::color(if index == 0 {
            Color::white()
        } else {
            Color::rgb(128, 216, 255)
        });
        paint.set_font(&[fonts.ids[font]]);
        paint.set_font_size(size);
        paint.set_text_baseline(Baseline::Top);
        let width = fonts.text.measure_text(0., 0., &row.label, &paint)?.width();
        // Same single-line center/top layout as the previous Slint fixture.
        let x = (560. + layout_delta - width) / 2.;
        Ok(Row {
            label: row.label.to_string(),
            paint,
            font,
            size,
            x,
            origin: (
                (context.size.width as f32 - 560.) / 2. + shift,
                (context.size.height as f32 - 420.) / 2. + index as f32 * 28.,
            ),
        })
    })
    .collect()
}
fn canvas(context: &Context, fonts: &Fonts) -> Result<GlCanvas, Error> {
    // SAFETY: Parent's EGL pbuffer is current throughout the probe; loaders
    // forward pointers unchanged and do not own the display/context.
    let renderer = unsafe {
        femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
            counters::wrap(
                name,
                glutin::prelude::GlDisplay::get_proc_address(&context.display, name),
            )
        })?
    };
    let mut canvas = Canvas::new_with_text_context(renderer, fonts.text.clone())?;
    canvas.set_size(context.size.width, context.size.height, 1.);
    canvas.clear_rect(
        0,
        0,
        context.size.width,
        context.size.height,
        Color::rgb(8, 10, 18),
    );
    canvas.flush();
    unsafe {
        context.gl.finish();
    }
    context.check()?;
    Ok(canvas)
}

struct Measurement {
    build: Time,
    upload: Time,
    submit: Time,
    drain: Time,
    total: Time,
    counts: counters::Counts,
    misses: usize,
    uploads: usize,
}
fn measure(
    canvas: &mut GlCanvas,
    cpu_atlas: &mut Option<CpuAtlas>,
    context: &Context,
    fonts: &Fonts,
    rows: &[Row],
) -> Result<Measurement, Error> {
    let before = cpu_atlas.as_ref().map_or(0, CpuAtlas::len);
    // Parser setup is outside frame timing, as are font I/O/shaping-cache
    // population and Canvas creation for BOTH producers. No atlas is prewarmed.
    let faces = [
        ttf_parser::Face::parse(&fonts.bytes[0], 0)?,
        ttf_parser::Face::parse(&fonts.bytes[1], 0)?,
    ];
    counters::reset();
    let total = Stamp::now()?;
    let start = Stamp::now()?;
    canvas.reset();
    canvas.clear_rect(
        0,
        0,
        context.size.width,
        context.size.height,
        Color::rgb(8, 10, 18),
    );
    let mut commands = Vec::new();
    for row in rows {
        if let Some(atlas) = cpu_atlas {
            let layout = fonts.text.measure_text(row.x, 0., &row.label, &row.paint)?;
            if layout
                .glyphs
                .iter()
                .any(|g| g.font_id != fonts.ids[row.font])
            {
                return Err("unexpected font fallback".into());
            }
            commands.push(atlas.commands(canvas, &faces[row.font], row.font, row.size, &layout)?);
        } else {
            canvas.save();
            canvas.translate(row.origin.0, row.origin.1);
            canvas.fill_text(row.x, 0., &row.label, &row.paint)?;
            canvas.restore();
        }
    }
    let build = start.elapsed()?;
    let start = Stamp::now()?;
    let uploads = if let Some(atlas) = cpu_atlas {
        atlas.upload(canvas)?
    } else {
        0
    };
    let upload = start.elapsed()?;
    let start = Stamp::now()?;
    for (row, commands) in rows.iter().zip(commands) {
        canvas.save();
        canvas.translate(row.origin.0, row.origin.1);
        canvas.draw_glyph_commands(commands, &row.paint, 1.);
        canvas.restore();
    }
    canvas.flush();
    let submit = start.elapsed()?;
    let start = Stamp::now()?;
    // Completion latency, deliberately not a pipelined throughput estimate.
    unsafe {
        context.gl.finish();
    }
    let drain = start.elapsed()?;
    let total = total.elapsed()?;
    let counts = counters::counts();
    context.check()?;
    Ok(Measurement {
        build,
        upload,
        submit,
        drain,
        total,
        counts,
        uploads,
        misses: cpu_atlas.as_ref().map_or(0, CpuAtlas::len) - before,
    })
}

pub(super) fn run(options: &Options, context: Rc<Context>) -> Result<(), Error> {
    let start = Stamp::now()?;
    let fonts = Fonts::new(
        options
            .gpu_text_font_regular
            .as_ref()
            .ok_or("regular font required")?,
        options
            .gpu_text_font_bold
            .as_ref()
            .ok_or("bold font required")?,
    )?;
    let mut fixtures = BTreeMap::new();
    for stage in STAGES {
        fixtures.insert(stage, fixture(&fonts, &context, stage)?);
    }
    let setup = start.elapsed()?;
    eprintln!(
        "mask_probe_version=1 setup_wall_ms={:.6} setup_cpu_ms={:.6} shape_cache=warm atlas=fresh_per_block format=rgba8 filtering=nearest upload=dirty_row_bands max_pages=4 production_renderer=false",
        setup.wall, setup.cpu
    );
    println!(
        "repeat,mode,stage,build_wall_ms,build_cpu_ms,upload_wall_ms,upload_cpu_ms,submit_wall_ms,submit_cpu_ms,drain_wall_ms,drain_cpu_ms,complete_wall_ms,complete_cpu_ms,screen_draws,atlas_draws,buffer_bytes,texture_store_bytes,texture_upload_bytes,mask_misses,upload_calls,pages"
    );
    let mut reference = BTreeMap::<(&str, &str), Vec<u8>>::new();
    let mut checked = 0;
    for repeat in 0..options.presentation_repeats {
        for mode in if repeat % 2 == 0 {
            ["vector", "cpu-mask"]
        } else {
            ["cpu-mask", "vector"]
        } {
            let mut canvas = canvas(&context, &fonts)?;
            let mut atlas = (mode == "cpu-mask").then(CpuAtlas::default);
            for stage in STAGES {
                let m = measure(&mut canvas, &mut atlas, &context, &fonts, &fixtures[stage])?;
                println!(
                    "{repeat},{mode},{stage},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{},{},{},{}",
                    m.build.wall,
                    m.build.cpu,
                    m.upload.wall,
                    m.upload.cpu,
                    m.submit.wall,
                    m.submit.cpu,
                    m.drain.wall,
                    m.drain.cpu,
                    m.total.wall,
                    m.total.cpu,
                    m.counts.screen_draws,
                    m.counts.atlas_draws,
                    m.counts.buffer_bytes,
                    m.counts.texture_store_bytes,
                    m.counts.texture_upload_bytes,
                    m.misses,
                    m.uploads,
                    atlas.as_ref().map_or(0, CpuAtlas::pages)
                );
                if m.counts.screen_draws != 14 {
                    return Err(format!("expected 14 text rows, got {:?}", m.counts).into());
                }
                if mode == "cpu-mask" && m.counts.atlas_draws != 0 {
                    return Err("CPU path generated vector atlas work".into());
                }
                let cached = matches!(
                    stage,
                    "repeat"
                        | "item-half-pixel"
                        | "item-repeat"
                        | "layout-repeat"
                        | "score-repeat"
                        | "restore"
                );
                if cached && (m.counts.atlas_draws != 0 || m.misses != 0 || m.uploads != 0) {
                    return Err(format!("unexpected cache work: {mode}/{stage}").into());
                }
                if stage == "first"
                    && ((mode == "vector" && m.counts.atlas_draws == 0)
                        || (mode == "cpu-mask" && m.misses == 0))
                {
                    return Err("fresh atlas did no work".into());
                }
                let pixels = context.pixels()?;
                if let Some(expected) = reference.get(&(mode, stage)) {
                    if pixels != *expected {
                        return Err(format!("nonrepeatable {mode}/{stage}").into());
                    }
                    checked += 1;
                } else {
                    reference.insert((mode, stage), pixels.clone());
                }
                if let Some(path) = options.gpu_text_output.as_ref().filter(|_| repeat == 0) {
                    save(
                        &path.join(format!("{mode}-{stage}.png")),
                        context.size,
                        &pixels,
                    )?;
                }
            }
            drop(atlas);
            drop(canvas);
            context.check()?;
        }
    }
    for mode in ["vector", "cpu-mask"] {
        for stage in ["repeat", "restore"] {
            if reference[&(mode, "first")] != reference[&(mode, stage)] {
                return Err(format!("{mode} warm/restored output differs").into());
            }
        }
    }
    for stage in STAGES {
        quality(
            &reference[&("vector", stage)],
            &reference[&("cpu-mask", stage)],
            &context,
            stage,
        )?;
    }
    eprintln!(
        "mask_validation exact_within_mode_comparisons={checked} cross_mode=coverage_and_translation_not_exact timings_exclude_readback=true"
    );
    Ok(())
}

// Quality diagnostics use the blue channel: both fixture colors have B=255.
// Restrict error metrics to text support rather than diluting them with the
// mostly blank canvas. These are gross placement/missing-text guards, not a
// substitute for visually judging antialiasing at native size.
fn quality(
    reference: &[u8],
    candidate: &[u8],
    context: &Context,
    stage: &str,
) -> Result<(), Error> {
    let width = context.size.width as usize;
    let top = ((context.size.height as f32 - 420.) / 2.).floor() as usize;
    let mut max_centroid = 0f64;
    let mut min_ratio = f64::INFINITY;
    let mut max_ratio = 0f64;
    let mut error = 0u64;
    let mut support = 0u64;
    for row in 0..14 {
        let mut mass = [0f64; 2];
        let mut mx = [0f64; 2];
        let mut my = [0f64; 2];
        for y in top + row * 28..top + (row + 1) * 28 {
            for x in 0..width {
                let i = (y * width + x) * 4 + 2;
                let values = [
                    reference[i].saturating_sub(18),
                    candidate[i].saturating_sub(18),
                ];
                for (index, value) in values.into_iter().enumerate() {
                    mass[index] += f64::from(value);
                    mx[index] += x as f64 * f64::from(value);
                    my[index] += y as f64 * f64::from(value);
                }
                if values[0] != 0 || values[1] != 0 {
                    support += 1;
                    error += u64::from(values[0].abs_diff(values[1]));
                }
            }
        }
        if mass.iter().any(|m| *m < 1000.) {
            return Err(format!("empty text row {row}/{stage}").into());
        }
        let ratio = mass[1] / mass[0];
        let centroid = (mx[1] / mass[1] - mx[0] / mass[0]).hypot(my[1] / mass[1] - my[0] / mass[0]);
        min_ratio = min_ratio.min(ratio);
        max_ratio = max_ratio.max(ratio);
        max_centroid = max_centroid.max(centroid);
        if !(0.8..=1.2).contains(&ratio) {
            return Err(format!(
                "text coverage guard failed: {stage} row={row} mass_ratio={ratio}"
            )
            .into());
        }
        check_translation(
            reference,
            candidate,
            width,
            top + row * 28,
            top + (row + 1) * 28,
        )?;
    }
    if error as f64 / support as f64 > 30. {
        return Err(format!("excessive mask edge error for {stage}").into());
    }
    eprintln!(
        "mask_quality stage={stage} support_pixels={support} mean_blue_error={:.6} row_mass_ratio_min={min_ratio:.6} row_mass_ratio_max={max_ratio:.6} row_centroid_max_px={max_centroid:.6}",
        error as f64 / support as f64
    );
    Ok(())
}

// Brightness centroids can drift even when every glyph is in its correct
// position: differently shaded glyphs change the weight along a long row.
// Instead require zero translation to minimize absolute pixel error among
// the 3x3 integer offsets. Keep centroid data as a diagnostic, not a gate.
fn check_translation(
    a: &[u8],
    b: &[u8],
    width: usize,
    top: usize,
    bottom: usize,
) -> Result<(), Error> {
    let sad = |dx: isize, dy: isize| -> u64 {
        let mut total = 0;
        for y in top..bottom {
            for x in 1..width - 1 {
                let i = (y * width + x) * 4 + 2;
                let j = (((y as isize + dy) as usize) * width + (x as isize + dx) as usize) * 4 + 2;
                total += u64::from(a[i].abs_diff(b[j]));
            }
        }
        total
    };
    let zero = sad(0, 0);
    for dy in -1..=1 {
        for dx in -1..=1 {
            if sad(dx, dy) < zero {
                return Err(format!(
                    "mask is better aligned at offset ({dx},{dy}) for rows {top}..{bottom}"
                )
                .into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translation_guard_accepts_coverage_changes_but_rejects_shift() {
        let width = 20;
        let mut a = vec![0; width * 10 * 4];
        let mut b = a.clone();
        let mut shifted = a.clone();
        for x in [4, 5, 12, 13] {
            a[(5 * width + x) * 4 + 2] = 240;
            b[(5 * width + x) * 4 + 2] = if x < 10 { 200 } else { 240 };
            shifted[(5 * width + x + 1) * 4 + 2] = 240;
        }
        check_translation(&a, &b, width, 2, 8).unwrap();
        assert!(check_translation(&a, &shifted, width, 2, 8).is_err());
    }
}
