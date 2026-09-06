use engine_common::RenderColor;

pub const COLOR_CYCLE_TICKS: u64 = 360;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DigitPalette {
    pub fill: RenderColor,
    pub edge: RenderColor,
}

impl Default for DigitPalette {
    fn default() -> Self {
        Self {
            fill: RenderColor::rgb(0.33, 0.94, 0.91),
            edge: RenderColor::rgb(0.72, 1.0, 0.96),
        }
    }
}

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
        // Slow, eased, high-luminance colors keep the face readable throughout.
        // The endpoints are exactly the normal palette; no persistent mutation.
        let normal = DigitPalette::default();
        let fills = [
            normal.fill,
            RenderColor::rgb(0.72, 0.58, 1.0),
            RenderColor::rgb(1.0, 0.48, 0.72),
            RenderColor::rgb(1.0, 0.82, 0.36),
            RenderColor::rgb(0.57, 0.98, 0.52),
            normal.fill,
        ];
        let position = (self.tick as f32 / COLOR_CYCLE_TICKS as f32).clamp(0.0, 1.0) * 5.0;
        let index = (position as usize).min(4);
        let t = position - index as f32;
        let t = t * t * (3.0 - 2.0 * t);
        let edge = |i: usize| {
            if i == 0 || i == 5 {
                normal.edge
            } else {
                blend(fills[i], RenderColor::rgb(1.0, 1.0, 1.0), 0.65)
            }
        };
        DigitPalette {
            fill: blend(fills[index], fills[index + 1], t),
            edge: blend(edge(index), edge(index + 1), t),
        }
    }
}

fn blend(a: RenderColor, b: RenderColor, t: f32) -> RenderColor {
    RenderColor::rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}
