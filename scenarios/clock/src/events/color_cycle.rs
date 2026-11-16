pub use crate::presentation::effects::DigitPalette;
use crate::presentation::effects::cycle_palette;

pub const COLOR_CYCLE_TICKS: u64 = 360;

#[derive(Default)]
pub(crate) struct ColorCycle {
    pub tick: u64,
}

impl ColorCycle {
    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.tick >= COLOR_CYCLE_TICKS
    }

    /// The colors `tick` of the way through the cycle, which leaves `base`
    /// and comes back to it.
    pub fn palette(&self, base: DigitPalette) -> DigitPalette {
        cycle_palette(base, self.tick as f32 / COLOR_CYCLE_TICKS as f32)
    }
}
