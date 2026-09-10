use engine_common::{ClockMarqueePreset, ClockMarqueeState};

use crate::{
    DisplaySnapshot,
    presentation::{
        content::Content,
        effects::{Lighting, Playback, Recipe, Rotation, Target, Wave, smoothstep},
    },
};

pub const MARQUEE_TICKS: u64 = 12 * 60;
const FADE_TICKS: u64 = 45;
pub(crate) const MARQUEE_MESSAGE: &str = "SPACE WARS";

pub(crate) struct MarqueeEvent {
    pub tick: u64,
    pub preset: ClockMarqueePreset,
    pub recipe: Recipe,
    pub content: Content,
    pub uses_clock: bool,
    display: DisplaySnapshot,
}

impl MarqueeEvent {
    pub fn new(preset: ClockMarqueePreset, display: DisplaySnapshot) -> Self {
        let uses_clock = matches!(
            preset,
            ClockMarqueePreset::ClockChase
                | ClockMarqueePreset::ClockWave
                | ClockMarqueePreset::ClockSpin
                | ClockMarqueePreset::DigitSpin
        );
        let recipe = match preset {
            ClockMarqueePreset::ClockChase => Recipe {
                lighting: Lighting::Chase,
                ..Recipe::default()
            },
            ClockMarqueePreset::ClockWave => Recipe {
                wave: Some(Wave {
                    target: Target::Glyph,
                    amplitude: 1.1,
                    wavelength: 22.0,
                    cycles_per_second: 0.25,
                }),
                lighting: Lighting::Cycle,
                ..Recipe::default()
            },
            ClockMarqueePreset::ClockSpin => Recipe {
                rotation: Some(Rotation {
                    target: Target::Content,
                    turns: 2.0,
                }),
                lighting: Lighting::Cycle,
                ..Recipe::default()
            },
            ClockMarqueePreset::DigitSpin | ClockMarqueePreset::TextSpin => Recipe {
                rotation: Some(Rotation {
                    target: Target::Glyph,
                    turns: 2.0,
                }),
                lighting: Lighting::Sweep,
                ..Recipe::default()
            },
            ClockMarqueePreset::TextScroll => Recipe {
                scroll: true,
                lighting: Lighting::Cycle,
                ..Recipe::default()
            },
            ClockMarqueePreset::TextRibbon => Recipe {
                scroll: true,
                wave: Some(Wave {
                    target: Target::Cell,
                    amplitude: 1.2,
                    wavelength: 18.0,
                    cycles_per_second: 0.3,
                }),
                lighting: Lighting::Sweep,
                ..Recipe::default()
            },
        };
        Self {
            tick: 0,
            preset,
            recipe,
            uses_clock,
            display,
            content: if uses_clock {
                Content::clock(display)
            } else {
                Content::text(MARQUEE_MESSAGE).expect("built-in message fits the bitmap font")
            },
        }
    }

    pub fn synchronize(&mut self, display: DisplaySnapshot) {
        if self.display != display {
            self.display = display;
            if self.uses_clock {
                self.content.update_clock(display);
            }
        }
    }

    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.tick >= MARQUEE_TICKS
    }

    pub fn playback(&self) -> Playback {
        Playback {
            seconds: self.tick as f32 / 60.0,
            progress: self.tick as f32 / MARQUEE_TICKS as f32,
            strength: smoothstep(
                self.tick.min(MARQUEE_TICKS.saturating_sub(self.tick)) as f32 / FADE_TICKS as f32,
            ),
        }
    }

    pub fn diagnostics(&self) -> ClockMarqueeState {
        ClockMarqueeState {
            preset: self.preset,
            content: if self.uses_clock {
                "clock"
            } else {
                MARQUEE_MESSAGE
            }
            .into(),
            cell_count: self.content.cells.len(),
            group_count: self.content.groups,
            progress_milli: (self.tick * 1000 / MARQUEE_TICKS) as u32,
            scrolling: self.recipe.scroll,
            waving: self.recipe.wave.is_some(),
            rotation_target: self.recipe.rotation.map(|r| r.target.as_str().into()),
            lighting: self.recipe.lighting.as_str().into(),
        }
    }
}
