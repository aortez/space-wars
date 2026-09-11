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

    pub fn palette(&self) -> DigitPalette {
        cycle_palette(self.tick as f32 / COLOR_CYCLE_TICKS as f32)
    }
}
