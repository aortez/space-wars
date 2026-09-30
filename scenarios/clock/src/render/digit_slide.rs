use engine_common::{RenderFrame, RenderPoint};
use engine_core::Vec2;

use crate::{
    DigitPalette, SegmentId, SegmentKind, events::digit_slide::DigitSlideEvent, layout::Layout,
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
        let from = crate::fonts::glyph(event.from_font, event.from[slot]);
        let to = crate::fonts::glyph(event.to_font, event.to[slot]);
        let guides = crate::fonts::CellMask(
            crate::fonts::guides(event.from_font).0 | crate::fonts::guides(event.to_font).0,
        );
        let partition_font = if event.from_font == engine_common::ClockFont::Classic
            && event.to_font == engine_common::ClockFont::Classic
        {
            engine_common::ClockFont::Classic
        } else {
            engine_common::ClockFont::Matrix
        };
        for kind in SegmentKind::ALL {
            let id = SegmentId {
                digit_slot: slot as u8,
                kind,
            };
            for cell in
                crate::fonts::CellMask(guides.0 & crate::fonts::region(partition_font, kind).0)
                    .cells()
            {
                let center = layout.cell_center(id, cell);
                if !event.changed[slot] {
                    super::render_square(
                        frame,
                        center,
                        layout.pitch,
                        0.0,
                        f32::from(to.contains(cell)),
                        palette,
                    );
                    continue;
                }
                super::render_square(frame, center, layout.pitch, 0.0, 0.0, palette);
                for (glyph, offset) in [(from, -progress * height), (to, (1.0 - progress) * height)]
                {
                    if glyph.contains(cell) {
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
    let min = RenderPoint::new(center.x - half, (center.y - half).max(lower));
    let max = RenderPoint::new(center.x + half, (center.y + half).min(upper));
    if max.y <= min.y {
        return;
    }
    super::glow::quad(
        frame,
        [
            min,
            RenderPoint::new(max.x, min.y),
            max,
            RenderPoint::new(min.x, max.y),
        ],
        palette.fill,
        Some(crate::presentation::Bounds {
            min: Vec2::new(layout.bounds_min.x, lower),
            max: Vec2::new(layout.bounds_max.x, upper),
        }),
    );
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
