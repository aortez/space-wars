//! Feasibility audit, NOT a cache policy. Ask how many actual guarded outlines
//! could have a second, folded-stroke template, with a conservative unmerged
//! scale interval. Test the hypothesis against ordinary Lyon at other scales,
//! rotations and screen positions. Sampling cannot prove a reuse domain safe.
//! Compiled only into the test harness; no live/benchmark renderer is changed.
use super::*;
use lyon_tessellation::{
    BuffersBuilder, StrokeTessellator, StrokeVertex,
    math::{Point, point},
};

#[derive(Clone, Copy, Debug)]
struct Band {
    low: f64,
    high: f64,
    folds: usize,
}

/// Geometry is already projected; scale 1 is the captured frame. A candidate
/// must stay 2x away from merging and either side of each folding transition.
/// Near-right-angle turns cannot be classified reliably after f32 projection:
/// keep the existing large-scale restriction there. This deliberately does not
/// address merged endpoints, curve flattening or nearly reversing turns.
fn band(points: &[Point], width: f32) -> Result<Band, &'static str> {
    if !(3..=256).contains(&points.len())
        || !width.is_finite()
        || width <= 0.
        || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err("unsupported");
    }
    let half = f64::from(width) * 0.5;
    let merge = (0.25_f64.powi(2) * 0.5)
        .min(f64::from(width).powi(2) * 0.05)
        .max(1e-8)
        .sqrt();
    let mut result = Band {
        low: 0.,
        high: f64::INFINITY,
        folds: 0,
    };
    let mut right_angle = false;
    let mut fold_transition = false;
    for i in 0..points.len() {
        let a = points[(i + points.len() - 1) % points.len()];
        let b = points[i];
        let c = points[(i + 1) % points.len()];
        let u = [
            f64::from(b.x) - f64::from(a.x),
            f64::from(b.y) - f64::from(a.y),
        ];
        let v = [
            f64::from(c.x) - f64::from(b.x),
            f64::from(c.y) - f64::from(b.y),
        ];
        let lu = u[0].hypot(u[1]);
        let lv = v[0].hypot(v[1]);
        if lu == 0. || lv == 0. {
            return Err("unsupported");
        }
        result.low = result.low.max(2. * merge / lu.min(lv));
        let dot = ((u[0] * v[0] + u[1] * v[1]) / (lu * lv)).clamp(-1., 1.);
        if dot <= -0.9999 {
            return Err("unsupported");
        }
        if dot > 1e-4 {
            continue;
        }
        let transition = half * ((1. - dot) / (1. + dot)).sqrt() / lu.max(lv);
        if dot >= -1e-4 {
            right_angle |= 1. <= 2. * transition;
            result.low = result.low.max(2. * transition);
        } else if 1. < transition * 0.5 {
            result.high = result.high.min(transition * 0.5);
            result.folds += 1;
        } else if 1. > transition * 2. {
            result.low = result.low.max(transition * 2.);
        } else {
            fold_transition = true;
        }
    }
    // Mutually exclusive, ordered rejection reasons; not independent shares.
    if right_angle {
        return Err("right-angle-margin");
    }
    if result.low >= 1. {
        return Err("merge-margin");
    }
    if fold_transition {
        return Err("fold-transition");
    }
    if result.folds == 0 {
        return Err("already-unfolded");
    }
    assert!(result.low > 0. && result.low < 1. && result.high > 1.);
    Ok(result)
}

