use engine_common::{
    Camera2, Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive, Stroke,
};

use crate::{
    ClockState, DigitPalette, REFORMING_TICKS, SegmentRepresentation, digits,
    layout::{CAMERA_HEIGHT, Layout},
};
use engine_core::Vec2;

mod digit_slide;
mod duck;
mod marquee;
mod meltdown;
mod meridiem;
mod rain;

const BACKGROUND_LAYER: i32 = 0;
const ARENA_LAYER: i32 = 1;
const INACTIVE_CELL_LAYER: i32 = 2;
const ACTIVE_CELL_LAYER: i32 = 3;
const LABEL_LAYER: i32 = 4;

const BACKGROUND_COLOR: RenderColor = RenderColor::rgb(0.018, 0.025, 0.055);
const FLOOR_COLOR: RenderColor = RenderColor::rgb(0.075, 0.105, 0.145);
const FLOOR_EDGE_COLOR: RenderColor = RenderColor::rgb(0.19, 0.40, 0.52);
const INACTIVE_CELL_COLOR: RenderColor = RenderColor::rgb(0.045, 0.105, 0.135);
const LABEL_COLOR: RenderColor = RenderColor::rgb(0.52, 0.72, 0.77);

const COLON_X_UNITS: f32 = 14.5;
const COLON_Y_UNITS: [f32; 2] = [2.25, 5.75];

pub(crate) fn water_fixture(frame: &mut RenderFrame, water: &engine_water::WaterWorld) {
    meltdown::render_water(frame, water, ACTIVE_CELL_LAYER, 1.0);
}

pub(crate) fn water_fixture_cell(frame: &mut RenderFrame, center: Vec2, pitch: f32, lit: bool) {
    render_square(
        frame,
        center,
        pitch,
        0.0,
        f32::from(lit),
        DigitPalette::default(),
    );
}

pub fn render_frame(state: &ClockState) -> RenderFrame {
    let layout = Layout::new(state.aspect_ratio());
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    frame.push_primitive(
        BACKGROUND_LAYER,
        rectangle(layout.bounds_min, layout.bounds_max, BACKGROUND_COLOR, None),
    );
    if let Some(crate::events::ActiveEvent::Duck(event)) = &state.active_event {
        // The custom course owns its floor, including the entrance/exit fade.
        // This backdrop never adds a collider across the course's physical pit.
        render_floor(
            &mut frame,
            crate::floor::FloorGeometry::closed(layout),
            layout.pitch,
            1.0 - event.course_opacity(),
        );
    } else if let Some(crate::events::ActiveEvent::Rain(event)) = &state.active_event {
        rain::floor(&mut frame, event);
        render_floor(
            &mut frame,
            crate::floor::FloorGeometry::closed(layout),
            layout.pitch,
            1.0 - event.opacity(),
        );
    } else {
        render_floor(&mut frame, state.floor.geometry(layout), layout.pitch, 1.0);
    }
    if let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event {
        let opacity = 1.0 - event.playback().strength;
        if opacity > 0.0 {
            render_segments(&mut frame, state, layout);
            render_colon(&mut frame, state, layout);
            render_meridiem(&mut frame, state, layout);
            // Fade only newly generated face primitives, never physical state
            // or arena/background. No offscreen image or extra frame allocation.
            for layer in &mut frame.layers {
                if layer.z < INACTIVE_CELL_LAYER {
                    continue;
                }
                for primitive in &mut layer.primitives {
                    if let RenderPrimitive::Polygon(polygon) = primitive {
                        if let Some(fill) = &mut polygon.fill {
                            fill.color.a *= opacity;
                        }
                        if let Some(stroke) = &mut polygon.stroke {
                            stroke.color.a *= opacity;
                        }
                    }
                }
            }
        }
        marquee::render(&mut frame, event, layout);
        return frame;
    }
    render_segments(&mut frame, state, layout);
    if let Some(crate::events::ActiveEvent::Rain(event)) = &state.active_event {
        rain::render(&mut frame, event);
    }
    if let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event {
        meltdown::render(&mut frame, event, layout);
    }
    if let Some(crate::events::ActiveEvent::Duck(event)) = &state.active_event {
        duck::render(&mut frame, event, state.config.duck_debug_overlay);
    }
    render_colon(&mut frame, state, layout);
    render_meridiem(&mut frame, state, layout);
    frame
}

