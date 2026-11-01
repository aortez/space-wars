//! Identity, lifetime and geometry for the lit cells of the four digits, so a
//! resident attached to a digit can follow the piece it is attached to. One
//! surface is one lit glyph cell. The colon only blinks in place and the AM/PM
//! pixels are too small to carry anything, so neither is a surface.
//!
//! Identity is an admission serial that a run never reuses. Where a surface was
//! admitted is provenance, not identity: a swapped cell keeps its identity at a
//! new place, and a replaced glyph's cells are new surfaces even where they
//! overlap the old ones. A surface is admitted once, handed off between
//! geometry sources any number of times, departs at most once and is retired
//! once with a reason. Nothing comes back from departing or retired.
//!
//! Publication is a read of the state that also draws the face, made at the
//! end of every step and every resize and kept until the next: the face table
//! for slots no event holds, and the event's own pieces for slots it does.
//! Rendering never changes it. The ledger records admissions and retirements as
//! they happen and finds hand-offs and departures by comparing each
//! publication with the last, so a consumer reading after a step, or after a
//! resize, never sees a surface in a state the ledger has not recorded.

use std::{
    collections::{BTreeMap, VecDeque},
    num::NonZeroU32,
};

use engine_common::{
    ClockDigitSurface, ClockDigitSurfaceState, ClockFont, ClockSurfaceAccounting,
    ClockSurfaceChange, ClockSurfaceChangeKind, ClockSurfaceLimits, ClockSurfaceRetirements,
    ClockSurfaceSourceCount,
};
pub use engine_common::{
    ClockSurfaceRetirement as Retirement, ClockSurfaceSource as SurfaceSource,
};
use engine_core::Vec2;
use engine_rapier::world::BodyId;

use crate::{
    ClockState, DIGIT_SLOT_COUNT, GridCell, SegmentRepresentation, SegmentState, fonts::CellMask,
    layout::Layout,
};

/// Half the side of a lit cell in grid units. Every face renderer draws one as
/// a square of 0.8 pitch about its centre.
pub const CELL_HALF: f32 = 0.4;
/// Surfaces alive at once: an outgoing and an incoming glyph in every slot, and
/// room for the departing cells a retargeted cross-fade can stack on a cell.
pub const MAX_SURFACES: usize = 3 * crate::fonts::MAX_DIGIT_CELLS;
/// Lifecycle changes kept for consumers that read less often than every step.
/// No step is bound to fit: one that ends an event and changes the reading,
/// font and format at once records several changes for some surfaces. A
/// consumer that falls behind sees a gap and catches up from the publication.
pub const MAX_RETAINED_CHANGES: usize = 2 * MAX_SURFACES;
/// The most surfaces the diagnostics sample, one for each slot, source and
/// departure.
pub const MAX_SAMPLED_SURFACES: usize = 16;
/// How many of the latest changes the diagnostics show.
pub const MAX_RECENT_CHANGES: usize = 16;
pub(crate) const GRID_CELLS: usize = 54;

/// A surface's identity: the serial it was admitted with, which a run never
/// reuses. A restart is a new run, with a new ledger, and carries nothing over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(NonZeroU32);

impl SurfaceId {
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// The supporting rectangle in a surface's own frame, in grid units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub min: Vec2,
    pub max: Vec2,
}

impl Footprint {
    /// A whole lit cell.
    pub const CELL: Self = Self::square(CELL_HALF);

    pub const fn square(half: f32) -> Self {
        Self {
            min: Vec2::new(-half, -half),
            max: Vec2::new(half, half),
        }
    }

    pub fn contains(self, point: Vec2) -> bool {
        (self.min.x..=self.max.x).contains(&point.x) && (self.min.y..=self.max.y).contains(&point.y)
    }

    pub fn is_empty(self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }
}

