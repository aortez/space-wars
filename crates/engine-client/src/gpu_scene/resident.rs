//! Ordered vertex references into a bounded, GPU-resident geometry atlas.
//! The CPU mirror is changed only on admission; frame data contains transforms
//! and ordinary/guarded geometry, never transformed copies of cached vertices.
use super::{
    mesh::{MAX_INDICES, MAX_VERTICES, MeshBuilder},
    retained::Placement,
};
use std::{collections::BTreeMap, ops::Range};

pub(super) type Vertex = [f32; 8];
pub(super) const ATLAS_VERTICES: usize = 16 * 1024 * 1024 / 32;
pub(super) const ROW_VERTICES: usize = 512;
pub(super) const STREAM: u32 = 1 << 31;
const MAX_ENTRIES: usize = 128;
pub(super) const MAX_PLACEMENTS: usize = 4096;
pub(super) const TRANSFORM_BITS: u32 = 20;
pub(super) const TRANSFORM_ADDRESS: u32 = (1 << TRANSFORM_BITS) - 1;
pub(super) const UNIFORM_PLACEMENTS: usize = 512;

struct Entry {
    range: Range<usize>,
    used: u64,
    position: f64,
    offset: f64,
}

pub(super) struct Atlas {
    pub vertices: Vec<Vertex>,
    pub dirty: Vec<Range<usize>>,
    entries: BTreeMap<u64, Entry>,
    free: Vec<Range<usize>>,
    frame: u64,
    limit: usize,
}
impl Default for Atlas {
    fn default() -> Self {
        Self::new(ATLAS_VERTICES)
    }
}
impl Atlas {
    fn new(limit: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(limit),
            dirty: Vec::new(),
            entries: BTreeMap::new(),
            free: std::iter::once(0..limit).collect(),
            frame: 0,
            limit,
        }
    }

    fn admit(
        &mut self,
        id: u64,
        vertices: impl ExactSizeIterator<Item = Vertex>,
    ) -> Result<Option<usize>, String> {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.used = self.frame;
            return Ok(Some(entry.range.start));
        }
        let count = vertices.len();
        if count == 0 || count > self.limit {
            return Ok(None);
        }
        let slot = loop {
            if self.entries.len() < MAX_ENTRIES
                && let Some(slot) = self.free.iter().position(|r| r.len() >= count)
            {
                break slot;
            }
            let victim = self
                .entries
                .iter()
                .filter(|(_, e)| e.used != self.frame)
                .min_by_key(|(_, e)| e.used)
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                return Ok(None);
            };
            let removed = self.entries.remove(&victim).unwrap();
            self.free.push(removed.range);
            self.free.sort_unstable_by_key(|r| r.start);
            let mut merged: Vec<Range<usize>> = Vec::with_capacity(self.free.len());
            for range in self.free.drain(..) {
                if let Some(last) = merged.last_mut()
                    && last.end == range.start
                {
                    last.end = range.end;
                    continue;
                }
                merged.push(range);
            }
            self.free = merged;
        };
        let start = self.free[slot].start;
        let end = start + count;
        // Round the CPU high-water mark so row uploads never read padding from
        // uninitialized memory. The production limit is an exact row multiple.
        let high = end
            .div_ceil(ROW_VERTICES)
            .saturating_mul(ROW_VERTICES)
            .min(self.limit);
        if high > self.vertices.len() {
            self.vertices.resize(high, [0.; 8]);
        }
        let (mut position, mut offset) = (0.0_f64, 0.0_f64);
        for (destination, vertex) in self.vertices[start..end].iter_mut().zip(vertices) {
            if !vertex.iter().all(|v| v.is_finite()) {
                return Err("non-finite resident vertex".into());
            }
            position = position.max(f64::from(vertex[0].abs().max(vertex[1].abs())));
            offset = offset.max(f64::from(vertex[2].abs().max(vertex[3].abs())));
            *destination = vertex;
        }
        // Only consume the range after validation; a rejected admission must
        // not leak atlas space. Previously evicted entries remain safely absent.
        self.free[slot].start = end;
        if self.free[slot].is_empty() {
            self.free.remove(slot);
        }
        self.entries.insert(
            id,
            Entry {
                range: start..end,
                used: self.frame,
                position,
                offset,
            },
        );
        self.dirty.push(start..end);
        // Preparation may be abandoned without submission. Keep pending dirt
        // bounded by the atlas's row count even across arbitrarily many edits.
        self.dirty = self
            .dirty_rows()
            .into_iter()
            .map(|r| r.start * ROW_VERTICES..(r.end * ROW_VERTICES).min(self.limit))
            .collect();
        Ok(Some(start))
    }

    pub fn entries(&self) -> usize {
        self.entries.len()
    }
    pub fn payload_bytes(&self) -> usize {
        self.entries.values().map(|e| e.range.len() * 32).sum()
    }

    pub fn dirty_rows(&self) -> Vec<Range<usize>> {
        let mut rows: Vec<_> = self
            .dirty
            .iter()
            .map(|r| r.start / ROW_VERTICES..r.end.div_ceil(ROW_VERTICES))
            .collect();
        rows.sort_unstable_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in rows {
            if let Some(last) = merged.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
                continue;
            }
            merged.push(range);
        }
        merged
    }
}

