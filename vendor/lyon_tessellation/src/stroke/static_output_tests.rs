//! Dispatch-only specialization: compare all output state and transactions.
use super::*;
use crate::geometry_builder::GeometryBuilder;

#[derive(Clone, Debug, PartialEq)]
struct Vertex {
    bits: [u32; 8],
    side: Side,
    source: VertexSource,
    attributes: Vec<u32>,
}

#[derive(Debug, Default, PartialEq)]
struct Output {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    first: (usize, usize),
    fail_after: Option<usize>,
    attempts: usize,
    lifecycle: Vec<&'static str>,
}

impl GeometryBuilder for Output {
    fn begin_geometry(&mut self) {
        self.first = (self.vertices.len(), self.indices.len());
        self.attempts = 0;
        self.lifecycle.push("begin");
    }
    fn end_geometry(&mut self) {
        self.lifecycle.push("end");
    }
    fn abort_geometry(&mut self) {
        self.vertices.truncate(self.first.0);
        self.indices.truncate(self.first.1);
        self.lifecycle.push("abort");
    }
    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        self.indices.extend([a.0, b.0, c.0]);
    }
}

impl StrokeGeometryBuilder for Output {
    fn add_stroke_vertex(&mut self, mut v: StrokeVertex) -> Result<VertexId, GeometryBuilderError> {
        let attempt = self.attempts;
        self.attempts += 1;
        if self.fail_after == Some(attempt) {
            return Err(GeometryBuilderError::TooManyVertices);
        }
        let p = v.position();
        let n = v.normal();
        let on = v.position_on_path();
        let id = VertexId(self.vertices.len() as u32);
        self.vertices.push(Vertex {
            bits: [
                p.x,
                p.y,
                n.x,
                n.y,
                on.x,
                on.y,
                v.line_width(),
                v.advancement(),
            ]
            .map(f32::to_bits),
            side: v.side(),
            source: v.source(),
            attributes: v
                .interpolated_attributes()
                .iter()
                .map(|v| v.to_bits())
                .collect(),
        });
        Ok(id)
    }
}

#[derive(Default)]
struct BalancedProfile { calls: usize, finished: usize }
impl StrokeProfiler for BalancedProfile {
    fn measure<R>(&mut self, _: StrokeStage, work: impl FnOnce() -> R) -> R {
        self.calls += 1;
        let result = work();
        self.finished += 1;
        result
    }
}

