use engine_common::RenderColor;
use engine_core::Vec2;
use std::f32::consts::TAU;

use super::{Bounds, content::Cell};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Content,
    Glyph,
    Cell,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Glyph => "glyph",
            Self::Cell => "cell",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Rotation {
    pub target: Target,
    pub turns: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Wave {
    pub target: Target,
    pub amplitude: f32,
    pub wavelength: f32,
    pub cycles_per_second: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Lighting {
    #[default]
    Normal,
    Cycle,
    Sweep,
    Chase,
}

impl Lighting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Cycle => "cycle",
            Self::Sweep => "sweep",
            Self::Chase => "chase",
        }
    }

    pub fn sample(
        self,
        cell: Cell,
        bounds: Bounds,
        seconds: f32,
        progress: f32,
    ) -> (DigitPalette, f32) {
        match self {
            Self::Normal => (DigitPalette::default(), 1.0),
            Self::Cycle => (cycle_palette(progress), 1.0),
            Self::Sweep | Self::Chase => {
                let trail = if self == Self::Chase {
                    (seconds * 0.45 - cell.path - f32::from(cell.group) * 0.10).rem_euclid(1.0)
                } else {
                    let x = (cell.center.x - bounds.min.x) / bounds.size().x;
                    (seconds * 0.22 - x).rem_euclid(1.0)
                };
                let strength = (-trail / 0.14).exp();
                let normal = DigitPalette::default();
                (
                    DigitPalette {
                        fill: blend(normal.fill, RenderColor::rgb(1.0, 0.79, 0.32), strength),
                        edge: blend(normal.edge, RenderColor::rgb(1.0, 0.97, 0.77), strength),
                    },
                    0.5 + strength * 0.5,
                )
            }
        }
    }
}

/// A small fixed set of orthogonal passes, not an unbounded effect graph.
/// Order: wave in content space, pivoted rotation, scale/placement, then clipping
/// by the caller. Lighting samples original coordinates, so it travels with text.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Recipe {
    pub scroll: bool,
    pub rotation: Option<Rotation>,
    pub wave: Option<Wave>,
    pub lighting: Lighting,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Playback {
    pub seconds: f32,
    pub progress: f32,
    pub strength: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    pub content: Bounds,
    pub viewport: Bounds,
    pub center: Vec2,
    pub pitch: f32,
}

impl Recipe {
    pub fn quad(self, cell: Cell, placement: Placement, playback: Playback) -> [Vec2; 4] {
        let content_center = placement.content.center();
        let mut pitch = placement.pitch;
        let available = Vec2::new(
            (placement.center.x - placement.viewport.min.x)
                .min(placement.viewport.max.x - placement.center.x)
                * 2.0,
            (placement.center.y - placement.viewport.min.y)
                .min(placement.viewport.max.y - placement.center.y)
                * 2.0,
        );
        if self
            .rotation
            .is_some_and(|rotation| rotation.target == Target::Content)
        {
            // A rotation-independent fit avoids breathing/zooming as the word
            // spins. Include wave displacement in its conservative envelope.
            let size = placement.content.size();
            let wave_margin = self.wave.map_or(0.0, |wave| wave.amplitude.abs() * 2.0);
            let diameter = size.x.hypot(size.y + wave_margin);
            pitch = pitch.min(available.x.min(available.y) / diameter);
        } else if !self.scroll && (self.rotation.is_some() || self.wave.is_some()) {
            // Glyph rotation can extend beyond a thin letter's normal bounds.
            let padding = if self.rotation.is_some() { 8.0 } else { 0.0 };
            let size = placement.content.size();
            let wave_margin = self.wave.map_or(0.0, |wave| wave.amplitude.abs() * 2.0);
            pitch = pitch
                .min(available.x / (size.x + padding))
                .min(available.y / (size.y + padding + wave_margin));
        }
        let mut center = placement.center;
        if self.scroll {
            let half = placement.content.size().x * pitch * 0.5;
            let start = placement.viewport.max.x + half;
            let end = placement.viewport.min.x - half;
            center.x = start + (end - start) * playback.progress;
        }
        let half = cell.size * 0.4;
        [
            Vec2::new(-half, -half),
            Vec2::new(half, -half),
            Vec2::new(half, half),
            Vec2::new(-half, half),
        ]
        .map(|offset| {
            let mut p = cell.center + offset;
            if let Some(wave) = self.wave {
                let sample_x = match wave.target {
                    Target::Content => content_center.x,
                    Target::Glyph => cell.glyph_pivot.x,
                    Target::Cell => p.x,
                };
                p.y += wave.amplitude
                    * playback.strength
                    * (TAU
                        * (sample_x / wave.wavelength - playback.seconds * wave.cycles_per_second))
                        .sin();
            }
            if let Some(rotation) = self.rotation {
                let pivot = match rotation.target {
                    Target::Content => content_center,
                    Target::Glyph => cell.glyph_pivot,
                    Target::Cell => cell.center,
                };
                p = pivot
                    + (p - pivot)
                        .rotate_radians(TAU * rotation.turns * smoothstep(playback.progress));
            }
            center + (p - content_center) * pitch
        })
    }
}

pub(crate) fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Color Cycle and composed Marquee recipes use the same palette pass.
pub(crate) fn cycle_palette(progress: f32) -> DigitPalette {
    let normal = DigitPalette::default();
    let fills = [
        normal.fill,
        RenderColor::rgb(0.72, 0.58, 1.0),
        RenderColor::rgb(1.0, 0.48, 0.72),
        RenderColor::rgb(1.0, 0.82, 0.36),
        RenderColor::rgb(0.57, 0.98, 0.52),
        normal.fill,
    ];
    let position = progress.clamp(0.0, 1.0) * 5.0;
    let index = (position as usize).min(4);
    let t = smoothstep(position - index as f32);
    let edge = |i| {
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

fn blend(a: RenderColor, b: RenderColor, t: f32) -> RenderColor {
    RenderColor::rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}
