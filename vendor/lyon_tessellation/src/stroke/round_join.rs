//! Narrow render-lab shortcut. Reference remains the default. The conservative
//! angular margin is intentional: numerical boundaries use the original code.
use super::{EndpointData, StrokeOptions, SIDE_NEGATIVE};

/// Opt-in Space-Wars round-join construction experiment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoundJoinMode {
    #[default]
    Reference,
    /// Skip only arcs proven to have zero subdivisions within conservative bounds.
    Exact,
    /// Run the original calculation and assert every proposed shortcut is empty.
    Checked,
}

/// Cumulative counts; incremented only in Exact and Checked modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundJoinStats {
    pub shortcuts: u64,
    pub fallbacks: u64,
}

mod sealed {
    pub trait Sealed {}
}

/// Compile-time join policy. The ordinary tessellator has zero-sized policy and
/// counter storage; runtime selection belongs outside the per-join path.
pub trait RoundJoinPolicy: sealed::Sealed + Default + 'static {
    const ENABLED: bool;
    const CHECKED: bool;
    type Counter<'a>: RoundJoinCounter;
    fn counter(&mut self) -> Self::Counter<'_>;
    fn stats(&self) -> RoundJoinStats;
}

#[doc(hidden)]
pub trait RoundJoinCounter {
    fn record(&mut self, shortcut: bool);
}

impl RoundJoinCounter for () {
    #[inline]
    fn record(&mut self, _: bool) {}
}

impl RoundJoinCounter for &mut RoundJoinStats {
    #[inline]
    fn record(&mut self, shortcut: bool) {
        let count = if shortcut {
            &mut self.shortcuts
        } else {
            &mut self.fallbacks
        };
        *count = count.saturating_add(1);
    }
}

#[derive(Default)]
pub struct ReferenceRoundJoins;
impl sealed::Sealed for ReferenceRoundJoins {}
impl RoundJoinPolicy for ReferenceRoundJoins {
    const ENABLED: bool = false;
    const CHECKED: bool = false;
    type Counter<'a> = ();
    #[inline]
    fn counter(&mut self) {}
    #[inline]
    fn stats(&self) -> RoundJoinStats {
        RoundJoinStats::default()
    }
}

#[derive(Default)]
pub struct EnabledRoundJoins<const CHECKED: bool>(RoundJoinStats);
impl<const CHECKED: bool> sealed::Sealed for EnabledRoundJoins<CHECKED> {}
impl<const CHECKED: bool> RoundJoinPolicy for EnabledRoundJoins<CHECKED> {
    const ENABLED: bool = true;
    const CHECKED: bool = CHECKED;
    type Counter<'a> = &'a mut RoundJoinStats;
    #[inline]
    fn counter(&mut self) -> &mut RoundJoinStats {
        &mut self.0
    }
    #[inline]
    fn stats(&self) -> RoundJoinStats {
        self.0
    }
}

pub(super) fn has_no_arc(join: &EndpointData, side: usize, options: &StrokeOptions) -> bool {
    let radius = join.half_width;
    if options.variable_line_width.is_some()
        || join.fold[0]
        || join.fold[1]
        || !radius.is_finite()
        || !(0.001..=1024.).contains(&radius)
        || !options.tolerance.is_finite()
        || options.tolerance < radius * 0.5
        || !join.position.x.is_finite()
        || !join.position.y.is_finite()
        || join.position.x.abs() > 16384.
        || join.position.y.abs() > 16384.
    {
        return false;
    }
    let a = join.side_points[side].prev - join.position;
    let b = join.side_points[side].next - join.position;
    let aa = a.square_length();
    let bb = b.square_length();
    let rr = radius * radius;
    if !(rr * 0.25..=rr * 4.).contains(&aa) || !(rr * 0.25..=rr * 4.).contains(&bb) {
        return false;
    }
    let dot = a.dot(b);
    let oriented_cross = a.cross(b) * if side == SIDE_NEGATIVE { 1. } else { -1. };
    // tolerance >= radius/2 makes the reference flattening step >= 120 degrees.
    // dot >= -min(|a|^2,|b|^2)/8 bounds the minor angle below 98 degrees.
    // Correct cross sign picks that minor arc, not its reflex complement. The
    // nonzero margin excludes collinear/branch-cut rounding ambiguities.
    dot >= -0.125 * aa.min(bb) && oriented_cross > 0.001 * aa.max(bb)
}

