//! Player intent and bounded wet movement. The shared world still owns gravity,
//! buoyancy, drag, currents, and collisions; these controls only add velocity deltas.
use super::{
    DT,
    controller::{Movement, Observation},
};

pub(super) const SWIM_COOLDOWN_TICKS: u64 = 18;
pub(super) const SWIM_MIN_IMMERSION: f64 = 0.2;
pub(super) const DIVE_TICKS: u32 = 4 * 60;
// A stroke must overcome the fully submerged hull's buoyancy. Its duration
// and downward speed are bounded; releasing restores ordinary flotation.
const DIVE_GRAVITY_FRACTION: f32 = 2.5;

pub(super) struct PlayerControl {
    pub session_id: u64,
    pub seat: u8,
    pub move_milli: i16,
    pub jump_held: bool,
    pub jump_pending: bool,
    pub run_held: bool,
    pub dive_held: bool,
    pub facing: f32,
    pub exit_at_tick: u64,
    pub swim_strokes: u32,
    pub next_swim_tick: u64,
    pub dive_ticks_left: u32,
    pub dive_recharging: bool,
    pub diving: bool,
}

impl PlayerControl {
    pub fn new(session_id: u64, seat: u8, facing: f32, exit_at_tick: u64) -> Self {
        Self {
            session_id,
            seat,
            move_milli: 0,
            jump_held: false,
            jump_pending: false,
            run_held: false,
            dive_held: false,
            facing,
            exit_at_tick,
            swim_strokes: 0,
            next_swim_tick: 0,
            dive_ticks_left: DIVE_TICKS,
            dive_recharging: false,
            diving: false,
        }
    }

    pub fn water_delta(
        &mut self,
        movement: &Movement,
        radius: f32,
        observed: Observation,
        submerged: f64,
        tick: u64,
    ) -> f32 {
        self.diving = false;
        // Releasing Down underwater cannot refill the allowance. At the
        // surface, two seconds of rest restores a completely spent dive.
        if !self.dive_held && submerged <= 0.55 {
            self.dive_ticks_left = (self.dive_ticks_left + 2).min(DIVE_TICKS);
            if self.dive_ticks_left == DIVE_TICKS {
                self.dive_recharging = false;
            }
        }
        if observed.grounded || submerged < SWIM_MIN_IMMERSION {
            return 0.0;
        }
        // Jump wins over Down. Cooldown survives releases, water exits, and
        // landings, so mashing/edge transitions cannot bypass the cadence.
        if self.jump_held {
            if tick < self.next_swim_tick {
                return 0.0;
            }
            let kick = (radius * 15.0 - observed.velocity.y).clamp(0.0, radius * 12.0);
            if kick > 0.0 {
                self.next_swim_tick = tick + SWIM_COOLDOWN_TICKS;
                self.swim_strokes += 1;
            }
            kick
        } else if self.dive_held && !self.dive_recharging && self.dive_ticks_left > 0 {
            self.diving = true;
            self.dive_ticks_left -= 1;
            self.dive_recharging = self.dive_ticks_left == 0;
            (-movement.gravity * DIVE_GRAVITY_FRACTION * DT)
                .max(-radius * 8.0 - observed.velocity.y)
                .min(0.0)
        } else {
            0.0
        }
    }
}
