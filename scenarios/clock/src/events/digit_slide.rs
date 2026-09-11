//! A bounded presentation transition. Physical segments stay anchored to the
//! latest reading; only the old/new visible cells move inside their digit slots.

use engine_common::ClockDigitSlideState;

use crate::{DIGIT_SLOT_COUNT, DisplaySnapshot};

pub const DIGIT_SLIDE_TICKS: u64 = 48;

pub(crate) struct DigitSlideEvent {
    pub from: [Option<u8>; DIGIT_SLOT_COUNT],
    pub to: [Option<u8>; DIGIT_SLOT_COUNT],
    pub changed: [bool; DIGIT_SLOT_COUNT],
    pub tick: u64,
    preview: bool,
}

impl DigitSlideEvent {
    pub fn new(from: Option<DisplaySnapshot>, to: DisplaySnapshot) -> Self {
        let preview = from.is_none();
        let from = from.unwrap_or(to).digits;
        let to = to.digits;
        Self {
            from,
            to,
            changed: std::array::from_fn(|slot| {
                from[slot] != to[slot] || (preview && to[slot].is_some())
            }),
            tick: 0,
            preview,
        }
    }

    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.tick >= DIGIT_SLIDE_TICKS
    }

    pub fn progress(&self) -> f32 {
        let t = (self.tick as f32 / DIGIT_SLIDE_TICKS as f32).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
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
