//! Scale-independent polygon tessellation with fixed-pixel stroke extrusion.
//! Still expanded into the ordinary triangle stream: this experiment changes
//! CPU tessellation reuse, not GPU submission, ordering, or antialiasing.
use super::{
    encode_profile::{EncodeProfile, elapsed as profile_elapsed, started as profile_started},
    mesh::{MAX_INDICES, MAX_VERTICES, MeshBuilder},
    plan::Draw,
    retained::{Definition, Placement},
};
use engine_common::{RenderColor, RenderPolygon};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
    math::{Point, point},
    path::Polygon,
};
use std::{
    collections::BTreeMap,
    ops::Range,
    time::{Duration, Instant},
};

// Local position.xy, fixed-pixel offset.xy, premultiplied rgba.
type Vertex = [f32; 8];

#[derive(Clone, PartialEq)]
struct Part {
    fill: Piece,
    stroke: Piece,
}

#[derive(Clone, PartialEq)]
struct Piece {
    vertices: Range<usize>,
    indices: Range<usize>,
    minimum_scale: f32,
}

#[derive(Default, Debug)]
pub(super) struct Counts {
    pub reused: u64,
    pub fallback: u64,
    pub fills: u64,
    pub strokes: u64,
    pub culled: u64,
    pub culled_indices: u64,
    pub stroke_bound_tests: u64,
    pub stroke_bound_unknown: u64,
    pub stroke_bound_rejections: u64,
}

pub(crate) struct Mesh {
    buffers: VertexBuffers<Vertex, u32>,
    parts: Vec<Part>,
    visibility: Vec<[super::visibility::Bounds; 2]>,
    pub build_reuses: u64,
}

// Construction-only metadata; no additional retained vertices or GPU draws.
// Definition already limits each key to 256 points and each build to 4096 parts.
const MAX_BUILD_KEYS: usize = 128;

fn build_key(points: &[Point], scale: f32, polygon: &RenderPolygon, key: &mut Vec<u32>) {
    key.clear();
    key.extend([scale.to_bits(), points.len() as u32]);
    for p in points {
        key.extend([p.x.to_bits(), p.y.to_bits()]);
    }
    let mut color = |c: Option<RenderColor>| {
        key.push(u32::from(c.is_some()));
        if let Some(c) = c {
            key.extend([c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]);
        }
    };
    color(polygon.fill.map(|f| f.color));
    color(polygon.stroke.map(|s| s.color));
    if let Some(stroke) = polygon.stroke {
        key.push(stroke.width.to_bits());
    }
}

impl Mesh {
    fn copy_part(&mut self, old: &Part, fill_scale: f32, stroke_scale: f32) -> Result<(), String> {
        let vertices = self.buffers.vertices.len();
        let indices = self.buffers.indices.len();
        let old_vertices = old.fill.vertices.start..old.stroke.vertices.end;
        let old_indices = old.fill.indices.start..old.stroke.indices.end;
        if vertices + old_vertices.len() > MAX_VERTICES || indices + old_indices.len() > MAX_INDICES
        {
            return Err("scalable template exceeds mesh budget".into());
        }
        self.buffers
            .vertices
            .extend_from_within(old_vertices.clone());
        for i in old_indices {
            self.buffers
                .indices
                .push(self.buffers.indices[i] - old_vertices.start as u32 + vertices as u32);
        }
        let stroke_vertices = vertices + old.fill.vertices.len();
        let stroke_indices = indices + old.fill.indices.len();
        self.parts.push(Part {
            fill: Piece {
                vertices: vertices..stroke_vertices,
                indices: indices..stroke_indices,
                minimum_scale: fill_scale,
            },
            stroke: Piece {
                vertices: stroke_vertices..self.buffers.vertices.len(),
                indices: stroke_indices..self.buffers.indices.len(),
                minimum_scale: stroke_scale,
            },
        });
        self.build_reuses += 1;
        Ok(())
    }
}

impl Mesh {
    pub(super) fn vertices(&self) -> &[Vertex] {
        &self.buffers.vertices
    }

    /// Same per-piece safety gates and painter order as CPU expansion, but
    /// append atlas references instead of transforming/copying cached vertices.
    pub(super) fn append_resident(
        &self,
        placement: &Placement,
        draws: &[Draw],
        output: &mut MeshBuilder,
        frame: &mut super::resident::Frame,
        location: super::resident::Location,
    ) -> Result<Counts, String> {
        let mut counts = Counts::default();
        let mut pending: Option<Piece> = None;
        let flush = |piece: &Piece, frame: &mut super::resident::Frame, output: &MeshBuilder| {
            frame.cached(
                location,
                piece.vertices.clone(),
                &self.buffers.indices[piece.indices.clone()],
                output,
            )
        };
        for (part, draw) in self.parts.iter().zip(draws) {
            let mut fallback = false;
            for (piece, fill) in [(&part.fill, true), (&part.stroke, false)] {
                if (fill && draw.fill.is_none()) || (!fill && draw.stroke.is_none()) {
                    continue;
                }
                if placement.scale < piece.minimum_scale {
                    if let Some(range) = pending.take() {
                        flush(&range, frame, output)?;
                    }
                    output.append_draw_with_styles(
                        draw,
                        if fill { draw.fill } else { None },
                        if fill { None } else { draw.stroke },
                    )?;
                    fallback = true;
                    continue;
                }
                if let Some(range) = &mut pending {
                    if range.vertices.end == piece.vertices.start
                        && range.indices.end == piece.indices.start
                    {
                        range.vertices.end = piece.vertices.end;
                        range.indices.end = piece.indices.end;
                    } else {
                        flush(range, frame, output)?;
                        *range = piece.clone();
                    }
                } else {
                    pending = Some(piece.clone());
                }
                counts.fills += u64::from(fill);
                counts.strokes += u64::from(!fill);
            }
            counts.reused += u64::from(!fallback);
            counts.fallback += u64::from(fallback);
        }
        if let Some(range) = pending {
            flush(&range, frame, output)?;
        }
        Ok(counts)
    }