fn compare<J: RoundJoinPolicy>(path: &Path, options: &StrokeOptions) {
    let mut dynamic = SpecializedStrokeTessellator::<J>::new();
    let mut concrete = SpecializedStrokeTessellator::<J>::new();
    let mut reused = SpecializedStrokeTessellator::<J>::new();
    let mut checked = SpecializedStrokeTessellator::<J>::new();
    let mut profiled = SpecializedStrokeTessellator::<J>::new();
    let mut prepared = SpecializedStrokeTessellator::<J>::new();
    let mut guard_checked = SpecializedStrokeTessellator::<J>::new();
    let mut profile = BalancedProfile::default();
    let mut e = Output::default();
    let (mut f, mut g) = (Output::default(), Output::default());
    let (mut reuse_stats, mut checked_stats) = (StrokeEdgeStats::default(), StrokeEdgeStats::default());
    let (mut c, mut d) = (Output::default(), Output::default());
    let (mut a, mut b) = (Output::default(), Output::default());
    dynamic.tessellate(path, options, &mut a).unwrap();
    concrete.tessellate_static(path, options, &mut b).unwrap();
    reused.tessellate_static_edges::<false, _>(path, options, &mut c, &mut reuse_stats).unwrap();
    checked.tessellate_static_edges::<true, _>(path, options, &mut d, &mut checked_stats).unwrap();
    profiled.tessellate_static_profiled(path, options, &mut e, &mut profile).unwrap();
    prepared.tessellate_static_invariants::<false, _>(path, options, &mut f).unwrap();
    guard_checked.tessellate_static_invariants::<true, _>(path, options, &mut g).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_eq!(a, d);
    assert_eq!(a, e);
    assert_eq!(a, f);
    assert_eq!(a, g);
    assert_eq!(profile.calls, profile.finished);
    assert_eq!(reuse_stats, checked_stats);
    let prefix = (a.vertices.clone(), a.indices.clone());
    let emitted = a.attempts;
    // Error at the first, middle, and last output; success after repeated aborts.
    for fail_after in [Some(0), Some(emitted / 2), emitted.checked_sub(1), None] {
        if emitted == 0 && fail_after.is_some() {
            continue;
        }
        a.fail_after = fail_after;
        b.fail_after = fail_after;
        c.fail_after = fail_after;
        d.fail_after = fail_after;
        e.fail_after = fail_after;
        f.fail_after = fail_after;
        g.fail_after = fail_after;
        let ra = dynamic.tessellate(path, options, &mut a);
        let rb = concrete.tessellate_static(path, options, &mut b);
        let rc = reused.tessellate_static_edges::<false, _>(path, options, &mut c, &mut reuse_stats);
        let rd = checked.tessellate_static_edges::<true, _>(path, options, &mut d, &mut checked_stats);
        let re = profiled.tessellate_static_profiled(path, options, &mut e, &mut profile);
        let rf = prepared.tessellate_static_invariants::<false, _>(path, options, &mut f);
        let rg = guard_checked.tessellate_static_invariants::<true, _>(path, options, &mut g);
        assert_eq!(ra, rb);
        assert_eq!(ra, rc);
        assert_eq!(ra, rd);
        assert_eq!(ra, re);
        assert_eq!(ra, rf);
        assert_eq!(ra, rg);
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a, d);
        assert_eq!(a, e);
        assert_eq!(a, f);
        assert_eq!(a, g);
        assert_eq!(profile.calls, profile.finished);
        assert_eq!(reuse_stats, checked_stats);
        assert_eq!(dynamic.round_join_stats(), concrete.round_join_stats());
        assert_eq!(dynamic.round_join_stats(), reused.round_join_stats());
        assert_eq!(dynamic.round_join_stats(), checked.round_join_stats());
        assert_eq!(dynamic.round_join_stats(), profiled.round_join_stats());
        assert_eq!(dynamic.round_join_stats(), prepared.round_join_stats());
        assert_eq!(dynamic.round_join_stats(), guard_checked.round_join_stats());
        if fail_after.is_some() {
            assert!(ra.is_err());
            assert_eq!((&a.vertices, &a.indices), (&prefix.0, &prefix.1));
            assert_eq!(a.lifecycle.last(), Some(&"abort"));
        } else {
            assert!(ra.is_ok());
            assert_eq!(a.lifecycle.last(), Some(&"end"));
        }
    }
}

#[test]
fn adjacent_edges_are_actually_reused_and_do_not_survive_a_stroke() {
    let polygon = Polygon {
        points: &[point(0., 0.), point(4., 0.), point(4., 4.), point(0., 4.)],
        closed: true,
    };
    let mut tess = StrokeTessellator::new();
    let mut stats = StrokeEdgeStats::default();
    let mut output = Output::default();
    for expected in [4, 8] {
        tess.tessellate_static_edges::<true, _>(polygon.path_events(), &StrokeOptions::default(), &mut output, &mut stats).unwrap();
        assert_eq!(stats, StrokeEdgeStats { reused: expected, missed: 0 });
    }
}

#[test]
fn concrete_output_preserves_curves_caps_subpaths_and_error_rollback() {
    for cap in [LineCap::Butt, LineCap::Square, LineCap::Round] {
        for join in [
            LineJoin::Miter,
            LineJoin::MiterClip,
            LineJoin::Bevel,
            LineJoin::Round,
        ] {
            for tolerance in [0.01, 0.25, 2.] {
                for width in [0.01, 0.75, 20.] {
                    for close in [false, true] {
                        for empty in [false, true] {
                            let mut p = Path::builder();
                            p.begin(point(1., 1.));
                            if !empty {
                                p.line_to(point(2., 1.));
                                p.line_to(point(2., 1.));
                                p.quadratic_bezier_to(point(0., 8.), point(4., 3.));
                                p.cubic_bezier_to(point(-1., 10.), point(8., -9.), point(1., 1.));
                            }
                            p.end(close);
                            p.begin(point(4., 4.));
                            p.line_to(point(4.0001, 4.));
                            p.end(false);
                            let path = p.build();
                            let options = StrokeOptions::default()
                                .with_line_cap(cap)
                                .with_line_join(join)
                                .with_tolerance(tolerance)
                                .with_line_width(width);
                            compare::<ReferenceRoundJoins>(&path, &options);
                            compare::<EnabledRoundJoins<false>>(&path, &options);
                            compare::<EnabledRoundJoins<true>>(&path, &options);
                        }
                    }
                }
            }
        }
    }
}
