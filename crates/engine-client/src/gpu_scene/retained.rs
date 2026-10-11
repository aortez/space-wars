//! Immutable, ordered polygon groups with explicit
//! replacement after edits. Retain CPU tessellation, then expand into the same
//! single triangle stream. Neither screen-point hashing nor extra GPU draws.
use super::{
    AdmissionMode,
    encode_profile::{EncodeProfile, elapsed, started},
    mesh::{Batch, MAX_INDICES, MAX_VERTICES, MeshBuilder, Vertex},
    plan::{Draw, Scene, Shape},
    resident, scalable,
};
pub(crate) use engine_common::PolygonGeometry as Definition;
use engine_common::RenderPoint;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Weak},
    time::Instant,
};

const MAX_ENTRIES: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROBATION: usize = 128;
const SETTLED_FRAMES: u8 = 3;
// Deterministic admission work, shared by all views in one frame. Squared
// edge counts include the scalable builder's pairwise shape validation. This
// is a pacing heuristic, NOT a wall-time bound on arbitrary tessellations.
pub(crate) const ADMISSION_WORK_LIMIT: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    Cold,
    Always,
    Adaptive,
    ScaleIndependent,
    Hybrid,
    HybridOutlines,
    HybridPaced,
    HybridMemoized,
    HybridScan,
    HybridRanges,
    HybridCulled,
    HybridResident,
    HybridResidentPasses,
    HybridResidentBuffers,
    HybridResidentUniforms,
}
impl Policy {
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    pub(crate) fn supported(self, gl: &glow::Context) -> bool {
        // SAFETY: Probe callers keep the context current.
        match self {
            Self::HybridResidentBuffers => unsafe { super::resident_storage::validate(gl).is_ok() },
            Self::HybridResidentUniforms => unsafe {
                super::resident_storage::validate_uniforms(gl).is_ok()
            },
            _ => true,
        }
    }
    fn hybrid(self) -> bool {
        matches!(
            self,
            Self::Hybrid
                | Self::HybridOutlines
                | Self::HybridPaced
                | Self::HybridMemoized
                | Self::HybridScan
                | Self::HybridRanges
                | Self::HybridCulled
                | Self::HybridResident
                | Self::HybridResidentPasses
                | Self::HybridResidentBuffers
                | Self::HybridResidentUniforms
        )
    }
}