#[derive(Clone, Copy)]
pub(super) struct Location {
    base: usize,
    transform: u32,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Stats {
    pub cached_vertices: usize,
    pub streamed_vertices: usize,
    pub placements: usize,
    pub atlas_entries: usize,
    pub atlas_payload_bytes: usize,
    pub atlas_upload_bytes: usize,
    pub fallback_groups: u64,
}
impl Stats {
    pub fn accumulate(&mut self, other: Self) {
        self.cached_vertices += other.cached_vertices;
        self.streamed_vertices += other.streamed_vertices;
        self.placements += other.placements;
        self.atlas_entries = other.atlas_entries;
        self.atlas_payload_bytes = other.atlas_payload_bytes;
        self.atlas_upload_bytes += other.atlas_upload_bytes;
        self.fallback_groups += other.fallback_groups;
    }
}

#[derive(Default)]
pub(crate) struct Frame {
    pub(super) atlas: Atlas,
    pub(super) references: Vec<[u32; 2]>,
    pub(super) indices: Vec<u32>,
    pub(super) data: Vec<Vertex>,
    pub(super) transforms: Vec<Vertex>,
    stream_vertices: usize,
    stream_indices: usize,
    placements: usize,
    pub fallback_groups: u64,
}
impl Frame {
    #[cfg(test)]
    pub fn with_capacity(limit: usize) -> Self {
        Self {
            atlas: Atlas::new(limit),
            ..Default::default()
        }
    }
    pub fn uploaded(&mut self) {
        self.atlas.dirty.clear();
    }
    pub fn stats(&self) -> Stats {
        Stats {
            cached_vertices: self.references.len() - self.stream_vertices,
            streamed_vertices: self.stream_vertices,
            placements: self.placements,
            atlas_entries: self.atlas.entries(),
            atlas_payload_bytes: self.atlas.payload_bytes(),
            atlas_upload_bytes: self
                .atlas
                .dirty_rows()
                .iter()
                .map(|r| r.len() * ROW_VERTICES * 32)
                .sum(),
            fallback_groups: self.fallback_groups,
        }
    }
    pub fn begin_frame(&mut self) {
        self.atlas.frame += 1;
    }
    pub fn begin_pass(&mut self) {
        self.references.clear();
        self.indices.clear();
        self.data.clear();
        self.transforms.clear();
        self.stream_vertices = 0;
        self.stream_indices = 0;
        self.placements = 0;
        self.fallback_groups = 0;
    }

    pub(super) fn place(
        &mut self,
        id: u64,
        vertices: impl ExactSizeIterator<Item = Vertex>,
        p: &Placement,
        scale: f32,
    ) -> Result<Option<Location>, String> {
        if self.placements == MAX_PLACEMENTS {
            self.fallback_groups += 1;
            return Ok(None);
        }
        let Some(base) = self.atlas.admit(id, vertices)? else {
            self.fallback_groups += 1;
            return Ok(None);
        };
        let e = &self.atlas.entries[&id];
        // Conservative bound on every intermediate operation, not just the
        // final point. Extreme transforms use the existing CPU/finite-check path.
        let bound = 2. * (e.position * f64::from(scale) + e.offset)
            + f64::from(p.translation.x.abs().max(p.translation.y.abs()));
        if !p.valid() || !scale.is_finite() || bound > f64::from(f32::MAX) * 0.5 {
            self.fallback_groups += 1;
            return Ok(None);
        }
        let (s, c) = p.angle.sin_cos();
        // 20 bits address at most 1,000,000 streamed vertices + 4096 poses;
        // the remaining 12 bits identify a compact placement-table entry.
        // All resident modes share these exact references and data records.
        let transform = self.data.len() as u32 | ((self.placements as u32) << TRANSFORM_BITS);
        let record = [scale, s, c, 0., p.translation.x, p.translation.y, 0., 0.];
        self.data.push(record);
        self.transforms.push(record);
        self.placements += 1;
        Ok(Some(Location { base, transform }))
    }

