//! Bounded render-lab experiment: reuse triangle connectivity, NOT positions.
//! Fixed-width closed polygons only. Re-evaluate merging/folding on the current
//! f32 points; refuse merged endpoints. The original experiment refuses arcs;
//! the single-pass experiment resumes from its prepared corner prefix instead.
//! The ordinary tessellator is unchanged and remains the fallback and oracle.
use super::*;
use crate::geometry_builder::{GeometryBuilder, GeometryBuilderError};
use alloc::collections::{btree_map::Entry, BTreeMap, VecDeque};

const MAX_POINTS: usize = 64;
const MAX_LAYOUTS: usize = 128;
const MAX_LAYOUT_BYTES: usize = 512 * 1024;

/// A refusal requires ordinary tessellation of the original polygon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeTopologyReject {
    Unsupported,
    Merge,
    Arc,
    NonFinite,
    Budget,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrokeTopologyStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    /// Key and index allocation payload, not allocator/map node overhead.
    pub bytes: usize,
}

/// Position-only output for triangle renderers. No source/advancement/attribute
/// API is implied. References are valid until the next mutable tessellator use.
pub struct StrokeTopologyMesh<'a> {
    pub positions: &'a [Point],
    pub indices: &'a [u32],
    pub source: StrokeTopologySource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeTopologySource {
    /// A cached layout (possibly just compiled on a cold miss).
    Layout,
    /// Fresh triangles/arcs from already-computed corner geometry; not cached.
    Prepared,
}

/// Diagnostic emission control. A skipped result has classified the current
/// points, but has NOT looked up/compiled a layout or emitted fresh arcs.
/// The caller must tessellate the original polygon with the ordinary writer.
pub enum StrokeTopologyOutput<'a> {
    Mesh(StrokeTopologyMesh<'a>),
    Skipped(StrokeTopologySource),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    count: u8,
    // Fold side and single-vertex side flags fully specify the no-arc topology.
    joins: [u8; MAX_POINTS],
}

pub struct StrokeTopologyTessellator {
    joins: Vec<EndpointData>,
    positions: Vec<Point>,
    layouts: BTreeMap<Key, Vec<u32>>,
    order: VecDeque<Key>,
    stats: StrokeTopologyStats,
    max_layouts: usize,
    computed_joins: usize,
    prepared_indices: Vec<u32>,
}

impl Default for StrokeTopologyTessellator {
    fn default() -> Self {
        Self {
            joins: Vec::with_capacity(MAX_POINTS),
            positions: Vec::with_capacity(MAX_POINTS * 4 + 2),
            layouts: BTreeMap::new(),
            order: VecDeque::new(),
            stats: StrokeTopologyStats::default(),
            max_layouts: MAX_LAYOUTS,
            computed_joins: 0,
            prepared_indices: Vec::new(),
        }
    }
}

fn vertex_data(half_width: f32) -> StrokeVertexData<'static> {
    StrokeVertexData {
        position_on_path: Point::zero(),
        normal: vector(0.0, 0.0),
        half_width,
        advancement: 0.0,
        side: Side::Negative,
        src: VertexSource::Endpoint {
            id: EndpointId::INVALID,
        },
        buffer: &mut [],
        buffer_is_valid: false,
    }
}

