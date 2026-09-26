use super::*;
use crate::events::{
    EventPhase,
    explosion::{ExplosionEvent, WARNING_TICKS},
};

pub(super) fn progress(event: &ExplosionEvent) -> f32 {
    if event.phase() != EventPhase::Reforming {
        return 0.0;
    }
    let t = (event.phase_tick() as f32 / REFORMING_TICKS as f32).min(1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn render(
    frame: &mut RenderFrame,
    state: &ClockState,
    event: &ExplosionEvent,
    layout: Layout,
) {
    let progress = progress(event);
    let mut palette = DigitPalette::default();
    if event.phase() == EventPhase::Warning {
        // One smooth amber pulse, not a strobe or full-screen flash.
        let pulse = (std::f32::consts::PI * event.phase_tick() as f32 / WARNING_TICKS as f32).sin();
        palette.fill = RenderColor::rgb(
            palette.fill.r + (1.0 - palette.fill.r) * pulse,
            palette.fill.g + (0.72 - palette.fill.g) * pulse,
            palette.fill.b * (1.0 - pulse),
        );
    }
    for segment in state.segments() {
        for cell in digits::cells(segment.id.kind) {
            render_square(
                frame,
                layout.cell_center(segment.id, *cell),
                layout.pitch,
                0.0,
                if segment.lit { progress } else { 0.0 },
                DigitPalette::default(),
            );
        }
    }
    for cell in &event.cells {
        let position = cell.position + (cell.origin - cell.position) * progress;
        let angle = cell.angle * (1.0 - progress);
        if cell.label {
            meridiem::pixel(frame, position, cell.side, angle, 1.0 - progress);
        } else {
            render_square(
                frame,
                position,
                cell.side / 0.8,
                angle,
                1.0 - progress,
                palette,
            );
        }
    }
}
