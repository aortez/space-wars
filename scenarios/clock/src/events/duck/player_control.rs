//! Player intent and bounded wet movement. The shared world still owns gravity,
//! buoyancy, drag, currents, and collisions; these controls only add velocity deltas.
use super::{
    DT,
    controller::{Movement, Observation},
};

pub(super) const SWIM_COOLDOWN_TICKS: u64 = 18;
const SWIM_MIN_IMMERSION: f64 = 0.2;
// Extra downward acceleration shifts this 0.45-density hull's equilibrium
// from 45% to about 79% immersed. Even fully submerged, buoyancy wins: holding
// Dive cannot propel it indefinitely into the depths. Water drag damps the bob.
const DIVE_GRAVITY_FRACTION: f32 = 0.75;

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
        } else if self.dive_held {
            -movement.gravity * DIVE_GRAVITY_FRACTION * DT
        } else {
            0.0
        }
    }
}