#[derive(Debug)]
pub(crate) struct Placement {
    pub first: usize,
    pub definition: Arc<Definition>,
    pub scale: f32,
    pub angle: f32,
    pub translation: RenderPoint,
}
impl Placement {
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
            && self.definition.extent() * f64::from(self.scale)
                + f64::from(self.translation.x.abs().max(self.translation.y.abs()))
                < f64::from(f32::MAX)
    }
    /// Projected-point check for explicitly authored offscreen fixtures. Live
    /// hints instead validate their exact authoritative world points in plan.rs.
    pub fn matches(&self, scene: &Scene) -> bool {
        let Some(draws) = scene
            .draws
            .get(self.first..self.first.saturating_add(self.definition.len()))
        else {
            return false;
        };
        let (s, c) = self.angle.sin_cos();
        draws.iter().zip(self.definition.polygons()).all(|(d, p)| {
            let Shape::Polygon(points) = &d.shape else {
                return false;
            };
            d.fill == p.fill.map(|f| f.color)
                && d.stroke == p.stroke
                && points.len() == p.points.len()
                && points.iter().zip(&p.points).all(|(a, b)| {
                    let x = self.translation.x + self.scale * (c * b.x - s * b.y);
                    let y = self.translation.y - self.scale * (s * b.x + c * b.y);
                    (a.x - x).abs() < 0.002 && (a.y - y).abs() < 0.002
                })
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    identity: usize,
    scale: u32,
}
struct Entry {
    // Monotonic admission identity, separate from mutable LRU timestamps.
    id: u64,
    // Own the key's identity so an allocator cannot recycle it under us.
    _definition: Arc<Definition>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    scalable: Option<scalable::Mesh>,
    visibility: super::visibility::Chunks,
    bytes: usize,
    used: u64,
}

struct Probation {
    // Keep the allocation identity alive without retaining its polygon payload.
    _definition: Weak<Definition>,
    frame: u64,
    consecutive: u8,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stats {
    pub hits: u64,
    pub misses: u64,
    /// Miss with the same definition resident under another scale/template key.
    pub scale_misses: u64,
    pub evictions: u64,
    pub fallbacks: u64,
    pub admissions: u64,
    pub bypasses: u64,
    pub zoom_reused_polygons: u64,
    pub zoom_fallback_polygons: u64,
    pub zoom_reused_fills: u64,
    pub zoom_reused_strokes: u64,
    pub hybrid_exact_groups: u64,
    pub hybrid_zoom_groups: u64,
    pub hybrid_fallback_groups: u64,
    pub promotions: u64,
    pub exact_entries: usize,
    pub zoom_entries: usize,
    pub probation_entries: usize,
    pub entries: usize,
    pub bytes: usize,
    pub build_ns: u64,
    pub max_build_ns: u64,
    pub build_work: u64,
    pub deferred: u64,
    pub oversized: u64,
    pub build_reuses: u64,
    pub culled_pieces: u64,
    pub culled_indices: u64,
    pub stroke_bound_tests: u64,
    pub stroke_bound_unknown: u64,
    pub stroke_bound_rejections: u64,
}
impl Stats {
    pub fn accumulate(&mut self, other: Self) {
        self.hits += other.hits;
        self.misses += other.misses;
        self.scale_misses += other.scale_misses;
        self.evictions += other.evictions;
        self.fallbacks += other.fallbacks;
        self.admissions += other.admissions;
        self.bypasses += other.bypasses;
        self.zoom_reused_polygons += other.zoom_reused_polygons;
        self.zoom_fallback_polygons += other.zoom_fallback_polygons;
        self.zoom_reused_fills += other.zoom_reused_fills;
        self.zoom_reused_strokes += other.zoom_reused_strokes;
        self.hybrid_exact_groups += other.hybrid_exact_groups;
        self.hybrid_zoom_groups += other.hybrid_zoom_groups;
        self.hybrid_fallback_groups += other.hybrid_fallback_groups;
        self.promotions += other.promotions;
        self.exact_entries = other.exact_entries;
        self.zoom_entries = other.zoom_entries;
        self.probation_entries = other.probation_entries;
        self.entries = other.entries;
        self.bytes = other.bytes;
        self.build_ns += other.build_ns;
        self.max_build_ns = self.max_build_ns.max(other.max_build_ns);
        self.build_work += other.build_work;
        self.deferred += other.deferred;
        self.oversized += other.oversized;
        self.build_reuses += other.build_reuses;
        self.culled_pieces += other.culled_pieces;
        self.culled_indices += other.culled_indices;
        self.stroke_bound_tests += other.stroke_bound_tests;
        self.stroke_bound_unknown += other.stroke_bound_unknown;
        self.stroke_bound_rejections += other.stroke_bound_rejections;
    }
}
pub(crate) struct Cache {
    entries: BTreeMap<Key, Entry>,
    bytes: usize,
    clock: u64,
    scratch: MeshBuilder,
    scalable: scalable::Builder,
    max_entries: usize,
    max_bytes: usize,
    // Weak identity tokens prevent address reuse without retaining the polygon
    // payload. FIFO probation bounds work/memory while rejecting one-use zooms.
    probation: BTreeMap<Key, Probation>,
    probation_order: VecDeque<Key>,
    admission: AdmissionMode,
    frame: u64,
    precise_outlines: bool,
    paced: bool,
    memoized: bool,
    culling: bool,
    visible_spans: Vec<super::visibility::Span>,
    frame_work: usize,
    work_limit: usize,
    pub stats: Stats,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            bytes: 0,
            clock: 0,
            scratch: MeshBuilder::default(),
            scalable: scalable::Builder::default(),
            max_entries: MAX_ENTRIES,
            max_bytes: MAX_BYTES,
            probation: BTreeMap::new(),
            probation_order: VecDeque::new(),
            admission: AdmissionMode::default(),
            frame: 0,
            precise_outlines: false,
            paced: false,
            memoized: false,
            culling: false,
            visible_spans: Vec::new(),
            frame_work: 0,
            work_limit: ADMISSION_WORK_LIMIT,
            stats: Stats::default(),
        }
    }
}
impl Cache {
    pub(crate) fn set_admission(&mut self, mode: AdmissionMode) {
        if self.admission != mode {
            self.admission = mode;
            // Old observations do not count towards the new policy. Completed
            // entries remain useful; this control never changes their geometry.
            self.probation.clear();
            self.probation_order.clear();
        }
    }

    /// All camera passes in one presentation share this frame. Seeing the same
    /// new scale in two cameras must not promote a continuously zooming view.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.frame_work = 0;
    }

    pub(crate) fn rebuild(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        policy: Policy,
    ) -> Result<(), String> {
        self.rebuild_inner::<false, false>(scene, groups, output, policy, None, None)
            .map(|_| ())
    }

    pub(crate) fn rebuild_clipped(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        policy: Policy,
        clip: super::presentation::Clip,
    ) -> Result<(), String> {
        self.rebuild_inner::<false, false>(scene, groups, output, policy, None, Some(clip))
            .map(|_| ())
    }

    pub(crate) fn rebuild_profiled(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        policy: Policy,
    ) -> Result<EncodeProfile, String> {
        self.rebuild_inner::<true, false>(scene, groups, output, policy, None, None)
    }

    pub(crate) fn rebuild_profiled_clipped(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        policy: Policy,
        clip: super::presentation::Clip,
    ) -> Result<EncodeProfile, String> {
        self.rebuild_inner::<true, false>(scene, groups, output, policy, None, Some(clip))
    }

    pub fn rebuild_resident(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        frame: &mut resident::Frame,
    ) -> Result<(), String> {
        frame.begin_pass();
        self.rebuild_inner::<false, true>(
            scene,
            groups,
            output,
            Policy::HybridResident,
            Some(frame),
            None,
        )?;
        frame.stream_pending(output)
    }

    fn rebuild_inner<const PROFILE: bool, const RESIDENT: bool>(
        &mut self,
        scene: &Scene,
        groups: &[Placement],
        output: &mut MeshBuilder,
        policy: Policy,
        mut frame: Option<&mut resident::Frame>,
        clip: Option<super::presentation::Clip>,
    ) -> Result<EncodeProfile, String> {
        let mut profile = EncodeProfile::default();
        self.stats = Stats::default();
        output.clear();
        self.paced = policy == Policy::HybridPaced;
        let culling = policy == Policy::HybridCulled;
        let clip = clip.unwrap_or(super::presentation::Clip {
            x: 0,
            y: 0,
            width: scene.width as i32,
            height: scene.height as i32,
        });
        let precise_outlines = matches!(
            policy,
            Policy::HybridOutlines
                | Policy::HybridPaced
                | Policy::HybridMemoized
                | Policy::HybridScan
                | Policy::HybridRanges
                | Policy::HybridCulled
                | Policy::HybridResident
                | Policy::HybridResidentPasses
                | Policy::HybridResidentBuffers
                | Policy::HybridResidentUniforms
        );
        let memoized = matches!(
            policy,
            Policy::HybridMemoized
                | Policy::HybridScan
                | Policy::HybridRanges
                | Policy::HybridCulled
                | Policy::HybridResident
                | Policy::HybridResidentPasses
                | Policy::HybridResidentBuffers
                | Policy::HybridResidentUniforms
        );
        if policy == Policy::Cold
            || precise_outlines != self.precise_outlines
            || memoized != self.memoized
            || culling != self.culling
        {
            self.entries.clear();
            self.bytes = 0;
            self.probation.clear();
            self.probation_order.clear();
        }
        self.precise_outlines = precise_outlines;
        self.memoized = memoized;
        self.culling = culling;
        let mut cursor = 0;
        for p in groups {
            let end = p
                .first
                .checked_add(p.definition.len())
                .ok_or("retained range overflow")?;
            if !p.valid() || p.first < cursor || end > scene.draws.len() {
                return Err("invalid or unordered retained geometry range".into());
            }
            for d in &scene.draws[cursor..p.first] {
                append_ordinary::<PROFILE>(output, d, &mut profile)?;
            }
            let start = started::<PROFILE>();
            let key = self.resolve(p, policy)?;
            profile.resolve += elapsed(start);
            if let Some(key) = key {
                let entry = &self.entries[&key];
                if let Some(mesh) = &entry.scalable {
                    let location = if RESIDENT {
                        frame.as_mut().unwrap().place(
                            entry.id,
                            mesh.vertices().iter().copied(),
                            p,
                            p.scale,
                        )?
                    } else {
                        None
                    };
                    let counts = if let Some(location) = location {
                        mesh.append_resident(
                            p,
                            &scene.draws[p.first..end],
                            output,
                            frame.as_mut().unwrap(),
                            location,
                        )?
                    } else if culling {
                        mesh.append_clipped::<PROFILE, true>(
                            p,
                            &scene.draws[p.first..end],
                            output,
                            &mut profile,
                            Some(clip),
                        )?
                    } else if matches!(
                        policy,
                        Policy::HybridRanges
                            | Policy::HybridResident
                            | Policy::HybridResidentPasses
                            | Policy::HybridResidentBuffers
                            | Policy::HybridResidentUniforms
                    ) {
                        mesh.append_inner::<PROFILE, true>(
                            p,
                            &scene.draws[p.first..end],
                            output,
                            &mut profile,
                        )?
                    } else {
                        mesh.append_inner::<PROFILE, false>(
                            p,
                            &scene.draws[p.first..end],
                            output,
                            &mut profile,
                        )?
                    };
                    self.stats.zoom_reused_polygons += counts.reused;
                    self.stats.zoom_fallback_polygons += counts.fallback;
                    self.stats.zoom_reused_fills += counts.fills;
                    self.stats.zoom_reused_strokes += counts.strokes;
                    self.stats.culled_pieces += counts.culled;
                    self.stats.culled_indices += counts.culled_indices;
                    self.stats.stroke_bound_tests += counts.stroke_bound_tests;
                    self.stats.stroke_bound_unknown += counts.stroke_bound_unknown;
                    self.stats.stroke_bound_rejections += counts.stroke_bound_rejections;
                    cursor = end;
                    continue;
                }
                if RESIDENT {
                    let frame = frame.as_mut().unwrap();
                    if let Some(location) = frame.place(
                        entry.id,
                        entry
                            .vertices
                            .iter()
                            .map(|v| [v[0], v[1], 0., 0., v[2], v[3], v[4], v[5]]),
                        p,
                        1.,
                    )? {
                        frame.cached(location, 0..entry.vertices.len(), &entry.indices, output)?;
                        cursor = end;
                        continue;
                    }
                }
                let sc = p.angle.sin_cos();
                if culling {
                    let removed = entry
                        .visibility
                        .select(p, sc, clip, &mut self.visible_spans);
                    let count: usize = self.visible_spans.iter().map(|s| s.vertices.len()).sum();
                    let indices: usize = self.visible_spans.iter().map(|s| s.indices.len()).sum();
                    // Adjacent index chunks share vertex ranges. If keeping
                    // separated chunks would duplicate too much geometry, use
                    // the original whole-entry expansion and its budget checks.
                    if count <= entry.vertices.len()
                        && output.buffers.vertices.len() + count <= MAX_VERTICES
                        && output.buffers.indices.len() + indices <= MAX_INDICES
                    {
                        let start = started::<PROFILE>();
                        for span in &self.visible_spans {
                            let base = output.buffers.vertices.len() as u32;
                            let (s, c) = sc;
                            output.buffers.vertices.extend(
                                entry.vertices[span.vertices.clone()].iter().map(|v| {
                                    [
                                        c * v[0] + s * v[1] + p.translation.x,
                                        -s * v[0] + c * v[1] + p.translation.y,
                                        v[2],
                                        v[3],
                                        v[4],
                                        v[5],
                                    ]
                                }),
                            );
                            output.buffers.indices.extend(
                                entry.indices[span.indices.clone()]
                                    .iter()
                                    .map(|i| base + i - span.vertices.start as u32),
                            );
                        }
                        if PROFILE {
                            profile.exact_expand += elapsed(start);
                            profile.exact_vertices += count as u64;
                        }
                        self.stats.culled_pieces += removed;
                        self.stats.culled_indices += (entry.indices.len() - indices) as u64;
                        cursor = end;
                        continue;
                    }
                }
                if output.buffers.vertices.len() + entry.vertices.len() > MAX_VERTICES
                    || output.buffers.indices.len() + entry.indices.len() > MAX_INDICES
                {
                    return Err("retained expansion exceeds frame budget".into());
                }
                let start = started::<PROFILE>();
                let base = output.buffers.vertices.len() as u32;
                let (s, c) = sc;
                output
                    .buffers
                    .vertices
                    .extend(entry.vertices.iter().map(|v| {
                        [
                            c * v[0] + s * v[1] + p.translation.x,
                            -s * v[0] + c * v[1] + p.translation.y,
                            v[2],
                            v[3],
                            v[4],
                            v[5],
                        ]
                    }));
                output
                    .buffers
                    .indices
                    .extend(entry.indices.iter().map(|i| base + i));
                if PROFILE {
                    profile.exact_expand += elapsed(start);
                    profile.exact_vertices += entry.vertices.len() as u64;
                }
            } else {
                for d in &scene.draws[p.first..end] {
                    append_ordinary::<PROFILE>(output, d, &mut profile)?;
                }
            }
            cursor = end;
        }
        for d in &scene.draws[cursor..] {
            append_ordinary::<PROFILE>(output, d, &mut profile)?;
        }
        if !output.buffers.indices.is_empty() {
            output.batches.push(Batch::Triangles {
                start: 0,
                count: output.buffers.indices.len(),
            });
        }
        self.stats.entries = self.entries.len();
        self.stats.zoom_entries = self.entries.keys().filter(|k| k.scale == 0).count();
        self.stats.exact_entries = self.stats.entries - self.stats.zoom_entries;
        self.stats.bytes = self.bytes;
        self.stats.probation_entries = self.probation.len();
        let start = started::<PROFILE>();
        if matches!(
            policy,
            Policy::HybridScan
                | Policy::HybridRanges
                | Policy::HybridCulled
                | Policy::HybridResident
                | Policy::HybridResidentPasses
                | Policy::HybridResidentBuffers
                | Policy::HybridResidentUniforms
        ) {
            output.validate_bulk()?;
        } else {
            output.validate()?;
        }
        profile.validate += elapsed(start);
        Ok(profile)
    }

    fn resolve(&mut self, p: &Placement, policy: Policy) -> Result<Option<Key>, String> {
        if !policy.hybrid() {
            return self.resolve_kind(p, policy);
        }
        let exact = Key {
            identity: Arc::as_ptr(&p.definition) as usize,
            scale: p.scale.to_bits(),
        };
        if let Some(entry) = self.entries.get_mut(&exact) {
            self.clock += 1;
            entry.used = self.clock;
            self.stats.hits += 1;
            self.stats.hybrid_exact_groups += 1;
            return Ok(Some(exact));
        }
        // Never quantize a changing zoom. Repeated exact scales qualify for the
        // fast expanded representation; first-use scales share the local mesh.
        let repeated = match self.admission {
            AdmissionMode::Repeated => self
                .probation
                .get(&exact)
                .is_some_and(|observation| observation.frame < self.frame),
            AdmissionMode::Settled => self.observe_settled(exact, &p.definition),
        };
        if repeated {
            if let Some(key) = self.resolve_kind(p, policy)? {
                self.stats.hybrid_exact_groups += 1;
                return Ok(Some(key));
            }
        } else {
            if self.admission == AdmissionMode::Repeated {
                self.seen_before(exact, &p.definition);
            }
            self.stats.bypasses += 1;
        }
        let key = self.resolve_kind(p, Policy::ScaleIndependent)?;
        if key.is_some() {
            self.stats.hybrid_zoom_groups += 1;
        } else {
            self.stats.hybrid_fallback_groups += 1;
        }
        Ok(key)
    }

    fn resolve_kind(&mut self, p: &Placement, policy: Policy) -> Result<Option<Key>, String> {
        self.clock += 1;
        let key = Key {
            identity: Arc::as_ptr(&p.definition) as usize,
            // Positive finite scales never have zero bits. Reserve that key
            // for one guarded template shared by all zooms and cameras.
            scale: if policy == Policy::ScaleIndependent {
                0
            } else {
                p.scale.to_bits()
            },
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            self.stats.hits += 1;
            return Ok(Some(key));
        }
        self.stats.misses += 1;
        if self
            .entries
            .range(
                Key {
                    identity: key.identity,
                    scale: 0,
                }..=Key {
                    identity: key.identity,
                    scale: u32::MAX,
                },
            )
            .next()
            .is_some()
        {
            self.stats.scale_misses += 1;
        }
        if self.max_entries == 0 || p.definition.payload_bytes() > self.max_bytes {
            self.stats.fallbacks += 1;
            return Ok(None);
        }
        if policy == Policy::Adaptive && !self.seen_before(key, &p.definition) {
            self.stats.bypasses += 1;
            return Ok(None);
        }
        let work: usize = p
            .definition
            .polygons()
            .iter()
            .map(|p| p.points.len().pow(2))
            .sum();
        if self.paced {
            // Allow one indivisible oversized group in an otherwise unused
            // frame so size alone cannot exclude it. Draw-order scheduling is
            // not a fairness or deadline guarantee under sustained churn.
            // Expose the exception; do not retain a pending geometry queue.
            if self.work_limit == 0
                || (self.frame_work != 0 && self.frame_work.saturating_add(work) > self.work_limit)
            {
                self.stats.deferred += 1;
                return Ok(None);
            }
            self.frame_work = self.frame_work.saturating_add(work);
            self.stats.oversized += u64::from(work > self.work_limit);
        }
        self.stats.build_work += work as u64;
        let started = Instant::now();
        let result = self.build_entry(p, key, policy);
        let elapsed = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.stats.build_ns += elapsed;
        self.stats.max_build_ns = self.stats.max_build_ns.max(elapsed);
        result
    }

    fn build_entry(
        &mut self,
        p: &Placement,
        key: Key,
        policy: Policy,
    ) -> Result<Option<Key>, String> {
        let scalable = if policy == Policy::ScaleIndependent {
            let mut mesh = if self.memoized {
                self.scalable
                    .build_memoized(&p.definition, self.precise_outlines)?
            } else {
                self.scalable.build(&p.definition, self.precise_outlines)?
            };
            if self.culling {
                mesh.enable_culling();
            }
            self.stats.build_reuses += mesh.build_reuses;
            Some(mesh)
        } else {
            None
        };
        self.scratch.clear();
        let mut promoted = false;
        if scalable.is_none() {
            let draws = || {
                p.definition.polygons().iter().map(|polygon| Draw {
                    shape: Shape::Polygon(
                        polygon
                            .points
                            .iter()
                            .map(|v| RenderPoint::new(v.x * p.scale, -v.y * p.scale))
                            .collect(),
                    ),
                    fill: polygon.fill.map(|f| f.color),
                    stroke: polygon.stroke,
                })
            };
            // Promote from the reusable template when available. Rebuild only
            // guarded pieces, in scaled local coordinates so later rigid motion
            // does not become part of the exact-scale key.
            let zoom_key = Key {
                identity: key.identity,
                scale: 0,
            };
            if policy.hybrid()
                && let Some(mesh) = self
                    .entries
                    .get(&zoom_key)
                    .and_then(|e| e.scalable.as_ref())
            {
                mesh.append(
                    &Placement {
                        first: 0,
                        definition: p.definition.clone(),
                        scale: p.scale,
                        angle: 0.,
                        translation: RenderPoint::ZERO,
                    },
                    &draws().collect::<Vec<_>>(),
                    &mut self.scratch,
                )?;
                promoted = true;
            } else {
                for draw in draws() {
                    self.scratch.append_draw(&draw)?;
                }
            }
        }
        self.scratch.validate()?;
        let visibility = if self.culling && scalable.is_none() {
            super::visibility::Chunks::new(
                &self.scratch.buffers.vertices,
                &self.scratch.buffers.indices,
            )
        } else {
            Default::default()
        };
        let bytes = p.definition.payload_bytes()
            + visibility.bytes()
            + scalable.as_ref().map_or(0, |mesh| mesh.bytes())
            + self.scratch.buffers.vertices.len() * std::mem::size_of::<Vertex>()
            + self.scratch.buffers.indices.len() * 4;
        if bytes > self.max_bytes {
            self.stats.fallbacks += 1;
            return Ok(None);
        }
        while self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes {
            let victim = *self.entries.iter().min_by_key(|(_, e)| e.used).unwrap().0;
            self.bytes -= self.entries.remove(&victim).unwrap().bytes;
            self.stats.evictions += 1;
        }
        self.entries.insert(
            key,
            Entry {
                id: self.clock,
                _definition: p.definition.clone(),
                vertices: self.scratch.buffers.vertices.clone(),
                indices: self.scratch.buffers.indices.clone(),
                scalable,
                visibility,
                bytes,
                used: self.clock,
            },
        );
        self.bytes += bytes;
        self.stats.admissions += 1;
        self.stats.promotions += u64::from(promoted);
        Ok(Some(key))
    }

    fn seen_before(&mut self, key: Key, definition: &Arc<Definition>) -> bool {
        if self.probation.contains_key(&key) {
            return true;
        }
        if self.probation.len() == MAX_PROBATION {
            self.probation
                .remove(&self.probation_order.pop_front().unwrap());
        }
        self.probation.insert(
            key,
            Probation {
                _definition: Arc::downgrade(definition),
                frame: self.frame,
                consecutive: 1,
            },
        );
        self.probation_order.push_back(key);
        false
    }

    fn observe_settled(&mut self, key: Key, definition: &Arc<Definition>) -> bool {
        if !self.seen_before(key, definition) {
            return false;
        }
        let observation = self.probation.get_mut(&key).unwrap();
        if observation.frame != self.frame {
            observation.consecutive = if observation.frame.checked_add(1) == Some(self.frame) {
                observation
                    .consecutive
                    .saturating_add(1)
                    .min(SETTLED_FRAMES)
            } else {
                1
            };
            observation.frame = self.frame;
        }
        // Duplicate groups and additional cameras in this frame never advance
        // the count. Recurring scales separated by a gap start probation again.
        observation.consecutive >= SETTLED_FRAMES
    }
}

