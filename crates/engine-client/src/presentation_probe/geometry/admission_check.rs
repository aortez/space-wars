//! Image correctness with independent cache histories, not warm-cache switching.
//! Bounded prepared production inputs keep definitions alive throughout a replay;
//! this is a quality check, not live producer lifetime or performance evidence.
use super::{Context, Error, save_png, stream_input as input};
use crate::{
    gpu_scene::{
        AdmissionMode, CompositePresenter, Mode, retained::Stats, streaming,
        stroke_culling::Control, stroke_output::StrokeWriter,
    },
    presentation_probe::{Options, geometry},
    render::Viewport,
};
use glow::HasContext;
use std::{fs::File, io::Write};

const HISTORY: usize = 96;
const FIXTURES: [&str; 5] = [
    "bots-early",
    "bots-late",
    "close-up-zoom",
    "terrain-reveal-zoom",
    "terrain-edit-replace",
];
const POLICIES: [AdmissionMode; 2] = [AdmissionMode::Repeated, AdmissionMode::Settled];

#[derive(Clone, Copy)]
struct Step {
    frame: usize,
    retire: bool,
}

fn schedule() -> Vec<Step> {
    let mut steps = Vec::new();
    // Revisit scales without clearing caches; pause long enough to cross the
    // third-presentation threshold, then resume moving and resize again.
    for _ in 0..2 {
        for frame in 0..HISTORY {
            steps.push(Step {
                frame,
                retire: false,
            });
            if frame == 47 || frame == HISTORY - 1 {
                steps.extend(
                    [Step {
                        frame,
                        retire: false,
                    }; 6],
                );
            }
        }
    }
    // Actual resource retirement resets policy. Reapply it as the host does,
    // and check first-use as well as promotion after resource recreation.
    for i in 0..6 {
        steps.push(Step {
            frame: HISTORY - 1,
            retire: i == 0,
        });
    }
    steps
}

fn error(reference: &[u8], actual: &[u8]) -> Result<(f64, f64), Error> {
    let rgb: Vec<_> = reference
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    Ok(geometry::difference(&rgb, actual)?)
}

fn outside_gate(e: (f64, f64)) -> bool {
    // Preserve the existing moving-retained and terrain-image gate.
    e.0 > 0.05 || e.1 > 0.001
}

fn presenter(context: &Context, policy: AdmissionMode) -> Result<CompositePresenter, Error> {
    let mut r = CompositePresenter::new(context.gl.clone())?;
    configure(&mut r, policy);
    Ok(r)
}