struct Template {
    // Position on source path plus fixed-pixel offset, as in scalable.rs.
    vertices: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

fn options(width: f32) -> StrokeOptions {
    StrokeOptions::default()
        .with_tolerance(0.25)
        .with_line_width(width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

fn template(outline: &Outline, scale: f32) -> Template {
    let origin = outline.points[0];
    let points: Vec<_> = outline
        .points
        .iter()
        .map(|p| point((p.x - origin.x) * scale, (p.y - origin.y) * scale))
        .collect();
    build_template(&points, outline.stroke.width, scale, point(0., 0.))
}

fn build_template(points: &[Point], width: f32, scale: f32, origin: Point) -> Template {
    let mut buffers = VertexBuffers::new();
    StrokeTessellator::new()
        .tessellate_polygon(
            Polygon {
                points,
                closed: true,
            },
            &options(width),
            &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex<'_, '_>| {
                let path = v.position_on_path();
                let offset = v.normal() * (v.line_width() * 0.5);
                [
                    (path.x - origin.x) / scale,
                    (path.y - origin.y) / scale,
                    offset.x,
                    offset.y,
                ]
            }),
        )
        .unwrap();
    Template {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

fn ordinary(points: &[Point], width: f32) -> VertexBuffers<[f32; 2], u32> {
    let mut buffers = VertexBuffers::new();
    StrokeTessellator::new()
        .tessellate_polygon(
            Polygon {
                points,
                closed: true,
            },
            &options(width),
            &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex<'_, '_>| {
                let p = v.position();
                [p.x, p.y]
            }),
        )
        .unwrap();
    buffers
}

fn transformed(p: [f32; 2], scale: f32, angle: f32, origin: Point) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    let x = p[0] * scale;
    let y = p[1] * scale;
    [c * x - s * y + origin.x, s * x + c * y + origin.y]
}

fn expanded(template: &Template, scale: f32, angle: f32, origin: Point) -> Vec<[f32; 2]> {
    template
        .vertices
        .iter()
        .map(|v| {
            transformed(
                [v[0] * scale + v[2], v[1] * scale + v[3]],
                1.,
                angle,
                origin,
            )
        })
        .collect()
}

#[derive(Default)]
struct Check {
    samples: u64,
    topology_differences: u64,
    over_002_px: u64,
    maximum_error: f32,
}
impl Check {
    fn compare(&mut self, a: &[[f32; 2]], ai: &[u32], b: &[[f32; 2]], bi: &[u32]) {
        self.samples += 1;
        assert!(a.iter().chain(b).flatten().all(|v| v.is_finite()));
        if a.len() != b.len() || ai != bi {
            self.topology_differences += 1;
            return;
        }
        let error = a
            .iter()
            .zip(b)
            .flat_map(|(a, b)| (0..2).map(move |i| (a[i] - b[i]).abs()))
            .fold(0.0_f32, f32::max);
        self.maximum_error = self.maximum_error.max(error);
        self.over_002_px += u64::from(error > 0.002);
    }
    fn json(&self) -> serde_json::Value {
        serde_json::json!({"samples":self.samples, "topology_differences":self.topology_differences,
            "over_0_002_px":self.over_002_px, "maximum_coordinate_error_px":self.maximum_error})
    }
}

fn inspect(captures: &[Capture]) -> serde_json::Value {
    let mut reasons = std::collections::BTreeMap::<&str, u64>::new();
    let mut checks = std::collections::BTreeMap::<&str, Check>::new();
    let (mut vertices, mut eligible_vertices, mut eligible, mut folds) = (0, 0, 0, 0);
    let mut examples = Vec::new();
    let mut span = [f64::INFINITY, 0.0_f64];
    for outline in captures.iter().flat_map(|c| &c.outlines) {
        vertices += outline.vertices.len();
        let b = match band(&outline.points, outline.stroke.width) {
            Ok(b) => b,
            Err(reason) => {
                *reasons.entry(reason).or_default() += 1;
                continue;
            }
        };
        eligible += 1;
        folds += b.folds;
        eligible_vertices += outline.vertices.len();
        span[0] = span[0].min(b.high / b.low);
        span[1] = span[1].max(b.high / b.low);
        let template = template(outline, (b.low * b.high).sqrt() as f32);
        let origin = outline.points[0];
        let positions: Vec<_> = outline.vertices.iter().map(|v| [v[0], v[1]]).collect();
        checks.entry("captured").or_default().compare(
            &positions,
            &outline.indices,
            &expanded(&template, 1., 0., origin),
            &template.indices,
        );
        for (label, scale, angle, offset) in [
            ("lower-band", b.low * 1.001, 0., [0., 0.]),
            ("upper-band", b.high / 1.001, 0., [0., 0.]),
            ("zoom-down", 0.99, 0., [0., 0.]),
            ("zoom-up", 1.01, 0., [0., 0.]),
            ("rotate", 1., 0.37, [0., 0.]),
            ("quarter-turn", 1., std::f32::consts::FRAC_PI_2, [0., 0.]),
            ("translate", 1., 0., [111.123, -87.456]),
        ] {
            if scale <= b.low || scale >= b.high {
                continue;
            }
            let target = point(origin.x + offset[0], origin.y + offset[1]);
            let points: Vec<_> = outline
                .points
                .iter()
                .map(|p| {
                    let a = transformed(
                        [p.x - origin.x, p.y - origin.y],
                        scale as f32,
                        angle,
                        target,
                    );
                    point(a[0], a[1])
                })
                .collect();
            let reference = ordinary(&points, outline.stroke.width);
            let actual = expanded(&template, scale as f32, angle, target);
            let check = checks.entry(label).or_default();
            let previous = check.topology_differences + check.over_002_px;
            check.compare(
                &reference.vertices,
                &reference.indices,
                &actual,
                &template.indices,
            );
            if check.topology_differences + check.over_002_px > previous && examples.len() < 4 {
                examples.push(serde_json::json!({"sample":label,"scale":scale,"angle":angle,
                    "width":outline.stroke.width,"source":outline.points.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),
                    "interval":[b.low,b.high],"reference_vertices":reference.vertices.len(),
                    "template_vertices":template.vertices.len()}));
            }
        }
    }
    serde_json::json!({"outlines":captures.iter().map(|c|c.outlines.len()).sum::<usize>(),
        "vertices":vertices,"candidate_outlines":eligible,"candidate_vertices":eligible_vertices,
        "candidate_folded_joins":folds,"candidate_band_ratios":if eligible>0 {Some(span)} else {None},
        "rejections_first_matching":reasons,"checks":checks.iter().map(|(k,v)|(*k,v.json())).collect::<std::collections::BTreeMap<_,_>>(),
        "counterexamples":examples})
}

/// Deliberately unguarded negative control. Capture a recipe at the ORIGINAL
/// screen coordinates (no recentering at construction), then ask whether even
/// nearby zoom/translation preserves it. Never eligible for rendering. Failure
/// counts are measured hazards, not an invitation to use the passing samples as
/// a runtime guard. A topology mismatch need not cause a visible pixel error.
fn unguarded(captures: &[Capture]) -> serde_json::Value {
    let mut checks = std::collections::BTreeMap::<&str, Check>::new();
    let mut examples = Vec::new();
    for outline in captures.iter().flat_map(|c| &c.outlines) {
        let origin = outline.points[0];
        let template = build_template(&outline.points, outline.stroke.width, 1., origin);
        let positions: Vec<_> = outline.vertices.iter().map(|v| [v[0], v[1]]).collect();
        let actual = expanded(&template, 1., 0., origin);
        let captured = checks.entry("captured").or_default();
        captured.compare(&positions, &outline.indices, &actual, &template.indices);
        // Guard the audit itself: this recipe must reproduce the source stroke
        // before any synthetic transformation. Do not hide capture errors in
        // a hypothesis's rejection statistics.
        assert_eq!(captured.topology_differences, 0);
        assert_eq!(captured.over_002_px, 0);
        for (label, scale, angle, offset) in [
            ("zoom-down", 0.99, 0., [0., 0.]),
            ("zoom-up", 1.01, 0., [0., 0.]),
            ("rotate", 1., 0.01, [0., 0.]),
            ("translate-subpixel", 1., 0., [0.125, -0.25]),
            ("translate", 1., 0., [111.123, -87.456]),
        ] {
            let target = point(origin.x + offset[0], origin.y + offset[1]);
            let points: Vec<_> = outline
                .points
                .iter()
                .map(|p| {
                    let a = transformed([p.x - origin.x, p.y - origin.y], scale, angle, target);
                    point(a[0], a[1])
                })
                .collect();
            let reference = ordinary(&points, outline.stroke.width);
            let actual = expanded(&template, scale, angle, target);
            let check = checks.entry(label).or_default();
            let previous = check.topology_differences + check.over_002_px;
            check.compare(
                &reference.vertices,
                &reference.indices,
                &actual,
                &template.indices,
            );
            if check.topology_differences + check.over_002_px > previous && examples.len() < 4 {
                examples.push(serde_json::json!({"sample":label,"scale":scale,"angle":angle,"offset":offset,
                    "width":outline.stroke.width,"source":outline.points.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),
                    "rejection":band(&outline.points,outline.stroke.width).err(),
                    "reference_vertices":reference.vertices.len(),"template_vertices":template.vertices.len(),
                    "reference_indices":reference.indices.len(),"template_indices":template.indices.len()}));
            }
        }
    }
    serde_json::json!({"checks":checks.iter().map(|(k,v)|(*k,v.json())).collect::<std::collections::BTreeMap<_,_>>(),
        "counterexamples":examples})
}

