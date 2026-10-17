//! A permutation of occupied cells, followed only when needed by a short fade
//! to the latest reading. All storage and paths are bounded by the four glyphs.

use engine_common::{ClockCellSwapPattern, ClockCellSwapState};
use engine_core::Vec2;
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::{
    DisplaySnapshot, GridCell, SegmentKind, fonts,
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
}

pub(crate) struct CellSwapEvent {
    pub from: DisplaySnapshot,
    pub target: DisplaySnapshot,
    pub pattern: ClockCellSwapPattern,
    pub exchanges: Vec<Exchange>,
    pub tick: u64,
    pub opacity: [f32; fonts::MAX_DIGIT_CELLS],
    fade_from: [f32; fonts::MAX_DIGIT_CELLS],
    fade_start: u64,
    fade_end: Option<u64>,
}

impl CellSwapEvent {
    pub fn new(display: DisplaySnapshot, seed: u64) -> Self {
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
        let exchanges = cells
            .into_iter()
            .zip(destinations)
            .map(|(from, to)| Exchange { from, to })
            .collect();
        let opacity = mask(display);
        Self {
            from: display,
            target: display,
            pattern,
            exchanges,
            tick: 0,
            opacity,
            fade_from: opacity,
            fade_start: CELL_SWAP_TICKS,
            fade_end: None,
        }
    }

    pub fn synchronize(&mut self, display: DisplaySnapshot) {
        let changed = self.target.digits != display.digits;
        self.target = display;
        if changed && self.tick >= CELL_SWAP_TICKS {
            // Preserve current brightness even for zero-dt/paused updates.
            // Further targets replace this fade; they do not enqueue readings.
            self.begin_fade();
        }
    }

    fn begin_fade(&mut self) {
        self.fade_from = self.opacity;
        self.fade_start = self.tick;
        self.fade_end = Some((self.tick + CELL_SWAP_FADE_TICKS).min(CELL_SWAP_MAX_TICKS));
    }

    pub fn step(&mut self) -> bool {
        self.tick += 1;
        if self.tick == CELL_SWAP_TICKS {
            if self.opacity == mask(self.target) {
                return true;
            }
            self.begin_fade();
        }
        if let Some(end) = self.fade_end {
            let progress =
                smooth((self.tick - self.fade_start) as f32 / (end - self.fade_start) as f32);
            let target = mask(self.target);
            for ((opacity, from), to) in self.opacity.iter_mut().zip(self.fade_from).zip(target) {
                *opacity = from + (to - from) * progress;
            }
            return self.tick >= end;
        }
        false
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
