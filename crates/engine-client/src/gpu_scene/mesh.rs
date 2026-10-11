//! Ordered, premultiplied-color triangles. Tessellation changes geometry, never
//! primitive order: fill, stroke, then the next primitive. Shared edges have no
//! artificial alpha fringe; GPU multisampling handles sample coverage instead.

use super::{
    circles,
    encode_profile::{DrawProfile, elapsed, started},
    plan::{Draw, Scene, Shape},
    stroke_output::{Bulk, StrokeWriter},
};
use engine_common::{RenderColor, RenderPoint, Stroke};
use lyon_tessellation::{
    BuffersBuilder, CheckedStrokeTessellator, ExactStrokeTessellator, FillOptions, FillRule,
    FillTessellator, FillVertex, LineCap, LineJoin, RoundJoinMode, RoundJoinPolicy, RoundJoinStats,
    SpecializedStrokeTessellator, StrokeEdgeStats, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
    math::{Point, point},
    path::Polygon,
};

// Arrays are tightly packed Pod on every platform; position.xy, premultiplied rgba.
pub(super) type Vertex = [f32; 6];
pub(super) const MAX_VERTICES: usize = 1_000_000;
pub(super) const MAX_INDICES: usize = 3_000_000;

// The extra modes are benchmark controls, not automatic production policies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GeometryMode {
    #[default]
    Triangles,
    TrianglesSplit,
    TrianglesPrograms,
    Circles,
    CirclesExpanded,
    CirclesUniforms,
    CirclesBindings,
    CirclesSubmission,
    CirclesUnified,
    CirclesUnifiedInstanced,
    CirclesUnifiedLayout,
    CirclesUnifiedVao,
    MeshesStreamed,
    Meshes,
    MeshesExpanded,
}

impl GeometryMode {
    pub fn reusable_meshes(self) -> bool {
        matches!(
            self,
            Self::MeshesStreamed | Self::Meshes | Self::MeshesExpanded
        )
    }
    pub fn instanced_meshes(self) -> bool {
        matches!(self, Self::MeshesStreamed | Self::Meshes)
    }
    pub fn split_triangles(self) -> bool {
        matches!(self, Self::TrianglesSplit | Self::TrianglesPrograms)
    }

    pub fn circle_submission_control(self) -> bool {
        matches!(
            self,
            Self::CirclesUniforms
                | Self::CirclesBindings
                | Self::CirclesSubmission
                | Self::CirclesUnified
                | Self::CirclesUnifiedInstanced
                | Self::CirclesUnifiedLayout
                | Self::CirclesUnifiedVao
        )
    }

    pub fn frame_uniforms(self) -> bool {
        matches!(self, Self::CirclesUniforms | Self::CirclesSubmission) || self.unified_program()
    }

    pub fn buffer_bindings(self) -> bool {
        matches!(self, Self::CirclesBindings | Self::CirclesSubmission) || self.unified_program()
    }

    pub fn unified_program(self) -> bool {
        matches!(self, Self::CirclesUnified | Self::CirclesUnifiedInstanced) || self.fixed_layout()
    }