#[test]
fn folded_interval_requires_merge_fold_and_angle_margins() {
    let triangle = [point(0., 0.), point(1., 0.), point(0.5, 0.8660254)];
    let b = band(&triangle, 3.).unwrap();
    assert_eq!(b.folds, 3);
    assert!(b.low < 1. && b.high > 1.);
    assert_eq!(band(&triangle, 2.).unwrap_err(), "fold-transition");
    assert_eq!(band(&triangle, 0.1).unwrap_err(), "already-unfolded");
    assert_eq!(band(&triangle[..2], 3.).unwrap_err(), "unsupported");
    assert_eq!(band(&triangle, f32::NAN).unwrap_err(), "unsupported");
    let square = [point(0., 0.), point(1., 0.), point(1., 1.), point(0., 1.)];
    assert_eq!(band(&square, 3.).unwrap_err(), "right-angle-margin");
    let small = triangle.map(|p| point(p.x * 0.1, p.y * 0.1));
    assert_eq!(band(&small, 3.).unwrap_err(), "merge-margin");
}

#[test]
fn folded_template_retains_synthetic_topology_in_its_candidate_band() {
    use engine_common::{RenderColor, Stroke};
    let outline = Outline {
        points: vec![point(0., 0.), point(1., 0.), point(0.5, 0.8660254)],
        stroke: Stroke::new(RenderColor::WHITE, 3.),
        vertices: vec![],
        indices: vec![],
    };
    let b = band(&outline.points, outline.stroke.width).unwrap();
    let t = template(&outline, (b.low * b.high).sqrt() as f32);
    let mut check = Check::default();
    for scale in [b.low * 1.001, 1., b.high / 1.001] {
        let points: Vec<_> = outline
            .points
            .iter()
            .map(|p| point(p.x * scale as f32, p.y * scale as f32))
            .collect();
        let r = ordinary(&points, 3.);
        check.compare(
            &r.vertices,
            &r.indices,
            &expanded(&t, scale as f32, 0., point(0., 0.)),
            &t.indices,
        );
    }
    assert_eq!(check.topology_differences, 0);
    assert_eq!(check.over_002_px, 0);
}