/// One surface as it stands after a step. The frame maps a local point `p` in
/// grid units to `origin + rotate(p * scale, angle)`, so a local point survives
/// a resize, while a shrinking or growing footprint changes which local points
/// are supported rather than moving them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DigitSurface {
    pub id: SurfaceId,
    /// The digit slot it was admitted in.
    pub slot: u8,
    /// The cell it was admitted at.
    pub cell: GridCell,
    pub origin: Vec2,
    pub angle: f32,
    /// World units per grid unit.
    pub scale: f32,
    pub footprint: Footprint,
    pub source: SurfaceSource,
    /// The canonical rigid body carrying it, while one does.
    pub body: Option<BodyId>,
    pub departing: bool,
    /// Presentation only: how much of the surface's ink is drawn. A surface
    /// can be invisible and still exist, and several surfaces sharing one
    /// drawn cell sum to its ink.
    pub opacity: f32,
    /// Presentation only: the world band of y it is drawn within.
    pub clip: Option<[f32; 2]>,
}

/// Where an event or the face has a surface this step, and how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SurfacePose {
    pub origin: Vec2,
    pub angle: f32,
    pub scale: f32,
    pub footprint: Footprint,
    pub source: SurfaceSource,
    pub body: Option<BodyId>,
    pub departing: bool,
    pub opacity: f32,
    pub clip: Option<[f32; 2]>,
}

impl SurfacePose {
    /// A whole cell resting at its place in the face.
    pub fn anchored(layout: Layout, slot: usize, cell: GridCell) -> Self {
        Self {
            origin: layout.slot_cell_center(slot, cell),
            angle: 0.0,
            scale: layout.pitch,
            footprint: Footprint::CELL,
            source: SurfaceSource::Anchored,
            body: None,
            departing: false,
            opacity: 1.0,
            clip: None,
        }
    }
}

impl DigitSurface {
    pub(crate) fn new(id: SurfaceId, slot: u8, cell: GridCell, pose: SurfacePose) -> Self {
        let SurfacePose {
            origin,
            angle,
            scale,
            footprint,
            source,
            body,
            departing,
            opacity,
            clip,
        } = pose;
        Self {
            id,
            slot,
            cell,
            origin,
            angle,
            scale,
            footprint,
            source,
            body,
            departing,
            opacity,
            clip,
        }
    }

    pub fn world(&self, local: Vec2) -> Vec2 {
        self.origin + (local * self.scale).rotate_radians(self.angle)
    }

    pub fn local(&self, world: Vec2) -> Vec2 {
        (world - self.origin).rotate_radians(-self.angle) / self.scale
    }

    pub fn supports(&self, local: Vec2) -> bool {
        self.footprint.contains(local)
    }

    /// Whether something new may attach. A departing surface still carries what
    /// is already on it until it is retired.
    pub fn eligible(&self) -> bool {
        !self.departing && !self.footprint.is_empty()
    }