impl StrokeTopologyTessellator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> StrokeTopologyStats {
        self.stats
    }

    /// Cache keys are geometry-independent topology states. Widths, positions
    /// and angles are NOT frozen: prepare() recomputes and certifies them before
    /// every lookup. Cold entries use the original edge/join triangle functions.
    pub fn tessellate(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<StrokeTopologyMesh<'_>, StrokeTopologyReject> {
        let key = self.prepare::<false, false>(polygon, options)?;
        self.cached_mesh(key, options)
    }

    /// Resume rejected round arcs using the corner prefix that classification
    /// already computed. Merged paths and unsupported styles still require the
    /// ordinary tessellator. Like tessellate(), this is position-only output.
    pub fn tessellate_single_pass(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<StrokeTopologyMesh<'_>, StrokeTopologyReject> {
        match self.tessellate_single_pass_control::<true, true>(polygon, options)? {
            StrokeTopologyOutput::Mesh(mesh) => Ok(mesh),
            StrokeTopologyOutput::Skipped(_) => unreachable!("both emitters are enabled"),
        }
    }

    /// Render-lab ablation: classification is identical in every control, but
    /// either emitter may be disabled before its output work. These switches do
    /// not predict acceptance cheaply or cache screen coordinates. In particular,
    /// false/false measures classification followed by ordinary fallback, not a
    /// production optimization. Const controls keep the all-enabled path direct.
    pub fn tessellate_single_pass_control<const LAYOUTS: bool, const PREPARED: bool>(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<StrokeTopologyOutput<'_>, StrokeTopologyReject> {
        self.single_pass_control::<LAYOUTS, PREPARED, false>(polygon, options, |_| {})
    }

    /// Opt-in scratch-reuse experiment. Keep the certified no-arc prefix's
    /// positions and IDs when resuming fresh arcs, never positions from a prior
    /// polygon/frame. The original single-pass entry point remains the control.
    pub fn tessellate_single_pass_prefix(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<StrokeTopologyMesh<'_>, StrokeTopologyReject> {
        match self.single_pass_control::<true, true, true>(polygon, options, |_| {})? {
            StrokeTopologyOutput::Mesh(mesh) => Ok(mesh),
            StrokeTopologyOutput::Skipped(_) => unreachable!("both emitters are enabled"),
        }
    }

    /// Untimed diagnostic counterpart. The count is the number of base positions
    /// retained from classification, not vertices removed from output. Keep this
    /// payload out of the common mesh result and the uninstrumented entry points.
    pub fn tessellate_single_pass_prefix_counted(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<(StrokeTopologyMesh<'_>, usize), StrokeTopologyReject> {
        let mut reused = 0;
        match self.single_pass_control::<true, true, true>(polygon, options, |n| reused = n)? {
            StrokeTopologyOutput::Mesh(mesh) => Ok((mesh, reused)),
            StrokeTopologyOutput::Skipped(_) => unreachable!("both emitters are enabled"),
        }
    }

    fn single_pass_control<const LAYOUTS: bool, const PREPARED: bool, const PREFIX: bool>(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
        count_prefix: impl FnOnce(usize),
    ) -> Result<StrokeTopologyOutput<'_>, StrokeTopologyReject> {
        let key = match self.prepare::<true, true>(polygon, options) {
            Ok(key) => key,
            // Bound arc recursion/work before emission. At this tolerance the
            // flattening step is at least 120 degrees, so any full round join
            // has at most four segments. Finer arcs keep the ordinary path.
            Err(StrokeTopologyReject::Arc) if options.tolerance >= options.line_width * 0.25 => {
                if !PREPARED {
                    return Ok(StrokeTopologyOutput::Skipped(
                        StrokeTopologySource::Prepared,
                    ));
                }
                let reused_prefix_vertices = if PREFIX { self.positions.len() } else { 0 };
                emit_prepared::<PREFIX>(
                    &mut self.joins,
                    self.computed_joins,
                    options,
                    &mut self.positions,
                    &mut self.prepared_indices,
                )?;
                count_prefix(reused_prefix_vertices);
                return Ok(StrokeTopologyOutput::Mesh(StrokeTopologyMesh {
                    positions: &self.positions,
                    indices: &self.prepared_indices,
                    source: StrokeTopologySource::Prepared,
                }));
            }
            Err(reason) => return Err(reason),
        };
        if !LAYOUTS {
            return Ok(StrokeTopologyOutput::Skipped(StrokeTopologySource::Layout));
        }
        self.cached_mesh(key, options)
            .map(StrokeTopologyOutput::Mesh)
    }

    fn cached_mesh(
        &mut self,
        key: Key,
        options: &StrokeOptions,
    ) -> Result<StrokeTopologyMesh<'_>, StrokeTopologyReject> {
        if self.layouts.len() == self.max_layouts && !self.layouts.contains_key(&key) {
            let old = self.order.pop_front().ok_or(StrokeTopologyReject::Budget)?;
            let indices = self
                .layouts
                .remove(&old)
                .ok_or(StrokeTopologyReject::Budget)?;
            self.stats.bytes -= layout_bytes(&indices);
            self.stats.evictions += 1;
            self.stats.entries = self.order.len();
        }
        let indices = match self.layouts.entry(key) {
            Entry::Occupied(e) => {
                self.stats.hits += 1;
                e.into_mut()
            }
            Entry::Vacant(e) => {
                let indices = compile(&self.joins, &self.positions, options)?;
                let bytes = layout_bytes(&indices);
                if bytes > MAX_LAYOUT_BYTES.saturating_sub(self.stats.bytes) {
                    return Err(StrokeTopologyReject::Budget);
                }
                self.stats.misses += 1;
                self.stats.bytes += bytes;
                self.order.push_back(key);
                self.stats.entries = self.order.len();
                e.insert(indices)
            }
        };
        Ok(StrokeTopologyMesh {
            positions: &self.positions,
            indices,
            source: StrokeTopologySource::Layout,
        })
    }

    fn prepare<const KEEP_PREFIX: bool, const EARLY_ARC_REFUSAL: bool>(
        &mut self,
        polygon: Polygon<'_, Point>,
        options: &StrokeOptions,
    ) -> Result<Key, StrokeTopologyReject> {
        self.joins.clear();
        self.positions.clear();
        if KEEP_PREFIX {
            self.computed_joins = 0;
        }
        let points = polygon.points;
        if !polygon.closed
            || !(3..=MAX_POINTS).contains(&points.len())
            || options.variable_line_width.is_some()
            || options.line_join != LineJoin::Round
            || !options.line_width.is_finite()
            || !(0.002..=2048.0).contains(&options.line_width)
            || !options.tolerance.is_finite()
            || options.tolerance <= 0.0
        {
            return Err(StrokeTopologyReject::Unsupported);
        }
        if points.iter().any(|p| {
            !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 16384.0 || p.y.abs() > 16384.0
        }) {
            return Err(StrokeTopologyReject::NonFinite);
        }
        // Exact pinned-Lyon arithmetic, including the closing edge. With no
        // skipped point there is no special closing-point rewrite or cap.
        let merge = (options.tolerance * options.tolerance * 0.5)
            .min(options.line_width * options.line_width * 0.05)
            .max(1e-8);
        if (0..points.len())
            .any(|i| (points[i] - points[(i + 1) % points.len()]).square_length() < merge)
        {
            return Err(StrokeTopologyReject::Merge);
        }
        // Round joins always have a front side without a single vertex, and
        // folding that side does not suppress its own arc test. Below this
        // tolerance has_no_arc() must fail, and emit_prepared() is also
        // ineligible. Keep the original validation/refusal priority, but avoid
        // initializing endpoints and computing that first doomed corner.
        // The false instantiation retains the original path as a test oracle
        // and for the separate topology-only experiment.
        if EARLY_ARC_REFUSAL && options.tolerance < options.line_width * 0.25 {
            return Err(StrokeTopologyReject::Arc);
        }
        let half = options.line_width * 0.5;
        let mut vertex = vertex_data(half);
        self.joins
            .extend(points.iter().map(|&position| EndpointData {
                position,
                half_width: half,
                advancement: 0.0,
                line_join: LineJoin::Round,
                ..EndpointData::default()
            }));
        let mut key = Key {
            count: points.len() as u8,
            joins: [0; MAX_POINTS],
        };
        // Original order: joins 1..n-1, then 0, then two closing-edge vertices.
        for step in 0..points.len() {
            let i = (step + 1) % points.len();
            let prev = self.joins[(i + points.len() - 1) % points.len()];
            let next = self.joins[(i + 1) % points.len()];
            let join = &mut self.joins[i];
            compute_join_side_positions_fixed_width(
                &prev,
                join,
                &next,
                options.miter_limit,
                &mut vertex,
                &mut UncachedEdges,
            )
            .map_err(|_| StrokeTopologyReject::NonFinite)?;
            if KEEP_PREFIX {
                self.computed_joins = step + 1;
            }
            for side in 0..2 {
                if join.side_points[side].single_vertex.is_none() && !join.fold[1 - side] {
                    // The geometric no-arc predicate does not depend on fold
                    // state: the original additionally excludes folds as a
                    // conservative selection policy. Retain ALL its numeric
                    // bounds and angular margin, checking the actual side.
                    let mut arc = *join;
                    arc.fold = [false, false];
                    if !round_join::has_no_arc(&arc, side, options) {
                        return Err(StrokeTopologyReject::Arc);
                    }
                }
            }
            key.joins[i] = u8::from(join.fold[0])
                | (u8::from(join.fold[1]) << 1)
                | (u8::from(join.side_points[0].single_vertex.is_some()) << 2)
                | (u8::from(join.side_points[1].single_vertex.is_some()) << 3);
            for side in [SIDE_NEGATIVE, SIDE_POSITIVE] {
                let data = &mut join.side_points[side];
                if let Some(p) = data.single_vertex {
                    let id = append_position(&mut self.positions, join.position, p, half)?;
                    data.prev_vertex = id;
                    data.next_vertex = id;
                } else {
                    data.prev_vertex =
                        append_position(&mut self.positions, join.position, data.prev, half)?;
                    data.next_vertex =
                        append_position(&mut self.positions, join.position, data.next, half)?;
                }
            }
        }
        let first = self.joins[0];
        for side in [SIDE_POSITIVE, SIDE_NEGATIVE] {
            let data = first.side_points[side];
            append_position(
                &mut self.positions,
                first.position,
                data.single_vertex.unwrap_or(data.next),
                half,
            )?;
        }
        Ok(key)
    }
}