    pub fn fixed_layout(self) -> bool {
        matches!(self, Self::CirclesUnifiedLayout | Self::CirclesUnifiedVao)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Batch {
    Triangles {
        start: usize,
        count: usize,
    },
    Circles {
        start: usize,
        count: usize,
        level: usize,
    },
}

#[derive(Default)]
pub(crate) struct MeshBuilder {
    pub buffers: VertexBuffers<Vertex, u32>,
    pub circles: Vec<circles::Instance>,
    pub batches: Vec<Batch>,
    // Explicit control for uncached strokes; retained template builders stay stock.
    pub stroke_writer: StrokeWriter,
    // Benchmark-only pre-tessellation rejection of guarded polygon strokes.
    pub stroke_culling: super::stroke_culling::Control,
    // Explicit diagnostic pass only; never enabled by live rendering or timings.
    pub stroke_census: Option<Box<super::stroke_census::Census>>,
    // Benchmark-only approximation. Keep cache construction and live defaults stock.
    pub small_strokes: bool,
    // Cumulative counter so a multi-pass probe can take before/after deltas.
    pub small_strokes_used: u64,
    fill: FillTessellator,
    stroke: StrokeSet,
    // Allocate only for the explicit single-pass writer, never for stock paths.
    single_pass: Option<Box<super::stroke_single_pass::State>>,
    points: Vec<Point>,
    templates: Vec<(Vec<circles::Vertex>, Vec<u32>)>,
    expanded_indices: Vec<u32>,
}

impl MeshBuilder {
    pub fn single_pass_stats(&self) -> super::stroke_single_pass::Stats {
        self.single_pass
            .as_ref()
            .map_or_else(Default::default, |s| s.stats())
    }

    pub fn set_stroke_writer(&mut self, writer: StrokeWriter) {
        if !writer.single_pass() {
            self.single_pass = None;
        }
        self.stroke_writer = writer;
    }

    pub fn edge_stats(&self) -> StrokeEdgeStats {
        self.stroke.edges
    }

    pub fn set_round_join_mode(&mut self, mode: lyon_tessellation::RoundJoinMode) {
        self.stroke.mode = mode;
    }

    pub fn round_join_stats(&self) -> lyon_tessellation::RoundJoinStats {
        let exact = self.stroke.exact.round_join_stats();
        let checked = self.stroke.checked.round_join_stats();
        RoundJoinStats {
            shortcuts: exact.shortcuts.saturating_add(checked.shortcuts),
            fallbacks: exact.fallbacks.saturating_add(checked.fallbacks),
        }
    }

    pub fn rebuild(&mut self, scene: &Scene) -> Result<(), String> {
        self.rebuild_mode(scene, GeometryMode::Triangles)
    }

    pub fn rebuild_mode(&mut self, scene: &Scene, mode: GeometryMode) -> Result<(), String> {
        self.clear();
        let mut previous_circle = None;
        for draw in &scene.draws {
            let circle_level = match draw.shape {
                _ if mode == GeometryMode::Triangles => None,
                Shape::Circle { radius, .. } => {
                    circles::level(radius + draw.stroke.map_or(0.0, |s| s.width * 0.5))
                }
                _ => None,
            };
            if (matches!(mode, GeometryMode::Circles | GeometryMode::CirclesExpanded)
                || mode.circle_submission_control())
                && let Shape::Circle { center, radius } = draw.shape
                && let Some(level) = circle_level
            {
                if self.circles.len() >= 100_000 {
                    return Err("GPU scene exceeds circle instance budget".into());
                }
                let start = self.circles.len();
                self.circles
                    .push(circles::instance(draw, center.x, center.y, radius));
                match self.batches.last_mut() {
                    Some(Batch::Circles {
                        count,
                        level: previous,
                        ..
                    }) => {
                        *count += 1;
                        *previous = (*previous).max(level);
                    }
                    _ => self.batches.push(Batch::Circles {
                        start,
                        count: 1,
                        level,
                    }),
                }
                continue;
            }
            let (start, count) = self.append_draw(draw)?;
            if count > 0 {
                // Same tessellation, uploads, shaders and source order as the
                // baseline; only draw boundaries change.
                let split =
                    mode.split_triangles() && previous_circle != Some(circle_level.is_some());
                match self.batches.last_mut() {
                    Some(Batch::Triangles {
                        count: previous, ..
                    }) if !split => *previous += count,
                    _ => self.batches.push(Batch::Triangles { start, count }),
                }
                previous_circle = Some(circle_level.is_some());
            }
        }
        if mode == GeometryMode::CirclesExpanded {
            self.expand_circles()?;
        }
        self.validate()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self
            .buffers
            .vertices
            .iter()
            .flatten()
            .chain(self.circles.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("GPU tessellation produced non-finite geometry".into());
        }
        Ok(())
    }

    /// Same finite-number contract as validate(), but no per-number early exit.
    /// Flat slices and a bitwise reduction allow portable auto-vectorization.
    /// Do not use short-circuit `||` inside the reduction: most frames are valid.
    pub fn validate_bulk(&self) -> Result<(), String> {
        let invalid = |values: &[f32]| {
            values
                .iter()
                .fold(false, |bad, value| bad | !value.is_finite())
        };
        if invalid(self.buffers.vertices.as_flattened()) | invalid(self.circles.as_flattened()) {
            return Err("GPU tessellation produced non-finite geometry".into());
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffers.vertices.clear();
        self.buffers.indices.clear();
        self.circles.clear();
        self.batches.clear();
    }

    pub fn append_draw(&mut self, draw: &Draw) -> Result<(usize, usize), String> {
        self.append_draw_with_styles(draw, draw.fill, draw.stroke)
    }

    pub fn append_draw_with_styles(
        &mut self,
        draw: &Draw,
        fill: Option<RenderColor>,
        stroke: Option<Stroke>,
    ) -> Result<(usize, usize), String> {
        self.append_draw_profiled::<false>(draw, fill, stroke, &mut DrawProfile::default())
    }

    pub(super) fn append_draw_profiled<const PROFILE: bool>(
        &mut self,
        draw: &Draw,
        fill: Option<RenderColor>,
        stroke: Option<Stroke>,
        profile: &mut DrawProfile,
    ) -> Result<(usize, usize), String> {
        let prepare = started::<PROFILE>();
        let vertex_start = self.buffers.vertices.len();
        let fill_options = FillOptions::default()
            .with_fill_rule(FillRule::NonZero)
            .with_tolerance(0.25);
        let start = self.buffers.indices.len();
        self.points.clear();
        match &draw.shape {
            Shape::Circle { .. } => {}
            Shape::Line { start, end } => self.points.extend([xy(*start), xy(*end)]),
            Shape::Polygon(points) => self.points.extend(points.iter().copied().map(xy)),
            // Expand only when the ordinary path or a cache-budget fallback
            // needs points. Reuse the tessellator's scratch storage.
            Shape::LocalPolygon(p) => self
                .points
                .extend(p.definition.points().iter().map(|v| xy(p.project(*v)))),
        }
        if PROFILE {
            profile.prepare += elapsed(prepare);
        }
        if let Some(color) = fill {
            let stamp = started::<PROFILE>();
            let mut output = BuffersBuilder::new(&mut self.buffers, |v: FillVertex<'_>| {
                vertex(v.position(), color)
            });
            let result = match &draw.shape {
                Shape::Circle { center, radius } => {
                    self.fill
                        .tessellate_circle(xy(*center), *radius, &fill_options, &mut output)
                }
                Shape::Polygon(_) | Shape::LocalPolygon(_) => self.fill.tessellate_polygon(
                    Polygon {
                        points: &self.points,
                        closed: true,
                    },
                    &fill_options,
                    &mut output,
                ),
                Shape::Line { .. } => Ok(()),
            };
            result.map_err(|e| format!("GPU fill tessellation: {e:?}"))?;
            if PROFILE {
                profile.fill += elapsed(stamp);
            }
        }
        if let Some(stroke) = stroke {
            let stamp = started::<PROFILE>();
            let join = if self.small_strokes
                && draw.stroke == Some(stroke)
                && super::small_strokes::bounds(draw).is_some()
            {
                self.small_strokes_used = self.small_strokes_used.saturating_add(1);
                LineJoin::Bevel
            } else {
                LineJoin::Round
            };
            let options = StrokeOptions::default()
                .with_tolerance(0.25)
                .with_line_width(stroke.width)
                .with_line_cap(LineCap::Round)
                .with_line_join(join);
            let appended = self.stroke_writer.single_pass()
                && matches!(draw.shape, Shape::Polygon(_) | Shape::LocalPolygon(_))
                && self
                    .single_pass
                    .get_or_insert_with(Default::default)
                    .append(
                        &self.points,
                        &options,
                        stroke.color,
                        self.stroke_writer,
                        &mut self.buffers,
                    )?;
            let result = if appended {
                Ok(())
            } else if self.stroke_writer != StrokeWriter::Stock {
                stroke_shape(
                    &mut self.stroke,
                    &draw.shape,
                    &self.points,
                    &options,
                    self.stroke_writer,
                    &mut Bulk::new(&mut self.buffers, stroke.color),
                )
            } else {
                stroke_shape(
                    &mut self.stroke,
                    &draw.shape,
                    &self.points,
                    &options,
                    StrokeWriter::Stock,
                    &mut BuffersBuilder::new(&mut self.buffers, |v: StrokeVertex<'_, '_>| {
                        vertex(v.position(), stroke.color)
                    }),
                )
            };
            result.map_err(|e| format!("GPU stroke tessellation: {e:?}"))?;
            if PROFILE {
                profile.stroke += elapsed(stamp);
            }
        }
        let stamp = started::<PROFILE>();
        let count = self.buffers.indices.len() - start;
        // A bounded diagnostic path, not an unbounded mesh upload queue.
        if self.buffers.vertices.len() > MAX_VERTICES || self.buffers.indices.len() > MAX_INDICES {
            return Err("GPU scene exceeds prototype mesh budget".into());
        }
        if PROFILE {
            profile.points += self.points.len() as u64;
            profile.vertices += (self.buffers.vertices.len() - vertex_start) as u64;
            profile.indices += count as u64;
            profile.empty += u64::from(count == 0);
            profile.finish += elapsed(stamp);
        }
        Ok((start, count))
    }

    // CPU expansion deliberately keeps the exact templates (including unused
    // degenerate fill/stroke geometry) and max LOD of each consecutive run.
    // This trades instancing/shader work for CPU transforms and larger uploads,
    // while allowing one ordinary draw without sorting transparent primitives.
    fn expand_circles(&mut self) -> Result<(), String> {
        if !self.circles.is_empty() && self.templates.is_empty() {
            self.templates = circles::LEVELS.into_iter().map(circles::template).collect();
        }
        let (mut vertices, mut indices) = (self.buffers.vertices.len(), self.buffers.indices.len());
        for batch in &self.batches {
            if let Batch::Circles { count, level, .. } = *batch {
                vertices += count * self.templates[level].0.len();
                indices += count * self.templates[level].1.len();
            }
        }
        if vertices > MAX_VERTICES || indices > MAX_INDICES {
            return Err("GPU scene exceeds expanded template mesh budget".into());
        }
        self.expanded_indices.clear();
        for batch in &self.batches {
            match *batch {
                Batch::Triangles { start, count } => self
                    .expanded_indices
                    .extend_from_slice(&self.buffers.indices[start..start + count]),
                Batch::Circles {
                    start,
                    count,
                    level,
                } => {
                    let (vertices, indices) = &self.templates[level];
                    for instance in &self.circles[start..start + count] {
                        let base = self.buffers.vertices.len() as u32;
                        self.buffers.vertices.extend(
                            vertices
                                .iter()
                                .map(|v| circles::expand_vertex(*v, instance)),
                        );
                        self.expanded_indices
                            .extend(indices.iter().map(|i| base + i));
                    }
                }
            }
        }
        std::mem::swap(&mut self.buffers.indices, &mut self.expanded_indices);
        self.expanded_indices.clear();
        self.circles.clear();
        self.batches.clear();
        if !self.buffers.indices.is_empty() {
            self.batches.push(Batch::Triangles {
                start: 0,
                count: self.buffers.indices.len(),
            });
        }
        Ok(())
    }

    pub fn upload_bytes(&self) -> usize {
        self.buffers.vertices.len() * std::mem::size_of::<Vertex>()
            + self.buffers.indices.len() * std::mem::size_of::<u32>()
            + self.circles.len() * std::mem::size_of::<circles::Instance>()
    }
}

// Dispatch once per stroke. The reference specialization has no mode branch,
// counter pointer or extra storage inside Lyon's join/point loops.
#[derive(Default)]
struct StrokeSet {
    mode: RoundJoinMode,
    reference: StrokeTessellator,
    exact: ExactStrokeTessellator,
    checked: CheckedStrokeTessellator,
    edges: StrokeEdgeStats,
}

// Keep the small policy dispatch at its caller, as the original single-policy
// wrapper was. Tessellation itself remains outside this wrapper.
#[inline(always)]
fn stroke_shape(
    tessellators: &mut StrokeSet,
    shape: &Shape,
    points: &[Point],
    options: &StrokeOptions,
    writer: StrokeWriter,
    output: &mut impl lyon_tessellation::geometry_builder::StrokeGeometryBuilder,
) -> Result<(), lyon_tessellation::TessellationError> {
    match tessellators.mode {
        RoundJoinMode::Reference => tessellate_shape(
            &mut tessellators.reference,
            &mut tessellators.edges,
            shape,
            points,
            options,
            writer,
            output,
        ),
        RoundJoinMode::Exact => tessellate_shape(
            &mut tessellators.exact,
            &mut tessellators.edges,
            shape,
            points,
            options,
            writer,
            output,
        ),
        RoundJoinMode::Checked => tessellate_shape(
            &mut tessellators.checked,
            &mut tessellators.edges,
            shape,
            points,
            options,
            writer,
            output,
        ),
    }
}

fn tessellate_shape<J: RoundJoinPolicy>(
    tessellator: &mut SpecializedStrokeTessellator<J>,
    edges: &mut StrokeEdgeStats,
    shape: &Shape,
    points: &[Point],
    options: &StrokeOptions,
    writer: StrokeWriter,
    output: &mut impl lyon_tessellation::geometry_builder::StrokeGeometryBuilder,
) -> Result<(), lyon_tessellation::TessellationError> {
    match shape {
        Shape::Circle { center, radius } => {
            tessellator.tessellate_circle(xy(*center), *radius, options, output)
        }
        _ => {
            let polygon = Polygon {
                points,
                closed: matches!(shape, Shape::Polygon(_) | Shape::LocalPolygon(_)),
            };
            match writer {
                StrokeWriter::BulkPrepared => tessellator.tessellate_static_invariants::<false, _>(
                    polygon.path_events(),
                    options,
                    output,
                ),
                StrokeWriter::BulkPreparedChecked => tessellator
                    .tessellate_static_invariants::<true, _>(
                        polygon.path_events(),
                        options,
                        output,
                    ),
                StrokeWriter::BulkEdges => tessellator.tessellate_static_edges::<false, _>(
                    polygon.path_events(),
                    options,
                    output,
                    edges,
                ),
                StrokeWriter::BulkEdgesChecked => tessellator.tessellate_static_edges::<true, _>(
                    polygon.path_events(),
                    options,
                    output,
                    edges,
                ),
                StrokeWriter::BulkPolygon => {
                    tessellator.tessellate_polygon_static(polygon, options, output)
                }
                StrokeWriter::BulkStatic
                | StrokeWriter::BulkSinglePass
                | StrokeWriter::BulkSinglePassChecked
                | StrokeWriter::BulkSinglePassLayouts
                | StrokeWriter::BulkSinglePassArcs
                | StrokeWriter::BulkSinglePassClassify
                | StrokeWriter::BulkSinglePassPrefix
                | StrokeWriter::BulkSinglePassPrefixChecked => {
                    tessellator.tessellate_static(polygon.path_events(), options, output)
                }
                _ => tessellator.tessellate_polygon(polygon, options, output),
            }
        }
    }
}

fn xy(p: RenderPoint) -> Point {
    point(p.x, p.y)
}
fn vertex(p: Point, c: RenderColor) -> Vertex {
    [p.x, p.y, c.r * c.a, c.g * c.a, c.b * c.a, c.a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_scene::plan::Draw;

    fn circle(x: f32, radius: f32) -> Draw {
        Draw {
            shape: Shape::Circle {
                center: RenderPoint::new(x, 20.0),
                radius,
            },
            fill: Some(RenderColor::rgba(1.0, 0.5, 0.0, 0.5)),
            stroke: Some(Stroke::new(RenderColor::WHITE, 2.0)),
        }
    }

    #[test]
    fn round_join_switching_preserves_geometry_and_cumulative_counts() {
        let draw = Draw {
            shape: Shape::Polygon(vec![
                RenderPoint::new(1., 1.),
                RenderPoint::new(2., 1.),
                RenderPoint::new(2., 2.),
                RenderPoint::new(1., 2.),
            ]),
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, 0.75)),
        };
        let mut mesh = MeshBuilder::default();
        mesh.append_draw(&draw).unwrap();
        let vertices = mesh.buffers.vertices.clone();
        let indices = mesh.buffers.indices.clone();
        assert_eq!(mesh.round_join_stats(), RoundJoinStats::default());
        let mut exact_counts = RoundJoinStats::default();
        for mode in [
            RoundJoinMode::Exact,
            RoundJoinMode::Checked,
            RoundJoinMode::Reference,
        ] {
            mesh.clear();
            mesh.set_round_join_mode(mode);
            mesh.append_draw(&draw).unwrap();
            assert!(
                mesh.buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .eq(vertices.iter().flatten().map(|v| v.to_bits()))
            );
            assert_eq!(mesh.buffers.indices, indices);
            let counts = mesh.round_join_stats();
            if mode == RoundJoinMode::Exact {
                assert!(counts.shortcuts > 0);
                exact_counts = counts;
            } else {
                assert_eq!(counts.shortcuts, exact_counts.shortcuts * 2);
                assert_eq!(counts.fallbacks, exact_counts.fallbacks * 2);
            }
        }
    }

    #[test]
    fn bulk_stroke_writer_preserves_all_shape_styles_prefixes_and_invalid_colors() {
        use super::super::reusable;
        use std::sync::Arc;
        let triangle = vec![
            RenderPoint::ZERO,
            RenderPoint::new(4., 0.),
            RenderPoint::new(1., 3.),
        ];
        let mut shapes = vec![
            Shape::Circle {
                center: RenderPoint::new(10., -4.),
                radius: 4.25,
            },
            Shape::Line {
                start: RenderPoint::ZERO,
                end: RenderPoint::new(1., -3.),
            },
            Shape::Line {
                start: RenderPoint::ZERO,
                end: RenderPoint::ZERO,
            },
            Shape::LocalPolygon(reusable::Polygon {
                definition: Arc::new(reusable::Definition::new(triangle.clone())),
                scale: 0.3,
                angle: 0.37,
                translation: RenderPoint::new(100., 60.),
            }),
        ];
        for points in [
            vec![],
            vec![RenderPoint::ZERO],
            vec![RenderPoint::ZERO; 3],
            triangle,
            vec![
                RenderPoint::ZERO,
                RenderPoint::new(4., 0.),
                RenderPoint::new(0.01, 0.08),
            ],
            vec![
                RenderPoint::ZERO,
                RenderPoint::new(3., 3.),
                RenderPoint::new(0., 3.),
                RenderPoint::new(3., 0.),
            ],
        ] {
            shapes.push(Shape::Polygon(points));
        }
        for writer in [
            StrokeWriter::Bulk,
            StrokeWriter::BulkStatic,
            StrokeWriter::BulkPolygon,
            StrokeWriter::BulkEdges,
            StrokeWriter::BulkEdgesChecked,
            StrokeWriter::BulkPrepared,
            StrokeWriter::BulkPreparedChecked,
            StrokeWriter::BulkSinglePass,
            StrokeWriter::BulkSinglePassChecked,
        ] {
            for color in [
                RenderColor::WHITE,
                RenderColor::rgba(0.1, 0.2, 0.3, 0.4),
                RenderColor::rgba(-0., f32::MIN_POSITIVE, 0.2, 0.),
                RenderColor::rgba(f32::NAN, f32::INFINITY, 1., 0.),
            ] {
                let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
                b.stroke_writer = writer;
                for shape in &shapes {
                    for width in [0.01, 1., 12.] {
                        for fill in [None, Some(RenderColor::RED)] {
                            let draw = Draw {
                                shape: match shape {
                                    Shape::Circle { center, radius } => Shape::Circle {
                                        center: *center,
                                        radius: *radius,
                                    },
                                    Shape::Line { start, end } => Shape::Line {
                                        start: *start,
                                        end: *end,
                                    },
                                    Shape::Polygon(p) => Shape::Polygon(p.clone()),
                                    Shape::LocalPolygon(p) => Shape::LocalPolygon(p.clone()),
                                },
                                fill,
                                stroke: Some(Stroke::new(color, width)),
                            };
                            assert_eq!(a.append_draw(&draw), b.append_draw(&draw));
                            assert!(
                                a.buffers
                                    .vertices
                                    .iter()
                                    .flatten()
                                    .map(|v| v.to_bits())
                                    .eq(b.buffers.vertices.iter().flatten().map(|v| v.to_bits()))
                            );
                            assert_eq!(a.buffers.indices, b.buffers.indices);
                            assert_eq!(a.validate_bulk(), b.validate_bulk());
                        }
                    }
                }
                b.clear();
                assert_eq!(b.stroke_writer, writer);
                assert!(b.buffers.vertices.is_empty() && b.buffers.indices.is_empty());
            }
        }
    }

    #[test]
    fn prepared_stroke_guards_refresh_per_outline_and_preserve_exact_join_decisions() {
        for writer in [
            StrokeWriter::BulkPrepared,
            StrokeWriter::BulkPreparedChecked,
        ] {
            for mode in [RoundJoinMode::Exact, RoundJoinMode::Checked] {
                let (mut reference, mut candidate) =
                    (MeshBuilder::default(), MeshBuilder::default());
                reference.stroke_writer = StrokeWriter::BulkStatic;
                candidate.stroke_writer = writer;
                reference.set_round_join_mode(mode);
                candidate.set_round_join_mode(mode);
                for width in [0.05, 32., 0.75, 3., 0.75] {
                    for shape in [
                        Shape::Polygon(vec![
                            RenderPoint::new(1., 1.),
                            RenderPoint::new(2., 1.),
                            RenderPoint::new(2., 2.),
                            RenderPoint::new(1., 2.),
                        ]),
                        Shape::Line {
                            start: RenderPoint::new(-2., -1.),
                            end: RenderPoint::new(3., 4.),
                        },
                        Shape::Circle {
                            center: RenderPoint::new(20., 20.),
                            radius: 3.,
                        },
                    ] {
                        let draw = Draw {
                            shape,
                            fill: None,
                            stroke: Some(Stroke::new(RenderColor::rgba(0.2, 0.3, 0.4, 0.5), width)),
                        };
                        reference.append_draw(&draw).unwrap();
                        candidate.append_draw(&draw).unwrap();
                        assert!(
                            reference
                                .buffers
                                .vertices
                                .iter()
                                .flatten()
                                .map(|v| v.to_bits())
                                .eq(candidate
                                    .buffers
                                    .vertices
                                    .iter()
                                    .flatten()
                                    .map(|v| v.to_bits()))
                        );
                        assert_eq!(reference.buffers.indices, candidate.buffers.indices);
                        assert_eq!(reference.round_join_stats(), candidate.round_join_stats());
                        assert_eq!(reference.validate_bulk(), candidate.validate_bulk());
                    }
                }
                assert!(candidate.round_join_stats().shortcuts > 0);
                assert!(candidate.round_join_stats().fallbacks > 0);
            }
        }
    }

    #[test]
    fn bulk_stroke_writer_keeps_frame_budgets() {
        for writer in [
            StrokeWriter::Stock,
            StrokeWriter::Bulk,
            StrokeWriter::BulkStatic,
            StrokeWriter::BulkPolygon,
            StrokeWriter::BulkEdges,
            StrokeWriter::BulkEdgesChecked,
            StrokeWriter::BulkPrepared,
            StrokeWriter::BulkPreparedChecked,
            StrokeWriter::BulkSinglePass,
            StrokeWriter::BulkSinglePassChecked,
        ] {
            let mut mesh = MeshBuilder::default();
            mesh.stroke_writer = writer;
            let mut draw = circle(0., 3.);
            draw.fill = None;
            mesh.buffers.vertices.resize(MAX_VERTICES, [0.; 6]);
            assert_eq!(
                mesh.append_draw(&draw).unwrap_err(),
                "GPU scene exceeds prototype mesh budget"
            );
            mesh.clear();
            mesh.buffers.indices.resize(MAX_INDICES, 0);
            assert_eq!(
                mesh.append_draw(&draw).unwrap_err(),
                "GPU scene exceeds prototype mesh budget"
            );
        }
    }

    #[test]
    fn single_pass_is_lazy_bounded_checked_and_reset_on_writer_exit() {
        let shapes = [
            vec![(0., 0.), (1., 0.), (1., 1.), (0., 1.)],
            vec![(0., 0.), (3., 0.), (0.2, 0.1)],
            vec![(0., 0.); 3],
        ];
        let mut mesh = MeshBuilder::default();
        mesh.append_draw(&circle(0., 1.)).unwrap();
        assert!(mesh.single_pass.is_none());
        mesh.set_stroke_writer(StrokeWriter::BulkSinglePassChecked);
        for _ in 0..3 {
            mesh.clear();
            for points in &shapes {
                mesh.append_draw(&Draw {
                    shape: Shape::Polygon(
                        points
                            .iter()
                            .map(|&(x, y)| RenderPoint::new(x, y))
                            .collect(),
                    ),
                    fill: None,
                    stroke: Some(Stroke::new(RenderColor::WHITE, 0.75)),
                })
                .unwrap();
            }
        }
        let stats = mesh.single_pass_stats();
        assert_eq!((stats.cached, stats.prepared, stats.checked), (3, 3, 6));
        assert_eq!(stats.refused, [0, 3, 0, 0, 0]);
        assert_eq!((stats.layouts.misses, stats.layouts.hits), (1, 2));
        assert!(stats.bounded());
        mesh.set_stroke_writer(StrokeWriter::BulkStatic);
        assert!(mesh.single_pass.is_none());
        assert_eq!(mesh.single_pass_stats(), Default::default());
    }

    #[test]
    fn single_pass_rejects_frame_overflow_before_appending_accepted_mesh() {
        let draw = Draw {
            shape: Shape::Polygon(vec![
                RenderPoint::new(0., 0.),
                RenderPoint::new(1., 0.),
                RenderPoint::new(1., 1.),
                RenderPoint::new(0., 1.),
            ]),
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, 0.75)),
        };
        for writer in [
            StrokeWriter::BulkSinglePassChecked,
            StrokeWriter::BulkSinglePassPrefix,
            StrokeWriter::BulkSinglePassPrefixChecked,
        ] {
            let mut mesh = MeshBuilder::default();
            mesh.set_stroke_writer(writer);
            for vertices in [true, false] {
                mesh.clear();
                if vertices {
                    mesh.buffers.vertices.resize(MAX_VERTICES, [0.; 6]);
                } else {
                    mesh.buffers.indices.resize(MAX_INDICES, 0);
                }
                let before = (mesh.buffers.vertices.len(), mesh.buffers.indices.len());
                let counts = mesh.single_pass_stats();
                assert_eq!(
                    mesh.append_draw(&draw).unwrap_err(),
                    "GPU scene exceeds prototype mesh budget"
                );
                assert_eq!(
                    (mesh.buffers.vertices.len(), mesh.buffers.indices.len()),
                    before
                );
                let delta = mesh.single_pass_stats().delta(counts);
                assert_eq!((delta.checked, delta.reused_prefix_vertices), (0, 0));
            }
        }
    }

    #[test]
    fn prefix_reuse_preserves_frame_prefix_colors_and_switching() {
        let mut reference = MeshBuilder::default();
        reference.set_stroke_writer(StrokeWriter::BulkStatic);
        let mut candidate = MeshBuilder::default();
        for writer in [
            StrokeWriter::BulkSinglePassPrefix,
            StrokeWriter::BulkSinglePass,
            StrokeWriter::BulkSinglePassPrefixChecked,
            StrokeWriter::BulkSinglePassPrefix,
        ] {
            candidate.set_stroke_writer(writer);
            let before = candidate.single_pass_stats();
            for mesh in [&mut reference, &mut candidate] {
                mesh.clear();
                mesh.append_draw(&circle(10., 2.)).unwrap();
                for points in [
                    vec![(0., 0.), (2., 0.), (2., 2.), (1., 1.), (0., 2.)],
                    vec![(0., 0.), (1., 0.), (1., 1.), (0., 1.)],
                    vec![(0., 0.); 3],
                ] {
                    mesh.append_draw(&Draw {
                        shape: Shape::Polygon(
                            points
                                .iter()
                                .map(|&(x, y)| RenderPoint::new(x, y))
                                .collect(),
                        ),
                        fill: Some(RenderColor::rgba(0.3, 0.8, 0.1, 0.6)),
                        stroke: Some(Stroke::new(RenderColor::rgba(0.7, 0.1, 0.2, 0.4), 0.75)),
                    })
                    .unwrap();
                }
            }
            assert!(
                reference
                    .buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|f| f.to_bits())
                    .eq(candidate
                        .buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|f| f.to_bits()))
            );
            assert_eq!(reference.buffers.indices, candidate.buffers.indices);
            assert_eq!(reference.batches, candidate.batches);
            candidate.validate().unwrap();
            let s = candidate.single_pass_stats().delta(before);
            assert_eq!((s.cached, s.prepared), (1, 1));
            assert_eq!(s.refused, [0, 1, 0, 0, 0]);
            assert_eq!(
                s.checked,
                if writer == StrokeWriter::BulkSinglePassPrefixChecked {
                    2
                } else {
                    0
                }
            );
            assert_eq!(
                s.reused_prefix_vertices,
                if writer == StrokeWriter::BulkSinglePassPrefixChecked {
                    3
                } else {
                    0
                }
            );
        }
        candidate.set_stroke_writer(StrokeWriter::BulkStatic);
        assert!(candidate.single_pass.is_none());
        assert_eq!(candidate.single_pass_stats(), Default::default());
    }

    #[test]
    fn selective_single_pass_keeps_prefixes_fallbacks_and_counts() {
        for (writer, layouts, arcs) in [
            (StrokeWriter::BulkSinglePass, true, true),
            (StrokeWriter::BulkSinglePassLayouts, true, false),
            (StrokeWriter::BulkSinglePassArcs, false, true),
            (StrokeWriter::BulkSinglePassClassify, false, false),
        ] {
            let mut reference = MeshBuilder::default();
            reference.set_stroke_writer(StrokeWriter::BulkStatic);
            let mut candidate = MeshBuilder::default();
            candidate.set_stroke_writer(writer);
            for _ in 0..3 {
                reference.clear();
                candidate.clear();
                for mesh in [&mut reference, &mut candidate] {
                    mesh.append_draw(&circle(10., 2.)).unwrap();
                }
                for points in [
                    vec![(0., 0.), (1., 0.), (1., 1.), (0., 1.)],
                    vec![(0., 0.), (3., 0.), (0.2, 0.1)],
                    vec![(0., 0.); 3],
                ] {
                    let draw = Draw {
                        shape: Shape::Polygon(
                            points
                                .iter()
                                .map(|&(x, y)| RenderPoint::new(x, y))
                                .collect(),
                        ),
                        fill: Some(RenderColor::rgba(0.3, 0.8, 0.1, 0.6)),
                        stroke: Some(Stroke::new(RenderColor::rgba(0.7, 0.1, 0.2, 0.4), 0.75)),
                    };
                    reference.append_draw(&draw).unwrap();
                    candidate.append_draw(&draw).unwrap();
                    assert!(
                        reference
                            .buffers
                            .vertices
                            .iter()
                            .flatten()
                            .map(|f| f.to_bits())
                            .eq(candidate
                                .buffers
                                .vertices
                                .iter()
                                .flatten()
                                .map(|f| f.to_bits()))
                    );
                    assert_eq!(reference.buffers.indices, candidate.buffers.indices);
                    assert_eq!(reference.batches, candidate.batches);
                    candidate.validate().unwrap();
                }
            }
            let s = candidate.single_pass_stats();
            assert_eq!(s.cached, if layouts { 3 } else { 0 });
            assert_eq!(s.prepared, if arcs { 3 } else { 0 });
            assert_eq!(
                s.skipped,
                [if layouts { 0 } else { 3 }, if arcs { 0 } else { 3 }]
            );
            assert_eq!(s.refused, [0, 3, 0, 0, 0]);
            assert_eq!(s.layouts.entries, usize::from(layouts));
            assert_eq!(s.layouts.hits, if layouts { 2 } else { 0 });
            assert!(s.bounded());
            candidate.set_stroke_writer(StrokeWriter::BulkStatic);
            assert!(candidate.single_pass.is_none());
        }
    }

    #[test]
    fn tessellation_profile_preserves_geometry_prefixes_and_disabled_counters() {
        let mut draws = vec![circle(10., 4.)];
        for points in [
            vec![],
            vec![RenderPoint::new(0., 0.)],
            vec![RenderPoint::new(0., 0.), RenderPoint::new(0., 0.)],
            vec![
                RenderPoint::new(-3., 4.),
                RenderPoint::new(1., -2.),
                RenderPoint::new(4., 4.),
                RenderPoint::new(-3., -2.),
            ],
        ] {
            draws.push(Draw {
                shape: Shape::Polygon(points),
                fill: Some(RenderColor::RED),
                stroke: Some(Stroke::new(RenderColor::rgba(0.1, 0.2, 0.3, 0.4), 1.75)),
            });
        }
        draws.push(Draw {
            shape: Shape::Line {
                start: RenderPoint::new(-1., 0.),
                end: RenderPoint::new(2., 0.),
            },
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, 0.25)),
        });
        let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
        let (mut disabled, mut enabled) = (DrawProfile::default(), DrawProfile::default());
        for draw in draws {
            for (fill, stroke) in [
                (draw.fill, draw.stroke),
                (None, draw.stroke),
                (draw.fill, None),
            ] {
                let before_vertices = b.buffers.vertices.len();
                let before_indices = b.buffers.indices.len();
                let profile_vertices = enabled.vertices;
                let profile_indices = enabled.indices;
                let x = a
                    .append_draw_profiled::<false>(&draw, fill, stroke, &mut disabled)
                    .unwrap();
                let y = b
                    .append_draw_profiled::<true>(&draw, fill, stroke, &mut enabled)
                    .unwrap();
                assert_eq!(x, y);
                assert!(
                    a.buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits())
                        .eq(b.buffers.vertices.iter().flatten().map(|v| v.to_bits()))
                );
                assert_eq!(a.buffers.indices, b.buffers.indices);
                assert_eq!(
                    enabled.vertices - profile_vertices,
                    (b.buffers.vertices.len() - before_vertices) as u64
                );
                assert_eq!(
                    enabled.indices - profile_indices,
                    (b.buffers.indices.len() - before_indices) as u64
                );
                assert_eq!(disabled, DrawProfile::default());
            }
        }
        assert!(enabled.empty > 0);
        assert_eq!(enabled.vertices, b.buffers.vertices.len() as u64);
        assert_eq!(enabled.indices, b.buffers.indices.len() as u64);
    }

    #[test]
    fn bulk_validation_checks_every_component_and_preserves_finite_contract() {
        let mut mesh = MeshBuilder::default();
        assert_eq!(mesh.validate_bulk(), mesh.validate());
        // Include tails that are not a SIMD register wide, and finite extremes.
        mesh.buffers.vertices = vec![[0., -0., f32::MAX, f32::MIN, f32::from_bits(1), 1.]; 7];
        mesh.circles = vec![[f32::MIN_POSITIVE; 12]; 3];
        assert!(mesh.validate_bulk().is_ok());
        for bad in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0xff80_0001),
        ] {
            for i in 0..mesh.buffers.vertices.len() * 6 {
                let previous =
                    std::mem::replace(&mut mesh.buffers.vertices.as_flattened_mut()[i], bad);
                assert!(mesh.validate_bulk().is_err());
                assert_eq!(mesh.validate_bulk(), mesh.validate());
                mesh.buffers.vertices.as_flattened_mut()[i] = previous;
            }
            for i in 0..mesh.circles.len() * 12 {
                let previous = std::mem::replace(&mut mesh.circles.as_flattened_mut()[i], bad);
                assert!(mesh.validate_bulk().is_err());
                assert_eq!(mesh.validate_bulk(), mesh.validate());
                mesh.circles.as_flattened_mut()[i] = previous;
            }
        }
        assert!(mesh.validate_bulk().is_ok());
    }

    #[test]
    fn submission_controls_leave_geometry_and_batches_unchanged() {
        let scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: vec![
                circle(1., 2.),
                polygon(&[(0., 0.), (10., 0.), (10., 10.)], RenderColor::WHITE),
                circle(30., 10.),
                circle(50., 30.),
            ],
        };
        for (control, reference, uniforms, bindings) in [
            (
                GeometryMode::TrianglesPrograms,
                GeometryMode::TrianglesSplit,
                false,
                false,
            ),
            (
                GeometryMode::CirclesUniforms,
                GeometryMode::Circles,
                true,
                false,
            ),
            (
                GeometryMode::CirclesBindings,
                GeometryMode::Circles,
                false,
                true,
            ),
            (
                GeometryMode::CirclesSubmission,
                GeometryMode::Circles,
                true,
                true,
            ),
            (
                GeometryMode::CirclesUnified,
                GeometryMode::Circles,
                true,
                true,
            ),
            (
                GeometryMode::CirclesUnifiedInstanced,
                GeometryMode::Circles,
                true,
                true,
            ),
            (
                GeometryMode::CirclesUnifiedLayout,
                GeometryMode::Circles,
                true,
                true,
            ),
            (
                GeometryMode::CirclesUnifiedVao,
                GeometryMode::Circles,
                true,
                true,
            ),
        ] {
            assert_eq!(control.frame_uniforms(), uniforms);
            assert_eq!(control.buffer_bindings(), bindings);
            let mut a = MeshBuilder::default();
            a.rebuild_mode(&scene, reference).unwrap();
            let mut b = MeshBuilder::default();
            b.rebuild_mode(&scene, control).unwrap();
            assert_eq!(a.buffers.vertices, b.buffers.vertices);
            assert_eq!(a.buffers.indices, b.buffers.indices);
            assert_eq!(a.circles, b.circles);
            assert_eq!(a.batches, b.batches);
            assert_eq!(a.upload_bytes(), b.upload_bytes());
            b.rebuild_mode(
                &Scene {
                    reuse: Vec::new(),
                    retained: Vec::new(),
                    draws: Vec::new(),
                    width: scene.width,
                    height: scene.height,
                },
                control,
            )
            .unwrap();
            assert!(b.batches.is_empty());
            assert!(b.circles.is_empty());
        }
    }

    #[test]
    fn instancing_keeps_consecutive_runs_and_source_order_without_sorting() {
        let scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: vec![
                circle(1.0, 1.0),
                circle(2.0, 30.0),
                polygon(&[(0., 0.), (10., 0.), (10., 10.)], RenderColor::WHITE),
                circle(3.0, 2.0),
            ],
        };
        let mut mesh = MeshBuilder::default();
        mesh.rebuild_mode(&scene, GeometryMode::Circles).unwrap();
        assert_eq!(mesh.circles.len(), 3);
        assert_eq!(
            mesh.circles[0],
            [1., 20., 1., 2., 0.5, 0.25, 0., 0.5, 1., 1., 1., 1.]
        );
        assert_eq!(
            mesh.batches,
            vec![
                Batch::Circles {
                    start: 0,
                    count: 2,
                    level: circles::level(31.0).unwrap()
                },
                Batch::Triangles { start: 0, count: 3 },
                Batch::Circles {
                    start: 2,
                    count: 1,
                    level: circles::level(3.0).unwrap()
                },
            ]
        );
        mesh.rebuild(&scene).unwrap();
        assert!(mesh.circles.is_empty());
        assert_eq!(mesh.batches.len(), 1);
        assert!(matches!(mesh.batches[0], Batch::Triangles { start: 0, .. }));
        mesh.rebuild_mode(&scene, GeometryMode::Circles).unwrap();
        assert_eq!(mesh.batches.len(), 3);
    }

    #[test]
    fn moving_instances_do_not_rebuild_or_upload_circle_vertices() {
        let mut scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: (0..10_000).map(|i| circle(i as f32, 2.0)).collect(),
        };
        let mut mesh = MeshBuilder::default();
        for tick in 0..3 {
            scene.draws[0] = circle(tick as f32, 2.0 + tick as f32);
            mesh.rebuild_mode(&scene, GeometryMode::Circles).unwrap();
            assert!(mesh.buffers.vertices.is_empty());
            assert!(mesh.buffers.indices.is_empty());
            assert_eq!(mesh.upload_bytes(), 10_000 * 48);
            assert_eq!(mesh.batches.len(), 1);
            assert_eq!(mesh.circles[0][0], tick as f32);
            assert_eq!(mesh.circles[0][2], 2.0 + tick as f32);
        }
        scene.draws.clear();
        mesh.rebuild_mode(&scene, GeometryMode::Circles).unwrap();
        assert!(mesh.circles.is_empty());
        assert!(mesh.batches.is_empty());
    }

    #[test]
    fn split_control_changes_only_draw_boundaries() {
        let scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: vec![
                circle(1.0, 1.0),
                circle(2.0, 30.0),
                polygon(&[(0., 0.), (10., 0.), (10., 10.)], RenderColor::WHITE),
                polygon(&[(20., 0.), (30., 0.), (30., 10.)], RenderColor::WHITE),
                circle(3.0, 2.0),
            ],
        };
        let mut baseline = MeshBuilder::default();
        baseline.rebuild(&scene).unwrap();
        let mut split = MeshBuilder::default();
        split
            .rebuild_mode(&scene, GeometryMode::TrianglesSplit)
            .unwrap();
        assert_eq!(split.buffers.vertices, baseline.buffers.vertices);
        assert_eq!(split.buffers.indices, baseline.buffers.indices);
        assert_eq!(split.upload_bytes(), baseline.upload_bytes());
        assert_eq!(split.batches.len(), 3);
        assert!(split.circles.is_empty());
        let mut cursor = 0;
        for batch in &split.batches {
            let Batch::Triangles { start, count } = *batch else {
                panic!("not triangles")
            };
            assert_eq!(start, cursor);
            assert!(count > 0);
            cursor += count;
        }
        assert_eq!(cursor, baseline.buffers.indices.len());
        split.rebuild(&scene).unwrap();
        assert_eq!(split.batches, baseline.batches);
    }

    #[test]
    fn expanded_control_keeps_run_lod_geometry_and_transparent_source_order() {
        let scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: vec![
                circle(1.0, 1.0),
                circle(2.0, 30.0),
                polygon(&[(0., 0.), (10., 0.), (10., 10.)], RenderColor::WHITE),
                circle(3.0, 2.0),
            ],
        };
        let mut instanced = MeshBuilder::default();
        instanced
            .rebuild_mode(&scene, GeometryMode::Circles)
            .unwrap();
        let mut expected = Vec::new();
        for batch in &instanced.batches {
            match *batch {
                Batch::Triangles { start, count } => expected.extend(
                    instanced.buffers.indices[start..start + count]
                        .iter()
                        .map(|&i| instanced.buffers.vertices[i as usize]),
                ),
                Batch::Circles {
                    start,
                    count,
                    level,
                } => {
                    let (v, ids) = circles::template(circles::LEVELS[level]);
                    for instance in &instanced.circles[start..start + count] {
                        expected.extend(
                            ids.iter()
                                .map(|&i| circles::expand_vertex(v[i as usize], instance)),
                        );
                    }
                }
            }
        }
        let mut expanded = MeshBuilder::default();
        for _ in 0..3 {
            expanded
                .rebuild_mode(&scene, GeometryMode::CirclesExpanded)
                .unwrap();
            assert!(expanded.circles.is_empty());
            assert_eq!(
                expanded.batches,
                vec![Batch::Triangles {
                    start: 0,
                    count: expected.len()
                }]
            );
            let actual: Vec<_> = expanded
                .buffers
                .indices
                .iter()
                .map(|&i| expanded.buffers.vertices[i as usize])
                .collect();
            assert_eq!(actual, expected);
            // Mode changes retain bounded scratch/template storage, not geometry.
            expanded
                .rebuild_mode(&scene, GeometryMode::Circles)
                .unwrap();
            assert_eq!(expanded.circles, instanced.circles);
            assert_eq!(expanded.buffers.indices, instanced.buffers.indices);
        }
        expanded
            .rebuild_mode(
                &Scene {
                    draws: Vec::new(),
                    ..scene
                },
                GeometryMode::CirclesExpanded,
            )
            .unwrap();
        assert!(expanded.buffers.vertices.is_empty());
        assert!(expanded.buffers.indices.is_empty());
        assert!(expanded.batches.is_empty());
    }

    #[test]
    fn expansion_rejects_excess_geometry_before_allocating_it() {
        let mut mesh = MeshBuilder::default();
        let scene = Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: (0..400).map(|i| circle(i as f32, 16000.0)).collect(),
        };
        assert!(
            mesh.rebuild_mode(&scene, GeometryMode::CirclesExpanded)
                .unwrap_err()
                .contains("expanded template mesh budget")
        );
        assert!(mesh.buffers.vertices.is_empty());
        assert!(mesh.buffers.indices.is_empty());
        mesh.rebuild_mode(
            &Scene {
                draws: vec![circle(0.0, 1.0)],
                ..scene
            },
            GeometryMode::CirclesExpanded,
        )
        .unwrap();
        assert_eq!(mesh.batches.len(), 1);
    }
    use engine_common::Stroke;

    fn polygon(points: &[(f32, f32)], color: RenderColor) -> Draw {
        Draw {
            shape: Shape::Polygon(
                points
                    .iter()
                    .map(|&(x, y)| RenderPoint::new(x, y))
                    .collect(),
            ),
            fill: Some(color),
            stroke: None,
        }
    }
    fn build(draws: Vec<Draw>) -> MeshBuilder {
        let mut builder = MeshBuilder::default();
        builder
            .rebuild(&Scene {
                reuse: Vec::new(),
                retained: Vec::new(),
                width: 100,
                height: 100,
                draws,
            })
            .unwrap();
        assert!(
            builder
                .buffers
                .indices
                .iter()
                .all(|&i| (i as usize) < builder.buffers.vertices.len())
        );
        builder
    }
    fn area(mesh: &MeshBuilder) -> f32 {
        mesh.buffers
            .indices
            .chunks_exact(3)
            .map(|ids| {
                let [a, b, c] = [ids[0], ids[1], ids[2]].map(|i| mesh.buffers.vertices[i as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }
    // Test-only point coverage avoids triangulation-edge sample positions.
    fn coverage(mesh: &MeshBuilder, x: f32, y: f32) -> usize {
        mesh.buffers
            .indices
            .chunks_exact(3)
            .filter(|ids| {
                let v = [ids[0], ids[1], ids[2]].map(|i| mesh.buffers.vertices[i as usize]);
                let cross =
                    |a: Vertex, b: Vertex| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
                let e = [cross(v[0], v[1]), cross(v[1], v[2]), cross(v[2], v[0])];
                e.iter().all(|c| *c > 0.0) || e.iter().all(|c| *c < 0.0)
            })
            .count()
    }

    #[test]
    fn concave_fill_handles_both_windings_and_keeps_the_notch_empty() {
        let p = [
            (0., 0.),
            (20., 0.),
            (20., 8.),
            (8., 8.),
            (8., 20.),
            (0., 20.),
        ];
        for points in [p.to_vec(), p.into_iter().rev().collect()] {
            let m = build(vec![polygon(&points, RenderColor::WHITE)]);
            assert!((area(&m) - 256.0).abs() < 0.01);
            assert_eq!(coverage(&m, 17.1, 16.8), 0);
            assert_eq!(coverage(&m, 4.1, 16.8), 1);
        }
    }

    #[test]
    fn adjacent_translucent_cells_have_no_fringe_or_duplicate_coverage() {
        let color = RenderColor::rgba(0.2, 0.6, 1.0, 0.5);
        let m = build(vec![
            polygon(&[(0., 0.), (10.25, 0.), (10.25, 20.), (0., 20.)], color),
            polygon(&[(10.25, 0.), (20., 0.), (20., 20.), (10.25, 20.)], color),
        ]);
        assert!((area(&m) - 400.).abs() < 0.01);
        for x in [10.249, 10.251] {
            assert_eq!(coverage(&m, x, 9.125), 1);
        }
        assert!(
            m.buffers
                .vertices
                .iter()
                .all(|v| v[2..] == [0.1, 0.3, 0.5, 0.5])
        );
    }

    #[test]
    fn overlapping_colors_and_fill_then_stroke_remain_in_source_order() {
        let mut first = polygon(
            &[(0., 0.), (20., 0.), (20., 20.), (0., 20.)],
            RenderColor::rgba(1., 0., 0., 0.5),
        );
        first.stroke = Some(Stroke::new(RenderColor::WHITE, 2.0));
        let second = polygon(
            &[(5., 5.), (25., 5.), (25., 25.), (5., 25.)],
            RenderColor::rgba(0., 0., 1., 0.25),
        );
        let m = build(vec![first, second]);
        let mut runs = Vec::new();
        for ids in m.buffers.indices.chunks_exact(3) {
            let color = m.buffers.vertices[ids[0] as usize][2..].to_vec();
            assert!(
                ids.iter()
                    .all(|&i| m.buffers.vertices[i as usize][2..] == color)
            );
            if runs.last() != Some(&color) {
                runs.push(color);
            }
        }
        assert_eq!(
            runs,
            vec![
                vec![0.5, 0., 0., 0.5],
                vec![1., 1., 1., 1.],
                vec![0., 0., 0.25, 0.25]
            ]
        );
    }

    #[test]
    fn circles_lines_degenerate_edges_and_offscreen_vertices_stay_finite() {
        let m = build(vec![
            Draw {
                shape: Shape::Circle {
                    center: RenderPoint::new(50., 50.),
                    radius: 10.,
                },
                fill: Some(RenderColor::WHITE),
                stroke: None,
            },
            Draw {
                shape: Shape::Line {
                    start: RenderPoint::new(-5., 20.),
                    end: RenderPoint::new(105., 20.),
                },
                fill: None,
                stroke: Some(Stroke::new(RenderColor::WHITE, 2.)),
            },
            polygon(
                &[(0., 0.), (0., 0.), (1., 1.), (1., 1.)],
                RenderColor::WHITE,
            ),
        ]);
        assert!(m.buffers.vertices.iter().flatten().all(|v| v.is_finite()));
        assert!(m.buffers.vertices.iter().any(|v| v[0] < 0.));
        assert!(m.buffers.vertices.iter().any(|v| v[0] > 100.));
        assert!(area(&m) > 500. && area(&m) < 550.);
    }

    #[test]
    fn rebuild_reuses_storage_and_empty_scene_drops_old_geometry() {
        let mut m = build(vec![polygon(
            &[(0., 0.), (20., 0.), (20., 20.), (0., 20.)],
            RenderColor::WHITE,
        )]);
        let capacity = m.buffers.vertices.capacity();
        assert!(m.upload_bytes() > 0);
        m.rebuild(&Scene {
            reuse: Vec::new(),
            retained: Vec::new(),
            width: 100,
            height: 100,
            draws: vec![],
        })
        .unwrap();
        assert_eq!(m.upload_bytes(), 0);
        assert_eq!(m.buffers.vertices.capacity(), capacity);
    }
}
