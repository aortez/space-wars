//! Reels drawn as travelling cells clipped to their own slot, then a chase over
//! the settled reading. Nothing is faded into place: a slot only ever shows
//! glyphs on their way through it, and the dim anchors stay put behind them.

use engine_common::RenderFrame;
use engine_core::Vec2;

use crate::{
    DIGIT_SLOT_COUNT, DigitPalette, events::slot_machine::SlotMachineEvent, layout::Layout,
    presentation::content::Content,
};

pub(super) fn render(
    frame: &mut RenderFrame,
    event: &SlotMachineEvent,
    layout: Layout,
    palette: DigitPalette,
) {
    // The anchors are the substrate and never move, exactly as a fall or an
    // explosion leaves them.
    let guides = crate::fonts::guides(event.font());
    for slot in 0..DIGIT_SLOT_COUNT {
        for cell in guides.cells() {
            super::render_square(
                frame,
                centre(layout, slot, cell),
                layout.pitch,
                0.0,
                0.0,
                palette,
            );
        }
    }
    if event.paying_out() {
        payout(frame, event, layout);
        return;
    }
    let height = 9.0 * layout.pitch;
    for slot in 0..DIGIT_SLOT_COUNT {
        let travel = event.travel(slot);
        let settled = travel.floor();
        // A slot window is one glyph tall, so at most the glyph leaving it and
        // the one arriving are ever visible.
        for step in 0..2 {
            let index = settled as u32 + step;
            let offset = Vec2::new(0.0, (index as f32 - travel) * height);
            for cell in event.glyph(slot, index).cells() {
                let at = centre(layout, slot, cell) + offset;
                // Only a cell straddling the band's edge needs the clipped path,
                // which costs an extra primitive and trades its stroke for two
                // rectangles. The interior of a reel is most of it, so the
                // ordinary cell is both cheaper and better drawn.
                let half = layout.pitch * 0.4;
                if at.y - half < layout.face_origin.y || at.y + half > layout.face_origin.y + height
                {
                    super::digit_slide::clipped_cell(frame, at, layout, palette);
                } else {
                    super::render_square(frame, at, layout.pitch, 0.0, 1.0, palette);
                }
            }
        }
    }
}

/// The reading the reels landed on, lit by the chase the Marquee already uses.
/// The cells sit at their resting places with their own brightness sampled per
/// cell, which is a lighting pattern rather than the reading being put out.
fn payout(frame: &mut RenderFrame, event: &SlotMachineEvent, layout: Layout) {
    let content = Content::clock(event.settled());
    for cell in &content.cells {
        // The colon and the label follow live time and are drawn by the ordinary
        // face, so only the digit groups pay out.
        if usize::from(cell.group) >= DIGIT_SLOT_COUNT {
            continue;
        }
        let (palette, brightness) = event.payout_light(*cell);
        super::render_square(
            frame,
            Vec2::new(
                layout.face_origin.x + cell.center.x * layout.pitch,
                layout.face_origin.y + cell.center.y * layout.pitch,
            ),
            layout.pitch,
            0.0,
            brightness,
            palette,
        );
    }
}

fn centre(layout: Layout, slot: usize, cell: crate::GridCell) -> Vec2 {
    let origin = layout.digit_origin(slot);
    Vec2::new(
        origin.x + (f32::from(cell.x) + 0.5) * layout.pitch,
        origin.y + (f32::from(cell.y) + 0.5) * layout.pitch,
    )
}