    pub fn identical(&self, other: &Self) -> bool {
        self.buffers.vertices == other.buffers.vertices
            && self.buffers.indices == other.buffers.indices
            && self.parts == other.parts
    }
    pub fn bytes(&self) -> usize {
        self.buffers.vertices.capacity() * std::mem::size_of::<Vertex>()
            + self.buffers.indices.capacity() * 4
            + self.parts.capacity() * std::mem::size_of::<Part>()
            + self.visibility.capacity() * std::mem::size_of::<[super::visibility::Bounds; 2]>()
    }

    pub fn enable_culling(&mut self) {
        self.visibility = self
            .parts
            .iter()
            .map(|part| {
                [&part.fill, &part.stroke].map(|piece| {
                    super::visibility::Bounds::new(
                        self.buffers.vertices[piece.vertices.clone()]
                            .iter()
                            .map(|v| [v[0], v[1], v[2], v[3]]),
                    )
                })
            })
            .collect();
    }

    /// Returns reused/fallback polygon counts. Unsupported and too-small parts
    /// use their authoritative projected Draw, interleaved in source order.
    pub(super) fn append(
        &self,
        placement: &Placement,
        draws: &[Draw],
        output: &mut MeshBuilder,
    ) -> Result<Counts, String> {
        self.append_inner::<false, false>(placement, draws, output, &mut EncodeProfile::default())
    }

    pub(super) fn append_inner<const PROFILE: bool, const RANGES: bool>(
        &self,
        placement: &Placement,
        draws: &[Draw],
        output: &mut MeshBuilder,
        profile: &mut EncodeProfile,
    ) -> Result<Counts, String> {
        self.append_clipped::<PROFILE, RANGES>(placement, draws, output, profile, None)
    }

    pub(super) fn append_clipped<const PROFILE: bool, const RANGES: bool>(
        &self,
        placement: &Placement,
        draws: &[Draw],
        output: &mut MeshBuilder,
        profile: &mut EncodeProfile,
        clip: Option<super::presentation::Clip>,
    ) -> Result<Counts, String> {
        let (s, c) = placement.angle.sin_cos();
        let mut counts = Counts::default();
        let mut pending: Option<Piece> = None;
        for (part_index, (part, draw)) in self.parts.iter().zip(draws).enumerate() {
            let mut fallback = false;
            for (piece, fill) in [(&part.fill, true), (&part.stroke, false)] {
                if (fill && draw.fill.is_none()) || (!fill && draw.stroke.is_none()) {
                    continue;
                }
                if placement.scale < piece.minimum_scale {
                    if let Some(range) = pending.take() {
                        self.expand::<PROFILE>(&range, placement, (s, c), output, profile)?;
                    }
                    if !fill
                        && output.stroke_culling != super::stroke_culling::Control::Off
                        && let Some(clip) = clip
                    {
                        counts.stroke_bound_tests += 1;
                        if let Some(bounds) = super::stroke_culling::Bounds::for_draw(draw) {
                            if bounds.outside(clip) {
                                if output.stroke_culling == super::stroke_culling::Control::Checked
                                {
                                    let vertices = output.buffers.vertices.len();
                                    let indices = output.buffers.indices.len();
                                    output.append_draw_with_styles(draw, None, draw.stroke)?;
                                    if !super::stroke_culling::vertices_outside(
                                        &output.buffers.vertices[vertices..],
                                        clip,
                                    ) {
                                        return Err(format!(
                                            "stroke rejection hid visible geometry: {draw:?}"
                                        ));
                                    }
                                    output.buffers.vertices.truncate(vertices);
                                    output.buffers.indices.truncate(indices);
                                }
                                counts.stroke_bound_rejections += 1;
                                // Preserve the reuse/fallback decision and painter-order boundary.
                                fallback = true;
                                continue;
                            }
                        } else {
                            counts.stroke_bound_unknown += 1;
                        }
                    }
                    let start = profile_started::<PROFILE>();
                    let vertex_start = output.buffers.vertices.len();
                    let index_start = output.buffers.indices.len();
                    let mut stroke_profile = super::encode_profile::DrawProfile::default();
                    if PROFILE && !fill {
                        output.append_draw_profiled::<true>(
                            draw,
                            None,
                            draw.stroke,
                            &mut stroke_profile,
                        )?;
                    } else {
                        output.append_draw_with_styles(
                            draw,
                            if fill { draw.fill } else { None },
                            if fill { None } else { draw.stroke },
                        )?;
                    }
                    if PROFILE {
                        if fill {
                            profile.fallback_fill += profile_elapsed(start);
                            profile.fallback_fills += 1;
                        } else {
                            let duration = profile_elapsed(start);
                            profile.fallback_stroke += duration;
                            profile.fallback_strokes += 1;
                            if let Some(clip) = clip {
                                // Diagnostic only: inspect actual generated vertices,
                                // not a proposed early-culling bound. Timed separately.
                                let stamp = profile_started::<true>();
                                let vertices = &output.buffers.vertices[vertex_start..];
                                let outside = !vertices.is_empty()
                                    && (vertices.iter().all(|v| v[0] < clip.x as f32)
                                        || vertices
                                            .iter()
                                            .all(|v| v[0] > (clip.x + clip.width) as f32)
                                        || vertices.iter().all(|v| v[1] < clip.y as f32)
                                        || vertices
                                            .iter()
                                            .all(|v| v[1] > (clip.y + clip.height) as f32));
                                if outside {
                                    profile.offscreen_strokes += 1;
                                    profile.offscreen_stroke += duration;
                                }
                                profile.stroke_visibility += profile_elapsed(stamp);
                            }
                            if let Some(census) = &mut output.stroke_census {
                                census.record(
                                    draw,
                                    &output.buffers.vertices[vertex_start..],
                                    clip,
                                    &stroke_profile,
                                );
                                if let Some(capture) = &mut census.capture {
                                    capture.record(
                                        draw,
                                        &output.buffers.vertices[vertex_start..],
                                        &output.buffers.indices[index_start..],
                                        vertex_start,
                                    );
                                }
                            }
                            profile.guarded_stroke.accumulate(stroke_profile);
                        }
                    }
                    fallback = true;
                    continue;
                }
                // The stock control always tessellates guarded pieces above.
                // Safe reusable pieces use bounds of actual cached vertices here.
                if let Some(clip) = clip
                    && self.visibility[part_index][usize::from(!fill)].outside(
                        placement.scale,
                        (s, c),
                        placement,
                        clip,
                    )
                {
                    if let Some(range) = pending.take() {
                        self.expand::<PROFILE>(&range, placement, (s, c), output, profile)?;
                    }
                    counts.culled += 1;
                    counts.culled_indices += piece.indices.len() as u64;
                    continue;
                }
                if RANGES {
                    // Coalesce only adjacent source ranges, never reorder or
                    // bridge a skipped/guarded piece. The scale safety decision
                    // above remains per piece; the pending guard is not reused.
                    if let Some(range) = &mut pending {
                        if range.vertices.end == piece.vertices.start
                            && range.indices.end == piece.indices.start
                        {
                            range.vertices.end = piece.vertices.end;
                            range.indices.end = piece.indices.end;
                        } else {
                            self.expand::<PROFILE>(range, placement, (s, c), output, profile)?;
                            *range = piece.clone();
                        }
                    } else {
                        pending = Some(piece.clone());
                    }
                } else {
                    self.expand::<PROFILE>(piece, placement, (s, c), output, profile)?;
                }
                counts.fills += u64::from(fill);
                counts.strokes += u64::from(!fill);
            }
            counts.reused += u64::from(!fallback);
            counts.fallback += u64::from(fallback);
        }
        if let Some(range) = pending {
            self.expand::<PROFILE>(&range, placement, (s, c), output, profile)?;
        }
        Ok(counts)
    }