    fn budget(&self, vertices: usize, indices: usize) -> Result<(), String> {
        if self.references.len() + vertices > MAX_VERTICES
            || self.indices.len() + indices > MAX_INDICES
        {
            return Err("resident frame exceeds mesh budget".into());
        }
        Ok(())
    }

    pub(super) fn cached(
        &mut self,
        location: Location,
        vertices: Range<usize>,
        indices: &[u32],
        stream: &MeshBuilder,
    ) -> Result<(), String> {
        self.stream_pending(stream)?;
        self.budget(vertices.len(), indices.len())?;
        let base = self.references.len() as u32;
        self.references.extend(
            vertices
                .clone()
                .map(|v| [(location.base + v) as u32, location.transform]),
        );
        self.indices
            .extend(indices.iter().map(|i| base + i - vertices.start as u32));
        Ok(())
    }

    pub fn stream_pending(&mut self, stream: &MeshBuilder) -> Result<(), String> {
        let vertices = &stream.buffers.vertices[self.stream_vertices..];
        let indices = &stream.buffers.indices[self.stream_indices..];
        self.budget(vertices.len(), indices.len())?;
        let base = self.references.len() as u32;
        let data = self.data.len() as u32;
        self.data.extend(
            vertices
                .iter()
                .map(|v| [v[0], v[1], 0., 0., v[2], v[3], v[4], v[5]]),
        );
        self.references
            .extend((0..vertices.len()).map(|i| [STREAM | (data + i as u32), 0]));
        self.indices.extend(
            indices
                .iter()
                .map(|i| base + i - self.stream_vertices as u32),
        );
        self.stream_vertices = stream.buffers.vertices.len();
        self.stream_indices = stream.buffers.indices.len();
        Ok(())
    }

    /// A CPU reference for the shader's fetch/transform, used by unit and
    /// offscreen validation, never the resident rendering hot path.
    pub fn expand_reference(&self) -> Result<MeshBuilder, String> {
        let mut output = MeshBuilder::default();
        for &[address, transform] in &self.references {
            let v = if address & STREAM != 0 {
                *self
                    .data
                    .get((address & !STREAM) as usize)
                    .ok_or("invalid streamed vertex address")?
            } else {
                let v = self
                    .atlas
                    .vertices
                    .get(address as usize)
                    .ok_or("invalid atlas vertex address")?;
                let t = self
                    .data
                    .get((transform & TRANSFORM_ADDRESS) as usize)
                    .ok_or("invalid transform address")?;
                if self.transforms.get((transform >> TRANSFORM_BITS) as usize) != Some(t) {
                    return Err("invalid compact transform reference".into());
                }
                let x = v[0] * t[0] + v[2];
                let y = v[1] * t[0] + v[3];
                [
                    t[2] * x + t[1] * y + t[4],
                    -t[1] * x + t[2] * y + t[5],
                    0.,
                    0.,
                    v[4],
                    v[5],
                    v[6],
                    v[7],
                ]
            };
            output
                .buffers
                .vertices
                .push([v[0], v[1], v[4], v[5], v[6], v[7]]);
        }
        if self
            .indices
            .iter()
            .any(|i| *i as usize >= output.buffers.vertices.len())
        {
            return Err("invalid resident element address".into());
        }
        output.buffers.indices.clone_from(&self.indices);
        output.validate_bulk()?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Fill, PolygonGeometry, RenderColor, RenderPoint, RenderPolygon};
    use std::sync::Arc;

    fn placement() -> Placement {
        Placement {
            first: 0,
            scale: 1.,
            angle: 0.37,
            translation: RenderPoint::new(20., 30.),
            definition: Arc::new(
                PolygonGeometry::new(vec![RenderPolygon {
                    points: vec![
                        RenderPoint::ZERO,
                        RenderPoint::new(1., 0.),
                        RenderPoint::new(0., 1.),
                    ],
                    fill: Some(Fill::new(RenderColor::WHITE)),
                    stroke: None,
                }])
                .unwrap(),
            ),
        }
    }

