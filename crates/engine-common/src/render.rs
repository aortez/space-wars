//! Platform-agnostic 2D render contract emitted by scenarios.
//!
//! Scenarios emit world-space primitives. Positive X points right, and positive
//! Y points up in world coordinates. The client camera maps world Y-up into
//! viewport coordinates where Y grows downward.

use serde::{Deserialize, Serialize};

/// Pixel storage carried directly from a scenario to a native-video host.
///
/// The first format keeps the emulator's compact palette-index framebuffer
/// intact until the platform adapter expands it into its own reusable image
/// slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePixelFormat {
    Indexed8Rgb565,
}

/// Visible source rectangle within a native framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVideoCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl NativeVideoCrop {
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

/// Optional emulation metadata carried beside a native frame for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVideoTiming {
    pub emulated_ticks: u64,
    pub input_sequence_id: u64,
}

/// Immutable, platform-neutral native framebuffer view.
///
/// Pixel ownership remains with the scenario. A client may copy/convert this
/// view into bounded platform-owned presentation slots, but it must not retain
/// the borrow after the scenario advances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVideoFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub visible_crop: NativeVideoCrop,
    pub pixel_format: NativePixelFormat,
    pub frame_id: u64,
    pub pixels: &'a [u8],
    pub palette_rgb565: &'a [u16],
    pub timing: Option<NativeVideoTiming>,
}

impl NativeVideoFrame<'_> {
    /// Checks the bounded shape that a native presentation adapter relies on.
    /// Palette index ranges remain the producing scenario's invariant so this
    /// structural check stays constant-time on the frame hot path.
    pub fn has_valid_layout(&self) -> bool {
        let Some(pixel_count) = self.width.checked_mul(self.height) else {
            return false;
        };
        let Ok(pixel_count) = usize::try_from(pixel_count) else {
            return false;
        };
        let Some(crop_right) = self.visible_crop.x.checked_add(self.visible_crop.width) else {
            return false;
        };
        let Some(crop_bottom) = self.visible_crop.y.checked_add(self.visible_crop.height) else {
            return false;
        };

        self.width != 0
            && self.height != 0
            && self.visible_crop.width != 0
            && self.visible_crop.height != 0
            && self.pixels.len() == pixel_count
            && crop_right <= self.width
            && crop_bottom <= self.height
            && match self.pixel_format {
                NativePixelFormat::Indexed8Rgb565 => !self.palette_rgb565.is_empty(),
            }
    }
}

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

    /// Convert normalized viewport coordinates back into world coordinates.
    ///
    /// The input uses `[0, 1]` coordinates with `(0, 0)` at the top left.
    pub fn viewport_to_world(self, point: RenderPoint, aspect_ratio: f32) -> RenderPoint {
        let bounds = self.world_bounds(aspect_ratio);
        RenderPoint::new(
            bounds.min.x + point.x * bounds.width(),
            bounds.max.y - point.y * bounds.height(),
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

    #[test]
    fn native_video_layout_checks_pixels_palette_and_crop() {
        let pixels = [0_u8; 12];
        let palette = [0_u16; 4];
        let frame = NativeVideoFrame {
            width: 4,
            height: 3,
            visible_crop: NativeVideoCrop {
                x: 0,
                y: 1,
                width: 4,
                height: 2,
            },
            pixel_format: NativePixelFormat::Indexed8Rgb565,
            frame_id: 7,
            pixels: &pixels,
            palette_rgb565: &palette,
            timing: Some(NativeVideoTiming {
                emulated_ticks: 99,
                input_sequence_id: 4,
            }),
        };

        assert!(frame.has_valid_layout());
        assert!(
            !NativeVideoFrame {
                visible_crop: NativeVideoCrop {
                    y: 2,
                    height: 2,
                    ..frame.visible_crop
                },
                ..frame
            }
            .has_valid_layout()
        );
        assert!(
            !NativeVideoFrame {
                pixels: &pixels[..11],
                ..frame
            }
            .has_valid_layout()
        );
    }

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
