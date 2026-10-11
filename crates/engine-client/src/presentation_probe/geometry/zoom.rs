//! Adversarial outline/parity checks for the scale-independent cache. Small
//! targets keep edge errors visible to the same tight gate as the real replay.
use super::{Context, Error, save_png};
use crate::{
    gpu_scene::{
        TrianglePresenter,
        plan::{Draw, Scene, Shape},
        retained::{Definition, Placement, Policy},
    },
    presentation_probe::{Options, geometry},
};
use engine_common::{Fill, RenderColor, RenderPoint, RenderPolygon, Stroke};
use std::sync::Arc;

pub(super) fn validate(context: &Context, options: &Options) -> Result<(), Error> {
    type Case<'a> = (&'a str, &'a [(f32, f32)], f32);
    let cases: &[Case<'_>] = &[
        (
            "square",
            &[(-3., -3.), (3., -3.), (3., 3.), (-3., 3.)],
            0.75,
        ),
        (
            "concave",
            &[(-3., -3.), (3., -3.), (0., 0.), (3., 3.), (-3., 3.)],
            2.,
        ),
        ("needle", &[(-3., -0.2), (3., 0.), (-3., 0.2)], 3.),
        (
            "unequal-edges",
            &[(-3., -0.5), (3., -0.5), (3., 0.5), (-3., 0.5)],
            2.,
        ),
        (
            "shallow-turns",
            &[
                (-2., -3.),
                (2., -3.),
                (3., -2.),
                (3., 2.),
                (2., 3.),
                (-2., 3.),
                (-3., 2.),
                (-3., -2.),
            ],
            12.,
        ),
        (
            "duplicate",
            &[(-3., -3.), (3., -3.), (3., -3.), (0., 3.)],
            1.,
        ),
        (
            "crossing",
            &[(-3., -3.), (3., 3.), (-3., 3.), (3., -3.)],
            1.,
        ),
        ("wide", &[(-3., -3.), (3., -3.), (3., 3.), (-3., 3.)], 12.),
        (
            "repeated-concave",
            &[(-3., -3.), (3., -3.), (0., 0.), (3., 3.), (-3., 3.)],
            2.,
        ),
        (
            "fill-only",
            &[(-3., -3.), (3., -3.), (0., 0.), (3., 3.), (-3., 3.)],
            0.,
        ),
    ];
    let mut ordinary = TrianglePresenter::new(context.gl.clone())?;
    let (mut reused, mut fallback, mut hits, mut checked) = (0, 0, 0, 0);
    let mut promotions = 0;
    let mut build_reuses = 0;
    let mut worst = (0.0_f64, 0.0_f64);
    for (label, policy) in [
        ("zoom", Policy::ScaleIndependent),
        ("hybrid", Policy::Hybrid),
        ("outlines", Policy::HybridOutlines),
        ("paced", Policy::HybridPaced),
        ("memoized", Policy::HybridMemoized),
        ("scan", Policy::HybridScan),
        ("ranges", Policy::HybridRanges),
        ("culled", Policy::HybridCulled),
        ("resident", Policy::HybridResident),
        ("resident-passes", Policy::HybridResidentPasses),
        ("resident-buffers", Policy::HybridResidentBuffers),
        ("resident-uniforms", Policy::HybridResidentUniforms),
    ] {
        if !policy.supported(&context.gl) {
            eprintln!("outline mode={label} unsupported on this device; skipped, not a pass");
            continue;
        }
        let mut cached = TrianglePresenter::new(context.gl.clone())?;
        cached.set_retention(Some(policy));
        let mut mode_worst = (0.0_f64, 0.0_f64);
        for &(name, points, width) in cases {
            let definition = Arc::new(Definition::new(
                (0..if name == "repeated-concave" { 4 } else { 3 })
                    .map(|layer| RenderPolygon {
                        points: points
                            .iter()
                            .map(|&(x, y)| {
                                RenderPoint::new(
                                    x + layer as f32
                                        * if name == "repeated-concave" { 1. } else { 0.8 },
                                    y,
                                )
                            })
                            .collect(),
                        fill: Some(Fill::new(RenderColor::rgba(
                            0.2,
                            (layer % if name == "repeated-concave" { 2 } else { 3 }) as f32 * 0.4,
                            0.9,
                            0.4,
                        ))),
                        stroke: (width > 0.)
                            .then_some(Stroke::new(RenderColor::rgba(1., 0.3, 0.1, 0.6), width)),
                    })
                    .collect(),
            )?);
            let mut scales = vec![0.02, 0.1, 0.3, 0.7, 1., 1.9, 2.01, 5., 12., 25.];
            if width > 0.
                && let Some((old, new)) =
                    crate::gpu_scene::scalable::stroke_domains(&definition.polygons()[0])
            {
                for guard in [old, new] {
                    // Also sample around the actual merge/fold transition,
                    // which lies at half the guarded 2x threshold.
                    scales.extend([0.49, 0.51, 0.99, 1.01].map(|factor| factor * guard));
                }
            }
            scales.sort_by(f32::total_cmp);
            scales.dedup();
            for scale in scales {
                for angle in [0.0_f32, 0.37, 1.57] {
                    let p = Placement {
                        first: 0,
                        definition: definition.clone(),
                        scale,
                        angle,
                        translation: RenderPoint::new(160.25, 120.125),
                    };
                    let (s, c) = angle.sin_cos();
                    let draws = definition
                        .polygons()
                        .iter()
                        .map(|d| Draw {
                            shape: Shape::Polygon(
                                d.points
                                    .iter()
                                    .map(|v| {
                                        RenderPoint::new(
                                            p.translation.x + scale * (c * v.x - s * v.y),
                                            p.translation.y - scale * (s * v.x + c * v.y),
                                        )
                                    })
                                    .collect(),
                            ),
                            fill: d.fill.map(|f| f.color),
                            stroke: d.stroke,
                        })
                        .collect();
                    let scene = Scene {
                        width: 320,
                        height: 240,
                        draws,
                        retained: vec![p],
                        reuse: vec![],
                    };
                    ordinary.draw(&scene)?;
                    let reference = ordinary.readback()?;
                    let output = cached.draw(&scene)?;
                    let actual = cached.readback()?;
                    reused += output.mesh.retained.zoom_reused_polygons;
                    build_reuses += output.mesh.retained.build_reuses;
                    fallback += output.mesh.retained.zoom_fallback_polygons;
                    hits += output.mesh.retained.hits;
                    promotions += output.mesh.retained.promotions;
                    let rgb: Vec<_> = reference
                        .chunks_exact(4)
                        .flat_map(|p| p[..3].iter().copied())
                        .collect();
                    let error = geometry::difference(&rgb, &actual)?;
                    mode_worst.0 = mode_worst.0.max(error.0);
                    mode_worst.1 = mode_worst.1.max(error.1);
                    let new_worst = error.0 > worst.0 || error.1 > worst.1;
                    worst.0 = worst.0.max(error.0);
                    worst.1 = worst.1.max(error.1);
                    let failed = error.0 > 0.05 || error.1 > 0.001;
                    if (failed || new_worst || (scale == 12. && angle == 0.37))
                        && let Some(path) = &options.geometry_output
                    {
                        for (label, pixels) in [(label, &actual), ("reference", &reference)] {
                            save_png(
                                &path.join(format!("outline-{name}-{checked}-{label}.png")),
                                320,
                                240,
                                png::ColorType::Rgba,
                                pixels,
                            )?;
                        }
                    }
                    if failed {
                        return Err(format!(
                            "{label} outline parity failed {name} scale={scale} angle={angle}: {error:?}"
                        ).into());
                    }
                    if new_worst {
                        eprintln!(
                            "outline worst label={label} case={name} scale={scale:.8} angle={angle} check={checked} error={error:?}"
                        );
                    }
                    checked += 1;
                }
            }
        }
        eprintln!(
            "outline mode={label} max_mae={:.8} max_different={:.8}",
            mode_worst.0, mode_worst.1
        );
    }
    if reused == 0 || fallback == 0 || hits == 0 || promotions == 0 || build_reuses == 0 {
        return Err("zoom outline checks did not exercise reuse and guarded fallback".into());
    }
    eprintln!(
        "validated zoom outlines={checked} reused_polygons={reused} fallback_polygons={fallback} hits={hits} promotions={promotions} max_mae={:.8} max_different={:.8}",
        worst.0, worst.1
    );
    validate_uniform_overflow(context)?;
    Ok(())
}

