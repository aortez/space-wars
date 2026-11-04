//! Unculled source-derived geometry for generic lighting capacity fixtures.
//! This is not Clock light admission: no event creates a physiological source.
use crate::{DisplaySnapshot, presentation::content::Content};
use engine_core::Vec2;
pub const MAX_TEXT_CELLS: usize = crate::presentation::content::MAX_CONTENT_CELLS;
pub fn clock_centers(display: DisplaySnapshot) -> Vec<Vec2> {
    Content::clock(display)
        .cells
        .into_iter()
        .map(|c| c.center)
        .collect()
}
pub fn text_centers(text: &str) -> Result<Vec<Vec2>, engine_common::ClockMessageError> {
    Ok(Content::text(text)?
        .cells
        .into_iter()
        .map(|c| c.center)
        .collect())
}
/// Uses the actual composed effect transform before clipping, including offscreen
/// cells. Presentation duplication never adds points to this geometry.
pub fn transformed_text_centers(
    text: &str,
    progress: f32,
) -> Result<Vec<Vec2>, engine_common::ClockMessageError> {
    use crate::presentation::{Bounds, effects::*};
    let content = Content::text(text)?;
    let recipe = Recipe {
        scroll: true,
        rotation: Some(Rotation {
            target: Target::Glyph,
            turns: 1.0,
        }),
        wave: Some(Wave {
            target: Target::Cell,
            amplitude: 1.0,
            wavelength: 12.0,
            cycles_per_second: 0.5,
        }),
        ..Default::default()
    };
    let placement = Placement {
        content: content.bounds,
        viewport: Bounds {
            min: Vec2::ZERO,
            max: Vec2::new(200.0, 30.0),
        },
        center: Vec2::new(100.0, 15.0),
        pitch: 1.0,
    };
    let playback = Playback {
        seconds: progress * 2.0,
        progress,
        strength: 1.0,
    };
    Ok(content
        .cells
        .into_iter()
        .map(|cell| {
            let quad = recipe.quad(cell, placement, playback);
            quad.into_iter().fold(Vec2::ZERO, |a, p| a + p) / 4.0
        })
        .collect())
}