fn render_floor(
    frame: &mut RenderFrame,
    floor: crate::floor::FloorGeometry,
    pitch: f32,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let edge_height = (pitch * 0.10).clamp(1.5, 3.0);
    for (min, max) in floor.slabs() {
        frame.push_primitive(
            ARENA_LAYER,
            rectangle(
                RenderPoint::new(min.x, min.y),
                RenderPoint::new(max.x, max.y),
                RenderColor {
                    a: opacity,
                    ..FLOOR_COLOR
                },
                None,
            ),
        );
        frame.push_primitive(
            ARENA_LAYER,
            rectangle(
                RenderPoint::new(min.x, max.y - edge_height),
                RenderPoint::new(max.x, max.y),
                RenderColor {
                    a: opacity,
                    ..FLOOR_EDGE_COLOR
                },
                None,
            ),
        );
    }
}

fn render_segments(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    if let Some(crate::events::ActiveEvent::DigitSlide(event)) = &state.active_event {
        digit_slide::render(frame, event, layout, state.palette());
        return;
    }
    if matches!(&state.active_event, Some(crate::events::ActiveEvent::Meltdown(event))
        if !event.lab && event.phase() == crate::events::EventPhase::Reforming)
    {
        meltdown::render_reforming_face(frame, state, layout);
        return;
    }
    let palette = state.palette();
    let t = (state.phase_tick() as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
    let progress = t * t * (3.0 - 2.0 * t);
    for segment in state.segments() {
        let anchor = layout.segment_center(segment.id);
        let (position, angle, brightness) = match segment.representation {
            SegmentRepresentation::Anchored => (anchor, 0.0, f32::from(segment.lit)),
            SegmentRepresentation::Disintegrated => (anchor, 0.0, 0.0),
            SegmentRepresentation::Rigid { position, angle } => (position, angle, 1.0),
            SegmentRepresentation::Reforming {
                position,
                angle,
                was_lit,
            } => (
                position + (anchor - position) * progress,
                angle * (1.0 - progress),
                f32::from(was_lit) * (1.0 - progress) + f32::from(segment.lit) * progress,
            ),
        };
        for cell in digits::cells(segment.id.kind) {
            let center = layout.cell_center(segment.id, *cell);
            if segment.representation == SegmentRepresentation::Anchored {
                render_square(frame, center, layout.pitch, 0.0, brightness, palette);
            } else {
                // Keep a faint clock outline while the illuminated bars move.
                render_square(frame, center, layout.pitch, 0.0, 0.0, palette);
                if brightness > 0.0 {
                    render_square(
                        frame,
                        position + (center - anchor).rotate_radians(angle),
                        layout.pitch,
                        angle,
                        brightness,
                        palette,
                    );
                }
            }
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
            state.palette(),
        );
    }
}

fn render_meridiem(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    meridiem::render(frame, state, layout);
}

fn render_cell(
    frame: &mut RenderFrame,
    lower_left: RenderPoint,
    pitch: f32,
    lit: bool,
    palette: DigitPalette,
) {
    render_square(
        frame,
        Vec2::new(lower_left.x + pitch * 0.5, lower_left.y + pitch * 0.5),
        pitch,
        0.0,
        f32::from(lit),
        palette,
    );
}

fn render_square(
    frame: &mut RenderFrame,
    center: Vec2,
    pitch: f32,
    angle: f32,
    brightness: f32,
    palette: DigitPalette,
) {
    let layer = if brightness > 0.0 {
        ACTIVE_CELL_LAYER
    } else {
        INACTIVE_CELL_LAYER
    };
    let color = if brightness > 0.0 {
        RenderColor {
            a: brightness,
            ..palette.fill
        }
    } else {
        INACTIVE_CELL_COLOR
    };
    let stroke = (brightness > 0.0).then(|| {
        Stroke::new(
            RenderColor {
                a: brightness,
                ..palette.edge
            },
            (pitch * 0.045).max(0.8),
        )
    });
    let half = pitch * 0.4;
    let points = [
        Vec2::new(-half, -half),
        Vec2::new(half, -half),
        Vec2::new(half, half),
        Vec2::new(-half, half),
    ]
    .map(|offset| {
        let point = center + offset.rotate_radians(angle);
        RenderPoint::new(point.x, point.y)
    })
    .to_vec();
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points,
            fill: Some(Fill::new(color)),
            stroke,
        }),
    );
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
                ..ClockConfig::default()
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
        // One background, one closed floor slab and edge, 96 cells, two dots.
        assert_eq!(primitive_count, 101);
    }

    #[test]
    fn twelve_hour_meridiem_is_visible_to_both_render_backends() {
        for hour in [0, 12] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    time_format: ClockTimeFormat::TwelveHour,
                    ..ClockConfig::default()
                },
                0,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(hour, 0, 0).unwrap(),
                )],
                std::time::Duration::ZERO,
            );
            let frame = ClockScenario::render_frame(&state);
            let label = frame
                .layers
                .iter()
                .find(|layer| layer.z == LABEL_LAYER)
                .unwrap();
            assert!(!label.primitives.is_empty());
            assert!(
                label
                    .primitives
                    .iter()
                    .all(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
            );
        }
    }
}
