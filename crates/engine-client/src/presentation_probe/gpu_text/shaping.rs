//! CPU-only shaping fixture. Fresh contexts prevent earlier UI work from
//! accidentally warming the operation under investigation. Font I/O/registration,
//! signatures and output validation are outside all measurement windows.
use super::{Error, Options, Stamp, detail, rows};
use femtovg::{Baseline, FontId, Paint, TextContext, TextMetrics, diagnostics};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

const STAGES: [(&str, u32, f32, f32); 8] = [
    ("fresh", 1, 560., 0.),
    ("repeat", 1, 560., 0.),
    ("width-change", 1, 561., 0.),
    ("width-repeat", 1, 561., 0.),
    ("score-change", 2, 560., 0.),
    ("score-repeat", 2, 560., 0.),
    ("size-change", 1, 560., 1.),
    ("size-repeat", 1, 560., 1.),
];
struct WordDetail;
impl Drop for WordDetail {
    fn drop(&mut self) {
        diagnostics::set_word_detail(false);
        detail::enable(false);
    }
}
struct Input {
    text: String,
    paint: Paint,
    width: f32,
}
fn inputs(fonts: [FontId; 2], round: u32, width: f32, size_delta: f32) -> Vec<Input> {
    rows(round)
        .into_iter()
        .map(|r| {
            let mut paint = Paint::default();
            paint.set_font(&[fonts[usize::from(r.weight >= 700)]]);
            paint.set_font_size(r.size + size_delta);
            paint.set_text_baseline(Baseline::Top);
            Input {
                text: r.label.to_string(),
                paint,
                width,
            }
        })
        .collect()
}
fn exercise(
    context: &TextContext,
    inputs: &[Input],
) -> Result<Vec<(TextMetrics, usize, TextMetrics)>, Error> {
    let mut outputs = Vec::with_capacity(inputs.len());
    for input in inputs {
        let whole = context.measure_text(0., 0., &input.text, &input.paint)?;
        let end = context.break_text(input.width, &input.text, &input.paint)?;
        let prefix = input.text.get(..end).ok_or("invalid UTF-8 line boundary")?;
        let line = context.measure_text(0., 0., prefix, &input.paint)?;
        outputs.push((whole, end, line));
    }
    Ok(outputs)
}
pub(super) fn run(options: &Options) -> Result<(), Error> {
    let policy = crate::gpu_text::Mode::parse(&options.gpu_text_policy)?;
    femtovg::request_glyph_cache(policy.into());
    let regular: Arc<[u8]> = std::fs::read(
        options
            .gpu_text_font_regular
            .as_ref()
            .ok_or("regular font required")?,
    )?
    .into();
    let bold: Arc<[u8]> = std::fs::read(
        options
            .gpu_text_font_bold
            .as_ref()
            .ok_or("bold font required")?,
    )?
    .into();
    eprintln!(
        "text_shaping_probe_version=1 cpu_only=true fresh_context_per_repeat=true font_registration_timed=false word_detail={} policy={} glyph_atlas=false gpu=false",
        options.gpu_text_detail,
        policy.name()
    );
    detail::enable(options.gpu_text_detail);
    diagnostics::set_word_detail(options.gpu_text_detail);
    let _word_detail = WordDetail;
    print!("repeat,mode,stage,wall_ms,cpu_ms,rows,signature");
    detail::header();
    let mut reference = BTreeMap::new();
    for repeat in 0..options.presentation_repeats {
        let context = TextContext::default();
        let fonts = [
            context.add_shared_font_with_index(regular.clone(), 0)?,
            context.add_shared_font_with_index(bold.clone(), 0)?,
        ];
        let mut previous = String::new();
        for (stage, round, width, size_delta) in STAGES {
            let input = inputs(fonts, round, width, size_delta);
            let capture = detail::Capture::begin();
            let start = Stamp::now()?;
            let output = exercise(&context, &input)?;
            let time = start.elapsed()?;
            let detail = capture.finish();
            let signature = format!("{output:?}");
            if (stage == "repeat" || stage.ends_with("-repeat")) && signature != previous {
                return Err(format!("warm shaping changed output: {stage}").into());
            }
            if let Some(expected) = reference.get(stage) {
                if expected != &signature {
                    return Err(format!("fresh context changed output: {stage}").into());
                }
            } else {
                reference.insert(stage, signature.clone());
            }
            let mut hash = std::hash::DefaultHasher::new();
            signature.hash(&mut hash);
            print!(
                "{repeat},{},{stage},{:.6},{:.6},{},{:016x}",
                policy.name(),
                time.wall,
                time.cpu,
                input.len(),
                hash.finish()
            );
            detail.report();
            previous = signature;
        }
    }
    eprintln!(
        "text_shaping_validation stages={} fresh_contexts={} exact_metrics=true",
        STAGES.len(),
        options.presentation_repeats
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_contexts_and_warm_calls_produce_identical_glyph_metrics() {
        let bytes = include_bytes!("../../../../../scenarios/clock/fonts/DejaVuSans.ttf");
        let mut expected = None;
        for _ in 0..2 {
            let context = TextContext::default();
            let font = context.add_font_mem(bytes).unwrap();
            let inputs = inputs([font; 2], 1, 90., 0.);
            let cold = format!("{:?}", exercise(&context, &inputs).unwrap());
            let warm = format!("{:?}", exercise(&context, &inputs).unwrap());
            assert_eq!(cold, warm);
            if let Some(expected) = &expected {
                assert_eq!(expected, &cold);
            }
            expected = Some(cold);
        }
    }
}