fn configure(r: &mut CompositePresenter, policy: AdmissionMode) {
    r.set_admission(policy);
    r.set_stream_policy(streaming::Policy::PassBucketed);
    r.set_stroke_culling(Control::Conservative);
    r.set_round_join_mode(lyon_tessellation::RoundJoinMode::Exact);
    r.set_stroke_writer(StrokeWriter::BulkStatic);
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    input::limits(HISTORY as u32, width, height, 3)?;
    let steps = schedule();
    let metadata = serde_json::json!({
        "version":1,"kind":"admission-history-quality","gl":context.identity,
        "width":width,"height":height,"source_frames":HISTORY,"presentations_per_fixture":steps.len(),
        "fixtures":FIXTURES,"policies":POLICIES.map(|p|p.name()),
        "reference":"ordinary triangles, independent presenter",
        "controls":"retained-culled, bucketed, conservative strokes, exact joins, bulk-static writer",
        "gate":{"rgb_mae":0.05,"pixel_error_over_30_fraction":0.001},
        "history":"two moving 96-frame cycles, six extra holds after 47/95, then retirement and six stopped frames",
        "events":"empty frame 22, nine-pass overflow 23, smaller viewport 64..72; terrain edits/replacement",
        "timing":false,"text_ui_scanout":false,"prepared_payload_limit_bytes":256*1024*1024
    });
    eprintln!("admission_check={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    println!(
        "fixture,presentation,source_frame,retired,policy,mae,different_fraction,pair_mae,pair_different_fraction,exact_pair,hits,admissions,promotions,evictions,entries,bytes,guarded_polygons"
    );
    let mut total = [Stats::default(); 2];
    let mut divergent_decisions = 0;
    let mut exact_pairs = 0;
    let mut worst = [0.0_f64; 4];
    for name in FIXTURES {
        let sequence = input::prepare(
            name,
            Viewport::new(width as f32, height as f32),
            HISTORY as u32,
            true,
        )?;
        let mut ordinary = CompositePresenter::new(context.gl.clone())?;
        let mut cached = [
            presenter(context, POLICIES[0])?,
            presenter(context, POLICIES[1])?,
        ];
        let mut previous = None;
        let mut changing_images = 0;
        for (index, step) in steps.iter().enumerate() {
            let plan = &sequence.frames[step.frame];
            if step.retire {
                ordinary.retire();
                for (r, policy) in cached.iter_mut().zip(POLICIES) {
                    r.retire();
                    configure(r, policy);
                }
            }
            ordinary.draw_with_mode(plan, Mode::Triangles)?;
            let reference = ordinary.readback()?;
            let mut pixels = [Vec::new(), Vec::new()];
            let mut stats = [Stats::default(); 2];
            for (i, r) in cached.iter_mut().enumerate() {
                stats[i] = r.draw_with_mode(plan, Mode::RetainedCulled)?.mesh.retained;
                if stats[i].entries > 128
                    || stats[i].bytes > 16 * 1024 * 1024
                    || stats[i].probation_entries > 128
                {
                    return Err(format!("admission bounds exceeded {name} frame={index}").into());
                }
                total[i].accumulate(stats[i]);
                pixels[i] = r.readback()?;
            }
            let errors = [
                error(&reference, &pixels[0])?,
                error(&reference, &pixels[1])?,
            ];
            let pair = error(&pixels[0], &pixels[1])?;
            exact_pairs += usize::from(pixels[0] == pixels[1]);
            divergent_decisions += usize::from(
                stats[0].admissions != stats[1].admissions
                    || stats[0].hybrid_exact_groups != stats[1].hybrid_exact_groups,
            );
            let failed = errors.iter().copied().chain([pair]).any(outside_gate);
            let metrics = [
                errors[0].0.max(errors[1].0),
                errors[0].1.max(errors[1].1),
                pair.0,
                pair.1,
            ];
            let new_worst = metrics.iter().zip(worst).any(|(value, old)| *value > old);
            for (old, value) in worst.iter_mut().zip(metrics) {
                *old = old.max(value);
            }
            if (failed
                || new_worst
                || [0, 47, 50, 69, 70, 77, 78, 107, 108, 216, 218, 221].contains(&index))
                && let Some(path) = &options.geometry_output
            {
                for (label, data) in [
                    ("triangles", &reference),
                    ("repeated", &pixels[0]),
                    ("settled", &pixels[1]),
                ] {
                    save_png(
                        &path.join(format!("{name}-{index:03}-{label}.png")),
                        plan.width,
                        plan.height,
                        png::ColorType::Rgba,
                        data,
                    )?;
                }
            }
            for i in 0..2 {
                let s = stats[i];
                println!(
                    "{name},{index},{},{},{},{:.9},{:.9},{:.9},{:.9},{},{},{},{},{},{},{},{}",
                    step.frame,
                    step.retire,
                    POLICIES[i].name(),
                    errors[i].0,
                    errors[i].1,
                    pair.0,
                    pair.1,
                    pixels[0] == pixels[1],
                    s.hits,
                    s.admissions,
                    s.promotions,
                    s.evictions,
                    s.entries,
                    s.bytes,
                    s.zoom_fallback_polygons
                );
            }
            if failed {
                return Err(format!("admission image mismatch {name} presentation={index}: reference={errors:?}, pair={pair:?}").into());
            }
            if previous.as_ref().is_some_and(|p| p != &reference) {
                changing_images += 1;
            }
            previous = Some(reference);
            ordinary.collect_retired();
            for r in &mut cached {
                r.collect_retired();
            }
            let gl_error = unsafe { context.gl.get_error() };
            if gl_error != glow::NO_ERROR {
                return Err(format!("admission GL error {gl_error:#x}").into());
            }
        }
        if changing_images < HISTORY {
            return Err(format!("admission fixture {name} did not move sufficiently").into());
        }
        eprintln!(
            "validated admission fixture={name} presentations={} changing_images={changing_images} payload_bytes={}",
            steps.len(),
            sequence.payload_bytes
        );
    }
    if divergent_decisions == 0
        || total.iter().any(|s| {
            s.hits == 0 || s.admissions == 0 || s.promotions == 0 || s.zoom_fallback_polygons == 0
        })
    {
        return Err("admission check did not exercise differing decisions, promotion, reuse and guarded fallback".into());
    }
    eprintln!(
        "validated admission presentations={} comparisons={} divergent_decisions={divergent_decisions} exact_pairs={exact_pairs} max_reference_mae={:.9} max_reference_different={:.9} max_pair_mae={:.9} max_pair_different={:.9}",
        steps.len() * FIXTURES.len(),
        steps.len() * FIXTURES.len() * 3,
        worst[0],
        worst[1],
        worst[2],
        worst[3]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_revisits_every_frame_holds_across_admission_and_retires_once() {
        let steps = schedule();
        assert_eq!(steps.len(), 222);
        assert!(steps.iter().all(|s| s.frame < HISTORY));
        assert_eq!(steps.iter().filter(|s| s.retire).count(), 1);
        assert!(steps[216].retire);
        for start in [0, 108] {
            assert_eq!(steps[start].frame, 0);
            assert!(steps[start + 47..start + 54].iter().all(|s| s.frame == 47));
            assert_eq!(steps[start + 54].frame, 48);
            assert_eq!(steps[start + 70].frame, 64);
        }
        assert!(steps[216..].iter().all(|s| s.frame == 95));
    }
}
