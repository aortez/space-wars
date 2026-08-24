use engine_common::{
    Camera2, Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
    RenderText, Stroke, TextAnchor,
};

use crate::{ClockState, digits};

const CAMERA_HEIGHT: f32 = 480.0;
const BACKGROUND_LAYER: i32 = 0;
const ARENA_LAYER: i32 = 1;
const INACTIVE_CELL_LAYER: i32 = 2;
const ACTIVE_CELL_LAYER: i32 = 3;
const LABEL_LAYER: i32 = 4;

const BACKGROUND_COLOR: RenderColor = RenderColor::rgb(0.018, 0.025, 0.055);
const FLOOR_COLOR: RenderColor = RenderColor::rgb(0.075, 0.105, 0.145);
const FLOOR_EDGE_COLOR: RenderColor = RenderColor::rgb(0.19, 0.40, 0.52);
const INACTIVE_CELL_COLOR: RenderColor = RenderColor::rgb(0.045, 0.105, 0.135);
const ACTIVE_CELL_COLOR: RenderColor = RenderColor::rgb(0.33, 0.94, 0.91);
const ACTIVE_CELL_EDGE_COLOR: RenderColor = RenderColor::rgb(0.72, 1.0, 0.96);
const LABEL_COLOR: RenderColor = RenderColor::rgb(0.52, 0.72, 0.77);

const DIGIT_ORIGINS: [f32; 4] = [0.0, 7.0, 17.0, 24.0];
const FACE_WIDTH_UNITS: f32 = 30.0;
const FACE_HEIGHT_UNITS: f32 = 9.0;
const COLON_X_UNITS: f32 = 14.5;
const COLON_Y_UNITS: [f32; 2] = [2.25, 5.75];

#[derive(Debug, Clone, Copy)]
struct Layout {
    bounds_min: RenderPoint,
    bounds_max: RenderPoint,
    pitch: f32,
    face_origin: RenderPoint,
    floor_y: f32,
}

impl Layout {
    fn new(aspect_ratio: f32) -> Self {
        let world_width = CAMERA_HEIGHT * aspect_ratio;
        let bounds_min = RenderPoint::new(-world_width * 0.5, -CAMERA_HEIGHT * 0.5);
        let bounds_max = RenderPoint::new(world_width * 0.5, CAMERA_HEIGHT * 0.5);
        let floor_y = bounds_min.y + CAMERA_HEIGHT * 0.16;
        let horizontal_margin = (world_width * 0.06).max(16.0);
        let face_bottom = floor_y + CAMERA_HEIGHT * 0.08;
        let face_top = bounds_max.y - CAMERA_HEIGHT * 0.10;
        let pitch = ((world_width - horizontal_margin * 2.0) / FACE_WIDTH_UNITS)
            .min((face_top - face_bottom) / (FACE_HEIGHT_UNITS + 1.0))
            .max(2.0);
        let face_origin = RenderPoint::new(
            -FACE_WIDTH_UNITS * pitch * 0.5,
            (face_bottom + face_top - FACE_HEIGHT_UNITS * pitch) * 0.5,
        );

        Self {
            bounds_min,
            bounds_max,
            pitch,
            face_origin,
            floor_y,
        }
    }

    fn digit_origin(self, slot: usize) -> RenderPoint {
        RenderPoint::new(
            self.face_origin.x + DIGIT_ORIGINS[slot] * self.pitch,
            self.face_origin.y,
        )
    }
}

pub fn render_frame(state: &ClockState) -> RenderFrame {
    let layout = Layout::new(state.aspect_ratio());
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    frame.push_primitive(
        BACKGROUND_LAYER,
        rectangle(layout.bounds_min, layout.bounds_max, BACKGROUND_COLOR, None),
    );
    render_floor(&mut frame, layout);
    render_segments(&mut frame, state, layout);
    render_colon(&mut frame, state, layout);
    render_meridiem(&mut frame, state, layout);
    frame
}

fn render_floor(frame: &mut RenderFrame, layout: Layout) {
    let drain_half_width = (layout.pitch * 0.85).max(10.0);
    let left_max = RenderPoint::new(-drain_half_width, layout.floor_y);
    let right_min = RenderPoint::new(drain_half_width, layout.bounds_min.y);
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(layout.bounds_min, left_max, FLOOR_COLOR, None),
    );
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            right_min,
            RenderPoint::new(layout.bounds_max.x, layout.floor_y),
            FLOOR_COLOR,
            None,
        ),
    );

    let edge_height = (layout.pitch * 0.10).clamp(1.5, 3.0);
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            RenderPoint::new(layout.bounds_min.x, layout.floor_y - edge_height),
            RenderPoint::new(-drain_half_width, layout.floor_y),
            FLOOR_EDGE_COLOR,
            None,
        ),
    );
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            RenderPoint::new(drain_half_width, layout.floor_y - edge_height),
            RenderPoint::new(layout.bounds_max.x, layout.floor_y),
            FLOOR_EDGE_COLOR,
            None,
        ),
    );
}

