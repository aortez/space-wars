use engine_common::{ClockMarqueeMessage, ClockMarqueePreset, ClockMarqueeState};
use engine_core::Vec2;

use crate::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell,
    digit_surfaces::{CELL_HALF, Footprint, Retirement, SurfaceLedger, SurfacePose, SurfaceSource},
    layout::Layout,
    presentation::{
        Bounds,
        content::Content,
        effects::{Lighting, Placement, Playback, Recipe, Rotation, Target, Wave, smoothstep},
    },
};

pub const MARQUEE_TICKS: u64 = 12 * 60;
const FADE_TICKS: u64 = 45;

/// How the anchored face is drawn beneath a takeover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FaceDraw {
    /// How far its lit cells are carried along x.
    pub offset: f32,
    /// How much of their ink is left.
    pub ink: f32,
}

/// The part of the view a takeover draws its content in.
pub(crate) fn viewport(layout: Layout) -> Bounds {
    Bounds {
        min: Vec2::new(layout.bounds_min.x + 8.0, layout.floor_y + 8.0),
        max: Vec2::new(layout.bounds_max.x - 8.0, layout.canopy_y - 8.0),
    }
}

pub(crate) struct MarqueeEvent {
    pub tick: u64,
    pub preset: ClockMarqueePreset,
    pub recipe: Recipe,
    pub content: Content,
    pub uses_clock: bool,
    message: ClockMarqueeMessage,
    display: DisplaySnapshot,
}

impl MarqueeEvent {
    pub fn new(
        preset: ClockMarqueePreset,
        message: ClockMarqueeMessage,
        display: DisplaySnapshot,
    ) -> Self {
        let uses_clock = matches!(
            preset,
            ClockMarqueePreset::ClockChase
                | ClockMarqueePreset::ClockWave
                | ClockMarqueePreset::ClockSpin
                | ClockMarqueePreset::DigitSpin
        );
        let recipe = match preset {
            ClockMarqueePreset::Random => unreachable!("Random is resolved at admission"),
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
        // A clock preset animates the face's own cells, so it has to open on the
        // face's pose. A text takeover brings its own content instead.
        let recipe = Recipe {
            handover: uses_clock,
            ..recipe
        };
        Self {
            tick: 0,
            preset,
            recipe,
            uses_clock,
            message,
            display,
            content: if uses_clock {
                Content::clock(display)
            } else {
                Content::text(message.as_str()).expect("validated message fits the bitmap font")
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

    /// How far the anchored face has travelled, as a signed fraction of the
    /// clearance it needs. A text takeover carries the lit cells off the way the
    /// message travels and returns them the same way, so the reading moves
    /// instead of dissolving where it stands. A clock preset animates those
    /// same cells and never sweeps them, so it reports nothing to move.
    pub fn face_sweep(&self) -> Option<f32> {
        if self.uses_clock {
            return None;
        }
        // The message always travels left, so the face leaves to the left and
        // comes back from the right rather than retreating the way it went.
        let strength = self.playback().strength;
        Some(if self.tick * 2 < MARQUEE_TICKS {
            -strength
        } else {
            strength
        })
    }

    /// How the anchored face is drawn. A text takeover carries its lit cells
    /// clear of the frame at full ink and has nothing left to draw once they
    /// are clear. A clock preset leaves them where they stand and dissolves
    /// them into its own copy of the reading, which carries the surfaces.
    pub fn face(&self, layout: Layout) -> FaceDraw {
        match self.face_sweep() {
            Some(sweep) => FaceDraw {
                offset: sweep * (layout.bounds_max.x - layout.bounds_min.x + layout.pitch),
                ink: if sweep.abs() >= 1.0 { 0.0 } else { 1.0 },
            },
            None => FaceDraw {
                offset: 0.0,
                ink: (1.0 - self.playback().strength).max(0.0),
            },
        }
    }

    /// Where the recipe lays its content out. A clock preset's content is the
    /// face's own reading, so this opens at the face's pitch and the recipe
    /// eases any fit of its own in from there.
    pub fn placement(&self, layout: Layout) -> Placement {
        Placement {
            content: self.content.bounds,
            viewport: viewport(layout),
            center: Vec2::new(0.0, layout.face_origin.y + 4.5 * layout.pitch),
            pitch: layout.pitch * if self.uses_clock { 1.0 } else { 0.7 },
        }
    }

    /// Where the face's own surface at `cell` of `slot` is while the takeover
    /// runs. A clock preset carries it on the recipe's copy of the reading,
    /// sampled the way that copy is drawn, so the copy dissolving in place
    /// owns nothing. A text takeover carries it with the swept reading.
    pub fn face_pose(&self, layout: Layout, slot: usize, cell: GridCell) -> SurfacePose {
        let anchored = SurfacePose::anchored(layout, slot, cell);
        if !self.uses_clock {
            let face = self.face(layout);
            return SurfacePose {
                origin: anchored.origin + Vec2::new(face.offset, 0.0),
                source: SurfaceSource::Marquee,
                opacity: face.ink,
                ..anchored
            };
        }
        let font = self.display.font;
        let kind = crate::fonts::segment_at(font, cell);
        debug_assert!(kind.is_some(), "a lit cell lies in a segment");
        let content =
            Content::digit_cell(font, slot, kind.unwrap_or(crate::SegmentKind::Top), cell);
        let playback = self.playback();
        // A clock recipe moves each cell rigidly, so its quad is the cell
        // turned, scaled and placed: the corners give the frame back.
        let quad = self.recipe.quad(content, self.placement(layout), playback);
        let edge = quad[1] - quad[0];
        let (_, brightness) = self.recipe.lighting.sample(
            content,
            self.content.bounds,
            playback.seconds,
            playback.progress,
        );
        let pose = SurfacePose {
            origin: (quad[0] + quad[2]) * 0.5,
            angle: edge.y.atan2(edge.x),
            scale: edge.length() / (2.0 * CELL_HALF * content.size),
            footprint: Footprint::CELL,
            source: SurfaceSource::Marquee,
            opacity: playback.strength * brightness,
            ..anchored
        };
        debug_assert!(
            ((pose.origin + (Vec2::splat(CELL_HALF) * pose.scale).rotate_radians(pose.angle))
                - quad[2])
                .length()
                <= 1e-3 * pose.scale,
            "a clock recipe moves a cell rigidly"
        );
        pose
    }

    /// A takeover cut short leaves the face wherever it had carried it, so
    /// the face's surfaces retire there rather than snap home, and the face
    /// admits new ones. One that never moved the face hands nothing over.
    pub fn release_surfaces(&self, surfaces: &mut SurfaceLedger) {
        let moves =
            !self.uses_clock || self.recipe.rotation.is_some() || self.recipe.wave.is_some();
        if self.tick < MARQUEE_TICKS && moves && self.playback().strength > 0.0 {
            for slot in 0..DIGIT_SLOT_COUNT {
                surfaces.retire_face(slot, Retirement::Interrupted);
            }
        }
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
                self.message.as_str()
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