    fn expand<const PROFILE: bool>(
        &self,
        piece: &Piece,
        placement: &Placement,
        (s, c): (f32, f32),
        output: &mut MeshBuilder,
        profile: &mut EncodeProfile,
    ) -> Result<(), String> {
        if output.buffers.vertices.len() + piece.vertices.len() > MAX_VERTICES
            || output.buffers.indices.len() + piece.indices.len() > MAX_INDICES
        {
            return Err("scalable expansion exceeds frame budget".into());
        }
        let start = profile_started::<PROFILE>();
        let base = output.buffers.vertices.len() as u32;
        output
            .buffers
            .vertices
            .extend(
                self.buffers.vertices[piece.vertices.clone()]
                    .iter()
                    .map(|v| {
                        let x = v[0] * placement.scale + v[2];
                        let y = v[1] * placement.scale + v[3];
                        [
                            c * x + s * y + placement.translation.x,
                            -s * x + c * y + placement.translation.y,
                            v[4],
                            v[5],
                            v[6],
                            v[7],
                        ]
                    }),
            );
        output.buffers.indices.extend(
            self.buffers.indices[piece.indices.clone()]
                .iter()
                .map(|i| base + i - piece.vertices.start as u32),
        );
        if PROFILE {
            profile.scalable_expand += profile_elapsed(start);
            profile.scalable_vertices += piece.vertices.len() as u64;
            profile.scalable_ranges += 1;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Builder {
    fill: FillTessellator,
    stroke: StrokeTessellator,
    points: Vec<Point>,
}

/// Offscreen-only detailed instrumentation. Normal builds monomorphize all
/// per-polygon clocks away; their existing per-admission timer is unchanged.
#[derive(Default)]
pub(crate) struct BuildProfile {
    pub domain: Duration,
    pub prepare: Duration,
    pub fill: Duration,
    pub stroke: Duration,
    pub finish: Duration,
    pub reuse: Duration,
}
impl BuildProfile {
    pub fn accumulate(&mut self, other: Self) {
        self.domain += other.domain;
        self.prepare += other.prepare;
        self.fill += other.fill;
        self.stroke += other.stroke;
        self.finish += other.finish;
        self.reuse += other.reuse;
    }
}
fn started<const PROFILE: bool>() -> Option<Instant> {
    PROFILE.then(Instant::now)
}
fn elapsed(start: Option<Instant>) -> Duration {
    start.map_or(Duration::ZERO, |s| s.elapsed())
}

impl Builder {
    pub fn build(
        &mut self,
        definition: &Definition,
        precise_outlines: bool,
    ) -> Result<Mesh, String> {
        self.build_inner::<false, false>(definition, precise_outlines)
            .map(|r| r.0)
    }

    pub fn build_memoized(
        &mut self,
        definition: &Definition,
        precise_outlines: bool,
    ) -> Result<Mesh, String> {
        self.build_inner::<false, true>(definition, precise_outlines)
            .map(|r| r.0)
    }

    pub fn build_profiled(
        &mut self,
        definition: &Definition,
        precise_outlines: bool,
        memoized: bool,
    ) -> Result<(Mesh, BuildProfile), String> {
        if memoized {
            self.build_inner::<true, true>(definition, precise_outlines)
        } else {
            self.build_inner::<true, false>(definition, precise_outlines)
        }
    }

    fn build_inner<const PROFILE: bool, const MEMO: bool>(
        &mut self,
        definition: &Definition,
        precise_outlines: bool,
    ) -> Result<(Mesh, BuildProfile), String> {
        let mut profile = BuildProfile::default();
        let mut mesh = Mesh {
            buffers: VertexBuffers::new(),
            parts: Vec::with_capacity(definition.len()),
            visibility: Vec::new(),
            build_reuses: 0,
        };
        let mut memo = BTreeMap::<Vec<u32>, Part>::new();
        let mut key = Vec::new();
        for polygon in definition.polygons() {
            let vertices = mesh.buffers.vertices.len();
            let indices = mesh.buffers.indices.len();
            let start = started::<PROFILE>();
            let bounds = domain_bounds(polygon);
            profile.domain += elapsed(start);
            let start = started::<PROFILE>();
            let mut fill_scale = bounds.as_ref().map_or(f32::INFINITY, |b| b.fill);
            let mut stroke_scale = bounds.as_ref().map_or(f32::INFINITY, |b| {
                if precise_outlines {
                    b.precise_stroke
                } else {
                    b.legacy_stroke
                }
            });
            // Keep canonical construction identical to the previous mode. Only
            // the proven domain in which these vertices are reused changes.
            let scale = bounds
                .as_ref()
                .map_or(f32::INFINITY, |b| b.legacy_stroke)
                .max(fill_scale)
                .max(1.0)
                * 2.0;
            let origin = polygon.points[0];
            self.points.clear();
            self.points.extend(
                polygon
                    .points
                    .iter()
                    .map(|p| point((p.x - origin.x) * scale, -(p.y - origin.y) * scale)),
            );
            // Avoid cancellation of tiny pixel offsets at huge reference
            // coordinates. Rejection is harmless: the ordinary path still runs.
            let rejected = !scale.is_finite()
                || self.points.iter().any(|p| {
                    !p.x.is_finite() || !p.y.is_finite() || p.x.abs().max(p.y.abs()) > 16_384.0
                });
            profile.prepare += elapsed(start);
            if rejected {
                fill_scale = f32::INFINITY;
                stroke_scale = f32::INFINITY;
                mesh.parts.push(Part {
                    fill: Piece {
                        vertices: vertices..vertices,
                        indices: indices..indices,
                        minimum_scale: fill_scale,
                    },
                    stroke: Piece {
                        vertices: vertices..vertices,
                        indices: indices..indices,
                        minimum_scale: stroke_scale,
                    },
                });
            } else {
                if MEMO {
                    let start = started::<PROFILE>();
                    // Exact canonical Lyon inputs, styles and scale. Guards
                    // above are still recomputed from EACH original polygon.
                    build_key(&self.points, scale, polygon, &mut key);
                    if let Some(old) = memo.get(&key) {
                        mesh.copy_part(old, fill_scale, stroke_scale)?;
                        profile.reuse += elapsed(start);
                        continue;
                    }
                    profile.reuse += elapsed(start);
                }
                let local = |p: Point, offset: [f32; 2], color: RenderColor| {
                    [
                        if MEMO {
                            p.x / scale
                        } else {
                            p.x / scale + origin.x
                        },
                        if MEMO {
                            p.y / scale
                        } else {
                            p.y / scale - origin.y
                        },
                        offset[0],
                        offset[1],
                        color.r * color.a,
                        color.g * color.a,
                        color.b * color.a,
                        color.a,
                    ]
                };
                if let Some(fill) = polygon.fill {
                    let start = started::<PROFILE>();
                    self.fill
                        .tessellate_polygon(
                            Polygon {
                                points: &self.points,
                                closed: true,
                            },
                            &FillOptions::default()
                                .with_fill_rule(FillRule::NonZero)
                                .with_tolerance(0.25),
                            &mut BuffersBuilder::new(&mut mesh.buffers, |v: FillVertex<'_>| {
                                local(v.position(), [0., 0.], fill.color)
                            }),
                        )
                        .map_err(|e| format!("scalable fill tessellation: {e:?}"))?;
                    profile.fill += elapsed(start);
                }
                let stroke_vertices = mesh.buffers.vertices.len();
                let stroke_indices = mesh.buffers.indices.len();
                if let Some(stroke) = polygon.stroke {
                    let start = started::<PROFILE>();
                    self.stroke
                        .tessellate_polygon(
                            Polygon {
                                points: &self.points,
                                closed: true,
                            },
                            &StrokeOptions::default()
                                .with_tolerance(0.25)
                                .with_line_width(stroke.width)
                                .with_line_cap(LineCap::Round)
                                .with_line_join(LineJoin::Round),
                            &mut BuffersBuilder::new(
                                &mut mesh.buffers,
                                |v: StrokeVertex<'_, '_>| {
                                    let n = v.normal() * (v.line_width() * 0.5);
                                    local(v.position_on_path(), [n.x, n.y], stroke.color)
                                },
                            ),
                        )
                        .map_err(|e| format!("scalable stroke tessellation: {e:?}"))?;
                    profile.stroke += elapsed(start);
                }
                mesh.parts.push(Part {
                    fill: Piece {
                        vertices: vertices..stroke_vertices,
                        indices: indices..stroke_indices,
                        minimum_scale: fill_scale,
                    },
                    stroke: Piece {
                        vertices: stroke_vertices..mesh.buffers.vertices.len(),
                        indices: stroke_indices..mesh.buffers.indices.len(),
                        minimum_scale: stroke_scale,
                    },
                });
                if MEMO && memo.len() < MAX_BUILD_KEYS {
                    let start = started::<PROFILE>();
                    memo.insert(key.clone(), mesh.parts.last().unwrap().clone());
                    profile.reuse += elapsed(start);
                }
            }
            if mesh.buffers.vertices.len() > MAX_VERTICES
                || mesh.buffers.indices.len() > MAX_INDICES
            {
                return Err("scalable template exceeds mesh budget".into());
            }
        }
        let start = started::<PROFILE>();
        if MEMO {
            // Keep canonical vertices relative until all reuse is finished.
            // Adding the origin exactly once preserves the baseline arithmetic;
            // subtracting it from already-rounded vertices would not do so.
            for (part, polygon) in mesh.parts.iter().zip(definition.polygons()) {
                let origin = polygon.points[0];
                for v in
                    &mut mesh.buffers.vertices[part.fill.vertices.start..part.stroke.vertices.end]
                {
                    v[0] += origin.x;
                    v[1] -= origin.y;
                }
            }
        }
        if mesh
            .buffers
            .vertices
            .iter()
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err("scalable tessellation produced non-finite geometry".into());
        }
        profile.finish += elapsed(start);
        Ok((mesh, profile))
    }
}

/// Conservative domain for reuse: simple straight-edge polygons, with no
/// duplicate/backtracking edges. Lyon merges subpixel edges and folds sharp
/// joins when the stroke is wider than neighboring edges; neither operation is
/// invariant under zoom. Stay comfortably above BOTH thresholds, per polygon.
/// Curves, self-intersections and unsafe scales keep ordinary tessellation.
#[cfg(test)]
fn domains(polygon: &RenderPolygon) -> Option<(f32, f32)> {
    let bounds = domain_bounds(polygon)?;
    Some((bounds.fill, bounds.legacy_stroke))
}

/// Used by the offscreen lab to sample either side of both guards, without
/// changing source artwork or quantizing the camera zoom.
pub(crate) fn stroke_domains(polygon: &RenderPolygon) -> Option<(f32, f32)> {
    let b = domain_bounds(polygon)?;
    Some((b.legacy_stroke, b.precise_stroke))
}

struct Bounds {
    fill: f32,
    legacy_stroke: f32,
    #[cfg(test)]
    merge: f32,
    precise_stroke: f32,
}

fn domain_bounds(polygon: &RenderPolygon) -> Option<Bounds> {
    let points: Vec<_> = polygon
        .points
        .iter()
        .map(|p| [f64::from(p.x), f64::from(p.y)])
        .collect();
    let n = points.len();
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    // Non-adjacent intersections or touches invalidate the simple-polygon
    // assumption. This bounded O(n²) check runs once per immutable definition.
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        for j in i + 2..n {
            if (j + 1) % n == i {
                continue;
            }
            let c = points[j];
            let d = points[(j + 1) % n];
            let overlaps = (0..2)
                .all(|k| a[k].min(b[k]) <= c[k].max(d[k]) && c[k].min(d[k]) <= a[k].max(b[k]));
            if overlaps
                && cross(a, b, c) * cross(a, b, d) <= 0.0
                && cross(c, d, a) * cross(c, d, b) <= 0.0
            {
                return None;
            }
        }
    }
    let width = polygon.stroke.map_or(0., |s| f64::from(s.width));
    // From Lyon's fixed-width square_merge_threshold, with 2x margin.
    let merge = (0.25_f64.powi(2) * 0.5)
        .min(width * width * 0.05)
        .max(1e-8)
        .sqrt();
    let mut minimum = 0.0_f64;
    let mut fill_minimum = 0.0_f64;
    let mut merge_minimum = 0.0_f64;
    let mut fold_minimum = 0.0_f64;
    for i in 0..n {
        let a = points[(i + n - 1) % n];
        let b = points[i];
        let c = points[(i + 1) % n];
        let u = [b[0] - a[0], b[1] - a[1]];
        let v = [c[0] - b[0], c[1] - b[1]];
        let lu = u[0].hypot(u[1]);
        let lv = v[0].hypot(v[1]);
        if lu == 0.0 || lv == 0.0 {
            return None;
        }
        let dot = ((u[0] * v[0] + u[1] * v[1]) / (lu * lv)).clamp(-1., 1.);
        if dot < -0.9999 {
            return None;
        }
        let extrusion = width * 0.5 * (2. / (1. + dot)).sqrt();
        minimum = minimum.max(2. * merge.max(extrusion) / lu.min(lv));
        merge_minimum = merge_minimum.max(2. * merge / lu.min(lv));
        // Pinned Lyon 1.0.20 fixed-width round joins fold only at sharp turns,
        // when the back-vertex projection extends beyond BOTH adjacent edges.
        // Its reach is half_width * tan(turn / 2), not the full miter length.
        // Include near-right-angle turns to guard projected f32 sign changes,
        // and retain the original 2x margin around the scale transition.
        if dot < 1e-4 {
            let reach = width * 0.5 * ((1. - dot) / (1. + dot)).sqrt();
            fold_minimum = fold_minimum.max(2. * reach / lu.max(lv));
        }
        // Straight-edge fills have no width-dependent folding/flattening.
        // Still reject almost coincident projected points.
        fill_minimum = fill_minimum.max(2e-4 / lu.min(lv));
    }
    let minimum = minimum as f32;
    let fill_minimum = fill_minimum as f32;
    (minimum.is_finite() && fill_minimum.is_finite()).then_some(Bounds {
        fill: fill_minimum.next_up(),
        legacy_stroke: minimum.next_up(),
        #[cfg(test)]
        merge: (merge_minimum as f32).next_up(),
        precise_stroke: (merge_minimum.max(fold_minimum) as f32).next_up(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Fill, RenderPoint, Stroke};

    fn polygon(points: &[(f32, f32)], width: f32) -> RenderPolygon {
        RenderPolygon {
            points: points
                .iter()
                .map(|&(x, y)| RenderPoint::new(x, y))
                .collect(),
            fill: Some(Fill::new(RenderColor::RED)),
            stroke: Some(Stroke::new(RenderColor::WHITE, width)),
        }
    }

    #[test]
    fn detailed_profiling_preserves_vertices_indices_and_guard_domains() {
        let definition = Definition::new(vec![
            polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 1.),
            polygon(&[(0., 0.), (4., 0.), (4., 1.), (0., 1.)], 12.),
            polygon(&[(0., 0.), (3., 0.), (3., 0.), (0., 3.)], 1.),
            polygon(&[(0., 0.), (3., 3.), (0., 3.), (3., 0.)], 1.),
        ])
        .unwrap();
        let mut builder = Builder::default();
        for precise in [false, true] {
            let (plain, disabled) = builder
                .build_inner::<false, false>(&definition, precise)
                .unwrap();
            let (profiled, profile) = builder.build_profiled(&definition, precise, false).unwrap();
            assert!(plain.identical(&profiled));
            assert_eq!(plain.bytes(), profiled.bytes());
            assert_eq!(
                disabled.domain
                    + disabled.prepare
                    + disabled.fill
                    + disabled.stroke
                    + disabled.finish
                    + disabled.reuse,
                Duration::ZERO
            );
            let domain = profile.domain;
            let mut accumulated = BuildProfile::default();
            accumulated.accumulate(profile);
            assert_eq!(accumulated.domain, domain);
        }
    }

    #[test]
    fn guarded_stroke_attribution_preserves_clipped_geometry_and_counts_only_new_output() {
        use super::super::{plan::Shape, presentation::Clip};
        use std::sync::Arc;

        // One visible stroke, one beyond each edge, and one whose outline is
        // outside but whose thick stroke overlaps the left edge of the clip.
        let polygons = [
            (40., 40.),
            (-20., 40.),
            (120., 40.),
            (40., -20.),
            (40., 120.),
            (-3., 40.),
        ]
        .map(|(x, y)| {
            let mut p = polygon(&[(x, y), (x + 2., y), (x + 2., y + 2.), (x, y + 2.)], 4.);
            p.fill = None;
            p
        });
        let definition = Arc::new(Definition::new(polygons.to_vec()).unwrap());
        let placement = Placement {
            first: 0,
            definition: definition.clone(),
            scale: 1.,
            angle: 0.,
            translation: RenderPoint::ZERO,
        };
        let draws: Vec<_> = polygons
            .iter()
            .map(|p| Draw {
                shape: Shape::Polygon(p.points.clone()),
                fill: None,
                stroke: p.stroke,
            })
            .collect();
        let mut template = Builder::default().build(&definition, true).unwrap();
        // Exercise diagnostics independently of the exact safety threshold.
        for part in &mut template.parts {
            part.stroke.minimum_scale = f32::INFINITY;
        }
        let clip = Clip {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        for clip in [None, Some(clip)] {
            let (mut plain, mut detailed) = (MeshBuilder::default(), MeshBuilder::default());
            plain.stroke_census = Some(Box::default());
            detailed.stroke_census = Some(Box::default());
            // A visible prefix must not make an offscreen stroke look visible,
            // nor be included in its point/vertex/index totals.
            plain.append_draw(&draws[0]).unwrap();
            detailed.append_draw(&draws[0]).unwrap();
            let prefix_vertices = plain.buffers.vertices.len();
            let prefix_indices = plain.buffers.indices.len();
            let (mut disabled, mut profile) = (EncodeProfile::default(), EncodeProfile::default());
            let a = template
                .append_clipped::<false, true>(&placement, &draws, &mut plain, &mut disabled, clip)
                .unwrap();
            let b = template
                .append_clipped::<true, true>(&placement, &draws, &mut detailed, &mut profile, clip)
                .unwrap();
            assert_eq!(disabled, EncodeProfile::default());
            assert!(
                plain
                    .buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .eq(detailed
                        .buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits()))
            );
            assert_eq!(plain.buffers.indices, detailed.buffers.indices);
            assert_eq!(
                (a.reused, a.fallback, a.culled),
                (b.reused, b.fallback, b.culled)
            );
            assert_eq!(profile.fallback_strokes, 6);
            let mut plain_csv = Vec::new();
            plain
                .stroke_census
                .as_ref()
                .unwrap()
                .write_csv(&mut plain_csv, "test", "plain", 1)
                .unwrap();
            assert!(
                plain_csv.is_empty(),
                "const-disabled path must not collect diagnostics"
            );
            let mut detailed_csv = Vec::new();
            detailed
                .stroke_census
                .as_ref()
                .unwrap()
                .write_csv(&mut detailed_csv, "test", "detailed", 1)
                .unwrap();
            let detailed_csv = String::from_utf8(detailed_csv).unwrap();
            assert_eq!(
                detailed_csv
                    .lines()
                    .map(|line| line.split(',').nth(6).unwrap().parse::<u64>().unwrap())
                    .sum::<u64>(),
                6
            );
            assert_eq!(profile.guarded_stroke.points, 24);
            assert_eq!(
                profile.guarded_stroke.vertices as usize,
                plain.buffers.vertices.len() - prefix_vertices
            );
            assert_eq!(
                profile.guarded_stroke.indices as usize,
                plain.buffers.indices.len() - prefix_indices
            );
            assert_eq!(profile.guarded_stroke.empty, 0);
            assert_eq!(
                profile.offscreen_strokes,
                if clip.is_some() { 4 } else { 0 }
            );
            if clip.is_none() {
                assert_eq!(profile.stroke_visibility, Duration::ZERO);
                assert_eq!(profile.offscreen_stroke, Duration::ZERO);
            }
        }
    }

    #[test]
    fn guarded_stroke_rejection_matches_checked_output_and_reveals_on_clip_changes() {
        use super::super::{plan::Shape, presentation::Clip, stroke_culling::Control};
        use std::sync::Arc;
        let polygons = [
            (40., 40.),
            (-200., 40.),
            (300., 40.),
            (40., -200.),
            (40., 300.),
            (-3., 40.),
        ]
        .map(|(x, y)| {
            let mut p = polygon(&[(x, y), (x + 2., y), (x + 2., y + 2.), (x, y + 2.)], 12.);
            p.fill = None;
            p
        });
        let definition = Arc::new(Definition::new(polygons.to_vec()).unwrap());
        let placement = Placement {
            first: 0,
            definition: definition.clone(),
            scale: 1.,
            angle: 0.,
            translation: RenderPoint::ZERO,
        };
        let draws: Vec<_> = polygons
            .iter()
            .map(|p| Draw {
                shape: Shape::Polygon(p.points.clone()),
                fill: None,
                stroke: p.stroke,
            })
            .collect();
        let mut template = Builder::default().build(&definition, true).unwrap();
        for part in &mut template.parts {
            part.stroke.minimum_scale = f32::INFINITY;
        }
        let clip = Clip {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        for clip in [Some(clip), None, Some(Clip { x: -250, ..clip }), Some(clip)] {
            let mut reference = None;
            for control in [Control::Conservative, Control::Checked] {
                let mut out = MeshBuilder::default();
                out.stroke_culling = control;
                out.append_draw(&draws[0]).unwrap();
                let prefix = out.buffers.vertices.clone();
                let counts = template
                    .append_clipped::<false, true>(
                        &placement,
                        &draws,
                        &mut out,
                        &mut EncodeProfile::default(),
                        clip,
                    )
                    .unwrap();
                assert_eq!(&out.buffers.vertices[..prefix.len()], &prefix);
                assert_eq!(counts.fallback, 6);
                assert_eq!(
                    counts.stroke_bound_tests,
                    if clip.is_some() { 6 } else { 0 }
                );
                assert_eq!(counts.stroke_bound_unknown, 0);
                assert_eq!(
                    counts.stroke_bound_rejections,
                    match clip {
                        None => 0,
                        Some(c) if c.x == 0 => 4,
                        _ => 5,
                    }
                );
                let geometry = (out.buffers.vertices.clone(), out.buffers.indices.clone());
                if let Some(reference) = &reference {
                    assert_eq!(reference, &geometry);
                }
                reference = Some(geometry);
                out.clear();
                assert_eq!(out.stroke_culling, control);
            }
        }
    }

    #[test]
    fn construction_reuse_preserves_order_geometry_styles_and_guards() {
        let mut polygons = Vec::new();
        for reversed in [false, true] {
            for style in 0..5 {
                for offset in [0., 16., 64.] {
                    let mut p = polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 1.);
                    if reversed {
                        p.points.reverse();
                    }
                    if style == 1 {
                        p.fill = None;
                    }
                    if style == 2 {
                        p.stroke = None;
                    }
                    if style == 3 {
                        p.fill.as_mut().unwrap().color.a = 0.5;
                    }
                    if style == 4 {
                        p.stroke.as_mut().unwrap().width = 3.;
                    }
                    for point in &mut p.points {
                        point.x += offset;
                        point.y -= offset;
                    }
                    polygons.push(p);
                }
            }
        }
        // Unsupported paths must remain empty templates with infinite guards.
        for _ in 0..2 {
            polygons.push(polygon(&[(0., 0.), (3., 3.), (0., 3.), (3., 0.)], 1.));
            polygons.push(polygon(&[(0., 0.), (3., 0.), (3., 0.), (0., 3.)], 1.));
        }
        let definition = Definition::new(polygons).unwrap();
        let mut builder = Builder::default();
        for precise in [false, true] {
            let plain = builder.build(&definition, precise).unwrap();
            let (memo, disabled) = builder
                .build_inner::<false, true>(&definition, precise)
                .unwrap();
            let (profiled, _) = builder.build_profiled(&definition, precise, true).unwrap();
            assert_eq!(memo.build_reuses, 20);
            assert!(plain.identical(&memo));
            assert!(plain.identical(&profiled));
            assert_eq!(disabled.reuse, Duration::ZERO);
        }
    }

    #[test]
    fn construction_dictionary_stops_admitting_but_keeps_existing_keys() {
        let mut polygons = Vec::new();
        for i in 0..MAX_BUILD_KEYS + 2 {
            let mut p = polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 1.);
            p.fill.as_mut().unwrap().color.r = i as f32 / 256.;
            polygons.push(p);
        }
        polygons.extend([polygons[0].clone(), polygons[MAX_BUILD_KEYS].clone()]);
        let definition = Definition::new(polygons).unwrap();
        let mut builder = Builder::default();
        let plain = builder.build(&definition, true).unwrap();
        let memo = builder.build_memoized(&definition, true).unwrap();
        assert_eq!(memo.build_reuses, 1);
        assert!(plain.identical(&memo));
    }

