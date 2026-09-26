use super::*;
use crate::{
    events::{ActiveEvent, EventPhase},
    meridiem::{Glyph, PIXEL_SIZE},
};

#[cfg(test)]
mod tests;

pub(super) fn pixel(frame: &mut RenderFrame, center: Vec2, side: f32, angle: f32, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    let half = side * 0.5;
    let points = [
        Vec2::new(-half, -half),
        Vec2::new(half, -half),
        Vec2::new(half, half),
        Vec2::new(-half, half),
    ]
    .map(|p| {
        let p = center + p.rotate_radians(angle);
        RenderPoint::new(p.x, p.y)
    })
    .to_vec();
    frame.push_primitive(
        LABEL_LAYER,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            points,
            RenderColor {
                a: opacity,
                ..LABEL_COLOR
            },
        )),
    );
}

pub(super) fn render(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let latest = state.display().meridiem.map(Glyph::for_label);
    if let Some(ActiveEvent::Explosion(event)) = &state.active_event {
        // Source pixels belong to the event. Fade in only the latest label as
        // those pixels return, including AM/PM and 12/24-hour changes mid-burst.
        let opacity = super::explosion::progress(event);
        for glyph in latest.into_iter().flatten() {
            for cell in glyph.cells() {
                pixel(
                    frame,
                    glyph.cell_center(layout, cell),
                    layout.pitch * PIXEL_SIZE,
                    0.0,
                    opacity,
                );
            }
        }
        return;
    }
    if let Some(ActiveEvent::Falling(event)) = &state.active_event {
        if event.phase() == EventPhase::Falling {
            for letter in event.letters().into_iter().flatten() {
                for cell in letter.glyph.cells() {
                    pixel(
                        frame,
                        letter.position
                            + (letter.glyph.cell_center(layout, cell)
                                - letter.glyph.center(layout))
                            .rotate_radians(letter.angle),
                        layout.pitch * PIXEL_SIZE,
                        letter.angle,
                        1.0,
                    );
                }
            }
        } else {
            let t = state.phase_tick() as f32 / REFORMING_TICKS as f32;
            let progress = t * t * (3.0 - 2.0 * t);
            for slot in 0..2 {
                let source = event.letters().map(|letters| letters[slot]);
                let target = latest.map(|glyphs| glyphs[slot]);
                let Some(glyph) = target.or(source.map(|letter| letter.glyph)) else {
                    continue;
                };
                let anchor = glyph.center(layout);
                let (position, angle) = source.map_or((anchor, 0.0), |letter| {
                    (
                        letter.position + (anchor - letter.position) * progress,
                        letter.angle * (1.0 - progress),
                    )
                });
                for y in (0..5).rev() {
                    for x in 0..glyph.width() {
                        let cell = digits::GridCell { x, y };
                        let was_lit = source.is_some_and(|letter| letter.glyph.lit(cell));
                        let lit = target.is_some_and(|glyph| glyph.lit(cell));
                        let opacity =
                            f32::from(was_lit) * (1.0 - progress) + f32::from(lit) * progress;
                        pixel(
                            frame,
                            position
                                + (glyph.cell_center(layout, cell) - anchor).rotate_radians(angle),
                            layout.pitch * PIXEL_SIZE,
                            angle,
                            opacity,
                        );
                    }
                }
            }
        }
        return;
    }
    let melting = matches!(&state.active_event, Some(ActiveEvent::Meltdown(event)) if !event.lab);
    if melting && state.event_phase() != Some(EventPhase::Reforming) {
        // The event owns the original label's ballistic pixels, just like digits.
        return;
    }
    for glyph in latest.into_iter().flatten() {
        for cell in glyph.cells() {
            let center = glyph.cell_center(layout, cell);
            let side = layout.pitch * PIXEL_SIZE;
            let progress = if melting {
                super::meltdown::reform_progress(state.phase_tick(), cell.y)
            } else {
                1.0
            };
            if progress <= 0.0 {
                continue;
            }
            if progress == 1.0 {
                pixel(frame, center, side, 0.0, 1.0);
            } else {
                let min = RenderPoint::new(center.x - side * 0.5, center.y - side * 0.5);
                frame.push_primitive(
                    LABEL_LAYER,
                    rectangle(
                        min,
                        RenderPoint::new(min.x + side, min.y + side * progress),
                        RenderColor {
                            a: progress,
                            ..LABEL_COLOR
                        },
                        None,
                    ),
                );
            }
        }
    }
}
