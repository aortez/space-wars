//! Bounded, explicit mesh reuse experiment. This is not a cache of arbitrary
//! transformed RenderFrame points and is not part of the scenario API.
use super::{
    mesh::{Batch, MAX_INDICES, MAX_VERTICES, MeshBuilder, Vertex},
    plan::{Draw, Scene, Shape},
};
use engine_common::{RenderColor, RenderPoint, Stroke};
use std::sync::Arc;

pub(crate) const MAX_ENTRIES: usize = 32;
pub(crate) const MAX_BYTES: usize = 1024 * 1024;
const MAX_POINTS: usize = 64;
pub(crate) type Instance = [f32; 16];

#[derive(Debug, PartialEq)]
pub(crate) struct Definition {
    points: Vec<RenderPoint>,
    // A rotation-independent bound, computed once. Invalid inputs stay invalid
    // even when shared by a cache hit; checking placement is O(1), not O(points).
    extent: f64,
}

impl Definition {
    pub fn new(points: Vec<RenderPoint>) -> Self {
        let extent = points.iter().fold(0.0_f64, |bound, p| {
            if !p.x.is_finite() || !p.y.is_finite() {
                f64::INFINITY
            } else {
                bound.max(f64::from(p.x).abs() + f64::from(p.y).abs())
            }
        });
        Self { points, extent }
    }

