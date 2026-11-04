//! A permutation of occupied cells, followed only when needed by a short fade
//! to the latest reading. All storage and paths are bounded by the four glyphs.
//! Each exchange carries the face's own surface to the cell it lands on, and
//! the fade retires a surface it fades out and admits one it fades in.

use engine_common::{ClockCellSwapPattern, ClockCellSwapState};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell, SegmentKind,
    digit_surfaces::{
        GRID_CELLS, Glyph, Retirement, SlotIds, SurfaceId, SurfaceLedger, SurfacePose,
        SurfaceSource, bit,
    },
    fonts,
    layout::{DIGIT_ORIGINS, Layout},
};

pub const CELL_SWAP_TICKS: u64 = 60;
pub const CELL_SWAP_FADE_TICKS: u64 = 15;
/// Repeated corrections can extend reconciliation, but never beyond 1.5 s.
pub const CELL_SWAP_MAX_TICKS: u64 = CELL_SWAP_TICKS + 2 * CELL_SWAP_FADE_TICKS;
const CELLS_PER_DIGIT: usize = 54;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellPosition {
    pub slot: u8,
    pub cell: GridCell,
}

impl CellPosition {
    pub fn index(self) -> usize {
        usize::from(self.slot) * CELLS_PER_DIGIT + self.cell.y as usize * 6 + self.cell.x as usize
    }

    pub fn point(self) -> Vec2 {
        Vec2::new(
            DIGIT_ORIGINS[usize::from(self.slot)] + f32::from(self.cell.x) + 0.5,
            f32::from(self.cell.y) + 0.5,
        )
    }