// Const-disabled calls keep the original append path. Nested clocks and adapter
// counter reads belong only to explicit encoder diagnostics, not normal draws.
#[inline]
fn append_ordinary<const PROFILE: bool>(
    output: &mut MeshBuilder,
    draw: &Draw,
    profile: &mut EncodeProfile,
) -> Result<(), String> {
    if PROFILE {
        let before = output.single_pass_stats();
        let start = started::<true>();
        output.append_draw_profiled::<true>(
            draw,
            draw.fill,
            draw.stroke,
            &mut profile.ordinary_geometry,
        )?;
        profile.ordinary += elapsed(start);
        profile.ordinary_draws += 1;
        let s = output.single_pass_stats().delta(before);
        for (total, value) in profile.ordinary_topology.iter_mut().zip([
            s.cached,
            s.prepared,
            s.refused[0],
            s.refused[1],
            s.refused[2],
            s.refused[3],
            s.refused[4],
        ]) {
            *total += value;
        }
    } else {
        output.append_draw(draw)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Fill, RenderColor, RenderPolygon, Stroke};

    #[test]
    #[cfg(feature = "pi-kiosk-gpu")]
    #[ignore = "manual cold-cache admission profile on production replay frames"]
    fn profile_cold_admissions() {
        use crate::client_scenarios::{self, RenderBackend, ScenarioAsset, ScenarioStartMode};
        use crate::render::Viewport;
        use engine_common::{Settings, SpacewarsController};
        use std::time::{Duration, Instant};
        for (seed, tick, human) in [(42, 120, false), (42, 1800, false), (0, 1500, true)] {
            let mut settings = Settings::default();
            settings.spacewars.player_1_controller = if human {
                SpacewarsController::Human
            } else {
                SpacewarsController::RuleBot
            };
            settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
            let viewport = Viewport::new(1024., 768.);
            let mut scenario = client_scenarios::registration("spacewars")
                .unwrap()
                .create_with_asset(
                    seed,
                    &settings,
                    viewport,
                    ScenarioStartMode::Normal,
                    &ScenarioAsset::None,
                )
                .unwrap();
            for _ in 0..=tick {
                scenario.step(&[], Duration::from_nanos(16_666_667));
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
            let mut cache = Cache {
                precise_outlines: true,
                ..Default::default()
            };
            cache.begin_frame();
            let mut samples = Vec::new();
            for pass in &plan.passes {
                for p in &pass.scene().retained {
                    let count = cache.stats.admissions;
                    let start = Instant::now();
                    cache.resolve(p, Policy::HybridOutlines).unwrap();
                    if cache.stats.admissions > count {
                        let work: usize = p
                            .definition
                            .polygons()
                            .iter()
                            .map(|p| p.points.len().pow(2))
                            .sum();
                        samples.push((start.elapsed().as_micros(), p.definition.len(), work));
                    }
                }
            }
            samples.sort_by(|a, b| b.cmp(a));
            eprintln!(
                "seed={seed} tick={tick} builds={} total_us={} total_work={} largest={:?}",
                samples.len(),
                samples.iter().map(|s| s.0).sum::<u128>(),
                samples.iter().map(|s| s.2).sum::<usize>(),
                &samples[..samples.len().min(10)]
            );
        }
    }
    fn source(color: RenderColor) -> Arc<Definition> {
        Arc::new(
            Definition::new(vec![RenderPolygon {
                points: vec![
                    RenderPoint::new(-3., -2.),
                    RenderPoint::new(4., -2.),
                    RenderPoint::new(1., 0.),
                    RenderPoint::new(3., 4.),
                ],
                fill: Some(Fill::new(color)),
                stroke: Some(Stroke::new(RenderColor::WHITE, 0.75)),
            }])
            .unwrap(),
        )
    }
    fn placement(definition: Arc<Definition>) -> Placement {
        Placement {
            first: 0,
            definition,
            scale: 2.,
            angle: 0.,
            translation: RenderPoint::new(10., 10.),
        }
    }
    #[test]
    fn culling_removes_only_offscreen_triangles_through_zoom_promotion_and_reveal() {
        use super::super::presentation::Clip;
        let mut polygons = Vec::new();
        for i in 0..160 {
            let mut p =
                source(RenderColor::rgba(1., i as f32 / 160., 0., 0.4)).polygons()[0].clone();
            // Interleave safe, wide and duplicate-point paths, all styles.
            if i % 3 == 0 {
                p.stroke.as_mut().unwrap().width = 12.;
            }
            if i % 7 == 0 {
                p.points[1] = p.points[0];
            }
            if i % 11 == 0 {
                p.fill = None;
            }
            if i % 13 == 0 && p.fill.is_some() {
                p.stroke = None;
            }
            for v in &mut p.points {
                v.x += (i % 20) as f32 * 12.;
                v.y -= (i / 20) as f32 * 12.;
            }
            polygons.push(p);
        }
        let mut p = placement(Arc::new(Definition::new(polygons).unwrap()));
        let mut baseline = Cache::default();
        let mut culled = Cache::default();
        let mut a = MeshBuilder::default();
        let mut b = MeshBuilder::default();
        let mut removed = 0;
        let mut promoted = 0;
        let mut fallback = 0;
        for i in 0..60 {
            p.scale = [0.001, 0.2, 1., 1., 2., 2., 3.1, 3.2, 0.2, 1.][i % 10];
            p.angle = i as f32 * 0.07;
            p.translation = RenderPoint::new(80. - (i % 5) as f32 * 70., 30.);
            let scene = scene(&p);
            // A player pane narrower than the target; alternate clipping must
            // not affect identities or permanently remove hidden geometry.
            let clip = Clip {
                x: if i % 2 == 0 { 0 } else { 50 },
                y: 0,
                width: 50,
                height: 100,
            };
            baseline.begin_frame();
            culled.begin_frame();
            baseline
                .rebuild(
                    &scene,
                    std::slice::from_ref(&p),
                    &mut a,
                    Policy::HybridRanges,
                )
                .unwrap();
            culled
                .rebuild_clipped(
                    &scene,
                    std::slice::from_ref(&p),
                    &mut b,
                    Policy::HybridCulled,
                    clip,
                )
                .unwrap();
            removed += culled.stats.culled_indices;
            promoted += culled.stats.promotions;
            fallback += culled.stats.zoom_fallback_polygons;
            assert!(culled.stats.entries <= MAX_ENTRIES && culled.stats.bytes <= MAX_BYTES);
            let triangle = |mesh: &MeshBuilder, t: &[u32]| {
                t.iter()
                    .map(|&i| mesh.buffers.vertices[i as usize].map(f32::to_bits))
                    .collect::<Vec<_>>()
            };
            let mut kept = b.buffers.indices.chunks_exact(3).peekable();
            for t in a.buffers.indices.chunks_exact(3) {
                if kept
                    .peek()
                    .is_some_and(|k| triangle(&a, t) == triangle(&b, k))
                {
                    kept.next();
                } else {
                    let v: Vec<_> = t.iter().map(|&i| a.buffers.vertices[i as usize]).collect();
                    assert!(
                        v.iter().all(|v| v[0] < clip.x as f32)
                            || v.iter().all(|v| v[0] > (clip.x + clip.width) as f32)
                            || v.iter().all(|v| v[1] < clip.y as f32)
                            || v.iter().all(|v| v[1] > (clip.y + clip.height) as f32),
                        "visible triangle removed at frame {i}"
                    );
                }
            }
            assert!(
                kept.next().is_none(),
                "culling changed triangle content/order"
            );
        }
        assert!(removed > 0 && promoted > 0 && fallback > 0);
        // All culling metadata contributes to the same payload budget. Forced
        // admission rejection still returns the ordinary authoritative path.
        culled.max_bytes = 0;
        culled.entries.clear();
        culled.bytes = 0;
        culled
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut b,
                Policy::HybridCulled,
            )
            .unwrap();
        a.rebuild(&scene(&p)).unwrap();
        assert_eq!(a.buffers.vertices, b.buffers.vertices);
        assert_eq!(a.buffers.indices, b.buffers.indices);
    }

    fn scene(p: &Placement) -> Scene {
        let (s, c) = p.angle.sin_cos();
        Scene {
            width: 100,
            height: 100,
            reuse: vec![],
            retained: Vec::new(),
            draws: p
                .definition
                .polygons()
                .iter()
                .map(|d| Draw {
                    shape: Shape::Polygon(
                        d.points
                            .iter()
                            .map(|v| {
                                RenderPoint::new(
                                    p.translation.x + p.scale * (c * v.x - s * v.y),
                                    p.translation.y - p.scale * (s * v.x + c * v.y),
                                )
                            })
                            .collect(),
                    ),
                    fill: d.fill.map(|f| f.color),
                    stroke: d.stroke,
                })
                .collect(),
        }
    }

    #[test]
    fn coalesced_ranges_preserve_exact_geometry_order_and_guarded_fallbacks() {
        let base = source(RenderColor::RED).polygons()[0].clone();
        let mut polygons = vec![base.clone(); 8];
        polygons[1].fill = None;
        polygons[2].stroke = None;
        polygons[3].stroke.as_mut().unwrap().width = 12.;
        polygons[4].points[1] = polygons[4].points[0];
        polygons[5].fill.as_mut().unwrap().color.a = 0.4;
        polygons[6].points.reverse();
        let mut p = placement(Arc::new(Definition::new(polygons).unwrap()));
        let mesh = scalable::Builder::default()
            .build_memoized(&p.definition, true)
            .unwrap();
        let mut saved_ranges = 0;
        let mut fallback_strokes = 0;
        for scale in [0.00001, 0.001, 0.1, 0.5, 1., 2., 3., 100.] {
            p.scale = scale;
            for angle in [0., 0.37, -1.25] {
                p.angle = angle;
                let mut draws = scene(&p).draws;
                // Exercise skipped template ranges, too. Live metadata would
                // reject this style mismatch, but expansion must not bridge it.
                draws[6].fill = None;
                let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
                a.append_draw(&draws[0]).unwrap();
                b.append_draw(&draws[0]).unwrap();
                let (mut ap, mut bp) = (EncodeProfile::default(), EncodeProfile::default());
                let ac = mesh
                    .append_inner::<true, false>(&p, &draws, &mut a, &mut ap)
                    .unwrap();
                let bc = mesh
                    .append_inner::<true, true>(&p, &draws, &mut b, &mut bp)
                    .unwrap();
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
                    (ac.reused, ac.fallback, ac.fills, ac.strokes),
                    (bc.reused, bc.fallback, bc.fills, bc.strokes)
                );
                assert_eq!(ap.scalable_vertices, bp.scalable_vertices);
                assert_eq!(ap.fallback_fills, bp.fallback_fills);
                assert_eq!(ap.fallback_strokes, bp.fallback_strokes);
                assert!(bp.scalable_ranges <= ap.scalable_ranges);
                saved_ranges += ap.scalable_ranges - bp.scalable_ranges;
                fallback_strokes += bp.fallback_strokes;
                b.validate().unwrap();
            }
        }
        assert!(saved_ranges > 0);
        assert!(fallback_strokes > 0);
    }

    #[test]
    fn coalesced_ranges_enforce_both_frame_budgets() {
        let mut p = placement(source(RenderColor::RED));
        p.scale = 100.;
        let mesh = scalable::Builder::default()
            .build_memoized(&p.definition, true)
            .unwrap();
        let draws = scene(&p).draws;
        for vertices in [false, true] {
            let mut output = MeshBuilder::default();
            if vertices {
                output.buffers.vertices.resize(MAX_VERTICES, [0.; 6]);
            } else {
                output.buffers.indices.resize(MAX_INDICES, 0);
            }
            let before = (output.buffers.vertices.len(), output.buffers.indices.len());
            assert!(
                mesh.append_inner::<false, true>(
                    &p,
                    &draws,
                    &mut output,
                    &mut EncodeProfile::default()
                )
                .unwrap_err()
                .contains("frame budget")
            );
            assert_eq!(
                before,
                (output.buffers.vertices.len(), output.buffers.indices.len())
            );
        }
    }

    #[test]
    fn resident_references_preserve_instances_guards_promotions_and_replacements() {
        // Zero capacity proves the all-streamed fallback; the normal case
        // exercises shared GPU geometry at distinct placements in one pass.
        for capacity in [0, resident::ATLAS_VERTICES] {
            let mut p = placement(source(RenderColor::rgba(0.4, 0.2, 0.7, 0.5)));
            let (mut baseline, mut candidate) = (Cache::default(), Cache::default());
            let (mut a, mut scratch) = (MeshBuilder::default(), MeshBuilder::default());
            let mut frame = resident::Frame::with_capacity(capacity);
            let mut guard_count = 0;
            for (step, scale) in [0.001, 0.001, 2., 2., 100., 100., 0.1, 4.]
                .into_iter()
                .enumerate()
            {
                p.scale = scale;
                p.angle += 0.19;
                if step == 6 {
                    p.definition = source(RenderColor::BLUE);
                }
                let mut s = scene(&p);
                s.draws.push(
                    scene(&placement(source(RenderColor::GREEN)))
                        .draws
                        .remove(0),
                );
                let other = Placement {
                    first: s.draws.len(),
                    scale: p.scale,
                    angle: -p.angle,
                    translation: RenderPoint::new(70., 40.),
                    definition: p.definition.clone(),
                };
                s.draws.extend(scene(&other).draws);
                let groups = [&p, &other].map(|p| Placement {
                    first: p.first,
                    definition: p.definition.clone(),
                    scale: p.scale,
                    angle: p.angle,
                    translation: p.translation,
                });
                baseline.begin_frame();
                candidate.begin_frame();
                frame.begin_frame();
                baseline
                    .rebuild(&s, &groups, &mut a, Policy::HybridRanges)
                    .unwrap();
                candidate
                    .rebuild_resident(&s, &groups, &mut scratch, &mut frame)
                    .unwrap();
                let b = frame.expand_reference().unwrap();
                assert!(
                    a.buffers
                        .vertices
                        .iter()
                        .flatten()
                        .map(|v| v.to_bits())
                        .eq(b.buffers.vertices.iter().flatten().map(|v| v.to_bits()))
                );
                assert_eq!(a.buffers.indices, b.buffers.indices);
                assert_eq!(baseline.stats.admissions, candidate.stats.admissions);
                assert_eq!(
                    baseline.stats.zoom_fallback_polygons,
                    candidate.stats.zoom_fallback_polygons
                );
                guard_count += candidate.stats.zoom_fallback_polygons;
                if capacity == 0 {
                    assert_eq!(frame.stats().cached_vertices, 0);
                } else {
                    assert!(frame.stats().cached_vertices > 0);
                }
                frame.uploaded();
            }
            assert!(guard_count > 0);
        }
    }

    #[test]
    fn ordinary_encoder_attribution_is_exact_optional_and_counts_each_attempt_once() {
        use super::super::stroke_output::StrokeWriter;
        for writer in [StrokeWriter::BulkStatic, StrokeWriter::BulkSinglePass] {
            let (mut plain, mut profiled) = (MeshBuilder::default(), MeshBuilder::default());
            plain.set_stroke_writer(writer);
            profiled.set_stroke_writer(writer);
            let (mut disabled, mut p) = (EncodeProfile::default(), EncodeProfile::default());
            for width in [0.75, 2.0, 0.75] {
                let draw = Draw {
                    shape: Shape::Polygon(vec![
                        RenderPoint::new(0., 0.),
                        RenderPoint::new(1., 0.),
                        RenderPoint::new(1., 1.),
                        RenderPoint::new(0., 1.),
                    ]),
                    fill: Some(RenderColor::BLUE),
                    stroke: Some(Stroke::new(RenderColor::RED, width)),
                };
                append_ordinary::<false>(&mut plain, &draw, &mut disabled).unwrap();
                append_ordinary::<true>(&mut profiled, &draw, &mut p).unwrap();
                assert_eq!(plain.buffers.vertices, profiled.buffers.vertices);
                assert_eq!(plain.buffers.indices, profiled.buffers.indices);
                assert_eq!(plain.single_pass_stats(), profiled.single_pass_stats());
            }
            assert_eq!(disabled, EncodeProfile::default());
            assert_eq!(p.ordinary_draws, 3);
            assert_eq!(p.ordinary_geometry.points, 12);
            assert_eq!(
                p.ordinary_geometry.vertices as usize,
                plain.buffers.vertices.len()
            );
            assert_eq!(
                p.ordinary_geometry.indices as usize,
                plain.buffers.indices.len()
            );
            assert_eq!(
                p.ordinary_topology,
                if writer.single_pass() {
                    [2, 0, 0, 0, 1, 0, 0]
                } else {
                    [0; 7]
                }
            );
        }
    }

    #[test]
    fn encoding_profile_preserves_geometry_and_compiles_out_of_plain_path() {
        let mut p = placement(source(RenderColor::RED));
        let (mut plain, mut detailed) = (Cache::default(), Cache::default());
        let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
        let mut total = EncodeProfile::default();
        for scale in [2., 2., 3., 0.001, 4.] {
            p.scale = scale;
            p.angle += 0.17;
            let mut scene = scene(&p);
            // An ordinary, unhinted draw.
            scene.draws.push(Draw {
                shape: Shape::Polygon(vec![
                    RenderPoint::new(0., 0.),
                    RenderPoint::new(2., 0.),
                    RenderPoint::new(0., 2.),
                ]),
                fill: Some(RenderColor::BLUE),
                stroke: None,
            });
            plain.begin_frame();
            detailed.begin_frame();
            let disabled = plain
                .rebuild_inner::<false, false>(
                    &scene,
                    std::slice::from_ref(&p),
                    &mut a,
                    Policy::HybridMemoized,
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(disabled, EncodeProfile::default());
            total.accumulate(
                detailed
                    .rebuild_profiled(
                        &scene,
                        std::slice::from_ref(&p),
                        &mut b,
                        Policy::HybridMemoized,
                    )
                    .unwrap(),
            );
            assert_eq!(a.buffers.vertices, b.buffers.vertices);
            assert_eq!(a.buffers.indices, b.buffers.indices);
            assert_eq!(a.batches, b.batches);
            assert_eq!(plain.stats.admissions, detailed.stats.admissions);
            assert_eq!(plain.stats.bytes, detailed.stats.bytes);
            assert_eq!(plain.stats.hits, detailed.stats.hits);
        }
        assert!(total.exact_vertices > 0);
        assert!(total.scalable_vertices > 0);
        assert!(total.fallback_strokes > 0);
        assert_eq!(total.ordinary_draws, 5);
        // No timing thresholds: clocks are diagnostics, not correctness checks.
    }

    #[test]
    fn live_encoding_detail_preserves_clipped_bulk_geometry_and_cache_decisions() {
        use super::super::{stroke_culling::Control, stroke_output::StrokeWriter};
        let mut p = placement(source(RenderColor::RED));
        let (mut plain, mut detailed) = (Cache::default(), Cache::default());
        let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
        for mesh in [&mut a, &mut b] {
            mesh.stroke_culling = Control::Conservative;
            mesh.stroke_writer = StrokeWriter::BulkStatic;
            mesh.set_round_join_mode(lyon_tessellation::RoundJoinMode::Exact);
        }
        let clip = super::super::presentation::Clip {
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        };
        let mut profile = EncodeProfile::default();
        for (scale, x) in [(2., 20.), (3., -100.), (0.001, 20.), (4., 48.), (2., 20.)] {
            plain.begin_frame();
            detailed.begin_frame();
            p.scale = scale;
            p.translation.x = x;
            p.angle += 0.17;
            let scene = scene(&p);
            plain
                .rebuild_clipped(
                    &scene,
                    std::slice::from_ref(&p),
                    &mut a,
                    Policy::HybridCulled,
                    clip,
                )
                .unwrap();
            profile.accumulate(
                detailed
                    .rebuild_profiled_clipped(
                        &scene,
                        std::slice::from_ref(&p),
                        &mut b,
                        Policy::HybridCulled,
                        clip,
                    )
                    .unwrap(),
            );
            assert_eq!(a.buffers.vertices, b.buffers.vertices);
            assert_eq!(a.buffers.indices, b.buffers.indices);
            assert_eq!(a.batches, b.batches);
            let mut left = plain.stats;
            let mut right = detailed.stats;
            for stats in [&mut left, &mut right] {
                stats.build_ns = 0;
                stats.max_build_ns = 0;
            }
            assert!(left == right);
        }
        assert!(profile.fallback_strokes > 0);
        assert!(profile.scalable_vertices > 0);
    }

    #[test]
    fn cache_keys_cover_identity_and_scale_but_not_rigid_motion() {
        let mut cache = Cache::default();
        let mut out = MeshBuilder::default();
        let mut p = placement(source(RenderColor::RED));
        for step in 0..6 {
            match step {
                1 => p.translation.x += 7.,
                2 => p.angle = 0.7,
                3 => p.scale = 3.,
                4 => p.definition = source(RenderColor::BLUE), // Same shape, replaced material.
                5 => p.definition = source(RenderColor::BLUE), // New world with reset revisions.
                _ => (),
            }
            let scene = scene(&p);
            assert!(p.matches(&scene));
            cache
                .rebuild(&scene, std::slice::from_ref(&p), &mut out, Policy::Always)
                .unwrap();
            assert_eq!(cache.stats.hits, u64::from(step == 1 || step == 2));
            assert_eq!(cache.stats.misses, u64::from(step != 1 && step != 2));
            assert_eq!(cache.stats.scale_misses, u64::from(step == 3));
            assert!(out.buffers.vertices.iter().flatten().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn zoom_cache_reuses_across_scales_and_keeps_fixed_pixel_stroke_width() {
        let definition = Arc::new(
            Definition::new(vec![RenderPolygon {
                points: vec![
                    RenderPoint::new(-2., -2.),
                    RenderPoint::new(2., -2.),
                    RenderPoint::new(2., 2.),
                    RenderPoint::new(-2., 2.),
                ],
                fill: None,
                stroke: Some(Stroke::new(RenderColor::WHITE, 2.)),
            }])
            .unwrap(),
        );
        let mut p = placement(definition);
        let mut cache = Cache::default();
        let mut out = MeshBuilder::default();
        for (i, scale) in [1., 10., 0.001, 100., 0.1, 2.].into_iter().enumerate() {
            p.scale = scale;
            let s = scene(&p);
            cache
                .rebuild(
                    &s,
                    std::slice::from_ref(&p),
                    &mut out,
                    Policy::ScaleIndependent,
                )
                .unwrap();
            assert_eq!(cache.stats.entries, 1);
            assert_eq!(cache.stats.misses, u64::from(i == 0));
            assert_eq!(cache.stats.hits, u64::from(i > 0));
            if scale >= 1. {
                assert_eq!(cache.stats.zoom_reused_polygons, 1);
                let right = out
                    .buffers
                    .vertices
                    .iter()
                    .map(|v| v[0])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((right - (p.translation.x + 2. * scale + 1.)).abs() < 0.001);
            } else {
                assert_eq!(cache.stats.zoom_fallback_polygons, 1);
                let mut reference = MeshBuilder::default();
                reference.rebuild(&s).unwrap();
                assert_eq!(out.buffers.vertices, reference.buffers.vertices);
                assert_eq!(out.buffers.indices, reference.buffers.indices);
            }
        }
        p.definition = source(RenderColor::BLUE);
        cache
            .rebuild(&scene(&p), &[p], &mut out, Policy::ScaleIndependent)
            .unwrap();
        assert_eq!(cache.stats.admissions, 1);
        assert_eq!(cache.stats.entries, 2);
    }

    #[test]
    fn zoom_cache_guards_parts_individually_and_preserves_fill_stroke_order() {
        let valid = source(RenderColor::RED).polygons()[0].clone();
        let mut invalid = valid.clone();
        invalid.points[1] = invalid.points[0];
        invalid.fill = Some(Fill::new(RenderColor::GREEN));
        let mut last = valid.clone();
        last.fill = Some(Fill::new(RenderColor::BLUE));
        let p = placement(Arc::new(
            Definition::new(vec![valid, invalid, last]).unwrap(),
        ));
        let mut cache = Cache::default();
        let mut output = MeshBuilder::default();
        let s = scene(&p);
        cache
            .rebuild(
                &s,
                std::slice::from_ref(&p),
                &mut output,
                Policy::ScaleIndependent,
            )
            .unwrap();
        assert_eq!(
            (
                cache.stats.zoom_reused_polygons,
                cache.stats.zoom_fallback_polygons
            ),
            (2, 1)
        );
        let colors: Vec<_> = output
            .buffers
            .indices
            .chunks_exact(3)
            .map(|t| output.buffers.vertices[t[0] as usize][2..6].to_vec())
            .collect();
        let mut runs = colors;
        runs.dedup();
        assert_eq!(
            runs,
            vec![
                vec![1., 0., 0., 1.],
                vec![1., 1., 1., 1.],
                vec![0., 1., 0., 1.],
                vec![1., 1., 1., 1.],
                vec![0., 0., 1., 1.],
                vec![1., 1., 1., 1.]
            ]
        );
        // Count the larger scalable vertices and per-polygon metadata in the
        // same cache cap. A rejected entry is a byte-identical ordinary frame.
        let bytes = cache.stats.bytes;
        cache = Cache {
            max_bytes: bytes - 1,
            ..Default::default()
        };
        cache
            .rebuild(&s, &[p], &mut output, Policy::ScaleIndependent)
            .unwrap();
        assert_eq!(cache.stats.fallbacks, 1);
        assert_eq!(cache.stats.bytes, 0);
        let mut reference = MeshBuilder::default();
        reference.rebuild(&s).unwrap();
        assert_eq!(output.buffers.vertices, reference.buffers.vertices);
        assert_eq!(output.buffers.indices, reference.buffers.indices);
    }
    #[test]
    fn bounds_evict_old_entries_and_fallback_is_the_ordinary_mesh() {
        let mut cache = Cache {
            max_entries: 1,
            ..Default::default()
        };
        let mut out = MeshBuilder::default();
        for color in [RenderColor::RED, RenderColor::BLUE] {
            let p = placement(source(color));
            cache
                .rebuild(&scene(&p), &[p], &mut out, Policy::Always)
                .unwrap();
            assert_eq!(cache.stats.entries, 1);
            assert_eq!(cache.stats.evictions, u64::from(color == RenderColor::BLUE));
        }
        cache.max_bytes = 0;
        let p = placement(source(RenderColor::GREEN));
        let s = scene(&p);
        cache.rebuild(&s, &[p], &mut out, Policy::Cold).unwrap();
        let mut reference = MeshBuilder::default();
        reference.rebuild(&s).unwrap();
        assert_eq!(out.buffers.vertices, reference.buffers.vertices);
        assert_eq!(out.buffers.indices, reference.buffers.indices);
        assert_eq!(out.batches, reference.batches);
        assert_eq!(cache.stats.fallbacks, 1);
        assert_eq!(cache.stats.bytes, 0);
    }
    #[test]
    fn invalid_sources_and_ranges_are_rejected() {
        assert!(Definition::new(vec![]).is_err());
        let mut invalid = source(RenderColor::RED).polygons().to_vec();
        invalid[0].points[0].x = f32::NAN;
        assert!(Definition::new(invalid).is_err());
        let mut cache = Cache::default();
        let mut output = MeshBuilder::default();
        let mut p = placement(source(RenderColor::RED));
        let scene = scene(&p);
        p.first = usize::MAX;
        assert!(
            cache
                .rebuild(&scene, &[p], &mut output, Policy::Always)
                .is_err()
        );
        let mut p = placement(source(RenderColor::RED));
        p.scale = f32::INFINITY;
        assert!(
            cache
                .rebuild(&scene, &[p], &mut output, Policy::Always)
                .is_err()
        );
    }
    #[test]
    fn cached_groups_preserve_interleaved_dynamic_draw_order() {
        let mut a = placement(source(RenderColor::RED));
        a.first = 1;
        let mut b = placement(source(RenderColor::BLUE));
        b.first = 3;
        let mut draws = Vec::new();
        for p in [&a, &a, &b, &b, &a] {
            draws.extend(scene(p).draws);
        }
        let s = Scene {
            width: 100,
            height: 100,
            draws,
            reuse: vec![],
            retained: Vec::new(),
        };
        let mut cache = Cache::default();
        let mut out = MeshBuilder::default();
        cache
            .rebuild(&s, &[a, b], &mut out, Policy::Always)
            .unwrap();
        // Translation can change Lyon's vertex ordering. Compare ordered
        // triangle colors, not vertex indices, at this integer-coordinate pose.
        let mut reference = MeshBuilder::default();
        reference.rebuild(&s).unwrap();
        let colors = |m: &MeshBuilder| {
            m.buffers
                .indices
                .iter()
                .map(|i| m.buffers.vertices[*i as usize][2..6].to_vec())
                .collect::<Vec<_>>()
        };
        assert_eq!(colors(&out), colors(&reference));
        assert_eq!(out.batches.len(), 1);
    }

    #[test]
    fn adaptive_zoom_bypasses_are_exact_ordinary_meshes_and_bounded() {
        let mut cache = Cache::default();
        let mut out = MeshBuilder::default();
        let mut reference = MeshBuilder::default();
        let mut p = placement(source(RenderColor::RED));
        for step in 0..300 {
            p.scale = 1. + step as f32 * 0.01;
            let scene = scene(&p);
            cache
                .rebuild(&scene, std::slice::from_ref(&p), &mut out, Policy::Adaptive)
                .unwrap();
            reference.rebuild(&scene).unwrap();
            assert_eq!(out.buffers.vertices, reference.buffers.vertices);
            assert_eq!(out.buffers.indices, reference.buffers.indices);
            assert_eq!(out.batches, reference.batches);
            assert_eq!(cache.stats.bypasses, 1);
            assert_eq!(cache.stats.admissions, 0);
            assert_eq!(cache.stats.fallbacks, 0);
            assert_eq!(cache.stats.entries, 0);
            assert_eq!(cache.stats.bytes, 0);
            assert!(cache.stats.probation_entries <= MAX_PROBATION);
        }
        // Probation cannot keep the source-art payload alive after its producer.
        assert_eq!(Arc::strong_count(&p.definition), 1);
        let weak = Arc::downgrade(&p.definition);
        drop(p);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn adaptive_reuse_supports_two_views_and_recovers_after_zoom_settles() {
        let mut cache = Cache::default();
        let mut out = MeshBuilder::default();
        let mut p = placement(source(RenderColor::RED));
        for frame in 0..4 {
            for scale in [2., 7.] {
                p.scale = scale;
                p.translation.x += 0.3;
                cache
                    .rebuild(
                        &scene(&p),
                        std::slice::from_ref(&p),
                        &mut out,
                        Policy::Adaptive,
                    )
                    .unwrap();
                assert_eq!(cache.stats.bypasses, u64::from(frame == 0));
                assert_eq!(cache.stats.admissions, u64::from(frame == 1));
                assert_eq!(cache.stats.hits, u64::from(frame > 1));
            }
        }
        assert_eq!(cache.stats.entries, 2);
        p.scale = 12.;
        for frame in 0..3 {
            cache
                .rebuild(
                    &scene(&p),
                    std::slice::from_ref(&p),
                    &mut out,
                    Policy::Adaptive,
                )
                .unwrap();
            assert_eq!(cache.stats.bypasses, u64::from(frame == 0));
            assert_eq!(cache.stats.admissions, u64::from(frame == 1));
            assert_eq!(cache.stats.hits, u64::from(frame == 2));
        }
        // Replaced geometry/material must not inherit another world's probation.
        p.definition = source(RenderColor::BLUE);
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::Adaptive,
            )
            .unwrap();
        assert_eq!(cache.stats.bypasses, 1);
        assert_eq!(cache.stats.admissions, 0);
    }

    #[test]
    fn hybrid_continuous_zoom_in_two_views_keeps_one_template() {
        for policy in [
            Policy::Hybrid,
            Policy::HybridOutlines,
            Policy::HybridPaced,
            Policy::HybridMemoized,
            Policy::HybridScan,
            Policy::HybridRanges,
        ] {
            let mut cache = Cache::default();
            let mut out = MeshBuilder::default();
            let mut p = placement(source(RenderColor::RED));
            for frame in 0..300 {
                cache.begin_frame();
                p.scale = 2. + frame as f32 * 0.01;
                for view in 0..2 {
                    p.translation.x += 0.3;
                    p.angle += 0.01;
                    cache
                        .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                        .unwrap();
                    assert_eq!(cache.stats.hybrid_zoom_groups, 1);
                    assert_eq!(cache.stats.hybrid_exact_groups, 0);
                    assert_eq!(cache.stats.promotions, 0);
                    assert_eq!(cache.stats.admissions, u64::from(frame == 0 && view == 0));
                    assert_eq!(cache.stats.evictions, 0);
                    assert_eq!(cache.stats.exact_entries, 0);
                    assert_eq!(cache.stats.zoom_entries, 1);
                    assert!(cache.stats.probation_entries <= MAX_PROBATION);
                }
            }
        }
    }

    #[test]
    fn settled_cyclic_zoom_does_not_promote_old_scales_or_count_cameras_as_frames() {
        let (mut repeated, mut settled) = (Cache::default(), Cache::default());
        settled.set_admission(AdmissionMode::Settled);
        let (mut a, mut b) = (MeshBuilder::default(), MeshBuilder::default());
        let mut p = placement(source(RenderColor::RED));
        p.angle = 0.;
        p.translation = RenderPoint::ZERO;
        for frame in 0..120 {
            repeated.begin_frame();
            settled.begin_frame();
            // Two moving views revisit previously observed scales, including
            // equal scales at crossings. Neither scale persists across frames.
            for scale in [[2., 4.], [3., 3.], [4., 2.], [5., 6.]][frame % 4] {
                p.scale = scale;
                let scene = scene(&p);
                for (cache, out) in [(&mut repeated, &mut a), (&mut settled, &mut b)] {
                    cache
                        .rebuild(&scene, std::slice::from_ref(&p), out, Policy::HybridCulled)
                        .unwrap();
                }
                assert_eq!(a.buffers.vertices, b.buffers.vertices);
                assert_eq!(a.buffers.indices, b.buffers.indices);
                assert_eq!(a.batches, b.batches);
                assert_eq!(settled.stats.hybrid_exact_groups, 0);
                assert_eq!(settled.stats.promotions, 0);
                assert_eq!(settled.stats.zoom_entries, 1);
                assert_eq!(settled.stats.evictions, 0);
                assert!(settled.stats.probation_entries <= MAX_PROBATION);
            }
        }
        assert!(repeated.stats.exact_entries > 0);
        assert_eq!(settled.stats.exact_entries, 0);
    }

    #[test]
    fn settled_two_views_promote_on_third_frame_and_reuse_completed_entries() {
        let mut cache = Cache::default();
        cache.set_admission(AdmissionMode::Settled);
        let mut p = placement(source(RenderColor::RED));
        let mut out = MeshBuilder::default();
        for frame in 0..5 {
            cache.begin_frame();
            for scale in [2., 7.] {
                p.scale = scale;
                for duplicate in 0..3 {
                    cache
                        .rebuild(
                            &scene(&p),
                            std::slice::from_ref(&p),
                            &mut out,
                            Policy::HybridCulled,
                        )
                        .unwrap();
                    assert_eq!(cache.stats.hybrid_exact_groups, u64::from(frame >= 2));
                    assert_eq!(
                        cache.stats.promotions,
                        u64::from(frame == 2 && duplicate == 0)
                    );
                    if frame >= 3 {
                        assert_eq!(cache.stats.build_ns, 0);
                        assert_eq!(cache.stats.admissions, 0);
                    }
                }
            }
        }
        assert_eq!(cache.stats.exact_entries, 2);
        assert_eq!(cache.stats.zoom_entries, 1);
        cache.begin_frame();
        p.scale = 5.;
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridCulled,
            )
            .unwrap();
        assert_eq!(cache.stats.hybrid_exact_groups, 0);
        cache.begin_frame();
        p.scale = 2.;
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridCulled,
            )
            .unwrap();
        assert_eq!(cache.stats.hybrid_exact_groups, 1);
        assert_eq!(cache.stats.build_ns, 0);
    }

    #[test]
    fn settled_probation_restarts_after_gaps_edits_and_policy_changes() {
        let mut cache = Cache::default();
        cache.set_admission(AdmissionMode::Settled);
        let mut p = placement(source(RenderColor::RED));
        let mut out = MeshBuilder::default();
        for observed in [true, true, false, true, true] {
            cache.begin_frame();
            if observed {
                cache
                    .rebuild(
                        &scene(&p),
                        std::slice::from_ref(&p),
                        &mut out,
                        Policy::HybridCulled,
                    )
                    .unwrap();
                assert_eq!(cache.stats.promotions, 0);
            }
        }
        // An idempotent request must not prevent a settled scale from warming.
        cache.set_admission(AdmissionMode::Settled);
        cache.begin_frame();
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridCulled,
            )
            .unwrap();
        assert_eq!(cache.stats.promotions, 1);
        let entries = cache.entries.len();
        cache.set_admission(AdmissionMode::Repeated);
        cache.set_admission(AdmissionMode::Settled);
        assert!(cache.probation.is_empty());
        assert_eq!(cache.entries.len(), entries);
        p.definition = source(RenderColor::BLUE);
        cache.begin_frame();
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridCulled,
            )
            .unwrap();
        assert_eq!(cache.stats.hybrid_exact_groups, 0);
        assert_eq!(cache.stats.promotions, 0);
    }

    #[test]
    fn settled_probation_is_bounded_and_does_not_keep_polygon_payloads_alive() {
        let mut cache = Cache {
            max_entries: 0,
            ..Default::default()
        };
        cache.set_admission(AdmissionMode::Settled);
        let mut out = MeshBuilder::default();
        for _ in 0..MAX_PROBATION * 3 {
            cache.begin_frame();
            let p = placement(source(RenderColor::RED));
            let weak = Arc::downgrade(&p.definition);
            cache
                .rebuild(
                    &scene(&p),
                    std::slice::from_ref(&p),
                    &mut out,
                    Policy::HybridCulled,
                )
                .unwrap();
            assert!(cache.probation.len() <= MAX_PROBATION);
            assert_eq!(cache.probation.len(), cache.probation_order.len());
            assert_eq!(cache.stats.bytes, 0);
            drop(p);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn hybrid_promotes_stable_views_and_immediately_reuses_after_zoom_changes() {
        for policy in [
            Policy::Hybrid,
            Policy::HybridOutlines,
            Policy::HybridPaced,
            Policy::HybridMemoized,
            Policy::HybridScan,
            Policy::HybridRanges,
        ] {
            let mut cache = Cache::default();
            let mut out = MeshBuilder::default();
            let mut p = placement(source(RenderColor::RED));
            for scales in [[2., 7.], [11., 13.]] {
                for frame in 0..3 {
                    cache.begin_frame();
                    for scale in scales {
                        p.scale = scale;
                        cache
                            .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                            .unwrap();
                        assert_eq!(cache.stats.hybrid_zoom_groups, u64::from(frame == 0));
                        assert_eq!(cache.stats.hybrid_exact_groups, u64::from(frame > 0));
                        assert_eq!(cache.stats.promotions, u64::from(frame == 1));
                        assert_eq!(cache.stats.evictions, 0);
                    }
                }
            }
            assert_eq!(cache.stats.exact_entries, 4);
            assert_eq!(cache.stats.zoom_entries, 1);
            // Same shape/scale with changed material is a new identity, not a hit.
            p.definition = source(RenderColor::BLUE);
            cache.begin_frame();
            cache.rebuild(&scene(&p), &[p], &mut out, policy).unwrap();
            assert_eq!(cache.stats.hybrid_zoom_groups, 1);
            assert_eq!(cache.stats.admissions, 1);
            assert_eq!(cache.stats.hits, 0);
        }
    }

    #[test]
    fn hybrid_promotion_preserves_guarded_parts_and_painter_order() {
        for policy in [
            Policy::Hybrid,
            Policy::HybridOutlines,
            Policy::HybridPaced,
            Policy::HybridMemoized,
            Policy::HybridScan,
            Policy::HybridRanges,
        ] {
            let valid = source(RenderColor::RED).polygons()[0].clone();
            let mut invalid = valid.clone();
            invalid.points[1] = invalid.points[0];
            invalid.fill = Some(Fill::new(RenderColor::GREEN));
            let definition = Arc::new(Definition::new(vec![valid, invalid]).unwrap());
            for scale in [0.001, 0.1, 2., 100.] {
                let mut p = placement(definition.clone());
                p.scale = scale;
                p.translation = RenderPoint::ZERO;
                let mut cache = Cache::default();
                let mut out = MeshBuilder::default();
                cache.begin_frame();
                cache
                    .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                    .unwrap();
                let vertices = out.buffers.vertices.clone();
                let indices = out.buffers.indices.clone();
                for _ in 0..2 {
                    cache.begin_frame();
                    cache
                        .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                        .unwrap();
                    assert_eq!(cache.stats.hybrid_exact_groups, 1);
                    assert_eq!(out.buffers.vertices, vertices);
                    assert_eq!(out.buffers.indices, indices);
                }
            }
        }
    }

    #[test]
    fn hybrid_representations_share_entry_and_byte_budgets() {
        for policy in [
            Policy::Hybrid,
            Policy::HybridOutlines,
            Policy::HybridPaced,
            Policy::HybridMemoized,
            Policy::HybridScan,
            Policy::HybridRanges,
        ] {
            let mut p = placement(source(RenderColor::RED));
            let mut out = MeshBuilder::default();
            let mut cache = Cache::default();
            for _ in 0..2 {
                cache.begin_frame();
                cache
                    .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                    .unwrap();
            }
            let combined_bytes = cache.stats.bytes;
            assert_eq!(cache.stats.entries, 2);
            for (max_entries, max_bytes) in [(1, MAX_BYTES), (MAX_ENTRIES, combined_bytes - 1)] {
                cache = Cache {
                    max_entries,
                    max_bytes,
                    ..Default::default()
                };
                for _ in 0..2 {
                    cache.begin_frame();
                    cache
                        .rebuild(&scene(&p), std::slice::from_ref(&p), &mut out, policy)
                        .unwrap();
                    assert!(cache.stats.bytes <= max_bytes);
                    assert!(cache.stats.entries <= max_entries);
                }
                assert_eq!(cache.stats.evictions, 1);
                assert_eq!(cache.stats.promotions, 1);
            }
            cache = Cache {
                max_bytes: 0,
                ..Default::default()
            };
            let mut ordinary = MeshBuilder::default();
            // Even rejected templates/exact representations render the ordinary scene.
            for _ in 0..2 {
                cache.begin_frame();
                p.translation.x += 0.37;
                let s = scene(&p);
                cache
                    .rebuild(&s, std::slice::from_ref(&p), &mut out, policy)
                    .unwrap();
                ordinary.rebuild(&s).unwrap();
                assert_eq!(cache.stats.hybrid_fallback_groups, 1);
                assert_eq!(cache.stats.bytes, 0);
                assert_eq!(out.buffers.vertices, ordinary.buffers.vertices);
                assert_eq!(out.buffers.indices, ordinary.buffers.indices);
            }
        }
    }

    #[test]
    fn switching_outline_policy_cannot_reuse_a_different_acceptance_domain() {
        let definition = Arc::new(
            Definition::new(vec![RenderPolygon {
                points: vec![
                    RenderPoint::new(0., 0.),
                    RenderPoint::new(4., 0.),
                    RenderPoint::new(4., 1.),
                    RenderPoint::new(0., 1.),
                ],
                fill: Some(Fill::new(RenderColor::RED)),
                stroke: Some(Stroke::new(RenderColor::WHITE, 2.)),
            }])
            .unwrap(),
        );
        let mut p = placement(definition);
        p.scale = 0.75;
        let mut cache = Cache::default();
        let mut output = MeshBuilder::default();
        for policy in [Policy::Hybrid, Policy::HybridOutlines, Policy::Hybrid] {
            cache.begin_frame();
            cache
                .rebuild(&scene(&p), std::slice::from_ref(&p), &mut output, policy)
                .unwrap();
            assert_eq!(cache.stats.entries, 1);
            assert_eq!(cache.stats.admissions, 1);
            assert_eq!(cache.stats.hits, 0);
            assert_eq!(cache.stats.zoom_reused_fills, 1);
            assert_eq!(
                cache.stats.zoom_reused_strokes,
                u64::from(policy == Policy::HybridOutlines)
            );
            assert_eq!(
                cache.stats.zoom_fallback_polygons,
                u64::from(policy == Policy::Hybrid)
            );
            // Stable-scale promotion must not retain the previous policy's
            // already-expanded mesh either.
            cache.begin_frame();
            cache
                .rebuild(&scene(&p), std::slice::from_ref(&p), &mut output, policy)
                .unwrap();
            assert_eq!(cache.stats.promotions, 1);
            assert_eq!(cache.stats.entries, 2);
        }
    }

    #[test]
    fn paced_admission_budget_is_shared_by_views_and_recovers_without_stale_art() {
        let mut a = placement(source(RenderColor::RED));
        let mut b = placement(source(RenderColor::BLUE));
        // One four-edge polygon per frame. No assertions depend on elapsed time.
        let mut cache = Cache {
            work_limit: 16,
            ..Default::default()
        };
        let mut out = MeshBuilder::default();
        cache.begin_frame();
        cache
            .rebuild(
                &scene(&a),
                std::slice::from_ref(&a),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.admissions, 1);
        assert_eq!(cache.stats.build_work, 16);
        cache
            .rebuild(
                &scene(&b),
                std::slice::from_ref(&b),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.deferred, 1);
        assert_eq!(cache.stats.admissions, 0);
        let mut reference = MeshBuilder::default();
        reference.rebuild(&scene(&b)).unwrap();
        assert_eq!(out.buffers.vertices, reference.buffers.vertices);
        assert_eq!(out.buffers.indices, reference.buffers.indices);
        // Changing zoom keeps the resident template useful without spending
        // the next frame's budget; the formerly deferred group is then admitted.
        cache.begin_frame();
        a.scale += 0.1;
        b.scale += 0.1;
        cache
            .rebuild(
                &scene(&a),
                std::slice::from_ref(&a),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.hits, 1);
        assert_eq!(cache.stats.build_work, 0);
        cache
            .rebuild(
                &scene(&b),
                std::slice::from_ref(&b),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.admissions, 1);
        // Replacement while the budget is spent must draw new authoritative
        // geometry, never the old cached material or a queued stale result.
        b.definition = source(RenderColor::GREEN);
        cache
            .rebuild(
                &scene(&b),
                std::slice::from_ref(&b),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.admissions, 0);
        reference.rebuild(&scene(&b)).unwrap();
        assert_eq!(out.buffers.vertices, reference.buffers.vertices);
    }

    #[test]
    fn paced_oversized_group_makes_progress_once_and_counters_accumulate() {
        let p = placement(source(RenderColor::RED));
        let mut cache = Cache {
            work_limit: 1,
            ..Default::default()
        };
        let mut out = MeshBuilder::default();
        cache.begin_frame();
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.oversized, 1);
        assert_eq!(cache.stats.admissions, 1);
        let first = cache.stats;
        cache.begin_frame();
        cache
            .rebuild(
                &scene(&p),
                std::slice::from_ref(&p),
                &mut out,
                Policy::HybridPaced,
            )
            .unwrap();
        assert_eq!(cache.stats.promotions, 1);
        assert_eq!(cache.stats.oversized, 1);
        let mut sum = first;
        sum.accumulate(cache.stats);
        assert_eq!(sum.build_work, 32);
        assert_eq!(sum.oversized, 2);
        assert_eq!(sum.build_ns, first.build_ns + cache.stats.build_ns);
        assert_eq!(
            sum.max_build_ns,
            first.max_build_ns.max(cache.stats.max_build_ns)
        );
        // Exact hit is free even if the nominal limit is disabled afterward.
        cache.work_limit = 0;
        cache.begin_frame();
        cache
            .rebuild(&scene(&p), &[p], &mut out, Policy::HybridPaced)
            .unwrap();
        assert_eq!(cache.stats.hits, 1);
        assert_eq!(cache.stats.build_work, 0);
    }
}
