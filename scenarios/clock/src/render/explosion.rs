use super::*;
use crate::events::{
    EventPhase,
    explosion::{ExplosionEvent, WARNING_TICKS},
};

pub(super) fn render(
    frame: &mut RenderFrame,
    state: &ClockState,
    event: &ExplosionEvent,
    layout: Layout,
) {
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
        for cell in segment.guides() {
            render_square(
                frame,
                layout.cell_center(segment.id, cell),
                layout.pitch,
                0.0,
                0.0,
                DigitPalette::default(),
            );
        }
    }
    for cell in &event.cells {
        if cell.side <= 0.0 {
            continue;
        }
        if cell.label {
            meridiem::pixel(frame, cell.position, cell.side, cell.angle, 1.0);
        } else {
            render_square(
                frame,
                cell.position,
                cell.side / 0.8,
                cell.angle,
                1.0,
                palette,
            );
        }
    }
}