    pub fn points(&self) -> &[RenderPoint] {
        &self.points
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Polygon {
    pub definition: Arc<Definition>,
    // Bake uniform scale into the mesh, so strokes stay logical-pixel width.
    pub scale: f32,
    pub angle: f32,
    pub translation: RenderPoint,
}

impl Polygon {
    pub fn valid(&self) -> bool {
        [
            self.scale,
            self.angle,
            self.translation.x,
            self.translation.y,
        ]
        .iter()
        .all(|v| v.is_finite())
            && self.scale > 0.0
            // The ordinary fallback rotates before scaling. Bound intermediate
            // sums too, even if a tiny final scale would fit on screen.
            && self.definition.extent < f64::from(f32::MAX)
            && self.definition.extent * f64::from(self.scale)
                + f64::from(self.translation.x.abs().max(self.translation.y.abs()))
                < f64::from(f32::MAX)
    }

    pub fn project(&self, p: RenderPoint) -> RenderPoint {
        let (s, c) = self.angle.sin_cos();
        RenderPoint::new(
            self.translation.x + self.scale * (c * p.x - s * p.y),
            self.translation.y - self.scale * (s * p.x + c * p.y),
        )
    }

    // Fixture validation outside timing: metadata must describe the exact same
    // source geometry used by the independent, ordinary-triangle reference.
    pub fn matches(&self, draw: &Draw) -> bool {
        let Shape::Polygon(points) = &draw.shape else {
            return false;
        };
        points.len() == self.definition.points.len()
            && points
                .iter()
                .zip(&self.definition.points)
                .all(|(actual, local)| {
                    let p = self.project(*local);
                    (actual.x - p.x).abs() < 0.002 && (actual.y - p.y).abs() < 0.002
                })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Placement {
    pub draw: usize,
    pub polygon: Polygon,
}

impl Placement {
    pub fn matches(&self, draw: &Draw) -> bool {
        self.polygon.matches(draw)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    identity: usize,
    scale: u32,
    fill: bool,
    stroke: Option<u32>,
}
impl Key {
    fn new(p: &Polygon, d: &Draw) -> Self {
        Self {
            identity: Arc::as_ptr(&p.definition) as usize,
            scale: p.scale.to_bits(),
            fill: d.fill.is_some(),
            stroke: d.stroke.map(|s| s.width.to_bits()),
        }
    }
}

struct Entry {
    key: Key,
    // Own the immutable definition: an address cannot be reused while keyed here.
    _definition: Arc<Definition>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    last_used: u64,
    pinned: bool,
}
impl Entry {
    fn bytes(&self) -> usize {
        self.vertices.len() * 24 + self.indices.len() * 4
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Stream {
        start: usize,
        count: usize,
    },
    Mesh {
        slot: usize,
        first: usize,
        count: usize,
    },
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Stats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub fallbacks: u64,
    pub entries: usize,
    pub bytes: usize,
}

pub(crate) struct Builder {
    entries: Vec<Option<Entry>>,
    scratch: MeshBuilder,
    frame: u64,
    pub revision: u64,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub ranges: Vec<(usize, usize)>,
    pub instances: Vec<Instance>,
    pub commands: Vec<Command>,
    pub stats: Stats,
    expanded_indices: Vec<u32>,
}
impl Default for Builder {
    fn default() -> Self {
        Self {
            entries: (0..MAX_ENTRIES).map(|_| None).collect(),
            scratch: MeshBuilder::default(),
            frame: 0,
            revision: 0,
            vertices: Vec::new(),
            indices: Vec::new(),
            ranges: vec![(0, 0); MAX_ENTRIES],
            instances: Vec::new(),
            commands: Vec::new(),
            stats: Stats::default(),
            expanded_indices: Vec::new(),
        }
    }
}

impl Builder {
    pub(super) fn rebuild(
        &mut self,
        scene: &Scene,
        stream: &mut MeshBuilder,
        retained: bool,
    ) -> Result<(), String> {
        self.frame += 1;
        self.stats = Stats::default();
        self.instances.clear();
        self.commands.clear();
        stream.clear();
        let mut changed = false;
        if !retained {
            for entry in &mut self.entries {
                changed |= entry.take().is_some();
            }
        }
        for entry in self.entries.iter_mut().flatten() {
            entry.pinned = false;
        }
        let mut previous = None;
        for p in &scene.reuse {
            if p.draw >= scene.draws.len()
                || previous.is_some_and(|i| p.draw <= i)
                || !matches!(scene.draws[p.draw].shape, Shape::Polygon(_))
            {
                return Err("invalid or unordered polygon reuse metadata".into());
            }
            previous = Some(p.draw);
        }
        for (draw, p) in polygons(scene).filter_map(|(d, p)| p.map(|p| (d, p))) {
            if !p.valid() {
                return Err("invalid polygon transform".into());
            }
            let key = Key::new(p, draw);
            // Pin ALL existing keys before misses can evict an entry needed later.
            if let Some(e) = self.entries.iter_mut().flatten().find(|e| e.key == key) {
                e.pinned = true;
            }
        }
        for (draw, hint) in polygons(scene) {
            let slot = if let Some(p) = hint {
                if self.instances.len() >= 100_000 {
                    return Err("reusable mesh instance budget exceeded".into());
                }
                let (slot, modified) = self.resolve(p, draw)?;
                changed |= modified;
                slot
            } else {
                None
            };
            if let (Some(slot), Some(p)) = (slot, hint) {
                let first = self.instances.len();
                let (s, c) = p.angle.sin_cos();
                let fill = premul(draw.fill);
                let stroke = premul(draw.stroke.map(|s| s.color));
                self.instances.push([
                    c,
                    -s,
                    s,
                    c,
                    p.translation.x,
                    p.translation.y,
                    0.,
                    0.,
                    fill[0],
                    fill[1],
                    fill[2],
                    fill[3],
                    stroke[0],
                    stroke[1],
                    stroke[2],
                    stroke[3],
                ]);
                match self.commands.last_mut() {
                    Some(Command::Mesh {
                        slot: last, count, ..
                    }) if *last == slot => *count += 1,
                    _ => self.commands.push(Command::Mesh {
                        slot,
                        first,
                        count: 1,
                    }),
                }
            } else {
                if hint.is_some() {
                    self.stats.fallbacks += 1;
                }
                let (start, count) = stream.append_draw(draw)?;
                if count > 0 {
                    match self.commands.last_mut() {
                        Some(Command::Stream { count: last, .. }) => *last += count,
                        _ => self.commands.push(Command::Stream { start, count }),
                    }
                }
            }
        }
        if changed {
            self.vertices.clear();
            self.indices.clear();
            self.ranges.fill((0, 0));
            for (slot, e) in self.entries.iter().enumerate() {
                if let Some(e) = e {
                    let base = self.vertices.len() as u32;
                    self.ranges[slot] = (self.indices.len(), e.indices.len());
                    self.vertices.extend_from_slice(&e.vertices);
                    self.indices.extend(e.indices.iter().map(|i| base + i));
                }
            }
            self.revision += 1;
        }
        self.stats.entries = self.entries.iter().flatten().count();
        self.stats.bytes = self.vertices.len() * 24 + self.indices.len() * 4;
        stream.validate()
    }

    // Same cached fill/stroke triangles and command order as instancing, but
    // transform/color on the CPU and submit one ordinary triangle batch. Dynamic
    // vertices already live in `stream`; only their indices need interleaving.
    pub(super) fn expand(&mut self, stream: &mut MeshBuilder) -> Result<(), String> {
        let (mut vertices, mut indices) =
            (stream.buffers.vertices.len(), stream.buffers.indices.len());
        for command in &self.commands {
            if let Command::Mesh { slot, count, .. } = *command {
                let entry = self.entries[slot]
                    .as_ref()
                    .ok_or("missing expanded mesh entry")?;
                vertices = count
                    .checked_mul(entry.vertices.len())
                    .and_then(|n| vertices.checked_add(n))
                    .filter(|n| *n <= MAX_VERTICES)
                    .ok_or("GPU scene exceeds expanded polygon vertex budget")?;
                indices = count
                    .checked_mul(entry.indices.len())
                    .and_then(|n| indices.checked_add(n))
                    .filter(|n| *n <= MAX_INDICES)
                    .ok_or("GPU scene exceeds expanded polygon index budget")?;
            }
        }
        // Reject oversize expansions before modifying the streamed geometry.
        stream
            .buffers
            .vertices
            .reserve(vertices - stream.buffers.vertices.len());
        self.expanded_indices.clear();
        self.expanded_indices.reserve(indices);
        for command in &self.commands {
            match *command {
                Command::Stream { start, count } => self
                    .expanded_indices
                    .extend_from_slice(&stream.buffers.indices[start..start + count]),
                Command::Mesh { slot, first, count } => {
                    let entry = self.entries[slot].as_ref().unwrap();
                    for instance in &self.instances[first..first + count] {
                        let base = stream.buffers.vertices.len() as u32;
                        stream
                            .buffers
                            .vertices
                            .extend(entry.vertices.iter().map(|v| expand_vertex(*v, instance)));
                        self.expanded_indices
                            .extend(entry.indices.iter().map(|i| base + i));
                    }
                }
            }
        }
        std::mem::swap(&mut stream.buffers.indices, &mut self.expanded_indices);
        self.expanded_indices.clear();
        stream.batches.clear();
        if !stream.buffers.indices.is_empty() {
            stream.batches.push(Batch::Triangles {
                start: 0,
                count: stream.buffers.indices.len(),
            });
        }
        stream.validate()
    }

    fn resolve(&mut self, p: &Polygon, draw: &Draw) -> Result<(Option<usize>, bool), String> {
        let key = Key::new(p, draw);
        if let Some((slot, e)) = self
            .entries
            .iter_mut()
            .enumerate()
            .find_map(|(i, e)| e.as_mut().filter(|e| e.key == key).map(|e| (i, e)))
        {
            e.last_used = self.frame;
            e.pinned = true;
            self.stats.hits += 1;
            return Ok((Some(slot), false));
        }
        self.stats.misses += 1;
        if !(3..=MAX_POINTS).contains(&p.definition.points.len()) {
            return Ok((None, false));
        }
        if !p
            .definition
            .points
            .iter()
            .all(|v| (v.x * p.scale).is_finite() && (v.y * p.scale).is_finite())
        {
            return Err("non-finite reusable polygon definition".into());
        }
        // Negative alpha is an internal fill/stroke role tag, never a scene color.
        let local = Draw {
            shape: Shape::Polygon(
                p.definition
                    .points
                    .iter()
                    .map(|v| RenderPoint::new(v.x * p.scale, -v.y * p.scale))
                    .collect(),
            ),
            fill: draw.fill.map(|_| RenderColor::rgba(0., 0., 0., -1.)),
            stroke: draw
                .stroke
                .map(|s| Stroke::new(RenderColor::rgba(0., 0., 0., -2.), s.width)),
        };
        self.scratch.clear();
        self.scratch.append_draw(&local)?;
        self.scratch.validate()?;
        let needed =
            self.scratch.buffers.vertices.len() * 24 + self.scratch.buffers.indices.len() * 4;
        if needed > MAX_BYTES {
            return Ok((None, false));
        }
        let mut changed = false;
        loop {
            let bytes: usize = self.entries.iter().flatten().map(Entry::bytes).sum();
            if bytes + needed <= MAX_BYTES
                && let Some(slot) = self.entries.iter().position(Option::is_none)
            {
                self.entries[slot] = Some(Entry {
                    key,
                    _definition: p.definition.clone(),
                    vertices: self.scratch.buffers.vertices.clone(),
                    indices: self.scratch.buffers.indices.clone(),
                    last_used: self.frame,
                    pinned: true,
                });
                return Ok((Some(slot), true));
            }
            let victim = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(slot, e)| {
                    e.as_ref()
                        .filter(|e| !e.pinned)
                        .map(|e| (slot, e.last_used))
                })
                .min_by_key(|(_, age)| *age)
                .map(|(slot, _)| slot);
            if let Some(slot) = victim {
                self.entries[slot] = None;
                self.stats.evictions += 1;
                changed = true;
            } else {
                return Ok((None, changed));
            }
        }
    }
}

// Legacy hints and direct local geometry share the exact same cache/submission
// path. A local draw needs no parallel hint vector or materialized point list.
fn polygons(scene: &Scene) -> impl Iterator<Item = (&Draw, Option<&Polygon>)> {
    let mut hints = scene.reuse.iter().peekable();
    scene.draws.iter().enumerate().map(move |(index, draw)| {
        let polygon = if hints.peek().is_some_and(|p| p.draw == index) {
            hints.next().map(|p| &p.polygon)
        } else if let Shape::LocalPolygon(p) = &draw.shape {
            Some(p)
        } else {
            None
        };
        (draw, polygon)
    })
}

fn premul(color: Option<RenderColor>) -> [f32; 4] {
    color.map_or([0.; 4], |c| [c.r * c.a, c.g * c.a, c.b * c.a, c.a])
}

fn expand_vertex(v: Vertex, instance: &Instance) -> Vertex {
    let color = if v[5] < -1.5 {
        &instance[12..16]
    } else {
        &instance[8..12]
    };
    [
        instance[0] * v[0] + instance[2] * v[1] + instance[4],
        instance[1] * v[0] + instance[3] * v[1] + instance[5],
        color[0],
        color[1],
        color[2],
        color[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> Arc<Definition> {
        Arc::new(Definition::new(vec![
            RenderPoint::new(0., -5.),
            RenderPoint::new(4., 4.),
            RenderPoint::new(0., 1.),
            RenderPoint::new(-4., 4.),
        ]))
    }
    fn scene(definitions: &[Arc<Definition>]) -> Scene {
        let reuse: Vec<_> = definitions
            .iter()
            .enumerate()
            .map(|(draw, definition)| Placement {
                draw,
                polygon: Polygon {
                    definition: definition.clone(),
                    scale: 2.,
                    angle: 0.3,
                    translation: RenderPoint::new(40. + draw as f32, 40.),
                },
            })
            .collect();
        let draws = reuse
            .iter()
            .map(|p| Draw {
                shape: Shape::Polygon(
                    p.polygon
                        .definition
                        .points
                        .iter()
                        .map(|v| p.polygon.project(*v))
                        .collect(),
                ),
                fill: Some(RenderColor::rgba(1., 0.3, 0.2, 0.5)),
                stroke: Some(Stroke::new(RenderColor::WHITE, 1.)),
            })
            .collect();
        Scene {
            width: 100,
            height: 100,
            draws,
            reuse,
            retained: Vec::new(),
        }
    }

    fn local_scene(definitions: &[Arc<Definition>]) -> Scene {
        let mut s = scene(definitions);
        for p in s.reuse.drain(..) {
            s.draws[p.draw].shape = Shape::LocalPolygon(p.polygon);
        }
        s
    }

    #[test]
    fn direct_local_geometry_preserves_cached_and_ordinary_batches() {
        let defs: Vec<_> = (0..4).map(|_| definition()).collect();
        let world = scene(&defs);
        let local = local_scene(&defs);
        let mut a = MeshBuilder::default();
        let mut b = MeshBuilder::default();
        a.rebuild(&world).unwrap();
        b.rebuild(&local).unwrap();
        assert_eq!(a.buffers.vertices, b.buffers.vertices);
        assert_eq!(a.buffers.indices, b.buffers.indices);
        let mut ca = Builder::default();
        let mut cb = Builder::default();
        for _ in 0..2 {
            ca.rebuild(&world, &mut a, true).unwrap();
            cb.rebuild(&local, &mut b, true).unwrap();
            assert_eq!(ca.commands, cb.commands);
            assert_eq!(ca.instances, cb.instances);
            assert_eq!(
                (ca.stats.hits, ca.stats.misses),
                (cb.stats.hits, cb.stats.misses)
            );
            ca.expand(&mut a).unwrap();
            cb.expand(&mut b).unwrap();
            assert_eq!(a.buffers.vertices, b.buffers.vertices);
            assert_eq!(a.buffers.indices, b.buffers.indices);
        }
    }

    #[test]
    fn local_geometry_can_fall_back_under_cache_pressure_and_recover() {
        let defs: Vec<_> = (0..MAX_ENTRIES + 1).map(|_| definition()).collect();
        let mut cache = Builder::default();
        let mut stream = MeshBuilder::default();
        let overflow = local_scene(&defs);
        cache.rebuild(&overflow, &mut stream, true).unwrap();
        assert_eq!(
            (cache.stats.fallbacks, cache.stats.entries),
            (1, MAX_ENTRIES)
        );
        let mut expected = MeshBuilder::default();
        expected
            .append_draw(overflow.draws.last().unwrap())
            .unwrap();
        assert_eq!(stream.buffers.vertices, expected.buffers.vertices);
        assert_eq!(stream.buffers.indices, expected.buffers.indices);
        cache.expand(&mut stream).unwrap();
        assert_eq!(stream.batches.len(), 1);
        cache
            .rebuild(&local_scene(&defs[..1]), &mut stream, true)
            .unwrap();
        assert_eq!((cache.stats.hits, cache.stats.fallbacks), (1, 0));
    }

    #[test]
    fn local_invalid_input_or_duplicate_hint_is_rejected_without_poisoning_cache() {
        let defs = [definition()];
        let mut cache = Builder::default();
        let mut stream = MeshBuilder::default();
        for i in 0..3 {
            let mut s = local_scene(&defs);
            if i == 2 {
                s.reuse = scene(&defs).reuse;
            } else {
                let Shape::LocalPolygon(p) = &mut s.draws[0].shape else {
                    unreachable!()
                };
                if i == 0 {
                    p.translation.x = f32::NAN;
                } else {
                    p.definition =
                        Arc::new(Definition::new(vec![RenderPoint::new(f32::NAN, 0.); 3]));
                }
            }
            assert!(cache.rebuild(&s, &mut stream, true).is_err());
            cache
                .rebuild(&local_scene(&defs), &mut stream, true)
                .unwrap();
            assert_eq!(cache.stats.fallbacks, 0);
        }
    }
    #[test]
    fn translation_rotation_colors_reuse_but_scale_style_and_definition_invalidate() {
        let def = definition();
        let mut s = scene(&[def.clone(), def]);
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&s, &mut stream, true).unwrap();
        assert_eq!((b.stats.misses, b.stats.hits, b.stats.entries), (1, 1, 1));
        let revision = b.revision;
        let vertices = b.vertices.clone();
        let instances = b.instances.clone();
        s.reuse[0].polygon.angle += 1.;
        s.reuse[0].polygon.translation.x += 9.;
        s.draws[0].fill = Some(RenderColor::rgba(0., 1., 0., 0.2));
        b.rebuild(&s, &mut stream, true).unwrap();
        assert_eq!(b.stats.misses, 0);
        assert_eq!(b.revision, revision);
        assert_eq!(b.vertices, vertices);
        assert_ne!(b.instances, instances);
        assert!(b.vertices.iter().any(|v| v[5] == -1.));
        assert!(b.vertices.iter().any(|v| v[5] == -2.));
        for mutation in 0..4 {
            match mutation {
                0 => s.reuse[0].polygon.scale *= 0.5,
                1 => s.draws[0].stroke.as_mut().unwrap().width = 2.,
                2 => s.draws[0].fill = None,
                _ => s.reuse[0].polygon.definition = definition(),
            }
            b.rebuild(&s, &mut stream, true).unwrap();
            assert_eq!(b.stats.misses, 1, "mutation {mutation}");
        }
    }
    #[test]
    fn retained_and_rebuilt_templates_have_identical_geometry_and_commands() {
        let def = definition();
        let s = scene(&[def.clone(), def]);
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&s, &mut stream, true).unwrap();
        let vertices = b.vertices.clone();
        let indices = b.indices.clone();
        let revision = b.revision;
        b.rebuild(&s, &mut stream, false).unwrap();
        assert_eq!(b.stats.misses, 1);
        assert!(b.revision > revision);
        assert_eq!(b.vertices, vertices);
        assert_eq!(b.indices, indices);
        assert_eq!(
            b.commands,
            vec![Command::Mesh {
                slot: 0,
                first: 0,
                count: 2
            }]
        );
    }
    #[test]
    fn interleaved_transparency_is_never_sorted_and_unhinted_shapes_are_streamed() {
        let def = definition();
        let mut s = scene(&[def.clone(), def.clone(), def.clone(), def]);
        s.reuse.remove(2);
        s.reuse.remove(1);
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&s, &mut stream, true).unwrap();
        assert_eq!(
            b.commands,
            vec![
                Command::Mesh {
                    slot: 0,
                    first: 0,
                    count: 1
                },
                Command::Stream {
                    start: 0,
                    count: stream.buffers.indices.len()
                },
                Command::Mesh {
                    slot: 0,
                    first: 1,
                    count: 1
                }
            ]
        );
        let original = stream.buffers.vertices.clone();
        if let Shape::Polygon(p) = &mut s.draws[1].shape {
            p[0].x += 2.;
        }
        b.rebuild(&s, &mut stream, true).unwrap();
        assert_ne!(stream.buffers.vertices, original);
        assert_eq!(b.stats.misses, 0);
    }
    #[test]
    fn bounded_cache_pins_later_draws_and_falls_back_instead_of_evicting_active_meshes() {
        let defs: Vec<_> = (0..MAX_ENTRIES + 1).map(|_| definition()).collect();
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&scene(&defs[..MAX_ENTRIES]), &mut stream, true)
            .unwrap();
        let reordered: Vec<_> = std::iter::once(defs[MAX_ENTRIES].clone())
            .chain(defs[..MAX_ENTRIES].iter().cloned())
            .collect();
        b.rebuild(&scene(&reordered), &mut stream, true).unwrap();
        assert_eq!(
            (b.stats.fallbacks, b.stats.evictions, b.stats.entries),
            (1, 0, MAX_ENTRIES)
        );
        assert!(matches!(b.commands[0], Command::Stream { .. }));
        for _ in 0..100 {
            b.rebuild(&scene(&[definition()]), &mut stream, true)
                .unwrap();
            assert_eq!(b.stats.evictions, 1);
            assert_eq!(b.stats.entries, MAX_ENTRIES);
            assert!(b.stats.bytes <= MAX_BYTES);
        }
    }
    #[test]
    fn invalid_metadata_rejects_and_unsupported_sizes_fall_back_with_recovery() {
        let mut s = scene(&[definition()]);
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        s.reuse[0].polygon.scale = f32::NAN;
        assert!(b.rebuild(&s, &mut stream, true).is_err());
        s.reuse[0].polygon.scale = 2.;
        s.reuse[0].draw = 99;
        assert!(b.rebuild(&s, &mut stream, true).is_err());
        let large = Arc::new(Definition::new(
            (0..MAX_POINTS + 1)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / (MAX_POINTS + 1) as f32;
                    RenderPoint::new(a.cos() * 5., a.sin() * 5.)
                })
                .collect(),
        ));
        b.rebuild(&scene(&[large]), &mut stream, true).unwrap();
        assert_eq!(b.stats.fallbacks, 1);
        assert!(!stream.buffers.indices.is_empty());
        b.rebuild(&scene(&[definition()]), &mut stream, true)
            .unwrap();
        assert_eq!(b.stats.fallbacks, 0);
        assert!(stream.buffers.indices.is_empty());
        b.rebuild(&scene(&[]), &mut stream, false).unwrap();
        assert!(b.vertices.is_empty());
        assert!(b.commands.is_empty());
        assert!(b.instances.is_empty());
    }
    #[test]
    fn byte_limit_also_bounds_resident_templates() {
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&scene(&[definition()]), &mut stream, true)
            .unwrap();
        // Pad an inactive entry to exercise the independent byte limit without a
        // pathological tessellator workload. It must be evicted to admit a mesh.
        b.entries[0]
            .as_mut()
            .unwrap()
            .vertices
            .resize(MAX_BYTES / 24, [0.; 6]);
        b.rebuild(&scene(&[definition()]), &mut stream, true)
            .unwrap();
        assert_eq!(b.stats.evictions, 1);
        assert!(b.stats.bytes <= MAX_BYTES);
    }

