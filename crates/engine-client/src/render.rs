//! Slint presentation adapter for scenario render frames.
//!
//! This is deliberately an engine-client boundary: scenarios emit
//! `engine_common::RenderFrame`; this module decides how to present that frame
//! in Slint. The first backend batches adjacent vector primitives into Slint
//! `Path` items so the proof path does not allocate one UI item per triangle.

use std::fmt::Write as _;
use std::time::Duration;

use engine_common::{
    Camera2, Fill, RenderCircle, RenderColor, RenderFrame, RenderLine, RenderPoint, RenderPolygon,
    RenderPrimitive, RenderText, Stroke, TextAnchor,
};
use slint::{Brush, Color, SharedString};

use crate::{PrimitiveKind, ScenePrimitive};

const TRANSPARENT: RenderColor = RenderColor::CLEAR;
const DEFAULT_VIEWPORT_WIDTH: f32 = 1280.0;
const DEFAULT_VIEWPORT_HEIGHT: f32 = 720.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub const fn new(width: f32, height: f32) -> Self {
        Self::with_origin(0.0, 0.0, width, height)
    }

    pub const fn with_origin(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_window(window: &slint::Window) -> Self {
        let physical_size = window.size();
        let scale_factor = window.scale_factor();
        let width = physical_size.width as f32 / scale_factor;
        let height = physical_size.height as f32 / scale_factor;
        Self::new(width, height).with_default_if_empty()
    }

    pub fn aspect_ratio(self) -> f32 {
        self.width / self.height
    }

    pub fn split_horizontally(self, count: usize) -> Vec<Self> {
        let count = count.max(1);
        let width = self.width / count as f32;
        (0..count)
            .map(|index| {
                Self::with_origin(self.x + width * index as f32, self.y, width, self.height)
            })
            .collect()
    }

    fn with_default_if_empty(self) -> Self {
        if self.width <= 0.0 || self.height <= 0.0 {
            Self::with_origin(
                self.x,
                self.y,
                DEFAULT_VIEWPORT_WIDTH,
                DEFAULT_VIEWPORT_HEIGHT,
            )
        } else {
            self
        }
    }
}

/// Convert scenario frames into ordered Slint scene primitives.
pub fn scene_primitives_from_frames(
    frames: &[RenderFrame],
    viewport: Viewport,
) -> Vec<ScenePrimitive> {
    if frames.is_empty() {
        return Vec::new();
    }

    if frames.len() == 1 {
        return scene_primitives_from_frame(&frames[0], viewport);
    }

    frames
        .iter()
        .zip(
            viewport
                .with_default_if_empty()
                .split_horizontally(frames.len()),
        )
        .flat_map(|(frame, viewport)| scene_primitives_from_frame(frame, viewport))
        .collect()
}

/// Convert a scenario frame into ordered Slint scene primitives.
pub fn scene_primitives_from_frame(frame: &RenderFrame, viewport: Viewport) -> Vec<ScenePrimitive> {
    let viewport = viewport.with_default_if_empty();
    let mut output = Vec::new();
    let mut batch = PathBatch::default();

    for layer in frame.ordered_layers() {
        for primitive in &layer.primitives {
            match primitive {
                RenderPrimitive::Circle(circle) => {
                    if let Some(fragment) = circle_fragment(circle, frame.camera, viewport) {
                        batch.push(fragment, viewport, &mut output);
                    }
                }
                RenderPrimitive::Line(line) => {
                    if let Some(fragment) = line_fragment(line, frame.camera, viewport) {
                        batch.push(fragment, viewport, &mut output);
                    }
                }
                RenderPrimitive::Polygon(polygon) => {
                    if let Some(fragment) = polygon_fragment(polygon, frame.camera, viewport) {
                        batch.push(fragment, viewport, &mut output);
                    }
                }
                RenderPrimitive::Text(text) => {
                    batch.flush(viewport, &mut output);
                    output.push(text_primitive(text, frame.camera, viewport));
                }
            }
        }
        batch.flush(viewport, &mut output);
    }

    output
}