// A full turn has at most two sides * three extra arc vertices per join in
// this bounded tolerance domain, in addition to four base vertices. Limits
// are checked as well as derived, so a future tessellator change fails closed.
const MAX_PREPARED_VERTICES: usize = MAX_POINTS * 10 + 2;
const MAX_PREPARED_INDICES: usize = MAX_POINTS * 48;

struct PreparedOutput<'a> {
    positions: &'a mut Vec<Point>,
    indices: &'a mut Vec<u32>,
    overflow: bool,
}

impl GeometryBuilder for PreparedOutput<'_> {
    fn begin_geometry(&mut self) {}
    fn abort_geometry(&mut self) {
        self.positions.clear();
        self.indices.clear();
    }
    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        if self.indices.len() + 3 > MAX_PREPARED_INDICES {
            self.overflow = true;
            return;
        }
        self.indices.extend_from_slice(&[a.0, b.0, c.0]);
    }
}

impl StrokeGeometryBuilder for PreparedOutput<'_> {
    fn add_stroke_vertex(
        &mut self,
        vertex: StrokeVertex,
    ) -> Result<VertexId, GeometryBuilderError> {
        let p = vertex.position();
        if self.positions.len() >= MAX_PREPARED_VERTICES || !p.x.is_finite() || !p.y.is_finite() {
            return Err(GeometryBuilderError::TooManyVertices);
        }
        let id = VertexId(self.positions.len() as u32);
        self.positions.push(p);
        Ok(id)
    }
}

fn emit_prepared<const PREFIX: bool>(
    joins: &mut [EndpointData],
    ready: usize,
    options: &StrokeOptions,
    positions: &mut Vec<Point>,
    indices: &mut Vec<u32>,
) -> Result<(), StrokeTopologyReject> {
    if !PREFIX {
        positions.clear();
    }
    // `ready` includes the first corner requiring an arc. prepare() returned
    // before emitting its bases, so only ready - 1 corners have reusable IDs.
    let prefix_corners = if PREFIX { ready.saturating_sub(1) } else { 0 };
    indices.clear();
    // Reserve fixed upper bounds rather than allowing geometric Vec growth to
    // overshoot the declared scratch limits. Allocation is reused on later calls.
    positions.reserve_exact(MAX_PREPARED_VERTICES.saturating_sub(positions.len()));
    indices.reserve_exact(MAX_PREPARED_INDICES.saturating_sub(indices.len()));
    let mut output = PreparedOutput {
        positions,
        indices,
        overflow: false,
    };
    let mut vertex = vertex_data(options.line_width * 0.5);
    let mut policy = EnabledRoundJoins::<false>::default();
    for step in 0..joins.len() {
        let i = (step + 1) % joins.len();
        let prev = joins[(i + joins.len() - 1) % joins.len()];
        let next = joins[(i + 1) % joins.len()];
        let join = &mut joins[i];
        if step >= ready {
            compute_join_side_positions_fixed_width(
                &prev,
                join,
                &next,
                options.miter_limit,
                &mut vertex,
                &mut UncachedEdges,
            )
            .map_err(|_| StrokeTopologyReject::NonFinite)?;
        }
        vertex.position_on_path = join.position;
        // Preserve interleaving: a join's base vertices, edge triangles, then
        // its arc vertices. Pre-emitting all bases would change vertex IDs.
        if step >= prefix_corners {
            for side in [Side::Negative, Side::Positive] {
                add_join_base_vertices(join, &mut vertex, &(), &mut output, side)
                    .map_err(|_| StrokeTopologyReject::Budget)?;
            }
        }
        if step > 0 {
            add_edge_triangles(&prev, join, &mut output);
        }
        let before = output.positions.len();
        tessellate_join::<EnabledRoundJoins<false>, _, OrdinaryGuard>(
            join,
            options,
            &mut vertex,
            &(),
            &mut output,
            &mut policy.counter(),
            &OrdinaryGuard,
        )
        .map_err(|_| StrokeTopologyReject::Budget)?;
        // Certification promises no extra vertices in the reused prefix. Fail
        // closed if a future join change breaks it, rather than corrupt IDs of
        // later prefix corners. Nothing has reached the caller's frame yet.
        if PREFIX && step < prefix_corners && output.positions.len() != before {
            return Err(StrokeTopologyReject::Budget);
        }
    }
    let mut first = joins[0];
    vertex.position_on_path = first.position;
    for side in [SIDE_POSITIVE, SIDE_NEGATIVE] {
        let data = &mut first.side_points[side];
        vertex.normal =
            (data.single_vertex.unwrap_or(data.next) - first.position) / first.half_width;
        data.next_vertex = output
            .add_stroke_vertex(StrokeVertex(&mut vertex, &()))
            .map_err(|_| StrokeTopologyReject::Budget)?;
    }
    add_edge_triangles(&first, &joins[1], &mut output);
    if output.overflow
        || output
            .indices
            .iter()
            .any(|&i| i as usize >= output.positions.len())
    {
        return Err(StrokeTopologyReject::Budget);
    }
    Ok(())
}