    #[test]
    fn expanded_meshes_preserve_triangle_order_colors_and_dynamic_geometry() {
        let def = definition();
        let mut s = scene(&[def.clone(), def.clone(), def.clone(), def]);
        // Two dynamic primitives between two uses of the same immutable mesh.
        s.reuse.remove(2);
        s.reuse.remove(1);
        s.draws[3].fill = Some(RenderColor::rgba(0.2, 0.8, 0.3, 0.25));
        s.draws[3].stroke.as_mut().unwrap().color = RenderColor::rgba(0.8, 0.1, 0.5, 0.5);
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&s, &mut stream, true).unwrap();
        let dynamic = stream.buffers.vertices.clone();
        let cached = b.vertices.clone();
        let revision = b.revision;
        let mut expected = Vec::new();
        for command in &b.commands {
            match *command {
                Command::Stream { start, count } => expected.extend(
                    stream.buffers.indices[start..start + count]
                        .iter()
                        .map(|i| dynamic[*i as usize]),
                ),
                Command::Mesh { slot, first, count } => {
                    let e = b.entries[slot].as_ref().unwrap();
                    for inst in &b.instances[first..first + count] {
                        for i in &e.indices {
                            let v = e.vertices[*i as usize];
                            let color = if v[5] == -2. {
                                &inst[12..]
                            } else {
                                &inst[8..12]
                            };
                            expected.push([
                                v[0] * inst[0] + v[1] * inst[2] + inst[4],
                                v[0] * inst[1] + v[1] * inst[3] + inst[5],
                                color[0],
                                color[1],
                                color[2],
                                color[3],
                            ]);
                        }
                    }
                }
            }
        }
        b.expand(&mut stream).unwrap();
        let actual: Vec<_> = stream
            .buffers
            .indices
            .iter()
            .map(|i| stream.buffers.vertices[*i as usize])
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(
            &stream.buffers.vertices[..dynamic.len()],
            dynamic.as_slice()
        );
        assert_eq!(
            stream.batches,
            vec![Batch::Triangles {
                start: 0,
                count: expected.len()
            }]
        );
        assert_eq!(b.vertices, cached);
        assert_eq!(b.revision, revision);
        assert!(
            stream
                .buffers
                .vertices
                .iter()
                .all(|v| (0.0..=1.0).contains(&v[5]))
        );
        assert!(stream.upload_bytes() > dynamic.len() * 24);
        b.rebuild(&s, &mut stream, true).unwrap();
        b.expand(&mut stream).unwrap();
        assert_eq!(b.stats.misses, 0);
        assert_eq!(
            stream
                .buffers
                .indices
                .iter()
                .map(|i| stream.buffers.vertices[*i as usize])
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn expansion_rotates_translates_and_selects_premultiplied_fill_or_stroke() {
        let instance = [
            0., -1., 1., 0., 20., 30., 0., 0., 0.5, 0.2, 0.1, 0.5, 0.1, 0.2, 0.3, 0.4,
        ];
        assert_eq!(
            expand_vertex([2., -3., 0., 0., 0., -1.], &instance),
            [17., 28., 0.5, 0.2, 0.1, 0.5]
        );
        assert_eq!(
            expand_vertex([2., -3., 0., 0., 0., -2.], &instance),
            [17., 28., 0.1, 0.2, 0.3, 0.4]
        );
    }

    #[test]
    fn expansion_budget_rejects_before_mutation_and_next_frame_recovers() {
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        let s = scene(&[definition()]);
        b.rebuild(&s, &mut stream, true).unwrap();
        let before = stream.buffers.vertices.clone();
        // The preflight must reject before indexing instances or allocating.
        b.commands = vec![Command::Mesh {
            slot: 0,
            first: 0,
            count: usize::MAX,
        }];
        assert!(b.expand(&mut stream).unwrap_err().contains("budget"));
        assert_eq!(stream.buffers.vertices, before);
        b.rebuild(&s, &mut stream, true).unwrap();
        b.expand(&mut stream).unwrap();
        assert_eq!(stream.batches.len(), 1);
        let capacity = stream.buffers.vertices.capacity();
        b.rebuild(&scene(&[]), &mut stream, true).unwrap();
        b.expand(&mut stream).unwrap();
        assert!(stream.buffers.vertices.is_empty());
        assert!(stream.buffers.indices.is_empty());
        assert!(stream.batches.is_empty());
        assert_eq!(stream.buffers.vertices.capacity(), capacity);
    }

    #[test]
    fn expansion_without_hints_is_identical_to_ordinary_streaming() {
        let mut s = scene(&[definition()]);
        s.reuse.clear();
        let mut baseline = MeshBuilder::default();
        baseline.rebuild(&s).unwrap();
        let mut b = Builder::default();
        let mut stream = MeshBuilder::default();
        b.rebuild(&s, &mut stream, true).unwrap();
        b.expand(&mut stream).unwrap();
        assert_eq!(stream.buffers.vertices, baseline.buffers.vertices);
        assert_eq!(stream.buffers.indices, baseline.buffers.indices);
        assert_eq!(stream.batches, baseline.batches);
    }
}
