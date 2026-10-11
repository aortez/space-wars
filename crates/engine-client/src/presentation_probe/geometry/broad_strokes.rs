//! Reuse the existing Clock/Pizza snapshots and mixed-shape animation to check
//! the exact stroke controls beyond Spacewars. This is not a simulation or
//! end-to-end frame-rate benchmark; source/projection/validation are untimed.
use super::{
    Context, Error, Options, Plan, Validation, Viewport, measure, presentation, selected_modes,
};
use crate::{presentation_probe::geometry::fixtures, render::FrameLayout};
use std::{fs::File, io::Write};

fn history(name: &str, viewport: Viewport) -> Result<(Vec<Plan>, serde_json::Value), Error> {
    let fixture = fixtures::build(name, viewport)?;
    let count = fixture.sequence.len().max(1);
    let plans = (0..count)
        .map(|i| {
            presentation::build_reusing(
                fixture.at(i),
                viewport,
                FrameLayout::EqualHorizontal,
                1.,
                true,
                crate::gpu_scene::PlanMode::Prepared,
                None,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let metadata = serde_json::json!({
        "name": fixture.name, "source_checksum": format!("{:016x}", fixture.checksum),
        "source_tick": fixture.tick, "objects": fixture.objects,
        "omitted_text": fixture.omitted_text, "history_frames": count,
        "source": if count == 1 { "frozen-production-snapshot" } else { "prescribed-animation" },
        "warmup_frames": count.max(super::HISTORY),
    });
    Ok((plans, metadata))
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let viewport = Viewport::new(width as f32, height as f32);
    let modes = selected_modes(options);
    let mut metadata = serde_json::json!({
        "version": 6, "kind": "broad-exact-strokes", "gl": context.identity,
        "width": width, "height": height, "frames": options.presentation_frames,
        "repeats": options.presentation_repeats, "stroke_writer": modes[0].writer.name(),
        "polygon_input_comparison":options.geometry_stroke_polygon,
        "edge_reuse_comparison":options.geometry_stroke_edges,
        "prepared_guard_comparison":options.geometry_stroke_prepared,
        "single_pass_comparison":options.geometry_stroke_single_pass,
        "single_pass_selection":options.geometry_stroke_selection.as_deref().unwrap_or("all"),
        "modes": modes.map(|m| m.name), "geometry": "retained-culled",
        "streams": "bucketed", "plan": "prepared", "fixtures": [],
        "quality": "exact GPU pixels plus vertex bits, indices, batches and retained-cache decisions across fifteen writers x reference/exact/checked; checked math and both single-pass oracles outside timing; quality counters include deliberately skipped emitters",
        "prefix_scope": "prefix pairs original single-pass with same-frame base-position reuse; other selections pair with bulk-static",
        "prefix_accounting": "checked-only: per-state prefix_reuse_check coverage in stderr outside timing; timed CSV topology_reused_prefix_vertices is zero/unmeasured, not evidence of no reuse; no count payload in common mesh results",
        "timing": "paired warm CPU encoding or completed GL drawing; source/projection/simulation/text/UI/readback/scanout excluded; no per-piece clocks",
    });
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let mut validation = Validation::new(context, options)?;
    println!("{}", super::HEADER);
    for name in &options.geometry_stroke_cases {
        let (plans, fixture_metadata) = history(name, viewport)?;
        eprintln!("broad_strokes_fixture={fixture_metadata}");
        metadata["fixtures"]
            .as_array_mut()
            .unwrap()
            .push(fixture_metadata);
        validation.reset();
        // Two whole traversals compare cold and warm histories at every pose,
        // independently of the separate timed caches and GPU presenters.
        for traversal in 0..2 {
            for (i, plan) in plans.iter().enumerate() {
                validation.check(
                    plan,
                    &format!("{name}-{traversal}-{i:03}"),
                    i == plans.len() - 1,
                )?;
            }
        }
        measure(context, options, modes, name, &plans)?;
    }
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    eprintln!(
        "broad_strokes_validation frames={} applied={} max_rgb_mae={:.6} max_channel_error={}",
        validation.quality.frames,
        validation.applied,
        validation.quality.max_mae,
        validation.quality.max_error
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histories_use_frozen_snapshots_and_the_complete_existing_animation() {
        let viewport = Viewport::new(128., 96.);
        for (name, count) in [("clock-idle", 1), ("mixed-interleaved", 32)] {
            let (plans, metadata) = history(name, viewport).unwrap();
            assert_eq!(plans.len(), count);
            assert_eq!(metadata["history_frames"], count);
            assert_eq!(metadata["warmup_frames"], count.max(super::super::HISTORY));
            assert!(
                plans
                    .iter()
                    .all(|p| p.width == 128 && p.height == 96 && !p.passes.is_empty())
            );
            let mut check = super::super::round_join_check::Validation::default();
            for plan in &plans {
                check.check(plan).unwrap();
            }
        }
    }
}
