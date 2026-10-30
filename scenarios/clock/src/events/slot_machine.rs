//! Four reels that scramble through glyphs and lock left to right onto the live
//! reading. The cells travel and are clipped to their own slot, so this follows
//! "Returning digit cells to their places" rather than dissolving anything, and
//! every reel position is sampled from the tick rather than accumulated, so a
//! paused or replayed event cannot drift.

use engine_common::{ClockFont, ClockSlotMachinePhase, ClockSlotMachineState};

use crate::{DIGIT_SLOT_COUNT, DisplaySnapshot, fonts::CellMask};

pub const SPIN_UP_TICKS: u64 = 60;
pub const SPINNING_TICKS: u64 = 120;
pub const DECELERATION_TICKS: u64 = 24;
/// Each slot begins stopping this long after the one on its left, so the lock
/// order is the thing that actually reads as a slot machine.
pub const LOCK_STAGGER_TICKS: u64 = 22;
pub const PAYOUT_TICKS: u64 = 30;
pub const SLOT_MACHINE_TICKS: u64 = SPIN_UP_TICKS
    + SPINNING_TICKS
    + LOCK_STAGGER_TICKS * (DIGIT_SLOT_COUNT as u64 - 1)
    + DECELERATION_TICKS
    + PAYOUT_TICKS;

/// Ten glyphs a second at full speed: fast enough to blur, slow enough that it
/// does not strobe against a 60 Hz frame.
const TICKS_PER_GLYPH: f32 = 6.0;
/// Scramble frames come from the 5x7 text font, which holds these plus some
/// punctuation. At 6x9 cells many letters are indistinguishable, so these are
/// texture rather than an alphabet and the reel never locks onto one.
const SCRAMBLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// What sits at one position on a reel's strip.
enum Frame {
    /// The reading the face was showing when the event opened, so the reel's
    /// first frame is the face itself and the takeover starts by moving.
    Opening,
    Scramble(u8),
    /// The last two stops before the lock, in the face's own font, so the reel
    /// resolves out of noise into legitimate readings.
    Digit(u8),
    Lock,
}

pub(crate) struct SlotMachineEvent {
    pub tick: u64,
    seed: u64,
    opening: DisplaySnapshot,
    display: DisplaySnapshot,
    /// The digit a slot settled on, which a later reading cannot move. Blank is
    /// a real outcome in 12-hour mode, hence the inner option.
    locked: [Option<Option<u8>>; DIGIT_SLOT_COUNT],
    retargets: u32,
    preview: bool,
}

/// When a slot stops accelerating and starts easing to a halt.
const fn deceleration_start(slot: usize) -> u64 {
    SPIN_UP_TICKS + SPINNING_TICKS + LOCK_STAGGER_TICKS * slot as u64
}

const fn locked_at(slot: usize) -> u64 {
    deceleration_start(slot) + DECELERATION_TICKS
}

/// Glyphs a reel covers before the scale below lands it on a whole stop. The
/// three phases are a ramp, a plateau and a ramp down, so this is the area
/// under that profile.
fn unscaled_distance(slot: usize) -> f32 {
    let speed = 1.0 / TICKS_PER_GLYPH;
    let plateau = (deceleration_start(slot) - SPIN_UP_TICKS) as f32;
    speed * (SPIN_UP_TICKS as f32 / 2.0 + plateau + DECELERATION_TICKS as f32 / 2.0)
}

/// The whole stop a reel lands on. Rounding the natural distance and scaling to
/// it keeps the deceleration's shape while landing exactly on a glyph, which is
/// a change of about one percent in speed and is not visible.
fn stop_index(slot: usize) -> u32 {
    unscaled_distance(slot).round() as u32
}

fn unscaled_travel(slot: usize, tick: u64) -> f32 {
    let speed = 1.0 / TICKS_PER_GLYPH;
    let start = deceleration_start(slot);
    let decel = DECELERATION_TICKS as f32;
    let tick = tick.min(locked_at(slot)) as f32;
    let up = SPIN_UP_TICKS as f32;
    if tick <= up {
        return speed * tick * tick / (2.0 * up);
    }
    let ramped = speed * up / 2.0;
    if tick <= start as f32 {
        return ramped + speed * (tick - up);
    }
    let plateau = ramped + speed * (start - SPIN_UP_TICKS) as f32;
    let since = tick - start as f32;
    plateau + speed * (since - since * since / (2.0 * decel))
}

impl SlotMachineEvent {
    pub fn new(display: DisplaySnapshot, seed: u64, preview: bool) -> Self {
        Self {
            tick: 0,
            seed,
            opening: display,
            display,
            locked: [None; DIGIT_SLOT_COUNT],
            retargets: 0,
            preview,
        }
    }