fn append_position(
    out: &mut Vec<Point>,
    path: Point,
    side: Point,
    half: f32,
) -> Result<VertexId, StrokeTopologyReject> {
    // Preserve the divide then multiply in add_join_base_vertices + position().
    // Using `side` directly is mathematically equal but NOT bit-equivalent.
    let normal = (side - path) / half;
    let position = path + normal * half;
    if !position.x.is_finite() || !position.y.is_finite() {
        return Err(StrokeTopologyReject::NonFinite);
    }
    let id = VertexId(out.len() as u32);
    out.push(position);
    Ok(id)
}

fn layout_bytes(indices: &Vec<u32>) -> usize {
    core::mem::size_of::<Key>()
        + core::mem::size_of::<Vec<u32>>()
        + indices.capacity() * core::mem::size_of::<u32>()
}

struct Indices {
    values: Vec<u32>,
    overflow: bool,
    limit: usize,
}
impl GeometryBuilder for Indices {
    fn begin_geometry(&mut self) {}
    fn abort_geometry(&mut self) {
        self.values.clear();
    }
    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        if self.values.len() + 3 > self.limit {
            self.overflow = true;
            return;
        }
        self.values.extend_from_slice(&[a.0, b.0, c.0]);
    }
}
impl StrokeGeometryBuilder for Indices {
    fn add_stroke_vertex(&mut self, _: StrokeVertex) -> Result<VertexId, GeometryBuilderError> {
        // A nonempty arc means the certification was insufficient. Fail closed.
        Err(GeometryBuilderError::TooManyVertices)
    }
}