fn captured_outline(points: &[Point]) -> Outline {
    use engine_common::{RenderColor, Stroke};
    let result = ordinary(points, 0.75);
    Outline {
        points: points.to_vec(),
        stroke: Stroke::new(RenderColor::WHITE, 0.75),
        vertices: result
            .vertices
            .iter()
            .map(|p| [p[0], p[1], 1., 1., 1., 1.])
            .collect(),
        indices: result.indices,
    }
}

#[test]
fn real_merge_crossing_rejects_blind_template_reuse() {
    // bots-late, 1024x768: a 1% zoom-out merges an endpoint.
    let capture = Capture {
        outlines: vec![captured_outline(&[
            point(271.32242, 535.57996),
            point(271.34363, 533.79584),
            point(271.1863, 533.73505),
            point(270.5834, 535.2939),
        ])],
        ..Default::default()
    };
    assert_eq!(
        band(&capture.outlines[0].points, 0.75).unwrap_err(),
        "merge-margin"
    );
    let result = unguarded(&[capture]);
    assert_eq!(result["checks"]["zoom-down"]["topology_differences"], 1);
}

#[test]
fn real_fold_crossing_rejects_blind_template_reuse() {
    // close-up-zoom, 1024x768: the join changes fold state under 1% zoom-out.
    let capture = Capture {
        outlines: vec![captured_outline(&[
            point(1005.36536, 546.7102),
            point(1005.1515, 546.41016),
            point(1004.6779, 546.7476),
        ])],
        ..Default::default()
    };
    assert_eq!(
        band(&capture.outlines[0].points, 0.75).unwrap_err(),
        "fold-transition"
    );
    let result = unguarded(&[capture]);
    assert_eq!(result["checks"]["zoom-down"]["topology_differences"], 1);
}