pub fn debug_frame(elapsed: Duration, stress_triangles: usize) -> RenderFrame {
    let seconds = elapsed.as_secs_f32();
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 720.0));

    add_stress_triangles(&mut frame, stress_triangles, seconds);

    let orbit = RenderPoint::new(seconds.cos() * 220.0, seconds.sin() * 140.0);
    frame.push_primitive(
        -5,
        RenderPrimitive::Line(RenderLine::new(
            RenderPoint::new(-300.0, -220.0),
            orbit,
            Stroke::new(RenderColor::rgba(0.35, 0.85, 1.0, 0.85), 2.0),
        )),
    );
    frame.push_primitive(
        -4,
        RenderPrimitive::Circle(RenderCircle {
            center: RenderPoint::ZERO,
            radius: 115.0,
            fill: Some(Fill::new(RenderColor::rgba(0.1, 0.7, 0.42, 0.22))),
            stroke: Some(Stroke::new(RenderColor::rgba(0.7, 1.0, 0.82, 0.85), 2.0)),
        }),
    );
    frame.push_primitive(
        -3,
        RenderPrimitive::Circle(RenderCircle::filled(
            orbit,
            24.0 + seconds.sin() * 6.0,
            RenderColor::rgba(1.0, 0.92, 0.22, 0.85),
        )),
    );
    frame.push_primitive(
        0,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            rotated_triangle(RenderPoint::new(0.0, -35.0), 70.0, seconds * 1.7),
            RenderColor::rgba(1.0, 0.28, 0.23, 0.92),
        )),
    );

    let mut text = RenderText::new(
        RenderPoint::new(-620.0, 330.0),
        format!("M8b debug renderer | stress triangles: {stress_triangles}"),
    );
    text.color = RenderColor::rgba(0.9, 0.96, 1.0, 0.92);
    text.size = 18.0;
    frame.push_primitive(10, RenderPrimitive::Text(text));

    frame
}

fn add_stress_triangles(frame: &mut RenderFrame, count: usize, seconds: f32) {
    if count == 0 {
        return;
    }

    let columns = (count as f32).sqrt().ceil() as usize;
    let spacing = 18.0;
    let origin_x = -(columns as f32) * spacing * 0.5;
    let origin_y = -300.0;
    let color = RenderColor::rgba(0.45, 0.55, 0.95, 0.38);

    for i in 0..count {
        let col = i % columns;
        let row = i / columns;
        let wobble = ((i as f32 * 0.17) + seconds * 2.2).sin() * 2.5;
        let center = RenderPoint::new(
            origin_x + col as f32 * spacing,
            origin_y + row as f32 * spacing + wobble,
        );
        frame.push_primitive(
            -20,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                rotated_triangle(center, 6.0, seconds + i as f32 * 0.01),
                color,
            )),
        );
    }
}

