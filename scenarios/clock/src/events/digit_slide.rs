//! A bounded presentation transition. Physical segments stay anchored to the
//! latest reading; only the old/new visible cells move inside their digit slots.
//! The old glyph's surfaces ride out with it and are retired as each cell
//! leaves its slot, and the new glyph is new surfaces from the start, even
//! where its cells overlap the old ones.

use engine_common::ClockDigitSlideState;
use engine_core::Vec2;

use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell,
    digit_surfaces::{
        Footprint, Glyph, Retirement, SlotIds, SurfaceId, SurfaceLedger, SurfacePose,
        SurfaceSource, bit,
    },
    layout::Layout,
};

pub const DIGIT_SLIDE_TICKS: u64 = 48;
/// A glyph is nine cells tall, and travels its own height in or out.
const TRAVEL: f32 = 9.0;

pub(crate) struct DigitSlideEvent {
    pub from_font: engine_common::ClockFont,
    pub to_font: engine_common::ClockFont,
    pub from: [Option<u8>; DIGIT_SLOT_COUNT],
    pub to: [Option<u8>; DIGIT_SLOT_COUNT],
    pub changed: [bool; DIGIT_SLOT_COUNT],
    pub tick: u64,
    preview: bool,
    /// The face's surfaces the old glyph carried, until each cell is out.
    outgoing: [SlotIds; DIGIT_SLOT_COUNT],
    incoming: [SlotIds; DIGIT_SLOT_COUNT],
}

impl DigitSlideEvent {
    pub fn new(
        from: Option<DisplaySnapshot>,
        to: DisplaySnapshot,
        surfaces: &mut SurfaceLedger,
    ) -> Self {
        let preview = from.is_none();
        let from_font = from.unwrap_or(to).font;
        let to_font = to.font;
        let from = from.unwrap_or(to).digits;
        let to = to.digits;
        let changed: [bool; DIGIT_SLOT_COUNT] = std::array::from_fn(|slot| {
            from[slot] != to[slot] || from_font != to_font || (preview && to[slot].is_some())
        });
        let mut outgoing = [[None; crate::digit_surfaces::GRID_CELLS]; DIGIT_SLOT_COUNT];
        let mut incoming = outgoing;
        for slot in (0..DIGIT_SLOT_COUNT).filter(|slot| changed[*slot]) {
            outgoing[slot] = surfaces.take(
                slot,
                Glyph::new(from_font, from[slot]),
                SurfaceSource::Slide,
            );
            for cell in Glyph::new(to_font, to[slot]).mask.cells() {
                incoming[slot][bit(cell)] =
                    Some(surfaces.admit(slot as u8, cell, SurfaceSource::Slide));
            }
        }
        Self {
            from_font,
            to_font,
            from,
            to,
            changed,
            tick: 0,
            preview,
            outgoing,
            incoming,
        }
    }

    /// Retires each outgoing cell once it has left its slot.
    pub fn step(&mut self, surfaces: &mut SurfaceLedger) -> bool {
        self.tick += 1;
        let [out, _] = self.offsets();
        for ids in &mut self.outgoing {
            for (bit, id) in ids.iter_mut().enumerate() {
                if id.is_some() && !visible(bit, out) {
                    surfaces.retire(id.take().unwrap(), Retirement::Replaced);
                }
            }
        }
        self.tick >= DIGIT_SLIDE_TICKS
    }

    pub fn progress(&self) -> f32 {
        let t = (self.tick as f32 / DIGIT_SLIDE_TICKS as f32).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// How far the outgoing and incoming glyphs sit above their places, in
    /// cells. The renderer and the surfaces both read this.
    pub fn offsets(&self) -> [f32; 2] {
        let progress = self.progress();
        [-progress * TRAVEL, (1.0 - progress) * TRAVEL]
    }

    pub fn surfaces(&self, layout: Layout, out: &mut Vec<(SurfaceId, SurfacePose)>) {
        let [outgoing, incoming] = self.offsets();
        for (ids, offset, departing) in [
            (&self.outgoing, outgoing, true),
            (&self.incoming, incoming, false),
        ] {
            for (slot, ids) in ids.iter().enumerate() {
                for (bit, id) in ids.iter().enumerate() {
                    let Some(id) = *id else {
                        continue;
                    };
                    let cell = GridCell {
                        x: (bit % 6) as i8,
                        y: (bit / 6) as i8,
                    };
                    out.push((
                        id,
                        SurfacePose {
                            origin: layout.slot_cell_center(slot, cell)
                                + Vec2::new(0.0, offset * layout.pitch),
                            footprint: Footprint::CELL,
                            source: SurfaceSource::Slide,
                            departing,
                            clip: Some(layout.digit_band()),
                            ..SurfacePose::anchored(layout, slot, cell)
                        },
                    ));
                }
            }
        }
    }

    /// Gives the changed slots back to the face. A finished slide has put the
    /// new glyph in place; one cut short leaves it part way, so its surfaces
    /// are retired rather than snapped home with whatever they carry.
    pub fn release_surfaces(&mut self, surfaces: &mut SurfaceLedger) {
        let settled = self.tick >= DIGIT_SLIDE_TICKS;
        for slot in (0..DIGIT_SLOT_COUNT).filter(|slot| self.changed[*slot]) {
            surfaces.retire_all(
                self.outgoing[slot].iter_mut().filter_map(Option::take),
                Retirement::Replaced,
            );
            let mut incoming = std::mem::replace(
                &mut self.incoming[slot],
                [None; crate::digit_surfaces::GRID_CELLS],
            );
            if !settled {
                surfaces.retire_all(
                    incoming.iter_mut().filter_map(Option::take),
                    Retirement::Interrupted,
                );
            }
            surfaces.hand_back(slot, Glyph::new(self.to_font, self.to[slot]), incoming);
        }
    }

    pub fn diagnostics(&self) -> ClockDigitSlideState {
        ClockDigitSlideState {
            from_digits: self.from,
            to_digits: self.to,
            changed_slots: self.changed,
            progress_milli: (self.progress() * 1000.0).round() as u32,
            preview: self.preview,
        }
    }
}

/// Whether a cell still shows inside its slot, `offset` cells from its place.
fn visible(bit: usize, offset: f32) -> bool {
    let centre = (bit / 6) as f32 + 0.5 + offset;
    centre + crate::digit_surfaces::CELL_HALF > 0.0
        && centre - crate::digit_surfaces::CELL_HALF < TRAVEL
}