fn compile(
    joins: &[EndpointData],
    positions: &[Point],
    options: &StrokeOptions,
) -> Result<Vec<u32>, StrokeTopologyReject> {
    let mut out = Indices {
        values: Vec::with_capacity(joins.len() * 12),
        overflow: false,
        limit: joins.len() * 12,
    };
    let mut vertex = vertex_data(options.line_width * 0.5);
    for step in 0..joins.len() {
        let i = (step + 1) % joins.len();
        let mut join = joins[i];
        if step > 0 {
            add_edge_triangles(&joins[(i + joins.len() - 1) % joins.len()], &join, &mut out);
        }
        vertex.position_on_path = join.position;
        tessellate_join::<ReferenceRoundJoins, _, OrdinaryGuard>(
            &mut join,
            options,
            &mut vertex,
            &(),
            &mut out,
            &mut (),
            &OrdinaryGuard,
        )
        .map_err(|_| StrokeTopologyReject::Arc)?;
    }
    let mut first = joins[0];
    first.side_points[SIDE_POSITIVE].next_vertex = VertexId(positions.len() as u32 - 2);
    first.side_points[SIDE_NEGATIVE].next_vertex = VertexId(positions.len() as u32 - 1);
    add_edge_triangles(&first, &joins[1], &mut out);
    if out.overflow || out.values.iter().any(|&i| i as usize >= positions.len()) {
        return Err(StrokeTopologyReject::Budget);
    }
    Ok(out.values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuffersBuilder, VertexBuffers};
    use alloc::vec;

    fn compare(tess: &mut StrokeTopologyTessellator, points: &[Point], width: f32) -> bool {
        compare_mode(tess, points, width, false).is_some()
    }

    fn compare_mode(
        tess: &mut StrokeTopologyTessellator,
        points: &[Point],
        width: f32,
        single_pass: bool,
    ) -> Option<StrokeTopologySource> {
        let options = StrokeOptions::default()
            .with_line_width(width)
            .with_tolerance(0.25)
            .with_line_join(LineJoin::Round)
            .with_line_cap(LineCap::Round);
        let polygon = Polygon {
            points,
            closed: true,
        };
        let mut reference = VertexBuffers::<Point, u32>::new();
        StrokeTessellator::new()
            .tessellate_polygon(
                polygon.clone(),
                &options,
                &mut BuffersBuilder::new(&mut reference, |v: StrokeVertex| v.position()),
            )
            .unwrap();
        let result = if single_pass {
            tess.tessellate_single_pass(polygon, &options)
        } else {
            tess.tessellate(polygon, &options)
        };
        let actual = match result {
            Ok(m) => m,
            Err(_) => return None,
        };
        assert_eq!(
            reference.indices, actual.indices,
            "indices points={points:?} width={width}"
        );
        assert_eq!(
            reference
                .vertices
                .iter()
                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                .collect::<Vec<_>>(),
            actual
                .positions
                .iter()
                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                .collect::<Vec<_>>(),
            "positions points={points:?} width={width}"
        );
        Some(actual.source)
    }

    #[test]
    fn topology_reuse_matches_reference_under_geometry_and_style_changes() {
        let mut tess = StrokeTopologyTessellator::new();
        let mut accepted = 0;
        let shapes: &[&[(f32, f32)]] = &[
            &[(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)],
            &[(0., 0.), (1., 0.), (0.5, 0.8660254)],
            &[(-2., -1.), (2., -1.), (0., 0.), (2., 1.), (-2., 1.)],
            &[(-1., -1.), (1., 1.), (-1., 1.), (1., -1.)],
            &[(0., 0.), (2., 0.), (2., 0.), (0., 2.)],
        ];
        for shape in shapes {
            for width in [0.002, 0.1, 0.75, 1., 3., 10.] {
                for scale in [0.01, 0.1, 0.2, 0.49, 0.5, 0.51, 1., 3., 10.] {
                    for angle in [0.0_f32, 0.01, 0.37, 1.57, 3.13] {
                        for offset in [point(0., 0.), point(800.125, 479.75)] {
                            let (s, c) = angle.sin_cos();
                            let points: Vec<_> = shape
                                .iter()
                                .map(|&(x, y)| {
                                    point(
                                        (c * x - s * y) * scale + offset.x,
                                        (s * x + c * y) * scale + offset.y,
                                    )
                                })
                                .collect();
                            accepted += usize::from(compare(&mut tess, &points, width));
                        }
                    }
                }
            }
        }
        assert!(accepted > 100);
        assert!(tess.stats.hits > 100);
        assert!(tess.stats.entries <= MAX_LAYOUTS);
        assert!(tess.stats.bytes <= MAX_LAYOUT_BYTES);
    }

    #[test]
    fn refusal_never_reuses_stale_points_and_styles_do_not_alias_geometry() {
        let mut tess = StrokeTopologyTessellator::new();
        let points = [point(0., 0.), point(1., 0.), point(1., 1.), point(0., 1.)];
        assert!(compare(&mut tess, &points, 0.75));
        let before = tess.stats();
        let options = StrokeOptions::default().with_line_join(LineJoin::Round);
        assert!(matches!(
            tess.tessellate(
                Polygon {
                    points: &points,
                    closed: false
                },
                &options
            ),
            Err(StrokeTopologyReject::Unsupported)
        ));
        assert!(matches!(
            tess.tessellate(
                Polygon {
                    points: &[point(0., 0.); 3],
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::Merge)
        ));
        assert_eq!(before, tess.stats());
        assert!(compare(&mut tess, &points, 0.75));
        assert!(compare(&mut tess, &points, 0.1));
        assert_eq!(tess.stats().hits, before.hits + 2);
    }

    fn regular_polygon(count: usize) -> Vec<Point> {
        (0..count)
            .map(|i| {
                let (s, c) = (i as f32 * 2.0 * PI / count as f32).sin_cos();
                point(c * 10.0, s * 10.0)
            })
            .collect()
    }

    #[test]
    fn fifo_eviction_is_bounded_and_rebuilt_entries_match_reference() {
        let mut tess = StrokeTopologyTessellator::new();
        tess.max_layouts = 2;
        // A hit does not refresh insertion order: 6 evicts 4, not 5.
        for count in [4, 5, 4, 6, 4] {
            assert!(compare(&mut tess, &regular_polygon(count), 0.75));
            assert!(tess.stats.entries <= 2);
            assert_eq!(tess.stats.entries, tess.layouts.len());
            assert_eq!(tess.stats.entries, tess.order.len());
            assert_eq!(
                tess.stats.bytes,
                tess.layouts.values().map(layout_bytes).sum()
            );
        }
        assert_eq!(tess.stats.hits, 1);
        assert_eq!(tess.stats.misses, 4);
        assert_eq!(tess.stats.evictions, 2);
        assert!(compare(&mut tess, &regular_polygon(MAX_POINTS), 0.75));
        assert!(tess.stats.bytes < MAX_LAYOUT_BYTES);
        assert!(!compare(&mut tess, &regular_polygon(MAX_POINTS + 1), 0.75));
    }

    #[test]
    fn invalid_styles_and_nonfinite_points_fail_closed() {
        let mut tess = StrokeTopologyTessellator::new();
        let points = regular_polygon(4);
        let options = StrokeOptions::default()
            .with_line_width(0.75)
            .with_tolerance(0.25)
            .with_line_join(LineJoin::Round);
        for bad in [
            options.with_variable_line_width(0),
            options.with_line_width(f32::NAN),
            options.with_line_width(0.0),
            options.with_line_width(4096.0),
            options.with_tolerance(f32::INFINITY),
            options.with_tolerance(0.0),
            options.with_line_join(LineJoin::Bevel),
        ] {
            assert!(matches!(
                tess.tessellate(
                    Polygon {
                        points: &points,
                        closed: true
                    },
                    &bad
                ),
                Err(StrokeTopologyReject::Unsupported)
            ));
        }
        for value in [f32::NAN, f32::INFINITY, 16385.0] {
            let mut bad = points.clone();
            bad[0].x = value;
            assert!(matches!(
                tess.tessellate(
                    Polygon {
                        points: &bad,
                        closed: true
                    },
                    &options
                ),
                Err(StrokeTopologyReject::NonFinite)
            ));
        }
        tess.max_layouts = 0;
        assert!(matches!(
            tess.tessellate(
                Polygon {
                    points: &points,
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::Budget)
        ));
        assert_eq!(tess.stats(), StrokeTopologyStats::default());
    }

    #[test]
    fn deterministic_mixed_polygons_match_on_cold_and_warm_paths() {
        let mut tess = StrokeTopologyTessellator::new();
        let mut random = 0x013579bd_u32;
        let mut next = || {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            (random >> 8) as f32 / (1u32 << 24) as f32
        };
        let mut accepted = 0;
        let mut prepared = 0;
        for sample in 0..3000 {
            let count = 3 + sample % 10;
            let radius = [0.15, 0.3, 0.5, 1.0, 3.0, 15.0][sample % 6];
            let offset = if sample % 2 == 0 { 0.0 } else { 800.125 };
            let points: Vec<_> = (0..count)
                .map(|i| {
                    let angle = (i as f32 + (next() - 0.5) * 0.2) * 2.0 * PI / count as f32;
                    let (s, c) = angle.sin_cos();
                    let r = radius * (0.7 + next() * 0.6);
                    point(c * r + offset, s * r + offset)
                })
                .collect();
            let width = [0.1, 0.5, 0.75, 1.0, 2.0][sample % 5];
            accepted += usize::from(compare(&mut tess, &points, width));
            accepted += usize::from(compare(&mut tess, &points, width));
            for _ in 0..2 {
                prepared += usize::from(
                    compare_mode(&mut tess, &points, width, true)
                        == Some(StrokeTopologySource::Prepared),
                );
            }
        }
        assert!(accepted > 1000, "only {} accepted checks", accepted);
        assert!(tess.stats.hits > 500);
        assert!(prepared > 1000, "only {} prepared checks", prepared);
        assert!(tess.stats.entries <= MAX_LAYOUTS);
        assert!(tess.stats.bytes <= MAX_LAYOUT_BYTES);
    }

    #[test]
    fn single_pass_resumes_partial_geometry_and_preserves_interleaved_arcs() {
        let points = [
            point(0., 0.),
            point(2., 0.),
            point(2., 2.),
            point(1., 1.),
            point(0., 2.),
        ];
        let options = StrokeOptions::default()
            .with_line_width(0.75)
            .with_tolerance(0.25)
            .with_line_join(LineJoin::Round);
        let mut tess = StrokeTopologyTessellator::new();
        assert_eq!(
            tess.prepare::<true, false>(
                Polygon {
                    points: &points,
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::Arc)
        );
        assert_eq!(
            tess.computed_joins, 2,
            "first corner accepted, second refused"
        );
        assert!(
            !tess.positions.is_empty(),
            "prefix has already emitted bases"
        );
        for _ in 0..3 {
            assert_eq!(
                compare_mode(&mut tess, &points, 0.75, true),
                Some(StrokeTopologySource::Prepared)
            );
            assert_eq!(
                compare_mode(&mut tess, &regular_polygon(4), 0.75, true),
                Some(StrokeTopologySource::Layout)
            );
            assert!(matches!(
                tess.tessellate_single_pass(
                    Polygon {
                        points: &[point(0., 0.); 3],
                        closed: true
                    },
                    &options
                ),
                Err(StrokeTopologyReject::Merge)
            ));
        }
        assert_eq!(tess.stats.misses, 1);
        assert_eq!(tess.stats.hits, 2);
        assert!(tess.positions.capacity() <= MAX_PREPARED_VERTICES);
        assert!(tess.prepared_indices.capacity() <= MAX_PREPARED_INDICES);
    }

    #[test]
    fn selective_emission_preserves_geometry_and_skips_output_work() {
        fn check<const L: bool, const P: bool>() {
            let mut tess = StrokeTopologyTessellator::new();
            let shapes = [
                vec![point(0., 0.), point(1., 0.), point(1., 1.), point(0., 1.)],
                vec![point(0., 0.), point(3., 0.), point(0.2, 0.1)],
            ];
            let options = StrokeOptions::default()
                .with_line_width(0.75)
                .with_tolerance(0.25)
                .with_line_join(LineJoin::Round);
            for _ in 0..3 {
                for (i, points) in shapes.iter().enumerate() {
                    let polygon = Polygon {
                        points,
                        closed: true,
                    };
                    let before = tess.stats();
                    let old_indices = tess.prepared_indices.clone();
                    let output = tess
                        .tessellate_single_pass_control::<L, P>(polygon.clone(), &options)
                        .unwrap();
                    let source = [StrokeTopologySource::Layout, StrokeTopologySource::Prepared][i];
                    if [L, P][i] {
                        let StrokeTopologyOutput::Mesh(mesh) = output else {
                            panic!("emitter disabled")
                        };
                        assert_eq!(mesh.source, source);
                        let mut reference = VertexBuffers::<Point, u32>::new();
                        StrokeTessellator::new()
                            .tessellate_polygon(
                                polygon,
                                &options,
                                &mut BuffersBuilder::new(&mut reference, |v: StrokeVertex| {
                                    v.position()
                                }),
                            )
                            .unwrap();
                        assert_eq!(mesh.indices, reference.indices);
                        assert!(mesh
                            .positions
                            .iter()
                            .map(|p| [p.x.to_bits(), p.y.to_bits()])
                            .eq(reference
                                .vertices
                                .iter()
                                .map(|p| [p.x.to_bits(), p.y.to_bits()])));
                    } else {
                        assert!(matches!(output, StrokeTopologyOutput::Skipped(s) if s == source));
                        assert_eq!(tess.stats(), before, "no cache lookup/compilation on skip");
                        assert_eq!(
                            tess.prepared_indices, old_indices,
                            "no arc emission on skip"
                        );
                    }
                }
                assert!(matches!(
                    tess.tessellate_single_pass_control::<L, P>(
                        Polygon {
                            points: &[point(0., 0.); 3],
                            closed: true
                        },
                        &options
                    ),
                    Err(StrokeTopologyReject::Merge)
                ));
                assert!(matches!(
                    tess.tessellate_single_pass_control::<L, P>(
                        Polygon {
                            points: &shapes[0],
                            closed: true
                        },
                        &options.with_line_width(4.)
                    ),
                    Err(StrokeTopologyReject::Arc)
                ));
            }
            assert_eq!(tess.stats().entries, usize::from(L));
            assert_eq!(tess.stats().misses, u64::from(L));
            assert_eq!(tess.stats().hits, if L { 2 } else { 0 });
        }
        check::<true, true>();
        check::<true, false>();
        check::<false, true>();
        check::<false, false>();
    }

    #[test]
    fn prefix_reuse_matches_original_and_ordinary_across_changing_geometry() {
        let mut old = StrokeTopologyTessellator::new();
        let mut prefix = StrokeTopologyTessellator::new();
        let mut plain = StrokeTopologyTessellator::new();
        old.max_layouts = 2;
        prefix.max_layouts = 2;
        plain.max_layouts = 2;
        let mut random = 0x163791bd_u32;
        let mut next = || {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            (random >> 8) as f32 / (1u32 << 24) as f32
        };
        let mut first_arc = 0;
        let mut later_arc = 0;
        let mut closing_arc = 0;
        let mut folded = 0;
        let mut reused = 0;
        for sample in 0..3000 {
            let count = 3 + sample % 62;
            let radius = [0.15, 0.5, 1., 3., 150.][sample % 5];
            let offset = [0., 800.125, -15000.25][sample % 3];
            let mut points: Vec<_> = (0..count)
                .map(|i| {
                    let angle = (i as f32 + next()) * 2. * PI / count as f32;
                    let (s, c) = angle.sin_cos();
                    let r = radius * (0.1 + next() * 1.9);
                    point(c * r + offset, s * r + offset)
                })
                .collect();
            if sample % 4 == 0 {
                points = regular_polygon(4 + sample % 3);
            }
            if sample % 7 == 0 {
                points.reverse();
            }
            if sample % 11 == 0 {
                points[1] = points[0];
            }
            if sample % 13 == 0 {
                let opposite = points.len() / 2;
                points.swap(0, opposite);
            }
            let width = [0.002, 0.1, 0.75, 1., 2., 10., 2048.][sample % 7];
            for tolerance in [0.25, width * 0.25, width * 0.5] {
                let options = StrokeOptions::default()
                    .with_line_width(width)
                    .with_tolerance(tolerance)
                    .with_line_join(LineJoin::Round);
                for _ in 0..2 {
                    let polygon = Polygon {
                        points: &points,
                        closed: true,
                    };
                    match (
                        old.tessellate_single_pass(polygon.clone(), &options),
                        prefix.tessellate_single_pass_prefix_counted(polygon.clone(), &options),
                        plain.tessellate_single_pass_prefix(polygon.clone(), &options),
                    ) {
                        (Ok(a), Ok((b, count)), Ok(c)) => {
                            assert_eq!(a.source, b.source);
                            assert_eq!(b.source, c.source);
                            assert_eq!(a.indices, b.indices);
                            assert_eq!(b.indices, c.indices);
                            assert!(a
                                .positions
                                .iter()
                                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                                .eq(b.positions.iter().map(|p| [p.x.to_bits(), p.y.to_bits()])));
                            assert!(b
                                .positions
                                .iter()
                                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                                .eq(c.positions.iter().map(|p| [p.x.to_bits(), p.y.to_bits()])));
                            if b.source == StrokeTopologySource::Layout {
                                assert_eq!(count, 0);
                            }
                            reused += count;
                            let mut reference = VertexBuffers::<Point, u32>::new();
                            StrokeTessellator::new()
                                .tessellate_polygon(
                                    polygon,
                                    &options,
                                    &mut BuffersBuilder::new(&mut reference, |v: StrokeVertex| {
                                        v.position()
                                    }),
                                )
                                .unwrap();
                            assert_eq!(b.indices, reference.indices);
                            assert!(b
                                .positions
                                .iter()
                                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                                .eq(reference
                                    .vertices
                                    .iter()
                                    .map(|p| [p.x.to_bits(), p.y.to_bits()])));
                            if b.source == StrokeTopologySource::Prepared {
                                let ready = prefix.computed_joins;
                                first_arc += usize::from(ready == 1);
                                later_arc += usize::from(ready > 1);
                                closing_arc += usize::from(ready == points.len());
                                folded +=
                                    usize::from(prefix.joins.iter().any(|j| j.fold != [false; 2]));
                            }
                        }
                        (Err(a), Err(b), Err(c)) => {
                            assert_eq!(a, b);
                            assert_eq!(b, c);
                        }
                        _ => panic!("prefix acceptance differs sample={}", sample),
                    }
                    assert_eq!(old.stats(), prefix.stats());
                    assert_eq!(prefix.stats(), plain.stats());
                    assert!(prefix.positions.capacity() <= MAX_PREPARED_VERTICES);
                    assert!(prefix.prepared_indices.capacity() <= MAX_PREPARED_INDICES);
                }
            }
        }
        assert!(first_arc > 100 && later_arc > 100 && closing_arc > 0 && folded > 0);
        assert!(reused > 1000);
        assert!(prefix.stats().evictions > 0);
    }

    #[test]
    fn prefix_reuse_retains_exact_bases_and_rejects_invalid_state() {
        let points = [
            point(0., 0.),
            point(2., 0.),
            point(2., 2.),
            point(1., 1.),
            point(0., 2.),
        ];
        let options = StrokeOptions::default()
            .with_line_width(0.75)
            .with_tolerance(0.25)
            .with_line_join(LineJoin::Round);
        let mut tess = StrokeTopologyTessellator::new();
        let polygon = Polygon {
            points: &points,
            closed: true,
        };
        assert_eq!(
            tess.prepare::<true, true>(polygon.clone(), &options),
            Err(StrokeTopologyReject::Arc)
        );
        assert_eq!(tess.computed_joins, 2);
        let bases = tess.positions.clone();
        assert!(!bases.is_empty());
        let (mesh, reused) = tess
            .tessellate_single_pass_prefix_counted(polygon, &options)
            .unwrap();
        assert_eq!(reused, bases.len());
        assert!(mesh.positions[..bases.len()]
            .iter()
            .map(|p| [p.x.to_bits(), p.y.to_bits()])
            .eq(bases.iter().map(|p| [p.x.to_bits(), p.y.to_bits()])));
        let stats = tess.stats();
        for (points, closed, opts, reason) in [
            (
                &points[..],
                false,
                options,
                StrokeTopologyReject::Unsupported,
            ),
            (
                &points[..],
                true,
                options.with_line_width(4.),
                StrokeTopologyReject::Arc,
            ),
            (
                &[point(0., 0.); 3][..],
                true,
                options,
                StrokeTopologyReject::Merge,
            ),
            (
                &[point(f32::NAN, 0.), point(0., 0.), point(0., 0.)][..],
                true,
                options,
                StrokeTopologyReject::NonFinite,
            ),
        ] {
            assert!(
                matches!(tess.tessellate_single_pass_prefix(Polygon { points, closed }, &opts), Err(e) if e == reason)
            );
            assert!(
                matches!(tess.tessellate_single_pass_prefix_counted(Polygon { points, closed }, &opts), Err(e) if e == reason)
            );
            assert!(tess.positions.is_empty());
            assert_eq!(tess.computed_joins, 0);
            assert_eq!(tess.stats(), stats);
        }
        tess.max_layouts = 0;
        assert!(matches!(
            tess.tessellate_single_pass_prefix(
                Polygon {
                    points: &regular_polygon(4),
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::Budget)
        ));
    }

    #[test]
    fn single_pass_fine_arcs_and_unsafe_inputs_still_require_ordinary_fallback() {
        let mut tess = StrokeTopologyTessellator::new();
        let points = regular_polygon(3);
        let options = StrokeOptions::default()
            .with_line_width(0.75)
            .with_tolerance(0.01)
            .with_line_join(LineJoin::Round);
        assert!(matches!(
            tess.tessellate_single_pass(
                Polygon {
                    points: &points,
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::Arc)
        ));
        assert_eq!(tess.stats(), StrokeTopologyStats::default());
        assert!(tess.prepared_indices.is_empty());
        assert!(tess.joins.is_empty());
        assert!(tess.positions.is_empty());
        assert_eq!(tess.computed_joins, 0);
        assert!(matches!(
            tess.tessellate_single_pass(
                Polygon {
                    points: &points,
                    closed: false
                },
                &options
            ),
            Err(StrokeTopologyReject::Unsupported)
        ));
        assert!(matches!(
            tess.tessellate_single_pass(
                Polygon {
                    points: &[point(f32::INFINITY, 0.); 3],
                    closed: true
                },
                &options
            ),
            Err(StrokeTopologyReject::NonFinite)
        ));
    }

    #[test]
    fn early_fine_arc_refusal_matches_original_at_style_and_f32_boundaries() {
        let mut old = StrokeTopologyTessellator::new();
        let mut early = StrokeTopologyTessellator::new();
        let mut fine = 0;
        let mut accepted = 0;
        let mut merged = 0;
        let mut random = 0x714f092d_u32;
        let mut next = || {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            (random >> 8) as f32 / (1u32 << 24) as f32
        };
        // Current f32 geometry, not a regular-polygon/rectangle-only shortcut.
        for sample in 0..3000 {
            let count = 3 + sample % 62;
            let radius = [0.01, 0.15, 0.5, 1., 3., 150.][sample % 6];
            let offset = [0., 800.125, -15000.25][sample % 3];
            let mut points: Vec<_> = (0..count)
                .map(|i| {
                    let angle = (i as f32 + next()) * 2. * PI / count as f32;
                    let (s, c) = angle.sin_cos();
                    let r = radius * (0.1 + next() * 1.9);
                    point(c * r + offset, s * r + offset)
                })
                .collect();
            // Keep substantial known-eligible coverage alongside jagged/merged
            // cases, rather than relying on random corners to enter the cache.
            if sample % 4 == 0 {
                points = regular_polygon(4);
            }
            if sample % 7 == 0 {
                points.reverse();
            }
            if sample % 11 == 0 {
                points[1] = points[0];
            }
            if sample % 13 == 0 {
                // Include self-intersections and near-collinear corners.
                let opposite = points.len() / 2;
                points.swap(0, opposite);
            }
            let width = if sample % 4 == 0 {
                0.75
            } else {
                [0.002, 0.1, 0.75, 1., 2., 10., 2048.][sample % 7]
            };
            let bound = width * 0.25_f32;
            for tolerance in [
                bound * 0.1,
                f32::from_bits(bound.to_bits() - 1),
                bound,
                f32::from_bits(bound.to_bits() + 1),
                bound * 2.,
                0.25,
            ] {
                let options = StrokeOptions::default()
                    .with_line_width(width)
                    .with_tolerance(tolerance)
                    .with_line_join(LineJoin::Round);
                let polygon = Polygon {
                    points: &points,
                    closed: true,
                };
                let expected = old.prepare::<true, false>(polygon.clone(), &options);
                let actual = early.prepare::<true, true>(polygon, &options);
                assert_eq!(
                    actual, expected,
                    "sample={sample} width={width} tolerance={tolerance}"
                );
                if tolerance < bound && actual == Err(StrokeTopologyReject::Arc) {
                    fine += 1;
                    assert_eq!(old.computed_joins, 1);
                    assert_eq!(early.computed_joins, 0);
                    assert!(early.joins.is_empty());
                    assert!(early.positions.is_empty());
                } else {
                    assert_eq!(old.computed_joins, early.computed_joins);
                    assert_eq!(
                        old.positions
                            .iter()
                            .map(|p| [p.x.to_bits(), p.y.to_bits()])
                            .collect::<Vec<_>>(),
                        early
                            .positions
                            .iter()
                            .map(|p| [p.x.to_bits(), p.y.to_bits()])
                            .collect::<Vec<_>>()
                    );
                }
                accepted += usize::from(actual.is_ok());
                merged += usize::from(actual == Err(StrokeTopologyReject::Merge));
            }
        }
        assert!(fine > 1000, "only {} fine-arc cases", fine);
        assert!(accepted > 1000, "only {} eligible cases", accepted);
        assert!(merged > 1000, "only {} merged cases", merged);
    }

    #[test]
    fn early_fine_arc_refusal_preserves_priority_and_warm_cache() {
        let mut old = StrokeTopologyTessellator::new();
        let mut early = StrokeTopologyTessellator::new();
        let points = regular_polygon(4);
        assert!(compare(&mut early, &points, 0.75));
        let warm = early.stats();
        let options = StrokeOptions::default()
            .with_line_width(2.)
            .with_tolerance(0.25)
            .with_line_join(LineJoin::Round);
        let invalid = [point(f32::NAN, 0.), point(0., 0.), point(0., 0.)];
        let cases: &[(&[Point], bool, StrokeOptions, StrokeTopologyReject)] = &[
            (&points, true, options, StrokeTopologyReject::Arc),
            (&points, false, options, StrokeTopologyReject::Unsupported),
            (
                &points[..2],
                true,
                options,
                StrokeTopologyReject::Unsupported,
            ),
            (
                &points,
                true,
                options.with_line_join(LineJoin::Bevel),
                StrokeTopologyReject::Unsupported,
            ),
            (
                &points,
                true,
                options.with_line_width(f32::NAN),
                StrokeTopologyReject::Unsupported,
            ),
            (
                &points,
                true,
                options.with_tolerance(f32::NAN),
                StrokeTopologyReject::Unsupported,
            ),
            (&invalid, true, options, StrokeTopologyReject::NonFinite),
            (&invalid, false, options, StrokeTopologyReject::Unsupported),
            (
                &[point(0., 0.); 3],
                true,
                options,
                StrokeTopologyReject::Merge,
            ),
        ];
        for &(points, closed, options, expected) in cases {
            let polygon = Polygon { points, closed };
            assert_eq!(
                old.prepare::<true, false>(polygon.clone(), &options),
                Err(expected)
            );
            assert_eq!(
                early.prepare::<true, true>(polygon, &options),
                Err(expected)
            );
            assert_eq!(early.stats(), warm);
        }
        assert!(compare(&mut early, &points, 0.75));
        assert_eq!(early.stats().hits, warm.hits + 1);
        assert_eq!(early.stats().entries, warm.entries);
        assert_eq!(early.stats().bytes, warm.bytes);
    }

    #[test]
    fn single_pass_many_arcs_stay_bounded_and_do_not_require_cache_capacity() {
        let points: Vec<_> = (0..MAX_POINTS)
            .map(|i| {
                let (s, c) = (i as f32 * 2.0 * PI / MAX_POINTS as f32).sin_cos();
                let radius = if i % 2 == 0 { 15.0 } else { 3.0 };
                point(c * radius, s * radius)
            })
            .collect();
        let mut tess = StrokeTopologyTessellator::new();
        tess.max_layouts = 0;
        for _ in 0..3 {
            assert_eq!(
                compare_mode(&mut tess, &points, 0.75, true),
                Some(StrokeTopologySource::Prepared)
            );
            assert!(
                tess.positions.len() > MAX_POINTS * 3,
                "exercise arc vertices"
            );
            assert!(tess.positions.len() <= MAX_PREPARED_VERTICES);
            assert!(tess.positions.capacity() <= MAX_PREPARED_VERTICES);
            assert!(tess.prepared_indices.len() <= MAX_PREPARED_INDICES);
            assert!(tess.prepared_indices.capacity() <= MAX_PREPARED_INDICES);
        }
        assert_eq!(tess.stats(), StrokeTopologyStats::default());
    }
}
