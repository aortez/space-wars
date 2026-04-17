//! Platform-agnostic 2D render contract emitted by scenarios.
//!
//! Scenarios emit world-space primitives. Positive X points right, and positive
//! Y points up in world coordinates. The client camera maps world Y-up into
//! viewport coordinates where Y grows downward.

use serde::{Deserialize, Serialize};

/// Draw list emitted by a scenario's `render_frame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderFrame {
    pub camera: Camera2,
    pub layers: Vec<RenderLayer>,
}

impl RenderFrame {
    pub fn new(camera: Camera2) -> Self {
        Self {
            camera,
            layers: Vec::new(),
        }
    }

    pub fn push_layer(&mut self, layer: RenderLayer) {
        self.layers.push(layer);
    }

    pub fn push_primitive(&mut self, z: i32, primitive: RenderPrimitive) {
        if let Some(layer) = self.layers.iter_mut().find(|layer| layer.z == z) {
            layer.primitives.push(primitive);
        } else {
            self.layers.push(RenderLayer {
                z,
                primitives: vec![primitive],
            });
        }
    }

    /// Return layers in draw order. Lower z draws first.
    pub fn ordered_layers(&self) -> Vec<&RenderLayer> {
        let mut layers: Vec<&RenderLayer> = self.layers.iter().collect();
        layers.sort_by_key(|layer| layer.z);
        layers
    }
}

impl Default for RenderFrame {
    fn default() -> Self {
        Self::new(Camera2::default())
    }
}

/// Ordered 2D layer within a [`RenderFrame`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLayer {
    pub z: i32,
    pub primitives: Vec<RenderPrimitive>,
}

impl RenderLayer {
    pub fn new(z: i32) -> Self {
        Self {
            z,
            primitives: Vec::new(),
        }
    }

    pub fn with_primitive(mut self, primitive: RenderPrimitive) -> Self {
        self.primitives.push(primitive);
        self
    }
}

impl Default for RenderLayer {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Camera describing the visible world-space rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera2 {
    pub center: RenderPoint,
    pub height: f32,
}

impl Camera2 {
    pub const DEFAULT_HEIGHT: f32 = 720.0;

    pub const fn new(center: RenderPoint, height: f32) -> Self {
        Self { center, height }
    }

    pub fn visible_width(self, aspect_ratio: f32) -> f32 {
        self.height * aspect_ratio
    }

    pub fn world_bounds(self, aspect_ratio: f32) -> RenderRect {
        let width = self.visible_width(aspect_ratio);
        RenderRect {
            min: RenderPoint::new(
                self.center.x - width * 0.5,
                self.center.y - self.height * 0.5,
            ),
            max: RenderPoint::new(
                self.center.x + width * 0.5,
                self.center.y + self.height * 0.5,
            ),
        }
    }