    #[test]
    fn placements_are_bounded_and_extreme_transforms_fall_back() {
        assert!(MAX_VERTICES + MAX_PLACEMENTS <= TRANSFORM_ADDRESS as usize);
        assert!(MAX_PLACEMENTS <= 1 << (32 - TRANSFORM_BITS));
        let mut f = Frame::default();
        let p = placement();
        let v = [[f32::MAX * 0.9, 0., 0., 0., 1., 1., 1., 1.]];
        assert!(f.place(1, v.into_iter(), &p, 1.).unwrap().is_none());
        assert_eq!(f.fallback_groups, 1);
        assert!(f.data.is_empty());
        for _ in 0..MAX_PLACEMENTS {
            assert!(f.place(2, [[0.; 8]].into_iter(), &p, 1.).unwrap().is_some());
        }
        assert_eq!(f.transforms.len(), MAX_PLACEMENTS);
        assert!(f.place(2, [[0.; 8]].into_iter(), &p, 1.).unwrap().is_none());
        f.begin_pass();
        assert!(f.place(2, [[0.; 8]].into_iter(), &p, 1.).unwrap().is_some());
    }

    #[test]
    fn reference_validation_rejects_bad_addresses_and_nonfinite_transforms() {
        let mut f = Frame::default();
        f.references.push([STREAM, 0]);
        assert!(f.expand_reference().is_err());
        f.data.push([0.; 8]);
        f.indices.push(1);
        assert!(f.expand_reference().is_err());
        f.indices[0] = 0;
        assert!(f.expand_reference().is_ok());
        f.data[0][0] = f32::INFINITY;
        assert!(f.expand_reference().is_err());
    }
    #[test]
    fn atlas_pins_this_frame_and_reuses_only_unpinned_ranges() {
        let mut a = Atlas::new(12);
        let data = [1.; 8];
        assert_eq!(a.admit(1, [data; 6].into_iter()).unwrap(), Some(0));
        assert_eq!(a.admit(2, [data; 6].into_iter()).unwrap(), Some(6));
        assert_eq!(a.admit(3, [data; 6].into_iter()).unwrap(), None);
        a.frame += 1;
        assert_eq!(a.admit(2, [data; 6].into_iter()).unwrap(), Some(6));
        assert_eq!(a.admit(3, [[2.; 8]; 6].into_iter()).unwrap(), Some(0));
        assert_eq!(a.vertices[6], data);
        assert_eq!(a.vertices[0], [2.; 8]);
        assert_eq!(a.vertices.len(), 12);
        a.frame += 1;
        assert_eq!(a.admit(4, [data; 12].into_iter()).unwrap(), Some(0));
        assert_eq!(a.entries(), 1);
    }

    #[test]
    fn atlas_rejects_nonfinite_and_oversized_inputs() {
        let mut a = Atlas::new(12);
        assert_eq!(a.admit(1, [[0.; 8]; 13].into_iter()).unwrap(), None);
        assert!(a.admit(2, [[f32::NAN; 8]; 3].into_iter()).is_err());
        assert_eq!(a.entries(), 0);
        assert_eq!(a.admit(3, [[0.; 8]; 12].into_iter()).unwrap(), Some(0));
    }

    #[test]
    fn abandoned_submissions_do_not_grow_dirty_metadata() {
        let mut a = Atlas::new(12);
        for id in 0..1000 {
            a.frame += 1;
            a.admit(id, [[0.; 8]; 4].into_iter()).unwrap();
            assert_eq!(a.dirty.len(), 1);
            assert_eq!(a.vertices.len(), 12);
            assert!(a.entries() <= 3);
        }
    }

    #[test]
    fn dirty_rows_merge_and_frame_budgets_remain_bounded() {
        let mut a = Atlas::default();
        a.admit(1, [[0.; 8]; 520].into_iter()).unwrap();
        a.admit(2, [[0.; 8]; 24].into_iter()).unwrap();
        assert_eq!(a.dirty_rows(), [0..2]);
        assert!(a.vertices.capacity() <= ATLAS_VERTICES);
        let f = Frame::default();
        assert!(f.budget(MAX_VERTICES, MAX_INDICES).is_ok());
        assert!(f.budget(MAX_VERTICES + 1, 0).is_err());
        assert!(f.budget(0, MAX_INDICES + 1).is_err());
    }
}