fn render_segments(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    for segment in state.segments() {
        let origin = layout.digit_origin(usize::from(segment.id.digit_slot));
        for cell in digits::cells(segment.id.kind) {
            render_cell(
                frame,
                RenderPoint::new(
                    origin.x + f32::from(cell.x) * layout.pitch,
                    origin.y + f32::from(cell.y) * layout.pitch,
                ),
                layout.pitch,
                segment.lit,
            );
        }
    }
}

fn render_colon(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    for y in COLON_Y_UNITS {
        render_cell(
            frame,
            RenderPoint::new(
                layout.face_origin.x + COLON_X_UNITS * layout.pitch,
                layout.face_origin.y + y * layout.pitch,
            ),
            layout.pitch,
            state.display().colon_lit,
        );
    }
}

fn render_meridiem(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    let Some(meridiem) = state.display().meridiem else {
        return;
    };
    let mut label = RenderText::new(
        RenderPoint::new(
            layout.face_origin.x + FACE_WIDTH_UNITS * layout.pitch,
            layout.face_origin.y - layout.pitch * 0.20,
        ),
        meridiem,
    );
    label.color = LABEL_COLOR;
    label.size = (layout.pitch * 0.68).clamp(10.0, 20.0);
    label.anchor = TextAnchor::TopLeft;
    frame.push_primitive(LABEL_LAYER, RenderPrimitive::Text(label));
}

fn render_cell(frame: &mut RenderFrame, lower_left: RenderPoint, pitch: f32, lit: bool) {
    let size = pitch * 0.80;
    let inset = (pitch - size) * 0.5;
    let min = RenderPoint::new(lower_left.x + inset, lower_left.y + inset);
    let max = RenderPoint::new(min.x + size, min.y + size);
    let layer = if lit {
        ACTIVE_CELL_LAYER
    } else {
        INACTIVE_CELL_LAYER
    };
    let color = if lit {
        ACTIVE_CELL_COLOR
    } else {
        INACTIVE_CELL_COLOR
    };
    let stroke = lit.then(|| Stroke::new(ACTIVE_CELL_EDGE_COLOR, (pitch * 0.045).max(0.8)));
    frame.push_primitive(layer, rectangle(min, max, color, stroke));
}

fn rectangle(
    min: RenderPoint,
    max: RenderPoint,
    color: RenderColor,
    stroke: Option<Stroke>,
) -> RenderPrimitive {
    RenderPrimitive::Polygon(RenderPolygon {
        points: vec![
            min,
            RenderPoint::new(max.x, min.y),
            max,
            RenderPoint::new(min.x, max.y),
        ],
        fill: Some(Fill::new(color)),
        stroke,
    })
}

#[cfg(test)]
mod tests {
    use engine_common::{ClockTimeFormat, RenderPrimitive, Scenario};

    use super::*;
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};

    fn rendered_at(aspect_ratio: f32) -> RenderFrame {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio,
                time_format: ClockTimeFormat::TwentyFourHour,
            },
            7,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 58, 2).unwrap(),
            )],
            std::time::Duration::ZERO,
        );
        ClockScenario::render_frame(&state)
    }

    #[test]
    fn cells_remain_inside_landscape_and_portrait_cameras() {
        for aspect_ratio in [5.0 / 3.0, 16.0 / 9.0, 3.0 / 4.0] {
            let frame = rendered_at(aspect_ratio);
            let bounds = frame.camera.world_bounds(aspect_ratio);
            for polygon in frame.layers.iter().flat_map(|layer| &layer.primitives) {
                let RenderPrimitive::Polygon(polygon) = polygon else {
                    continue;
                };
                for point in &polygon.points {
                    assert!(point.x >= bounds.min.x - f32::EPSILON);
                    assert!(point.x <= bounds.max.x + f32::EPSILON);
                    assert!(point.y >= bounds.min.y - f32::EPSILON);
                    assert!(point.y <= bounds.max.y + f32::EPSILON);
                }
            }
        }
    }

    #[test]
    fn normal_face_has_a_small_bounded_primitive_count() {
        let frame = rendered_at(5.0 / 3.0);
        let primitive_count = frame
            .layers
            .iter()
            .map(|layer| layer.primitives.len())
            .sum::<usize>();
        assert_eq!(primitive_count, 103);
    }
}