    /// Convert world coordinates to normalized viewport coordinates.
    ///
    /// The returned point uses `[0, 1]` coordinates with `(0, 0)` at the top
    /// left and `(1, 1)` at the bottom right.
    pub fn world_to_viewport(self, point: RenderPoint, aspect_ratio: f32) -> RenderPoint {
        let bounds = self.world_bounds(aspect_ratio);
        let width = bounds.width();
        let height = bounds.height();
        RenderPoint::new(
            (point.x - bounds.min.x) / width,
            (bounds.max.y - point.y) / height,
        )
    }
}

impl Default for Camera2 {
    fn default() -> Self {
        Self::new(RenderPoint::ZERO, Self::DEFAULT_HEIGHT)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct RenderPoint {
    pub x: f32,
    pub y: f32,
}

impl RenderPoint {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderRect {
    pub min: RenderPoint,
    pub max: RenderPoint,
}

impl RenderRect {
    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl RenderColor {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const CLEAR: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

impl Default for RenderColor {
    fn default() -> Self {
        Self::WHITE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: RenderColor,
    pub width: f32,
}

impl Stroke {
    pub const fn new(color: RenderColor, width: f32) -> Self {
        Self { color, width }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self::new(RenderColor::WHITE, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub color: RenderColor,
}

impl Fill {
    pub const fn new(color: RenderColor) -> Self {
        Self { color }
    }
}

impl Default for Fill {
    fn default() -> Self {
        Self::new(RenderColor::WHITE)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderPrimitive {
    Circle(RenderCircle),
    Line(RenderLine),
    Polygon(RenderPolygon),
    Text(RenderText),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderCircle {
    pub center: RenderPoint,
    pub radius: f32,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

impl RenderCircle {
    pub fn new(center: RenderPoint, radius: f32) -> Self {
        Self {
            center,
            radius,
            fill: None,
            stroke: Some(Stroke::default()),
        }
    }

    pub fn filled(center: RenderPoint, radius: f32, color: RenderColor) -> Self {
        Self {
            center,
            radius,
            fill: Some(Fill::new(color)),
            stroke: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLine {
    pub start: RenderPoint,
    pub end: RenderPoint,
    pub stroke: Stroke,
}

impl RenderLine {
    pub fn new(start: RenderPoint, end: RenderPoint, stroke: Stroke) -> Self {
        Self { start, end, stroke }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderPolygon {
    pub points: Vec<RenderPoint>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

impl RenderPolygon {
    pub fn new(points: Vec<RenderPoint>) -> Self {
        Self {
            points,
            fill: None,
            stroke: Some(Stroke::default()),
        }
    }

    pub fn filled(points: Vec<RenderPoint>, color: RenderColor) -> Self {
        Self {
            points,
            fill: Some(Fill::new(color)),
            stroke: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderText {
    pub position: RenderPoint,
    pub text: String,
    pub color: RenderColor,
    pub size: f32,
    pub anchor: TextAnchor,
}

impl RenderText {
    pub fn new(position: RenderPoint, text: impl Into<String>) -> Self {
        Self {
            position,
            text: text.into(),
            color: RenderColor::WHITE,
            size: 16.0,
            anchor: TextAnchor::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAnchor {
    #[default]
    TopLeft,
    Center,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-5;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    fn assert_point_close(actual: RenderPoint, expected: RenderPoint) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn default_camera_is_centered_with_startup_window_height() {
        let camera = Camera2::default();
        assert_eq!(camera.center, RenderPoint::ZERO);
        assert_eq!(camera.height, 720.0);
    }

    #[test]
    fn camera_maps_world_y_up_to_viewport_y_down() {
        let camera = Camera2::new(RenderPoint::new(0.0, 0.0), 100.0);

        assert_point_close(
            camera.world_to_viewport(RenderPoint::new(0.0, 0.0), 2.0),
            RenderPoint::new(0.5, 0.5),
        );
        assert_point_close(
            camera.world_to_viewport(RenderPoint::new(-100.0, 50.0), 2.0),
            RenderPoint::new(0.0, 0.0),
        );
        assert_point_close(
            camera.world_to_viewport(RenderPoint::new(100.0, -50.0), 2.0),
            RenderPoint::new(1.0, 1.0),
        );
    }

    #[test]
    fn ordered_layers_draw_low_z_first_without_mutating_storage() {
        let mut frame = RenderFrame::default();
        frame.push_layer(RenderLayer::new(10));
        frame.push_layer(RenderLayer::new(-2));
        frame.push_layer(RenderLayer::new(5));

        let ordered_z: Vec<i32> = frame
            .ordered_layers()
            .into_iter()
            .map(|layer| layer.z)
            .collect();
        assert_eq!(ordered_z, [-2, 5, 10]);
        assert_eq!(
            frame.layers.iter().map(|layer| layer.z).collect::<Vec<_>>(),
            [10, -2, 5]
        );
    }

    #[test]
    fn push_primitive_reuses_matching_layer() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(
            3,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                10.0,
                RenderColor::RED,
            )),
        );
        frame.push_primitive(
            3,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::ZERO,
                RenderPoint::new(1.0, 1.0),
                Stroke::default(),
            )),
        );

        assert_eq!(frame.layers.len(), 1);
        assert_eq!(frame.layers[0].z, 3);
        assert_eq!(frame.layers[0].primitives.len(), 2);
    }

    #[test]
    fn frame_can_hold_all_initial_primitive_shapes() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                2.0,
                RenderColor::BLUE,
            )),
        );
        frame.push_primitive(
            1,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::new(-1.0, -1.0),
                RenderPoint::new(1.0, 1.0),
                Stroke::new(RenderColor::GREEN, 0.5),
            )),
        );
        frame.push_primitive(
            2,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(0.0, 1.0),
                    RenderPoint::new(-1.0, -1.0),
                    RenderPoint::new(1.0, -1.0),
                ],
                RenderColor::YELLOW,
            )),
        );
        frame.push_primitive(
            3,
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "debug")),
        );

        assert_eq!(frame.layers.len(), 4);
        assert_eq!(
            frame
                .layers
                .iter()
                .map(|layer| layer.primitives.len())
                .sum::<usize>(),
            4
        );
    }
}