    pub fn corners(&self) -> [Vec2; 4] {
        let Footprint { min, max } = self.footprint;
        [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)].map(|local| self.world(local))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceChange {
    /// Counts every change the run has recorded, so a consumer that reads less
    /// often than the ledger retains can tell it missed some.
    pub sequence: u64,
    pub tick: u64,
    pub id: SurfaceId,
    pub kind: SurfaceChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceChangeKind {
    Admitted {
        slot: u8,
        cell: GridCell,
        source: SurfaceSource,
    },
    HandedOff {
        from: SurfaceSource,
        to: SurfaceSource,
    },
    Departing,
    Retired(Retirement),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceAccounting {
    pub admitted: u64,
    pub handed_off: u64,
    pub departed: u64,
    /// Indexed like `Retirement::ALL`.
    pub retired: [u64; Retirement::ALL.len()],
    pub live: usize,
    pub peak_live: usize,
    /// The last change's sequence number.
    pub sequence: u64,
}

impl SurfaceAccounting {
    pub fn retired(&self, reason: Retirement) -> u64 {
        self.retired[reason as usize]
    }

    pub fn total_retired(&self) -> u64 {
        self.retired.iter().sum()
    }
}

/// The lit cells of one slot as a font draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glyph {
    pub font: ClockFont,
    pub mask: CellMask,
}

impl Glyph {
    pub fn new(font: ClockFont, digit: Option<u8>) -> Self {
        Self {
            font,
            mask: crate::fonts::glyph(font, digit),
        }
    }

    /// What the anchored face draws in a slot.
    pub fn anchored(segments: &[SegmentState], slot: usize) -> Self {
        Self::lit_where(segments, slot, |segment| {
            segment.representation == SegmentRepresentation::Anchored
        })
    }

    /// The lit cells of a slot, wherever its segments are.
    pub fn lit(segments: &[SegmentState], slot: usize) -> Self {
        Self::lit_where(segments, slot, |_| true)
    }

    fn lit_where(
        segments: &[SegmentState],
        slot: usize,
        drawn: impl Fn(&SegmentState) -> bool,
    ) -> Self {
        let mut glyph = Self {
            font: ClockFont::Classic,
            mask: CellMask(0),
        };
        for segment in segments
            .iter()
            .filter(|segment| usize::from(segment.id.digit_slot) == slot)
        {
            glyph.font = segment.font;
            if segment.lit && drawn(segment) {
                glyph.mask.0 |= segment.shape.0;
            }
        }
        glyph
    }

    /// The cells of `ids`, in `font`.
    pub fn of(font: ClockFont, ids: &SlotIds) -> Self {
        let mut mask = CellMask(0);
        for (bit, id) in ids.iter().enumerate() {
            if id.is_some() {
                mask.0 |= 1 << bit;
            }
        }
        Self { font, mask }
    }
}

pub(crate) fn bit(cell: GridCell) -> usize {
    cell.y as usize * 6 + cell.x as usize
}

/// Surface identities by grid bit, for one slot.
pub(crate) type SlotIds = [Option<SurfaceId>; GRID_CELLS];

#[derive(Debug, Clone, Copy)]
struct FaceSlot {
    /// The glyph `ids` belong to. None after an event released the slot with
    /// nothing settled, so the next reconciliation admits the face afresh.
    glyph: Option<Glyph>,
    held: bool,
    ids: SlotIds,
}

#[derive(Debug, Clone, Copy)]
struct Record {
    slot: u8,
    cell: GridCell,
    source: SurfaceSource,
    departing: bool,
}

/// Who exists, and what has happened to them. Poses are not stored here; they
/// are read from the state each time they are published.
pub(crate) struct SurfaceLedger {
    next: u32,
    tick: u64,
    face: [FaceSlot; DIGIT_SLOT_COUNT],
    live: BTreeMap<SurfaceId, Record>,
    changes: VecDeque<SurfaceChange>,
    accounting: SurfaceAccounting,
}

impl SurfaceLedger {
    pub fn new() -> Self {
        Self {
            next: 0,
            tick: 0,
            face: [FaceSlot {
                glyph: None,
                held: false,
                ids: [None; GRID_CELLS],
            }; DIGIT_SLOT_COUNT],
            live: BTreeMap::new(),
            changes: VecDeque::with_capacity(MAX_RETAINED_CHANGES),
            accounting: SurfaceAccounting::default(),
        }
    }

    /// The simulation tick the following changes belong to.
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    pub fn admit(&mut self, slot: u8, cell: GridCell, source: SurfaceSource) -> SurfaceId {
        self.next += 1;
        let id = SurfaceId(NonZeroU32::new(self.next).expect("a run admits fewer than 2^32"));
        self.live.insert(
            id,
            Record {
                slot,
                cell,
                source,
                departing: false,
            },
        );
        debug_assert!(self.live.len() <= MAX_SURFACES, "surfaces stay bounded");
        self.accounting.admitted += 1;
        self.accounting.live = self.live.len();
        self.accounting.peak_live = self.accounting.peak_live.max(self.live.len());
        self.record(id, SurfaceChangeKind::Admitted { slot, cell, source });
        id
    }

    pub fn retire(&mut self, id: SurfaceId, reason: Retirement) {
        let removed = self.live.remove(&id);
        debug_assert!(removed.is_some(), "only a live surface retires");
        if removed.is_none() {
            return;
        }
        self.accounting.retired[reason as usize] += 1;
        self.accounting.live = self.live.len();
        self.record(id, SurfaceChangeKind::Retired(reason));
    }

    pub fn retire_all(&mut self, ids: impl IntoIterator<Item = SurfaceId>, reason: Retirement) {
        for id in ids {
            self.retire(id, reason);
        }
    }

    fn record(&mut self, id: SurfaceId, kind: SurfaceChangeKind) {
        self.accounting.sequence += 1;
        if self.changes.len() == MAX_RETAINED_CHANGES {
            self.changes.pop_front();
        }
        self.changes.push_back(SurfaceChange {
            sequence: self.accounting.sequence,
            tick: self.tick,
            id,
            kind,
        });
    }

    /// Hands one slot's face to an event that will draw `glyph` there. The
    /// face's own surfaces carry on when they are that glyph. Otherwise they
    /// are replaced, and so are any cells the face has no surface for yet.
    pub fn take(&mut self, slot: usize, glyph: Glyph, source: SurfaceSource) -> SlotIds {
        let face = self.face[slot];
        debug_assert!(!face.held, "one event holds a slot at a time");
        self.face[slot] = FaceSlot {
            glyph: None,
            held: true,
            ids: [None; GRID_CELLS],
        };
        let mut ids = [None; GRID_CELLS];
        let keep = face.glyph == Some(glyph);
        for (bit, id) in face.ids.into_iter().enumerate() {
            match id {
                Some(id) if keep => ids[bit] = Some(id),
                Some(id) => self.retire(id, Retirement::Replaced),
                None => (),
            }
        }
        for cell in glyph.mask.cells() {
            if ids[bit(cell)].is_none() {
                ids[bit(cell)] = Some(self.admit(slot as u8, cell, source));
            }
        }
        ids
    }

    /// Gives a slot back to the face with the surfaces an event settled in it,
    /// which have to be cells of `glyph`. The face admits the rest of whatever
    /// it draws there at the end of the step.
    pub fn hand_back(&mut self, slot: usize, glyph: Glyph, ids: SlotIds) {
        debug_assert!(self.face[slot].held, "only a held slot comes back");
        debug_assert!(
            ids.iter()
                .enumerate()
                .all(|(bit, id)| id.is_none() || glyph.mask.0 & (1 << bit) != 0),
            "a settled surface is a cell of the glyph it settled in"
        );
        self.face[slot] = FaceSlot {
            glyph: Some(glyph),
            held: false,
            ids,
        };
    }

    /// Gives a slot back with nothing settled in it.
    pub fn release(&mut self, slot: usize) {
        debug_assert!(self.face[slot].held, "only a held slot comes back");
        self.face[slot] = FaceSlot {
            glyph: None,
            held: false,
            ids: [None; GRID_CELLS],
        };
    }

    /// Retires the face's own surfaces in a slot no event holds, so the face
    /// admits whatever it draws there afresh at the end of the step.
    pub fn retire_face(&mut self, slot: usize, reason: Retirement) {
        debug_assert!(!self.face[slot].held, "an event's pieces are its own");
        let ids = std::mem::replace(&mut self.face[slot].ids, [None; GRID_CELLS]);
        self.retire_all(ids.into_iter().flatten(), reason);
    }

    pub fn held(&self, slot: usize) -> bool {
        self.face[slot].held
    }

    /// Brings the slots no event holds up to what the face draws: a changed
    /// glyph replaces every surface in its slot, and a cell without a surface
    /// gets a new one. Seconds, the colon and colours change none of this.
    pub fn reconcile(&mut self, segments: &[SegmentState], source: SurfaceSource) {
        for slot in 0..DIGIT_SLOT_COUNT {
            if self.face[slot].held {
                continue;
            }
            let glyph = Glyph::anchored(segments, slot);
            if self.face[slot].glyph != Some(glyph) {
                let ids = std::mem::replace(&mut self.face[slot].ids, [None; GRID_CELLS]);
                self.retire_all(ids.into_iter().flatten(), Retirement::Replaced);
                self.face[slot].glyph = Some(glyph);
            }
            for cell in glyph.mask.cells() {
                if self.face[slot].ids[bit(cell)].is_none() {
                    let id = self.admit(slot as u8, cell, source);
                    self.face[slot].ids[bit(cell)] = Some(id);
                }
            }
        }
    }

    /// The face's own surfaces in a slot no event holds, in cell order.
    pub fn face(&self, slot: usize) -> impl Iterator<Item = (GridCell, SurfaceId)> + '_ {
        let face = &self.face[slot];
        (!face.held).then_some(()).into_iter().flat_map(move |()| {
            face.ids.iter().enumerate().filter_map(|(bit, id)| {
                id.map(|id| {
                    (
                        GridCell {
                            x: (bit % 6) as i8,
                            y: (bit / 6) as i8,
                        },
                        id,
                    )
                })
            })
        })
    }

    /// Records the hand-offs and departures one step's publication shows, so
    /// that every change is on the ledger before anything reads the step.
    pub fn observe(&mut self, surfaces: &[DigitSurface]) {
        if cfg!(debug_assertions) {
            let mut published: Vec<_> = surfaces.iter().map(|surface| surface.id).collect();
            published.sort_unstable();
            let live: Vec<_> = self.live.keys().copied().collect();
            assert_eq!(
                published, live,
                "every live surface is published exactly once"
            );
        }
        for surface in surfaces {
            let Some(record) = self.live.get_mut(&surface.id) else {
                continue;
            };
            let Record {
                source, departing, ..
            } = *record;
            debug_assert!(
                !departing || surface.departing,
                "a departing surface never comes back"
            );
            record.source = surface.source;
            record.departing = departing || surface.departing;
            if source != surface.source {
                self.accounting.handed_off += 1;
                self.record(
                    surface.id,
                    SurfaceChangeKind::HandedOff {
                        from: source,
                        to: surface.source,
                    },
                );
            }
            if surface.departing && !departing {
                self.accounting.departed += 1;
                self.record(surface.id, SurfaceChangeKind::Departing);
            }
        }
    }

    /// Where a live surface was admitted.
    fn provenance(&self, id: SurfaceId) -> (u8, GridCell) {
        let record = self.live.get(&id);
        debug_assert!(record.is_some(), "only a live surface is published");
        record.map_or((0, GridCell { x: 0, y: 0 }), |record| {
            (record.slot, record.cell)
        })
    }

    pub fn changes(&self) -> impl ExactSizeIterator<Item = &SurfaceChange> + Clone {
        self.changes.iter()
    }

    pub fn accounting(&self) -> SurfaceAccounting {
        self.accounting
    }
}

/// Every surface in a fixed order: the slots no event holds, slot by slot in
/// cell order and posed wherever the active event carries the face, then the
/// active event's pieces in the event's own order.
pub(crate) fn publish(state: &ClockState) -> Vec<DigitSurface> {
    let layout = Layout::new(state.aspect_ratio());
    let mut poses = Vec::with_capacity(state.surfaces.accounting().live);
    for slot in 0..DIGIT_SLOT_COUNT {
        for (cell, id) in state.surfaces.face(slot) {
            let pose = match &state.active_event {
                Some(event) => event.face_pose(layout, slot, cell),
                None => SurfacePose::anchored(layout, slot, cell),
            };
            poses.push((id, pose));
        }
    }
    if let Some(event) = &state.active_event {
        event.surfaces(layout, &state.segments, &mut poses);
    }
    poses
        .into_iter()
        .map(|(id, pose)| {
            let (slot, cell) = state.surfaces.provenance(id);
            DigitSurface::new(id, slot, cell, pose)
        })
        .collect()
}

/// The surfaces as Clock's state publishes them: a bounded sample of those
/// alive now, the latest changes and the run's accounting.
pub(crate) fn diagnostics(state: &ClockState) -> ClockDigitSurfaceState {
    let published = &state.published;
    let accounting = state.surfaces.accounting();
    let sources = SurfaceSource::ALL
        .into_iter()
        .filter_map(|source| {
            let live = published.iter().filter(|s| s.source == source).count();
            (live > 0).then_some(ClockSurfaceSourceCount { source, live })
        })
        .collect();
    let mut sample: Vec<&DigitSurface> = Vec::new();
    for surface in published {
        let key = |s: &DigitSurface| (s.slot, s.source, s.departing);
        match sample.iter().position(|first| key(first) == key(surface)) {
            Some(index) if surface.id < sample[index].id => sample[index] = surface,
            Some(_) => {}
            None => sample.push(surface),
        }
    }
    let order = |surface: &&DigitSurface| {
        let source = SurfaceSource::ALL.iter().position(|s| *s == surface.source);
        (surface.slot, surface.departing, source)
    };
    sample.sort_by_key(order);
    sample.truncate(MAX_SAMPLED_SURFACES);
    let changes = state.surfaces.changes();
    let recent = changes
        .clone()
        .skip(changes.len().saturating_sub(MAX_RECENT_CHANGES))
        .map(SurfaceChange::diagnostics)
        .collect();
    ClockDigitSurfaceState {
        accounting: ClockSurfaceAccounting {
            live: accounting.live,
            departing: published.iter().filter(|s| s.departing).count(),
            peak_live: accounting.peak_live,
            admitted: accounting.admitted,
            handed_off: accounting.handed_off,
            departed: accounting.departed,
            retired: ClockSurfaceRetirements {
                replaced: accounting.retired(Retirement::Replaced),
                interrupted: accounting.retired(Retirement::Interrupted),
                merged_away: accounting.retired(Retirement::MergedAway),
                melted: accounting.retired(Retirement::Melted),
                exited: accounting.retired(Retirement::Exited),
                reclaimed: accounting.retired(Retirement::Reclaimed),
            },
            sequence: accounting.sequence,
        },
        sources,
        sample: sample.into_iter().map(DigitSurface::diagnostics).collect(),
        recent,
        limits: ClockSurfaceLimits {
            live: MAX_SURFACES,
            retained_changes: MAX_RETAINED_CHANGES,
            sample: MAX_SAMPLED_SURFACES,
            recent: MAX_RECENT_CHANGES,
        },
    }
}

/// World or grid units in thousandths.
fn milli(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

impl DigitSurface {
    fn diagnostics(&self) -> ClockDigitSurface {
        let Footprint { min, max } = self.footprint;
        ClockDigitSurface {
            id: self.id.get(),
            slot: self.slot,
            cell: [self.cell.x, self.cell.y],
            source: self.source,
            body: self.body.map(|body| body.entity.value()),
            origin_milli: [milli(self.origin.x), milli(self.origin.y)],
            angle_millidegrees: milli(self.angle.to_degrees()),
            scale_milli: milli(self.scale),
            footprint_milli: [milli(min.x), milli(min.y), milli(max.x), milli(max.y)],
            departing: self.departing,
            opacity_milli: milli(self.opacity.clamp(0.0, 1.0)) as u16,
            clip_milli: self.clip.map(|[low, high]| [milli(low), milli(high)]),
        }
    }
}

impl SurfaceChange {
    fn diagnostics(&self) -> ClockSurfaceChange {
        ClockSurfaceChange {
            sequence: self.sequence,
            tick: self.tick,
            id: self.id.get(),
            change: match self.kind {
                SurfaceChangeKind::Admitted { slot, cell, source } => {
                    ClockSurfaceChangeKind::Admitted {
                        slot,
                        cell: [cell.x, cell.y],
                        source,
                    }
                }
                SurfaceChangeKind::HandedOff { from, to } => {
                    ClockSurfaceChangeKind::HandedOff { from, to }
                }
                SurfaceChangeKind::Departing => ClockSurfaceChangeKind::Departing,
                SurfaceChangeKind::Retired(reason) => ClockSurfaceChangeKind::Retired { reason },
            },
        }
    }
}