    pub fn center(self, layout: Layout) -> Vec2 {
        let origin = layout.digit_origin(usize::from(self.slot));
        Vec2::new(
            origin.x + (f32::from(self.cell.x) + 0.5) * layout.pitch,
            origin.y + (f32::from(self.cell.y) + 0.5) * layout.pitch,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exchange {
    pub from: CellPosition,
    pub to: CellPosition,
}

impl Exchange {
    /// Opposing paths bow in opposite directions. A bounded control point keeps
    /// the complete curve inside the face, including at narrow aspect ratios.
    pub fn position(self, progress: f32) -> Vec2 {
        let from = self.from.point();
        let to = self.to.point();
        let delta = to - from;
        let length = delta.length();
        let bend = if length > 0.0 {
            Vec2::new(-delta.y, delta.x) * ((length * 0.16).min(1.5) / length)
        } else {
            Vec2::ZERO
        };
        let control = (from + to) * 0.5 + bend * 2.0;
        let control = Vec2::new(control.x.clamp(0.5, 29.5), control.y.clamp(0.5, 8.5));
        from * (1.0 - progress).powi(2)
            + control * (2.0 * progress * (1.0 - progress))
            + to * progress.powi(2)
    }

    /// Where the cell it carries is drawn, `progress` of the way along.
    pub fn centre(self, layout: Layout, progress: f32) -> Vec2 {
        self.from.center(layout) + (self.position(progress) - self.from.point()) * layout.pitch
    }
}

/// One lit cell the event carries. An exchange carries the face's own surface
/// to where it lands, and a fade the reading forces fades one surface out and
/// a new one in on a cell rather than handing one over.
#[derive(Debug, Clone, Copy)]
struct Piece {
    id: SurfaceId,
    /// Where it travels from and lands. A piece a fade brings in starts where
    /// it lands.
    path: Exchange,
    /// Its ink when the current fade opened, and whether it stays rather than
    /// fading out.
    from: f32,
    goal: bool,
}

impl Piece {
    fn ink(self, fade: f32) -> f32 {
        self.from + (f32::from(u8::from(self.goal)) - self.from) * fade
    }
}

pub(crate) struct CellSwapEvent {
    pub from: DisplaySnapshot,
    pub target: DisplaySnapshot,
    pub pattern: ClockCellSwapPattern,
    pub exchanges: Vec<Exchange>,
    pub tick: u64,
    /// Each cell's ink once the exchanges have landed: what its pieces add up
    /// to.
    pub opacity: [f32; fonts::MAX_DIGIT_CELLS],
    pieces: Vec<Piece>,
    fade_start: u64,
    fade_end: Option<u64>,
}

impl CellSwapEvent {
    pub fn new(display: DisplaySnapshot, seed: u64, surfaces: &mut SurfaceLedger) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut pattern =
            ClockCellSwapPattern::ALL[rng.random_range(0..ClockCellSwapPattern::ALL.len())];
        let cells: Vec<_> = display
            .digits
            .iter()
            .enumerate()
            .flat_map(|(slot, digit)| {
                let glyph = fonts::glyph(display.font, *digit);
                SegmentKind::ALL
                    .into_iter()
                    .flat_map(move |kind| {
                        fonts::CellMask(glyph.0 & fonts::region(display.font, kind).0).cells()
                    })
                    .map(move |cell| CellPosition {
                        slot: slot as u8,
                        cell,
                    })
            })
            .collect();
        let mut destinations = cells.clone();
        match pattern {
            ClockCellSwapPattern::Shuffle => destinations.shuffle(&mut rng),
            ClockCellSwapPattern::MirrorHorizontal | ClockCellSwapPattern::MirrorVertical => {
                for (source, destination) in cells.iter().zip(&mut destinations) {
                    let point = source.point();
                    let reflected = match pattern {
                        ClockCellSwapPattern::MirrorHorizontal => Vec2::new(point.x, 9.0 - point.y),
                        _ => Vec2::new(30.0 - point.x, point.y),
                    };
                    if let Some(partner) = cells.iter().find(|cell| cell.point() == reflected) {
                        *destination = *partner;
                    }
                }
            }
        }
        // Asymmetric faces can have no mirror partners at all. Still provide
        // a visible exchange, using only the admission's private random stream.
        if cells.len() > 1 && destinations == cells {
            pattern = ClockCellSwapPattern::Shuffle;
            destinations.shuffle(&mut rng);
            if destinations == cells {
                destinations.rotate_left(1);
            }
        }
        let exchanges: Vec<_> = cells
            .into_iter()
            .zip(destinations)
            .map(|(from, to)| Exchange { from, to })
            .collect();
        let mut faces: [SlotIds; DIGIT_SLOT_COUNT] = std::array::from_fn(|slot| {
            surfaces.take(
                slot,
                Glyph::new(display.font, display.digits[slot]),
                SurfaceSource::Swap,
            )
        });
        let pieces = exchanges
            .iter()
            .map(|path| {
                let id = faces[usize::from(path.from.slot)][bit(path.from.cell)].take();
                debug_assert!(id.is_some(), "every exchange starts on a lit cell");
                Piece {
                    id: id.unwrap_or_else(|| {
                        surfaces.admit(path.from.slot, path.from.cell, SurfaceSource::Swap)
                    }),
                    path: *path,
                    from: 1.0,
                    goal: true,
                }
            })
            .collect();
        debug_assert!(
            faces.iter().flatten().all(Option::is_none),
            "every lit cell is exchanged"
        );
        Self {
            from: display,
            target: display,
            pattern,
            exchanges,
            tick: 0,
            opacity: mask(display),
            pieces,
            fade_start: CELL_SWAP_TICKS,
            fade_end: None,
        }
    }

    pub fn synchronize(&mut self, display: DisplaySnapshot, surfaces: &mut SurfaceLedger) {
        let changed = self.target.digits != display.digits;
        self.target = display;
        if changed && self.tick >= CELL_SWAP_TICKS {
            // Preserve current brightness even for zero-dt/paused updates.
            // Further targets replace this fade; they do not enqueue readings.
            self.begin_fade(surfaces);
        }
    }

    /// How far the current fade has run.
    fn fade(&self) -> f32 {
        match self.fade_end {
            Some(end) if end > self.fade_start => {
                smooth((self.tick - self.fade_start) as f32 / (end - self.fade_start) as f32)
            }
            Some(_) => 1.0,
            None => 0.0,
        }
    }

    /// Opens a fade to the target from wherever every piece's ink is. A piece
    /// still wanted stays; one no longer wanted fades out from the ink it has;
    /// a newly lit cell is a new surface fading in from nothing, even where a
    /// piece of an older reading is fading out.
    fn begin_fade(&mut self, surfaces: &mut SurfaceLedger) {
        let fade = self.fade();
        for piece in &mut self.pieces {
            piece.from = piece.ink(fade);
        }
        self.fade_start = self.tick;
        self.fade_end = Some((self.tick + CELL_SWAP_FADE_TICKS).min(CELL_SWAP_MAX_TICKS));
        let target = mask(self.target);
        for piece in &mut self.pieces {
            piece.goal &= target[piece.path.to.index()] > 0.0;
        }
        for (slot, digit) in self.target.digits.into_iter().enumerate() {
            for cell in fonts::glyph(self.target.font, digit).cells() {
                let at = CellPosition {
                    slot: slot as u8,
                    cell,
                };
                if !self
                    .pieces
                    .iter()
                    .any(|piece| piece.goal && piece.path.to == at)
                {
                    self.pieces.push(Piece {
                        id: surfaces.admit(at.slot, cell, SurfaceSource::Swap),
                        path: Exchange { from: at, to: at },
                        from: 0.0,
                        goal: true,
                    });
                }
            }
        }
        // Two pieces fading out of one cell fade together, so keep one: the
        // older is retired, and the newer carries both their ink.
        let mut index = 0;
        while index < self.pieces.len() {
            let piece = self.pieces[index];
            if let Some(newer) = self.pieces[index + 1..]
                .iter_mut()
                .find(|other| !piece.goal && !other.goal && other.path.to == piece.path.to)
            {
                newer.from += piece.from;
                surfaces.retire(piece.id, Retirement::Replaced);
                self.pieces.remove(index);
            } else {
                index += 1;
            }
        }
    }

    pub fn step(&mut self, surfaces: &mut SurfaceLedger) -> bool {
        self.tick += 1;
        if self.tick == CELL_SWAP_TICKS {
            if self.opacity == mask(self.target) {
                return true;
            }
            self.begin_fade(surfaces);
        }
        if let Some(end) = self.fade_end {
            let fade = self.fade();
            self.opacity = [0.0; fonts::MAX_DIGIT_CELLS];
            for piece in &self.pieces {
                let ink = &mut self.opacity[piece.path.to.index()];
                *ink = (*ink + piece.ink(fade)).min(1.0);
            }
            return self.tick >= end;
        }
        false
    }

    /// Every piece where it is drawn: on its exchange's path until the
    /// exchanges land, then on its cell with the ink the fade gives it.
    pub fn surfaces(&self, layout: Layout, out: &mut Vec<(SurfaceId, SurfacePose)>) {
        let swapping = self.tick < CELL_SWAP_TICKS;
        let (progress, fade) = (self.progress(), self.fade());
        for piece in &self.pieces {
            let at = piece.path.to;
            out.push((
                piece.id,
                SurfacePose {
                    origin: if swapping {
                        piece.path.centre(layout, progress)
                    } else {
                        at.center(layout)
                    },
                    source: SurfaceSource::Swap,
                    departing: !piece.goal,
                    opacity: piece.ink(fade),
                    ..SurfacePose::anchored(layout, usize::from(at.slot), at.cell)
                },
            ));
        }
    }

    /// Gives the face back the pieces resting on cells of the reading it will
    /// show: every piece once the exchanges have landed, and before then only
    /// a piece that has not moved. One cut short on its way is retired there
    /// rather than snapped home, and one fading out is retired either way.
    pub fn release_surfaces(&mut self, surfaces: &mut SurfaceLedger) {
        let swapping = self.tick < CELL_SWAP_TICKS;
        let glyphs: [Glyph; DIGIT_SLOT_COUNT] =
            std::array::from_fn(|slot| Glyph::new(self.target.font, self.target.digits[slot]));
        let mut ids = [[None; GRID_CELLS]; DIGIT_SLOT_COUNT];
        for piece in self.pieces.drain(..) {
            let path = piece.path;
            let rest = if !swapping {
                Some(path.to)
            } else if self.tick == 0 || path.from == path.to {
                Some(path.from)
            } else {
                None
            };
            match rest {
                _ if !piece.goal => surfaces.retire(piece.id, Retirement::Replaced),
                Some(at) if glyphs[usize::from(at.slot)].mask.contains(at.cell) => {
                    ids[usize::from(at.slot)][bit(at.cell)] = Some(piece.id);
                }
                // The reading moved on while it travelled.
                Some(_) => surfaces.retire(piece.id, Retirement::Replaced),
                None => surfaces.retire(piece.id, Retirement::Interrupted),
            }
        }
        for (slot, ids) in ids.into_iter().enumerate() {
            surfaces.hand_back(slot, glyphs[slot], ids);
        }
    }

    pub fn progress(&self) -> f32 {
        smooth(self.tick as f32 / CELL_SWAP_TICKS as f32)
    }

    pub fn diagnostics(&self) -> ClockCellSwapState {
        ClockCellSwapState {
            pattern: self.pattern,
            from_digits: self.from.digits,
            to_digits: self.target.digits,
            elapsed_ticks: self.tick,
            progress_milli: (self.progress() * 1000.0).round() as u32,
            participating_cells: self.exchanges.len() as u32,
            moving_cells: self
                .exchanges
                .iter()
                .filter(|cell| cell.from != cell.to)
                .count() as u32,
        }
    }
}

fn mask(display: DisplaySnapshot) -> [f32; fonts::MAX_DIGIT_CELLS] {
    let mut result = [0.0; fonts::MAX_DIGIT_CELLS];
    for (slot, digit) in display.digits.into_iter().enumerate() {
        for cell in fonts::glyph(display.font, digit).cells() {
            result[CellPosition {
                slot: slot as u8,
                cell,
            }
            .index()] = 1.0;
        }
    }
    result
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