    /// A reel still turning adopts a new reading without restarting, so a minute
    /// rolling over mid-spin is invisible and the event lands on live time. A
    /// slot that has already locked keeps what it landed on and follows the
    /// ordinary face once the event completes.
    pub fn synchronize(&mut self, display: DisplaySnapshot) {
        if self.display.digits == display.digits && self.display.font == display.font {
            self.display = display;
            return;
        }
        let turning = (0..DIGIT_SLOT_COUNT).any(|slot| self.locked[slot].is_none());
        self.display = display;
        if turning {
            self.retargets += 1;
        }
    }

    pub fn step(&mut self) -> bool {
        self.tick += 1;
        for slot in 0..DIGIT_SLOT_COUNT {
            if self.locked[slot].is_none() && self.tick >= locked_at(slot) {
                self.locked[slot] = Some(self.display.digits[slot]);
            }
        }
        self.tick >= SLOT_MACHINE_TICKS
    }

    pub fn phase(&self) -> ClockSlotMachinePhase {
        if self.tick < SPIN_UP_TICKS {
            ClockSlotMachinePhase::SpinUp
        } else if self.tick < deceleration_start(0) {
            ClockSlotMachinePhase::Spinning
        } else if self.tick < locked_at(DIGIT_SLOT_COUNT - 1) {
            ClockSlotMachinePhase::Settling
        } else {
            ClockSlotMachinePhase::Payout
        }
    }

    pub fn phase_tick(&self) -> u64 {
        let start = match self.phase() {
            ClockSlotMachinePhase::SpinUp => 0,
            ClockSlotMachinePhase::Spinning => SPIN_UP_TICKS,
            ClockSlotMachinePhase::Settling => deceleration_start(0),
            ClockSlotMachinePhase::Payout => locked_at(DIGIT_SLOT_COUNT - 1),
        };
        self.tick - start
    }

    /// True once every reel has stopped and the settled reading is on show.
    pub fn paying_out(&self) -> bool {
        self.phase() == ClockSlotMachinePhase::Payout
    }

    /// How far this reel has turned, in glyphs. A whole number means a glyph is
    /// centred in the slot.
    pub fn travel(&self, slot: usize) -> f32 {
        let total = unscaled_distance(slot);
        if total <= 0.0 {
            return 0.0;
        }
        unscaled_travel(slot, self.tick) * stop_index(slot) as f32 / total
    }

    fn target(&self, slot: usize) -> Option<u8> {
        self.locked[slot].unwrap_or(self.display.digits[slot])
    }

    fn frame(&self, slot: usize, index: u32) -> Frame {
        if index == 0 {
            return Frame::Opening;
        }
        match stop_index(slot).saturating_sub(index) {
            0 => Frame::Lock,
            1 | 2 => Frame::Digit((mix(self.seed, slot, index) % 10) as u8),
            _ => Frame::Scramble(SCRAMBLE[(mix(self.seed, slot, index) as usize) % SCRAMBLE.len()]),
        }
    }

    /// The cells one reel position puts in a slot. Scramble frames go through
    /// the 5x7 sampling, which is the only path that reaches a non-digit glyph,
    /// so they look the same whichever face font is selected.
    pub fn glyph(&self, slot: usize, index: u32) -> CellMask {
        let font = self.display.font;
        match self.frame(slot, index) {
            Frame::Opening => crate::fonts::glyph(self.opening.font, self.opening.digits[slot]),
            Frame::Scramble(byte) => crate::fonts::sampled(byte),
            Frame::Digit(digit) => crate::fonts::glyph(font, Some(digit)),
            Frame::Lock => crate::fonts::glyph(font, self.target(slot)),
        }
    }

    /// The reading the reels actually landed on. A minute rolling over after a
    /// slot has locked cannot move it; the ordinary face takes over again when
    /// the event completes.
    pub fn settled(&self) -> DisplaySnapshot {
        let mut settled = self.display;
        settled.digits = std::array::from_fn(|slot| self.target(slot));
        settled
    }

    pub fn font(&self) -> ClockFont {
        self.display.font
    }

    pub fn diagnostics(&self) -> ClockSlotMachineState {
        ClockSlotMachineState {
            phase: self.phase(),
            target_digits: std::array::from_fn(|slot| self.target(slot)),
            locked_slots: std::array::from_fn(|slot| self.locked[slot].is_some()),
            reel_offset_milli: std::array::from_fn(|slot| {
                let travel = self.travel(slot);
                ((travel - travel.floor()) * 1000.0).round() as u32
            }),
            glyphs_passed: std::array::from_fn(|slot| self.travel(slot).floor() as u32),
            retargets: self.retargets,
            preview: self.preview,
        }
    }
}

/// Addressable by position rather than drawn from a stream, so any tick renders
/// without replaying the reel and a replay is identical.
fn mix(seed: u64, slot: usize, index: u32) -> u64 {
    let mut x = seed ^ (u64::from(index) << 8) ^ ((slot as u64) << 1);
    x = x.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 31;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^ (x >> 27)
}

#[cfg(test)]
mod tests;
