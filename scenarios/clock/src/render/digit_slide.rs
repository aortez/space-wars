use engine_common::{RenderFrame, RenderPoint};
use engine_core::Vec2;

use crate::{
    DigitPalette, SegmentId, SegmentKind, digits, events::digit_slide::DigitSlideEvent,
    layout::Layout,
};

pub(super) fn render(
    frame: &mut RenderFrame,
    event: &DigitSlideEvent,
    layout: Layout,
    palette: DigitPalette,
) {
    let height = 9.0 * layout.pitch;
    let progress = event.progress();
    for slot in 0..crate::DIGIT_SLOT_COUNT {
        let lit = |digit: Option<u8>, kind: SegmentKind| {
            digit.is_some_and(|digit| digits::digit_mask(digit) & (1 << kind as u8) != 0)
        };
        for kind in SegmentKind::ALL {
            let id = SegmentId {
                digit_slot: slot as u8,
                kind,
            };
            for cell in digits::cells(kind) {
                let center = layout.cell_center(id, *cell);
                if !event.changed[slot] {
                    super::render_square(
                        frame,
                        center,
                        layout.pitch,
                        0.0,
                        f32::from(lit(event.to[slot], kind)),
                        palette,
                    );
                    continue;
                }
                // Keep a stable dim slot while old cells leave below and the
                // new cells enter from above. Both are clipped to this slot.
                super::render_square(frame, center, layout.pitch, 0.0, 0.0, palette);
                for (digit, offset) in [
                    (event.from[slot], -progress * height),
                    (event.to[slot], (1.0 - progress) * height),
                ] {
                    if lit(digit, kind) {
                        clipped_cell(frame, center + Vec2::new(0.0, offset), layout, palette);
                    }
                }
            }
        }
    }
}

fn clipped_cell(frame: &mut RenderFrame, center: Vec2, layout: Layout, palette: DigitPalette) {
    let lower = layout.face_origin.y;
    let upper = lower + 9.0 * layout.pitch;
    let half = layout.pitch * 0.4;
    let stroke_half = (layout.pitch * 0.045).max(0.8) * 0.5;
    if center.y - half - stroke_half >= lower && center.y + half + stroke_half <= upper {
        super::render_square(frame, center, layout.pitch, 0.0, 1.0, palette);
        return;
    }
    // Clip the edge and fill separately, so the clip boundary does not acquire
    // a false stroke or let half a stroke bleed into the rest of the arena.
    for (radius, color) in [
        (half + stroke_half, palette.edge),
        (half - stroke_half, palette.fill),
    ] {
        let min = RenderPoint::new(center.x - radius, (center.y - radius).max(lower));
        let max = RenderPoint::new(center.x + radius, (center.y + radius).min(upper));
        if max.y > min.y {
            frame.push_primitive(
                super::ACTIVE_CELL_LAYER,
                super::rectangle(min, max, color, None),
            );
        }
    }
}