fn validate_uniform_overflow(context: &Context) -> Result<(), Error> {
    if !Policy::HybridResidentUniforms.supported(&context.gl) {
        return Ok(());
    }
    let definition = Arc::new(Definition::new(vec![RenderPolygon {
        points: vec![
            RenderPoint::ZERO,
            RenderPoint::new(3., 0.),
            RenderPoint::new(0., 3.),
        ],
        fill: Some(Fill::new(RenderColor::rgba(0.3, 0.9, 0.7, 0.6))),
        stroke: None,
    }])?);
    let mut reference = TrianglePresenter::new(context.gl.clone())?;
    let mut candidate = TrianglePresenter::new(context.gl.clone())?;
    candidate.set_retention(Some(Policy::HybridResidentUniforms));
    for count in [511, 512, 513, 600, 4096, 12] {
        let mut scene = Scene {
            width: 320,
            height: 240,
            draws: Vec::new(),
            retained: Vec::new(),
            reuse: Vec::new(),
        };
        for i in 0..count {
            let x = (i % 80) as f32 * 4. + 0.25;
            let y = (i / 80) as f32 * 4. + 0.25;
            let points = definition.polygons()[0]
                .points
                .iter()
                .map(|p| RenderPoint::new(x + p.x, y - p.y))
                .collect();
            scene.retained.push(Placement {
                first: scene.draws.len(),
                definition: definition.clone(),
                scale: 1.,
                angle: 0.,
                translation: RenderPoint::new(x, y),
            });
            scene.draws.push(Draw {
                shape: Shape::Polygon(points),
                fill: definition.polygons()[0].fill.map(|f| f.color),
                stroke: None,
            });
        }
        reference.draw(&scene)?;
        let expected = reference.readback()?;
        let output = candidate.draw(&scene)?;
        let actual = candidate.readback()?;
        if output.mesh.resident.placements != count || output.mesh.resident.fallback_groups != 0 {
            return Err("uniform overflow changed placement admission".into());
        }
        // All triangles have the same exact raster geometry in this fixture.
        if expected != actual {
            return Err(
                format!("uniform placement overflow/shrink parity failed at {count}").into(),
            );
        }
    }
    eprintln!(
        "validated uniform placement table: 6 exact images including 511/512/513 boundary, 4096 placements, shrink; overflow fetches texture without extra draws"
    );
    Ok(())
}
