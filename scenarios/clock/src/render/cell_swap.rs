use engine_common::RenderFrame;

use crate::{
    DigitPalette,
    events::cell_swap::{CELL_SWAP_TICKS, CellPosition, CellSwapEvent},
    layout::Layout,
};

pub(super) fn render(
    frame: &mut RenderFrame,
    event: &CellSwapEvent,
    layout: Layout,
    palette: DigitPalette,
) {
    for slot in 0..crate::DIGIT_SLOT_COUNT {
        let glyph = crate::fonts::glyph(event.from.font, event.from.digits[slot]);
        let guides = crate::fonts::guides(event.from.font);
        for cell in crate::SegmentKind::ALL.into_iter().flat_map(|kind| {
            crate::fonts::CellMask(guides.0 & crate::fonts::region(event.from.font, kind).0).cells()
        }) {
            let position = CellPosition {
                slot: slot as u8,
                cell,
            };
            // Match the ordinary face's compositing: no extra dark tile under
            // a lit cell's antialiased edge, including at the first frame.
            if event.tick < CELL_SWAP_TICKS && glyph.contains(cell) {
                continue;
            }
            super::render_square(
                frame,
                position.center(layout),
                layout.pitch,
                0.0,
                if event.tick >= CELL_SWAP_TICKS {
                    event.opacity[position.index()]
                } else {
                    0.0
                },
                palette,
            );
        }
    }
    if event.tick < CELL_SWAP_TICKS {
        for exchange in &event.exchanges {
            super::render_square(
                frame,
                exchange.centre(layout, event.progress()),
                layout.pitch,
                0.0,
                1.0,
                palette,
            );
        }
    }
}