fn rotated_triangle(center: RenderPoint, radius: f32, angle: f32) -> Vec<RenderPoint> {
    (0..3)
        .map(|i| {
            let theta = angle + i as f32 * std::f32::consts::TAU / 3.0;
            RenderPoint::new(
                center.x + theta.cos() * radius,
                center.y + theta.sin() * radius,
            )
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
struct PathBatch {
    style: Option<PathStyle>,
    commands: String,
}

impl PathBatch {
    fn push(
        &mut self,
        fragment: PathFragment,
        viewport: Viewport,
        output: &mut Vec<ScenePrimitive>,
    ) {
        if self.style != Some(fragment.style) {
            self.flush(viewport, output);
            self.style = Some(fragment.style);
        }

        if !self.commands.is_empty() {
            self.commands.push(' ');
        }
        self.commands.push_str(&fragment.commands);
    }

    fn flush(&mut self, viewport: Viewport, output: &mut Vec<ScenePrimitive>) {
        let Some(style) = self.style.take() else {
            return;
        };
        if self.commands.is_empty() {
            return;
        }

        output.push(path_primitive(
            std::mem::take(&mut self.commands),
            style,
            viewport,
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PathStyle {
    fill: RenderColor,
    stroke: RenderColor,
    stroke_width: f32,
}

impl PathStyle {
    fn new(fill: Option<Fill>, stroke: Option<Stroke>) -> Self {
        let stroke_width = stroke.map(|stroke| stroke.width).unwrap_or(0.0).max(0.0);
        Self {
            fill: fill.map(|fill| fill.color).unwrap_or(TRANSPARENT),
            stroke: stroke.map(|stroke| stroke.color).unwrap_or(TRANSPARENT),
            stroke_width,
        }
    }
}

struct PathFragment {
    style: PathStyle,
    commands: String,
}

fn circle_fragment(
    circle: &RenderCircle,
    camera: Camera2,
    viewport: Viewport,
) -> Option<PathFragment> {
    if circle.radius <= 0.0 {
        return None;
    }

    let center = project_point(camera, viewport, circle.center);
    let radius = circle.radius * viewport.height / camera.height;
    let mut commands = String::new();
    write!(
        commands,
        "M {} {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 {} {} Z",
        fmt(center.x + radius),
        fmt(center.y),
        fmt(radius),
        fmt(radius),
        fmt(center.x - radius),
        fmt(center.y),
        fmt(radius),
        fmt(radius),
        fmt(center.x + radius),
        fmt(center.y)
    )
    .expect("writing to String cannot fail");

    Some(PathFragment {
        style: PathStyle::new(circle.fill, circle.stroke),
        commands,
    })
}

fn line_fragment(line: &RenderLine, camera: Camera2, viewport: Viewport) -> Option<PathFragment> {
    if line.stroke.width <= 0.0 {
        return None;
    }

    let start = project_point(camera, viewport, line.start);
    let end = project_point(camera, viewport, line.end);
    Some(PathFragment {
        style: PathStyle::new(None, Some(line.stroke)),
        commands: format!(
            "M {} {} L {} {}",
            fmt(start.x),
            fmt(start.y),
            fmt(end.x),
            fmt(end.y)
        ),
    })
}

fn polygon_fragment(
    polygon: &RenderPolygon,
    camera: Camera2,
    viewport: Viewport,
) -> Option<PathFragment> {
    let (first, rest) = polygon.points.split_first()?;
    let first = project_point(camera, viewport, *first);
    let mut commands = format!("M {} {}", fmt(first.x), fmt(first.y));
    for point in rest {
        let projected = project_point(camera, viewport, *point);
        write!(commands, " L {} {}", fmt(projected.x), fmt(projected.y))
            .expect("writing to String cannot fail");
    }
    commands.push_str(" Z");

    Some(PathFragment {
        style: PathStyle::new(polygon.fill, polygon.stroke),
        commands,
    })
}

fn text_primitive(text: &RenderText, camera: Camera2, viewport: Viewport) -> ScenePrimitive {
    let projected = project_point(camera, viewport, text.position);
    let (x, y) = match text.anchor {
        TextAnchor::TopLeft => (projected.x, projected.y),
        TextAnchor::Center => (
            projected.x - text.text.len() as f32 * text.size * 0.25,
            projected.y - text.size * 0.5,
        ),
    };

    ScenePrimitive {
        kind: PrimitiveKind::Text,
        x: viewport.x,
        y: viewport.y,
        width: viewport.width,
        height: viewport.height,
        viewbox_width: viewport.width,
        viewbox_height: viewport.height,
        commands: SharedString::default(),
        fill: transparent_brush(),
        stroke: transparent_brush(),
        stroke_width: 0.0,
        text: text.text.as_str().into(),
        color: brush(text.color),
        font_size: text.size,
        text_x: x,
        text_y: y,
    }
}

fn path_primitive(commands: String, style: PathStyle, viewport: Viewport) -> ScenePrimitive {
    ScenePrimitive {
        kind: PrimitiveKind::Path,
        x: viewport.x,
        y: viewport.y,
        width: viewport.width,
        height: viewport.height,
        viewbox_width: viewport.width,
        viewbox_height: viewport.height,
        commands: commands.into(),
        fill: brush(style.fill),
        stroke: brush(style.stroke),
        stroke_width: style.stroke_width,
        text: SharedString::default(),
        color: transparent_brush(),
        font_size: 0.0,
        text_x: 0.0,
        text_y: 0.0,
    }
}

fn project_point(camera: Camera2, viewport: Viewport, point: RenderPoint) -> RenderPoint {
    let normalized = camera.world_to_viewport(point, viewport.aspect_ratio());
    RenderPoint::new(
        normalized.x * viewport.width,
        normalized.y * viewport.height,
    )
}

fn brush(color: RenderColor) -> Brush {
    Brush::SolidColor(Color::from_argb_f32(
        color.a.clamp(0.0, 1.0),
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
    ))
}

fn transparent_brush() -> Brush {
    brush(TRANSPARENT)
}

fn fmt(value: f32) -> String {
    format!("{value:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Fill, RenderLayer};

    const EPS: f32 = 1.0e-4;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    fn triangle_at(x: f32, color: RenderColor) -> RenderPrimitive {
        RenderPrimitive::Polygon(RenderPolygon {
            points: vec![
                RenderPoint::new(x, 10.0),
                RenderPoint::new(x - 5.0, -10.0),
                RenderPoint::new(x + 5.0, -10.0),
            ],
            fill: Some(Fill::new(color)),
            stroke: None,
        })
    }

    #[test]
    fn projection_maps_world_y_up_to_slint_y_down() {
        let camera = Camera2::new(RenderPoint::ZERO, 100.0);
        let viewport = Viewport::new(200.0, 100.0);

        let center = project_point(camera, viewport, RenderPoint::ZERO);
        assert_close(center.x, 100.0);
        assert_close(center.y, 50.0);

        let top_left = project_point(camera, viewport, RenderPoint::new(-100.0, 50.0));
        assert_close(top_left.x, 0.0);
        assert_close(top_left.y, 0.0);

        let bottom_right = project_point(camera, viewport, RenderPoint::new(100.0, -50.0));
        assert_close(bottom_right.x, 200.0);
        assert_close(bottom_right.y, 100.0);
    }

    #[test]
    fn multiple_frames_are_laid_out_as_equal_horizontal_viewports() {
        let mut left = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        left.push_primitive(0, triangle_at(0.0, RenderColor::RED));
        let mut right = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        right.push_primitive(0, triangle_at(0.0, RenderColor::GREEN));

        let primitives = scene_primitives_from_frames(&[left, right], Viewport::new(200.0, 100.0));

        assert_eq!(primitives.len(), 2);
        assert_close(primitives[0].x, 0.0);
        assert_close(primitives[0].y, 0.0);
        assert_close(primitives[0].width, 100.0);
        assert_close(primitives[0].height, 100.0);
        assert_close(primitives[1].x, 100.0);
        assert_close(primitives[1].y, 0.0);
        assert_close(primitives[1].width, 100.0);
        assert_close(primitives[1].height, 100.0);
    }

    #[test]
    fn adjacent_same_style_polygons_are_batched() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(0, triangle_at(-20.0, RenderColor::RED));
        frame.push_primitive(0, triangle_at(20.0, RenderColor::RED));

        let primitives = scene_primitives_from_frame(&frame, Viewport::new(1280.0, 720.0));

        assert_eq!(primitives.len(), 1);
        assert_eq!(primitives[0].kind, PrimitiveKind::Path);
        assert_eq!(primitives[0].commands.matches('M').count(), 2);
    }

    #[test]
    fn style_change_splits_path_batches() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(0, triangle_at(-20.0, RenderColor::RED));
        frame.push_primitive(0, triangle_at(20.0, RenderColor::GREEN));

        let primitives = scene_primitives_from_frame(&frame, Viewport::new(1280.0, 720.0));

        assert_eq!(primitives.len(), 2);
        assert!(
            primitives
                .iter()
                .all(|primitive| primitive.kind == PrimitiveKind::Path)
        );
    }

    #[test]
    fn text_flushes_pending_path_batch_to_preserve_order() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(0, triangle_at(-20.0, RenderColor::RED));
        frame.push_primitive(
            0,
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "middle")),
        );
        frame.push_primitive(0, triangle_at(20.0, RenderColor::RED));

        let primitives = scene_primitives_from_frame(&frame, Viewport::new(1280.0, 720.0));

        assert_eq!(
            primitives
                .iter()
                .map(|primitive| primitive.kind)
                .collect::<Vec<_>>(),
            [
                PrimitiveKind::Path,
                PrimitiveKind::Text,
                PrimitiveKind::Path
            ]
        );
    }

    #[test]
    fn layer_order_controls_scene_order() {
        let mut frame = RenderFrame::default();
        frame.push_layer(RenderLayer::new(10).with_primitive(triangle_at(10.0, RenderColor::RED)));
        frame.push_layer(RenderLayer::new(-1).with_primitive(RenderPrimitive::Text(
            RenderText::new(RenderPoint::ZERO, "first"),
        )));

        let primitives = scene_primitives_from_frame(&frame, Viewport::new(1280.0, 720.0));

        assert_eq!(primitives[0].kind, PrimitiveKind::Text);
        assert_eq!(primitives[1].kind, PrimitiveKind::Path);
    }
}