// Select once per fixed-width stroke; no runtime experiment branch in the
// ordinary builder. Keep the original guard above as the independent oracle.
pub(crate) trait GuardPolicy {
    fn new(options: &StrokeOptions) -> Self;
    fn has_no_arc(&self, join: &EndpointData, side: usize, options: &StrokeOptions) -> bool;
}

pub(crate) struct OrdinaryGuard;
impl GuardPolicy for OrdinaryGuard {
    #[inline]
    fn new(_: &StrokeOptions) -> Self { Self }
    #[inline]
    fn has_no_arc(&self, join: &EndpointData, side: usize, options: &StrokeOptions) -> bool {
        has_no_arc(join, side, options)
    }
}

pub(crate) struct PreparedGuard<const CHECKED: bool> {
    eligible: bool,
    min_squared: f32,
    max_squared: f32,
}
impl<const CHECKED: bool> GuardPolicy for PreparedGuard<CHECKED> {
    #[inline]
    fn new(options: &StrokeOptions) -> Self {
        let radius = options.line_width * 0.5;
        let rr = radius * radius;
        Self {
            eligible: options.variable_line_width.is_none()
                && radius.is_finite() && (0.001..=1024.).contains(&radius)
                && options.tolerance.is_finite() && options.tolerance >= radius * 0.5,
            min_squared: rr * 0.25,
            max_squared: rr * 4.,
        }
    }