#[test]
fn real_right_angle_changes_topology_when_only_translated() {
    // bots-late, 800x480: projected f32 rounding changes a corner decision.
    let capture = Capture {
        outlines: vec![captured_outline(&[
            point(186.77505, 215.63063),
            point(187.02226, 215.72528),
            point(186.87161, 215.37845),
        ])],
        ..Default::default()
    };
    assert_eq!(
        band(&capture.outlines[0].points, 0.75).unwrap_err(),
        "right-angle-margin"
    );
    let result = unguarded(&[capture]);
    assert_eq!(result["checks"]["translate"]["topology_differences"], 1);
}

#[test]
fn collapsed_outline_cap_cannot_be_rotated_like_a_path_join() {
    // close-up-zoom, 800x480: all endpoints collapse to an empty round cap.
    // Lyon's cap tessellation is screen-aligned. Reusing its mesh under rotation
    // changes coordinates even though its vertex count/connectivity are stable.
    let capture = Capture {
        outlines: vec![captured_outline(&[
            point(752.3717, 341.4065),
            point(752.3513, 341.3778),
            point(752.30286, 341.4123),
        ])],
        ..Default::default()
    };
    let result = unguarded(&[capture]);
    assert_eq!(result["checks"]["rotate"]["topology_differences"], 0);
    assert_eq!(result["checks"]["rotate"]["over_0_002_px"], 1);
}

#[test]
#[ignore = "manual folded-stroke template feasibility audit, no renderer changes or GPU needed"]
fn audit_guarded_stroke_reuse_domains() {
    let mut rows = Vec::new();
    for (width, height) in [(1024, 768), (800, 480)] {
        let viewport = Viewport::new(width as f32, height as f32);
        for fixture in live_retained::FIXTURES {
            let mut scenario = live_retained::scenario(viewport, fixture).unwrap();
            let layout = scenario.frame_layout();
            let mut plans = Vec::new();
            for _ in 0..HISTORY {
                scenario.step(&[], Duration::from_nanos(16_666_667));
                plans.push(
                    presentation::build_reusing(
                        &scenario.render_frames(RenderBackend::Raster, viewport),
                        viewport,
                        layout,
                        1.,
                        true,
                        PlanMode::Prepared,
                        None,
                    )
                    .unwrap(),
                );
            }
            let captures = capture(&plans).unwrap();
            let row = serde_json::json!({"fixture":fixture.name,"viewport":[width,height],"history_frames":HISTORY,
                "audit":inspect(&captures),"unguarded_negative_control":unguarded(&captures)});
            eprintln!("folded_stroke_audit={row}");
            rows.push(row);
        }
    }
    let result = serde_json::json!({"version":2,"kind":"folded-stroke-feasibility","rows":rows,
        "scope":"actual guarded projected outlines after warmed retained-culled encoding and conservative rejection; no GPU or live renderer changes",
        "limitations":"candidate bands use already-rounded captured points, not immutable local geometry; sampled topology/coordinate agreement is not a proven guard or pixel quality check; no timing claims"});
    if let Some(path) = std::env::var_os("SPACEWARS_STROKE_REUSE_AUDIT") {
        File::create_new(path)
            .unwrap()
            .write_all(serde_json::to_string_pretty(&result).unwrap().as_bytes())
            .unwrap();
    }
}