    #[test]
    fn copying_template_checks_both_budgets_before_appending() {
        let definition =
            Definition::new(vec![polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 1.)]).unwrap();
        let mut mesh = Builder::default().build(&definition, true).unwrap();
        let part = mesh.parts[0].clone();
        let original_vertices = mesh.buffers.vertices.len();
        mesh.buffers.vertices.resize(MAX_VERTICES, [0.; 8]);
        assert!(mesh.copy_part(&part, 1., 1.).is_err());
        assert_eq!(mesh.buffers.vertices.len(), MAX_VERTICES);
        mesh.buffers.vertices.truncate(original_vertices);
        mesh.buffers.indices.resize(MAX_INDICES, 0);
        assert!(mesh.copy_part(&part, 1., 1.).is_err());
        assert_eq!(mesh.buffers.vertices.len(), original_vertices);
        assert_eq!(mesh.buffers.indices.len(), MAX_INDICES);
        assert_eq!(mesh.build_reuses, 0);
    }

    #[test]
    fn guards_degenerate_and_self_intersecting_paths_but_accepts_concave_paths() {
        for points in [
            vec![(0., 0.), (3., 0.), (3., 0.), (0., 3.)],
            vec![(0., 0.), (3., 0.), (0., 0.)],
            vec![(0., 0.), (3., 3.), (0., 3.), (3., 0.)],
        ] {
            assert!(domains(&polygon(&points, 1.)).is_none());
        }
        assert!(domains(&polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 1.)).is_some());
    }

    #[test]
    fn wider_strokes_and_shorter_edges_require_larger_scales() {
        let p = polygon(&[(0., 0.), (4., 0.), (4., 4.), (0., 4.)], 1.);
        let baseline = domains(&p).unwrap().1;
        let mut wide = p.clone();
        wide.stroke.as_mut().unwrap().width = 4.;
        assert_eq!(domains(&wide).unwrap().1, baseline * 4.);
        let mut small = p;
        for p in &mut small.points {
            p.x *= 0.25;
            p.y *= 0.25;
        }
        assert_eq!(domains(&small).unwrap().1, baseline * 4.);
    }

    #[test]
    fn refined_domain_keeps_merge_margin_and_guards_both_edges_at_sharp_turns() {
        let rectangle = polygon(&[(0., 0.), (4., 0.), (4., 1.), (0., 1.)], 2.);
        let bounds = domain_bounds(&rectangle).unwrap();
        // Near-right-angle turns are treated as potentially sharp. Folding
        // requires reaching past both edges: 2 * half_width / longer_edge.
        assert_eq!(bounds.precise_stroke, 0.5_f32.next_up());
        assert!(bounds.precise_stroke >= bounds.merge);
        assert!(bounds.precise_stroke < bounds.legacy_stroke);
        let mut reversed = rectangle.clone();
        reversed.points.reverse();
        assert_eq!(
            domain_bounds(&reversed).unwrap().precise_stroke,
            bounds.precise_stroke
        );
        // A shallow turn never folds, even with a thick stroke. Edge merging
        // still sets a lower bound; this is not unrestricted stroke reuse.
        let octagon = polygon(
            &[
                (0., 1.),
                (1., 0.),
                (3., 0.),
                (4., 1.),
                (4., 3.),
                (3., 4.),
                (1., 4.),
                (0., 3.),
            ],
            12.,
        );
        let bounds = domain_bounds(&octagon).unwrap();
        assert_eq!(bounds.precise_stroke, bounds.merge);
        assert!(bounds.precise_stroke > 0.);
        assert!(bounds.precise_stroke < bounds.legacy_stroke);
    }

    #[test]
    fn refined_domain_scales_with_geometry_and_preserves_canonical_mesh() {
        let p = polygon(&[(0., 0.), (4., 0.), (1., 1.), (0., 4.)], 2.);
        let bounds = domain_bounds(&p).unwrap();
        let mut small = p.clone();
        for point in &mut small.points {
            point.x *= 0.25;
            point.y *= 0.25;
        }
        assert_eq!(
            domain_bounds(&small).unwrap().precise_stroke,
            bounds.precise_stroke * 4.
        );
        let definition = Definition::new(vec![p]).unwrap();
        let mut builder = Builder::default();
        let legacy = builder.build(&definition, false).unwrap();
        let refined = builder.build(&definition, true).unwrap();
        assert_eq!(legacy.buffers.vertices, refined.buffers.vertices);
        assert_eq!(legacy.buffers.indices, refined.buffers.indices);
        assert_eq!(
            legacy.parts[0].fill.minimum_scale,
            refined.parts[0].fill.minimum_scale
        );
        assert!(refined.parts[0].stroke.minimum_scale < legacy.parts[0].stroke.minimum_scale);
    }

    #[test]
    #[cfg(feature = "pi-kiosk-gpu")]
    #[ignore = "manual geometry-domain audit of the real zoom replay"]
    fn audit_zoom_outline_domains() {
        use crate::client_scenarios::{self, RenderBackend, ScenarioAsset, ScenarioStartMode};
        use crate::render::Viewport;
        use engine_common::{Settings, SpacewarsController};
        use std::time::Duration;
        let mut settings = Settings::default();
        settings.spacewars.player_1_controller = SpacewarsController::Human;
        settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
        let viewport = Viewport::new(1024., 768.);
        let mut scenario = client_scenarios::registration("spacewars")
            .unwrap()
            .create_with_asset(
                0,
                &settings,
                viewport,
                ScenarioStartMode::Normal,
                &ScenarioAsset::None,
            )
            .unwrap();
        for tick in 0..1637 {
            scenario.step(&[], Duration::from_nanos(16_666_667));
            if ![1500, 1560, 1636].contains(&tick) {
                continue;
            }
            let frames = scenario.render_frames(RenderBackend::Raster, viewport);
            let plan = super::super::presentation::build_with_retained(
                &frames,
                viewport,
                scenario.frame_layout(),
                1.,
                true,
            )
            .unwrap();
            for (view, pass) in plan.passes.iter().enumerate() {
                let mut counts = [0; 5];
                let mut local = BTreeMap::<Vec<u32>, usize>::new();
                let mut projected = BTreeMap::<Vec<u32>, usize>::new();
                let mut geometry_only = BTreeMap::<Vec<u32>, usize>::new();
                for p in &pass.scene().retained {
                    for (index, polygon) in p.definition.polygons().iter().enumerate() {
                        if polygon.stroke.is_none() {
                            continue;
                        }
                        counts[0] += 1;
                        match domain_bounds(polygon) {
                            None => counts[1] += 1,
                            Some(d) => {
                                counts[2] += usize::from(p.scale < d.legacy_stroke);
                                counts[3] += usize::from(p.scale < d.precise_stroke);
                                counts[4] += usize::from(p.scale < d.merge);
                            }
                        }
                        if domain_bounds(polygon).is_none_or(|b| p.scale < b.precise_stroke) {
                            let stroke = polygon.stroke.unwrap();
                            let style = [
                                stroke.width,
                                stroke.color.r,
                                stroke.color.g,
                                stroke.color.b,
                                stroke.color.a,
                            ]
                            .map(f32::to_bits);
                            let origin = polygon.points[0];
                            let mut key = vec![p.scale.to_bits()];
                            key.extend(style);
                            key.extend(polygon.points.iter().flat_map(|v| {
                                [(v.x - origin.x).to_bits(), (v.y - origin.y).to_bits()]
                            }));
                            let mut shape_key = vec![key[0], key[1]];
                            shape_key.extend_from_slice(&key[6..]);
                            *geometry_only.entry(shape_key).or_default() += 1;
                            *local.entry(key).or_default() += 1;
                            if let super::super::plan::Shape::Polygon(points) =
                                &pass.scene().draws[p.first + index].shape
                            {
                                let origin = points[0];
                                let mut key = style.to_vec();
                                key.extend(points.iter().flat_map(|v| {
                                    [(v.x - origin.x).to_bits(), (v.y - origin.y).to_bits()]
                                }));
                                *projected.entry(key).or_default() += 1;
                            }
                        }
                    }
                }
                eprintln!("tick={tick} view={view} [all,invalid,legacy,precise,merge]={counts:?}");
                let mut repeated: Vec<_> = local.values().copied().collect();
                repeated.sort_unstable_by(|a, b| b.cmp(a));
                eprintln!(
                    "fallbacks={} local_shapes={} projected_shapes={} geometry_only={} largest_repeats={:?}",
                    local.values().sum::<usize>(),
                    local.len(),
                    projected.len(),
                    geometry_only.len(),
                    &repeated[..repeated.len().min(10)]
                );
            }
        }
    }
}