    #[inline]
    fn has_no_arc(&self, join: &EndpointData, side: usize, options: &StrokeOptions) -> bool {
        let result = self.eligible
            && !join.fold[0] && !join.fold[1]
            && join.position.x.is_finite() && join.position.y.is_finite()
            && join.position.x.abs() <= 16384. && join.position.y.abs() <= 16384.
            && {
                let a = join.side_points[side].prev - join.position;
                let b = join.side_points[side].next - join.position;
                let aa = a.square_length();
                let bb = b.square_length();
                (self.min_squared..=self.max_squared).contains(&aa)
                    && (self.min_squared..=self.max_squared).contains(&bb)
                    && a.dot(b) >= -0.125 * aa.min(bb)
                    && a.cross(b) * if side == SIDE_NEGATIVE { 1. } else { -1. }
                        > 0.001 * aa.max(bb)
            };
        if CHECKED {
            assert_eq!(join.half_width.to_bits(), (options.line_width * 0.5).to_bits());
            assert_eq!(result, has_no_arc(join, side, options));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry_builder::{BuffersBuilder, VertexBuffers};
    use crate::math::{point, vector};
    use crate::path::{
        builder::{Build, PathBuilder},
        polygon::Polygon,
        Path,
    };
    use crate::{
        LineCap, LineJoin, Side, SpecializedStrokeTessellator, StrokeVertex, VertexSource,
    };
    use core::f32::consts::PI;

    #[derive(Debug, PartialEq)]
    struct Vertex {
        bits: [u32; 8],
        side: Side,
        source: VertexSource,
        attributes: alloc::vec::Vec<u32>,
    }

    fn vertex(mut v: StrokeVertex) -> Vertex {
        let p = v.position();
        let n = v.normal();
        let on = v.position_on_path();
        Vertex {
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
        }
    }

    fn polygon_result<J: RoundJoinPolicy>(
        points: &[crate::math::Point],
        options: &StrokeOptions,
    ) -> (VertexBuffers<Vertex, u32>, RoundJoinStats) {
        polygon_input_result::<J>(points, true, options)
    }

    fn polygon_input_result<J: RoundJoinPolicy>(
        points: &[crate::math::Point],
        closed: bool,
        options: &StrokeOptions,
    ) -> (VertexBuffers<Vertex, u32>, RoundJoinStats) {
        let mut tess = SpecializedStrokeTessellator::<J>::new();
        let mut out = VertexBuffers::new();
        tess.tessellate_polygon(
            Polygon {
                points,
                closed,
            },
            options,
            &mut BuffersBuilder::new(&mut out, vertex),
        )
        .unwrap();
        // The concrete-output specialization must preserve every vertex field,
        // index and join decision, including folds/merges in this path corpus.
        let mut concrete_tess = SpecializedStrokeTessellator::<J>::new();
        let mut concrete = VertexBuffers::new();
        concrete_tess.tessellate_static(
            Polygon { points, closed }.path_events(),
            options,
            &mut BuffersBuilder::new(&mut concrete, vertex),
        ).unwrap();
        assert_eq!(out.vertices, concrete.vertices);
        assert_eq!(out.indices, concrete.indices);
        assert_eq!(tess.round_join_stats(), concrete_tess.round_join_stats());
        let mut polygon_tess = SpecializedStrokeTessellator::<J>::new();
        let mut direct = VertexBuffers::new();
        polygon_tess.tessellate_polygon_static(
            Polygon { points, closed }, options,
            &mut BuffersBuilder::new(&mut direct, vertex),
        ).unwrap();
        assert_eq!(out.vertices, direct.vertices);
        assert_eq!(out.indices, direct.indices);
        assert_eq!(tess.round_join_stats(), polygon_tess.round_join_stats());
        let mut stats = [crate::StrokeEdgeStats::default(); 2];
        for checked in [false, true] {
            let mut edge_tess = SpecializedStrokeTessellator::<J>::new();
            let mut edges = VertexBuffers::new();
            let mut output = BuffersBuilder::new(&mut edges, vertex);
            let polygon = Polygon { points, closed };
            let path = polygon.path_events();
            if checked {
                edge_tess.tessellate_static_edges::<true, _>(path, options, &mut output, &mut stats[1]).unwrap();
            } else {
                edge_tess.tessellate_static_edges::<false, _>(path, options, &mut output, &mut stats[0]).unwrap();
            }
            assert_eq!(out.vertices, edges.vertices);
            assert_eq!(out.indices, edges.indices);
            assert_eq!(tess.round_join_stats(), edge_tess.round_join_stats());
        }
        assert_eq!(stats[0], stats[1]);
        for checked in [false, true] {
            let mut prepared_tess = SpecializedStrokeTessellator::<J>::new();
            let mut prepared = VertexBuffers::new();
            let mut output = BuffersBuilder::new(&mut prepared, vertex);
            let polygon = Polygon { points, closed };
            let path = polygon.path_events();
            if checked {
                prepared_tess.tessellate_static_invariants::<true, _>(path, options, &mut output).unwrap();
            } else {
                prepared_tess.tessellate_static_invariants::<false, _>(path, options, &mut output).unwrap();
            }
            assert_eq!(out.vertices, prepared.vertices);
            assert_eq!(out.indices, prepared.indices);
            assert_eq!(tess.round_join_stats(), prepared_tess.round_join_stats());
        }
        (out, tess.round_join_stats())
    }

    #[test]
    fn direct_polygon_preserves_empty_merged_open_and_closed_caps_and_joins() {
        let points = [point(0., 0.), point(0., 0.), point(0.001, 0.002),
            point(1., 0.), point(0., 1.), point(0., 0.)];
        for len in 0..=points.len() {
            for closed in [false, true] {
                for cap in [LineCap::Butt, LineCap::Square, LineCap::Round] {
                    for join in [LineJoin::Miter, LineJoin::MiterClip, LineJoin::Round, LineJoin::Bevel] {
                        for width in [0.001, 0.75, 20.] {
                            let options = StrokeOptions::default()
                                .with_line_width(width).with_line_cap(cap).with_line_join(join);
                            polygon_input_result::<ReferenceRoundJoins>(&points[..len], closed, &options);
                            polygon_input_result::<EnabledRoundJoins<false>>(&points[..len], closed, &options);
                            polygon_input_result::<EnabledRoundJoins<true>>(&points[..len], closed, &options);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn direct_polygon_preserves_output_error_order_and_aborts_once() {
        use crate::{GeometryBuilder, GeometryBuilderError, StrokeGeometryBuilder, VertexId};
        #[derive(Debug, PartialEq)]
        enum Event { Begin, End, Abort, Vertex(Vertex), Triangle(VertexId, VertexId, VertexId) }
        struct Output { events: alloc::vec::Vec<Event>, limit: u32, vertices: u32 }
        impl GeometryBuilder for Output {
            fn begin_geometry(&mut self) { self.events.push(Event::Begin); }
            fn end_geometry(&mut self) { self.events.push(Event::End); }
            fn abort_geometry(&mut self) { self.events.push(Event::Abort); }
            fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
                self.events.push(Event::Triangle(a, b, c));
            }
        }
        impl StrokeGeometryBuilder for Output {
            fn add_stroke_vertex(&mut self, v: StrokeVertex) -> Result<VertexId, GeometryBuilderError> {
                self.events.push(Event::Vertex(vertex(v)));
                if self.vertices == self.limit { return Err(GeometryBuilderError::TooManyVertices); }
                let id = VertexId(self.vertices);
                self.vertices += 1;
                Ok(id)
            }
        }
        let points = [point(0., 0.), point(1., 0.), point(0.01, 0.02), point(0., 1.)];
        let options = StrokeOptions::default().with_line_width(0.75)
            .with_line_cap(LineCap::Round).with_line_join(LineJoin::Round);
        for closed in [false, true] {
            for limit in 0..80 {
                let mut expected = None;
                for direct in [false, true] {
                    let mut tess = SpecializedStrokeTessellator::<EnabledRoundJoins<true>>::new();
                    let mut out = Output { events: alloc::vec::Vec::new(), limit, vertices: 0 };
                    let polygon = Polygon { points: &points, closed };
                    let result = if direct {
                        tess.tessellate_polygon_static(polygon, &options, &mut out)
                    } else {
                        tess.tessellate_static(polygon.path_events(), &options, &mut out)
                    };
                    assert_eq!(out.events.iter().filter(|e| matches!(e, Event::Abort)).count(), usize::from(result.is_err()));
                    let actual = (result, out.events, tess.round_join_stats());
                    if let Some(ref expected) = expected { assert_eq!(&actual, expected); }
                    else { expected = Some(actual); }
                }
            }
        }
    }

    #[test]
    fn reference_join_policy_and_counter_have_no_runtime_storage() {
        assert_eq!(core::mem::size_of::<ReferenceRoundJoins>(), 0);
        assert_eq!(
            core::mem::size_of::<<ReferenceRoundJoins as RoundJoinPolicy>::Counter<'static>>(),
            0
        );
        assert!(!ReferenceRoundJoins::ENABLED);
        assert!(!ReferenceRoundJoins::CHECKED);
        assert_eq!(
            core::mem::size_of::<crate::StrokeTessellator>(),
            core::mem::size_of::<alloc::vec::Vec<f32>>()
                + core::mem::size_of::<crate::SimpleAttributeStore>()
        );
    }

    #[test]
    fn exact_and_checked_match_every_vertex_bit_index_and_state_on_difficult_paths() {
        let shapes: [&[(f32, f32)]; 7] = [
            &[(0., 0.), (1., 0.), (1., 1.), (0., 1.)],
            &[(0., 0.), (1., 0.), (0.01, 0.02)],
            &[(0., 0.), (1., 0.), (0.3, 0.4), (1., 1.), (0., 1.)],
            &[(0., 0.), (0., 0.), (1., 0.), (1., 1.), (0., 1.), (0., 0.)],
            &[(0., 0.), (1., 0.), (0., 0.), (0., 1.)],
            &[(0., 0.), (1., 1.), (0., 1.), (1., 0.)],
            &[(0., 0.), (0.01, 0.), (0.02, 0.), (0.03, 0.)],
        ];
        let mut total = RoundJoinStats::default();
        for shape in shapes {
            for scale in [0.01, 0.2, 1., 80.] {
                for angle in [0_f32, 0.01, 0.41, PI - 0.0001, PI, PI + 0.0001] {
                    for offset in [0., 1024.25] {
                        let points: alloc::vec::Vec<_> = shape
                            .iter()
                            .map(|&(x, y)| {
                                point(
                                    offset + scale * (x * angle.cos() - y * angle.sin()),
                                    offset + scale * (x * angle.sin() + y * angle.cos()),
                                )
                            })
                            .collect();
                        for width in [0.05, 0.5, 0.75, 1., 2., 20.] {
                            for tolerance in [0.1, 0.25, 1.] {
                                let options = StrokeOptions::default()
                                    .with_line_join(LineJoin::Round)
                                    .with_line_cap(LineCap::Round)
                                    .with_line_width(width)
                                    .with_tolerance(tolerance);
                                let mut reference = None;
                                for mode in [
                                    RoundJoinMode::Reference,
                                    RoundJoinMode::Exact,
                                    RoundJoinMode::Checked,
                                ] {
                                    let (out, counts) = match mode {
                                        RoundJoinMode::Reference => {
                                            polygon_result::<ReferenceRoundJoins>(&points, &options)
                                        }
                                        RoundJoinMode::Exact => {
                                            polygon_result::<EnabledRoundJoins<false>>(
                                                &points, &options,
                                            )
                                        }
                                        RoundJoinMode::Checked => {
                                            polygon_result::<EnabledRoundJoins<true>>(
                                                &points, &options,
                                            )
                                        }
                                    };
                                    if let Some((ref v, ref i, ref stats)) = reference {
                                        assert_eq!(&out.vertices, v, "mode={mode:?} width={width} tol={tolerance} points={points:?}");
                                        assert_eq!(&out.indices, i);
                                        if mode == RoundJoinMode::Checked {
                                            assert_eq!(&counts, stats);
                                        }
                                    }
                                    if mode == RoundJoinMode::Exact {
                                        let stats = counts;
                                        total.shortcuts += stats.shortcuts;
                                        total.fallbacks += stats.fallbacks;
                                        reference.as_mut().unwrap().2 = stats;
                                    } else if mode == RoundJoinMode::Reference {
                                        assert_eq!(counts, RoundJoinStats::default());
                                        reference = Some((out.vertices, out.indices, counts));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(total.shortcuts > 1000);
        assert!(total.fallbacks > 1000);
    }

    fn attributed_path(b: &mut impl PathBuilder) {
        b.begin(point(1., 1.), &[1., 0.25]);
        b.line_to(point(2., 1.), &[0.5, 0.75]);
        b.line_to(point(2., 2.), &[1., 0.5]);
        b.quadratic_bezier_to(point(3., 3.), point(1., 3.), &[0.25, 1.]);
        b.end(false);
    }

    fn attributed_result<J: RoundJoinPolicy>(
        options: &StrokeOptions,
        direct: bool,
    ) -> (VertexBuffers<Vertex, u32>, RoundJoinStats) {
        let mut tess = SpecializedStrokeTessellator::<J>::new();
        let mut out = VertexBuffers::new();
        let mut output = BuffersBuilder::new(&mut out, vertex);
        if direct {
            let mut b = tess.builder_with_attributes(2, options, &mut output);
            attributed_path(&mut b);
            b.build().unwrap();
        } else {
            let mut b = Path::builder_with_attributes(2);
            attributed_path(&mut b);
            let p = b.build();
            tess.tessellate_path(&p, options, &mut output).unwrap();
        }
        (out, tess.round_join_stats())
    }

    #[test]
    fn builder_attributes_curves_and_caps_preserve_reference_state() {
        for variable in [false, true] {
            for cap in [LineCap::Butt, LineCap::Square, LineCap::Round] {
                for join in [LineJoin::Round, LineJoin::Bevel, LineJoin::Miter] {
                    let mut options = StrokeOptions::default()
                        .with_line_width(0.75)
                        .with_tolerance(0.25)
                        .with_line_join(join)
                        .with_line_cap(cap);
                    if variable {
                        options = options.with_variable_line_width(0);
                    }
                    for direct in [false, true] {
                        let mut reference = None;
                        for mode in [
                            RoundJoinMode::Reference,
                            RoundJoinMode::Exact,
                            RoundJoinMode::Checked,
                        ] {
                            let (out, counts) = match mode {
                                RoundJoinMode::Reference => {
                                    attributed_result::<ReferenceRoundJoins>(&options, direct)
                                }
                                RoundJoinMode::Exact => {
                                    attributed_result::<EnabledRoundJoins<false>>(&options, direct)
                                }
                                RoundJoinMode::Checked => {
                                    attributed_result::<EnabledRoundJoins<true>>(&options, direct)
                                }
                            };
                            if variable {
                                assert_eq!(counts.shortcuts, 0);
                            }
                            if let Some((ref v, ref i)) = reference {
                                assert_eq!(&out.vertices, v);
                                assert_eq!(&out.indices, i);
                            } else {
                                reference = Some((out.vertices, out.indices));
                            }
                        }
                    }
                }
            }
        }
    }

    fn reference_subdivisions(join: &EndpointData, side: usize, options: &StrokeOptions) -> u32 {
        let a = join.side_points[side].prev - join.position;
        let b = join.side_points[side].next - join.position;
        let sign = if side == SIDE_NEGATIVE { 1. } else { -1. };
        let mut diff = a.angle_from_x_axis().angle_to(b.angle_from_x_axis());
        if diff.radians * sign < 0. {
            diff.radians = sign * (2. * PI - diff.radians.abs());
        }
        let step = super::super::circle_flattening_step(join.half_width, options.tolerance);
        (diff.radians.abs() / step).ceil().log2().round() as u32
    }

    #[test]
    fn conservative_domain_never_skips_nonempty_arcs_across_angles_and_quantization() {
        let mut accepted = 0;
        let mut rejected = 0;
        for radius in [0.001, 0.005, 0.05, 0.375, 1., 4., 64., 1024.] {
            for ratio in [0.25, 0.4999, 0.5, 0.6667, 1., 2.] {
                let options = StrokeOptions::default().with_tolerance(radius * ratio).with_line_width(radius * 2.);
                let prepared = PreparedGuard::<true>::new(&options);
                for center in [
                    point(0., 0.),
                    point(511.333, -64.234),
                    point(16383., 16380.),
                    point(-16383., -16380.),
                ] {
                    for rotation in 0..32 {
                        let angle = rotation as f32 * 2. * PI / 32.;
                        for turn in -80..=80 {
                            let end = angle + turn as f32 * PI / 80.;
                            let mut join = EndpointData {
                                position: center,
                                half_width: radius,
                                ..Default::default()
                            };
                            for side in 0..2 {
                                join.side_points[side].prev =
                                    center + vector(angle.cos(), angle.sin()) * radius;
                                join.side_points[side].next =
                                    center + vector(end.cos(), end.sin()) * radius;
                                assert_eq!(prepared.has_no_arc(&join, side, &options), has_no_arc(&join, side, &options));
                                if has_no_arc(&join, side, &options) {
                                    accepted += 1;
                                    assert_eq!(
                                        reference_subdivisions(&join, side, &options),
                                        0,
                                        "{join:?}"
                                    );
                                } else {
                                    rejected += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(accepted > 100_000);
        assert!(rejected > accepted);
    }

    #[test]
    fn folds_variable_width_and_unsafe_numeric_domains_use_reference() {
        let mut join = EndpointData {
            position: point(1., 1.),
            half_width: 0.375,
            ..Default::default()
        };
        join.side_points[1].prev = point(1.375, 1.);
        join.side_points[1].next = point(1., 1.375);
        let options = StrokeOptions::default().with_tolerance(0.25);
        assert!(has_no_arc(&join, 1, &options));
        assert!(!has_no_arc(&join, 1, &options.with_variable_line_width(0)));
        for fold in [[true, false], [false, true], [true, true]] {
            assert!(!has_no_arc(&EndpointData { fold, ..join }, 1, &options));
        }
        for radius in [0., -1., f32::NAN, f32::INFINITY, 2048.] {
            assert!(!has_no_arc(
                &EndpointData {
                    half_width: radius,
                    ..join
                },
                1,
                &options
            ));
        }
        for tolerance in [0., -1., f32::NAN, f32::INFINITY, 0.18] {
            assert!(!has_no_arc(&join, 1, &options.with_tolerance(tolerance)));
        }
        join.side_points[1].next = join.position;
        assert!(!has_no_arc(&join, 1, &options));
        join.side_points[1].next = point(f32::NAN, 1.);
        assert!(!has_no_arc(&join, 1, &options));
    }

    #[test]
    fn prepared_guard_preserves_numeric_rejections_and_folds() {
        assert_eq!(core::mem::size_of::<OrdinaryGuard>(), 0);
        for radius in [-1., -0., 0., 0.0009, 0.001, 0.375, 1024., 2048., f32::NAN, f32::INFINITY] {
            for tolerance in [-1., 0., 0.18, 0.25, 4096., f32::NAN, f32::INFINITY] {
                let options = StrokeOptions::default().with_line_width(radius * 2.).with_tolerance(tolerance);
                let guard = PreparedGuard::<true>::new(&options);
                for x in [0., 16384., 16385., f32::NAN, f32::INFINITY] {
                    for fold in [[false, false], [true, false], [false, true], [true, true]] {
                        let mut join = EndpointData { position: point(x, 0.), half_width: radius, fold, ..Default::default() };
                        for side in 0..2 {
                            join.side_points[side].prev = join.position + vector(radius, 0.);
                            join.side_points[side].next = join.position + vector(0., radius);
                            assert_eq!(guard.has_no_arc(&join, side, &options), has_no_arc(&join, side, &options));
                        }
                    }
                }
            }
        }
    }
}
