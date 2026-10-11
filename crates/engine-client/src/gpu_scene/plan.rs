//! Typed geometry, without SVG strings, UI items, or graphics context access.
//! Stroke widths and text remain logical pixels, matching the raster adapter.

use super::PlanMode;
use crate::render::{FrameLayout, Viewport};
use engine_common::{
    Camera2, Fill, RenderColor, RenderFrame, RenderPoint, RenderPrimitive, Stroke,
};
use engine_core::Vec2;

#[derive(Debug, PartialEq)]
pub(crate) enum Shape {
    Circle {
        center: RenderPoint,
        radius: f32,
    },
    Line {
        start: RenderPoint,
        end: RenderPoint,
    },
    Polygon(Vec<RenderPoint>),
    LocalPolygon(super::reusable::Polygon),
}

#[derive(Debug)]
pub(crate) struct Draw {
    pub shape: Shape,
    pub fill: Option<RenderColor>,
    pub stroke: Option<Stroke>,
}

#[derive(Debug)]
pub(crate) struct Scene {
    pub width: u32,
    pub height: u32,
    pub draws: Vec<Draw>,
    // Older per-polygon offscreen experiment; groups below serve live frames.
    pub reuse: Vec<super::reusable::Placement>,
    pub retained: Vec<super::retained::Placement>,
}

impl Scene {
    pub fn reusable_polygons(&self) -> usize {
        self.reuse.len()
            + self
                .draws
                .iter()
                .filter(|d| matches!(d.shape, Shape::LocalPolygon(_)))
                .count()
    }
}

pub(crate) fn build(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
) -> Result<Scene, String> {
    if frames.len() != 1
        || layout != FrameLayout::EqualHorizontal
        || scale != 1.0
        || viewport.x != 0.0
        || viewport.y != 0.0
    {
        return Err("gpu-scene currently supports one full-window frame at 1x only".into());
    }
    let frame = &frames[0];
    let mut builder = Projection::new(&frame.camera, viewport)?;
    for layer in frame.ordered_layers() {
        for primitive in &layer.primitives {
            builder.primitive(primitive)?;
        }
    }
    Ok(builder.finish())
}

/// Project into a shared target, retaining the camera's actual logical viewport.
/// Clipping/compositing is a separate presentation operation, not per-item alpha.
pub(super) fn frame_scene(
    frame: &RenderFrame,
    viewport: Viewport,
    size: (u32, u32),
    minimum_size: Option<f32>,
    retain: bool,
    mode: PlanMode,
    storage: Option<Scene>,
) -> Result<Scene, String> {
    let mut builder = Projection::new(&frame.camera, viewport)?;
    builder.mode = mode;
    if mode == PlanMode::Prepared {
        builder.prepared = Some(PreparedCamera::new(frame.camera, viewport));
    }
    if mode.reuses_storage()
        && let Some(mut storage) = storage
    {
        storage.reuse.clear();
        storage.retained.clear();
        builder.scene = storage;
    }
    if mode != PlanMode::Reference {
        builder.scene.draws.reserve(
            frame
                .layers
                .iter()
                .map(|l| l.primitives.len())
                .sum::<usize>()
                .saturating_sub(builder.scene.draws.len()),
        );
    }
    builder.scene.width = size.0;
    builder.scene.height = size.1;
    let mut layers: Vec<_> = frame.layers.iter().enumerate().collect();
    layers.sort_by_key(|(_, layer)| layer.z);
    for (layer_index, layer) in layers {
        let index_hints = retain
            && (mode == PlanMode::Reference
                || frame.geometry_hints.iter().any(|h| h.layer == layer_index));
        let mut starts = if index_hints && mode != PlanMode::Reference {
            Vec::with_capacity(layer.primitives.len() + 1)
        } else {
            Vec::new()
        };
        for primitive in &layer.primitives {
            if index_hints {
                starts.push(builder.next_draw);
            }
            if minimum_size.is_none_or(|minimum| {
                crate::render::spacewars_overview_primitive_visible(
                    frame.camera,
                    viewport,
                    layer.z,
                    primitive,
                    minimum,
                )
            }) {
                builder.primitive(primitive)?;
            }
        }
        if index_hints {
            starts.push(builder.next_draw);
            for hint in frame
                .geometry_hints
                .iter()
                .filter(|h| h.layer == layer_index)
            {
                let Some(end) = hint.first.checked_add(hint.geometry.len()) else {
                    continue;
                };
                let Some(range) = starts.get(hint.first..=end) else {
                    continue;
                };
                // Text, transparent/empty primitives and overview filtering may
                // remove draws. Never let a hint shift or swallow its neighbors.
                if !range.windows(2).all(|w| w[1] == w[0] + 1) {
                    continue;
                }
                let Ok(translation) = builder.point(hint.position) else {
                    continue;
                };
                let p = super::retained::Placement {
                    first: range[0],
                    definition: hint.geometry.clone(),
                    scale: viewport.height / frame.camera.height,
                    angle: hint.angle,
                    translation,
                };
                // Compare authoritative world points, not two differently
                // rounded screen-space projections. Close-up views magnify
                // cancellation in the latter even for exactly matching art.
                if p.valid() && hint_matches(hint, &layer.primitives[hint.first..end], mode) {
                    builder.scene.retained.push(p);
                }
            }
        }
    }
    builder.scene.retained.sort_by_key(|p| p.first);
    let mut end = 0;
    builder.scene.retained.retain(|p| {
        if p.first < end {
            return false;
        }
        end = p.first + p.definition.len();
        true
    });
    Ok(builder.finish())
}

fn hint_matches(
    hint: &engine_common::PolygonGeometryHint,
    primitives: &[RenderPrimitive],
    mode: PlanMode,
) -> bool {
    let position = Vec2::new(hint.position.x, hint.position.y);
    let rotation = (mode == PlanMode::Prepared).then(|| hint.angle.sin_cos());
    primitives
        .iter()
        .zip(hint.geometry.polygons())
        .all(|(world, local)| {
            let RenderPrimitive::Polygon(world) = world else {
                return false;
            };
            world.fill == local.fill
                && world.stroke == local.stroke
                && world.points.len() == local.points.len()
                && world.points.iter().zip(&local.points).all(|(a, b)| {
                    let rotated = if let Some((sin, cos)) = rotation {
                        Vec2::new(b.x * cos - b.y * sin, b.x * sin + b.y * cos)
                    } else {
                        Vec2::new(b.x, b.y).rotate_radians(hint.angle)
                    };
                    let p = position + rotated;
                    *a == RenderPoint::new(p.x, p.y)
                })
        })
}

pub(crate) fn build_authored(
    frame: &super::authored::Frame,
    viewport: Viewport,
) -> Result<Scene, String> {
    let mut builder = Projection::new(&frame.camera, viewport)?;
    for layer in frame.ordered_layers() {
        for primitive in &layer.primitives {
            match primitive {
                super::authored::Primitive::World(p) => builder.primitive(p)?,
                super::authored::Primitive::Local(p) => builder.local(p)?,
            }
        }
    }
    Ok(builder.finish())
}

struct Projection<'a> {
    camera: &'a Camera2,
    viewport: Viewport,
    scene: Scene,
    mode: PlanMode,
    next_draw: usize,
    prepared: Option<PreparedCamera>,
}

/// Per-pass invariants, not a cached affine approximation. Keep bounds rounding,
/// subtraction, division and viewport mapping in the reference order: reciprocal
/// multiplication or folding the translations can change close-up pixels.
struct PreparedCamera {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl PreparedCamera {
    fn new(camera: Camera2, viewport: Viewport) -> Self {
        let bounds = camera.world_bounds(viewport.aspect_ratio());
        Self {
            left: bounds.min.x,
            top: bounds.max.y,
            width: bounds.width(),
            height: bounds.height(),
        }
    }

    fn normalized(&self, point: RenderPoint) -> RenderPoint {
        RenderPoint::new(
            (point.x - self.left) / self.width,
            (self.top - point.y) / self.height,
        )
    }
}

impl<'a> Projection<'a> {
    fn new(camera: &'a Camera2, viewport: Viewport) -> Result<Self, String> {
        if ![viewport.width, viewport.height, camera.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            || !finite(camera.center)
            || !viewport.x.is_finite()
            || !viewport.y.is_finite()
            || viewport.width > 16384.0
            || viewport.height > 16384.0
        {
            return Err("invalid GPU scene camera or viewport".into());
        }
        Ok(Self {
            camera,
            viewport,
            mode: PlanMode::Reference,
            next_draw: 0,
            prepared: None,
            scene: Scene {
                width: viewport.width.ceil() as u32,
                height: viewport.height.ceil() as u32,
                draws: Vec::new(),
                reuse: Vec::new(),
                retained: Vec::new(),
            },
        })
    }

    fn point(&self, point: RenderPoint) -> Result<RenderPoint, String> {
        let viewport = self.viewport;
        let normalized = if let Some(prepared) = &self.prepared {
            prepared.normalized(point)
        } else {
            self.camera
                .world_to_viewport(point, viewport.aspect_ratio())
        };
        let point = RenderPoint::new(
            viewport.x + normalized.x * viewport.width,
            viewport.y + normalized.y * viewport.height,
        );
        if finite(point) {
            Ok(point)
        } else {
            Err("non-finite GPU scene point".into())
        }
    }

    fn finish(mut self) -> Scene {
        // Reused storage may have belonged to a larger or differently filtered
        // scene. Only the freshly validated prefix is ever handed to a renderer.
        self.scene.draws.truncate(self.next_draw);
        self.scene
    }

    fn primitive(&mut self, primitive: &RenderPrimitive) -> Result<(), String> {
        let (shape, fill, stroke) = match primitive {
            RenderPrimitive::Text(_) => return Ok(()), // Existing Slint text overlay.
            RenderPrimitive::Circle(circle) => {
                if !circle.radius.is_finite() {
                    return Err("non-finite GPU circle radius".into());
                }
                if circle.radius <= 0.0 {
                    return Ok(());
                }
                let radius = circle.radius * self.viewport.height / self.camera.height;
                if !radius.is_finite() {
                    return Err("invalid projected GPU circle radius".into());
                }
                (
                    Shape::Circle {
                        center: self.point(circle.center)?,
                        radius,
                    },
                    circle.fill,
                    circle.stroke,
                )
            }
            RenderPrimitive::Line(line) => (
                Shape::Line {
                    start: self.point(line.start)?,
                    end: self.point(line.end)?,
                },
                None,
                Some(line.stroke),
            ),
            RenderPrimitive::Polygon(polygon) => {
                if polygon.points.len() < 2 {
                    return Ok(());
                }
                let points = if self.mode != PlanMode::Reference {
                    let mut points = if self.mode.reuses_storage() {
                        match self.scene.draws.get_mut(self.next_draw) {
                            Some(Draw {
                                shape: Shape::Polygon(points),
                                ..
                            }) => std::mem::take(points),
                            _ => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    };
                    points.clear();
                    points.reserve(polygon.points.len());
                    for &point in &polygon.points {
                        points.push(self.point(point)?);
                    }
                    points
                } else {
                    polygon
                        .points
                        .iter()
                        .copied()
                        .map(|p| self.point(p))
                        .collect::<Result<Vec<_>, _>>()?
                };
                (Shape::Polygon(points), polygon.fill, polygon.stroke)
            }
        };
        self.push(shape, fill, stroke)
    }

    fn local(&mut self, polygon: &super::authored::Polygon) -> Result<(), String> {
        if polygon.definition.points().len() < 2 {
            return Ok(());
        }
        let t = polygon.transform;
        if t.scale.x != t.scale.y || t.scale.x <= 0.0 {
            // Nonuniform/reflected/degenerate transforms still have a correct
            // ordinary path. They do not assert compatibility with this cache.
            return self.primitive(&polygon.materialize());
        }
        let origin = t.transform_point(Vec2::ZERO);
        let p = super::reusable::Polygon {
            definition: polygon.definition.clone(),
            scale: self.viewport.height / self.camera.height * t.scale.x,
            angle: t.rotation_radians,
            translation: self.point(RenderPoint::new(origin.x, origin.y))?,
        };
        // Validate before either cached or ordinary submission; never rely on a
        // cache miss to notice malformed input. No transformed copy is retained.
        if !p.valid() {
            return Err("invalid projected local polygon".into());
        }
        self.push(Shape::LocalPolygon(p), polygon.fill, polygon.stroke)
    }

    fn push(
        &mut self,
        shape: Shape,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
    ) -> Result<(), String> {
        let fill = fill
            .map(|f| color(f.color))
            .transpose()?
            .filter(|c| c.a > 0.0);
        let stroke = stroke
            .map(|s| -> Result<Stroke, String> {
                if !s.width.is_finite() {
                    return Err("non-finite GPU stroke width".into());
                }
                Ok(Stroke::new(color(s.color)?, s.width.max(0.0)))
            })
            .transpose()?
            .filter(|s| s.width > 0.0 && s.color.a > 0.0);
        if fill.is_some() || stroke.is_some() {
            let draw = Draw {
                shape,
                fill,
                stroke,
            };
            if self.next_draw < self.scene.draws.len() {
                self.scene.draws[self.next_draw] = draw;
            } else {
                self.scene.draws.push(draw);
            }
            self.next_draw += 1;
        }
        Ok(())
    }
}

fn finite(p: RenderPoint) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn color(c: RenderColor) -> Result<RenderColor, String> {
    if ![c.r, c.g, c.b, c.a].iter().all(|v| v.is_finite()) {
        return Err("non-finite GPU scene color".into());
    }
    Ok(RenderColor::rgba(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
        c.a.clamp(0.0, 1.0),
    ))
}

#[cfg(test)]
#[path = "plan/prepared_tests.rs"]
mod prepared_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{Camera2, RenderCircle, RenderLine, RenderPolygon, RenderText};

    #[test]
    fn production_clock_fixtures_prepare_without_changing_inputs() {
        use crate::client_scenarios::{
            self, BenchmarkConfiguration, ClockBenchmarkCase, ClockBenchmarkConfig, RenderBackend,
            ScenarioStartMode,
        };
        for viewport in [Viewport::new(1024.0, 768.0), Viewport::new(800.0, 480.0)] {
            for case in ClockBenchmarkCase::ALL {
                let config = BenchmarkConfiguration {
                    clock: ClockBenchmarkConfig {
                        case,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut scenario = client_scenarios::registration("clock")
                    .unwrap()
                    .create(
                        0,
                        &engine_common::Settings::default(),
                        viewport,
                        ScenarioStartMode::Benchmark(config),
                    )
                    .unwrap();
                for tick in 0..=600 {
                    if [0, 120, 360, 600].contains(&tick) {
                        let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                        let before = frames.clone();
                        let scene = build(&frames, viewport, scenario.frame_layout(), 1.0).unwrap();
                        assert!(!scene.draws.is_empty(), "{case:?} at {tick}");
                        let mut mesh = super::super::mesh::MeshBuilder::default();
                        mesh.rebuild(&scene)
                            .unwrap_or_else(|e| panic!("{case:?} at {tick}: {e}"));
                        assert!(!mesh.buffers.indices.is_empty());
                        assert_eq!(frames, before);
                        assert_eq!(scene.width, viewport.width as u32);
                    }
                    scenario.step(&[], std::time::Duration::from_nanos(16_666_667));
                }
            }
        }
    }

    fn frame() -> RenderFrame {
        RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0))
    }
    fn prepare(frame: RenderFrame) -> Scene {
        build(
            &[frame],
            Viewport::new(200.0, 100.0),
            FrameLayout::EqualHorizontal,
            1.0,
        )
        .unwrap()
    }

    #[test]
    fn projection_sorting_alpha_and_logical_strokes_match_the_contract() {
        let mut f = frame();
        f.push_primitive(
            7,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                10.0,
                RenderColor::rgba(1.0, 0.0, 0.0, 0.5),
            )),
        );
        f.push_primitive(
            -3,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::new(-100.0, 50.0),
                RenderPoint::new(100.0, -50.0),
                Stroke::new(RenderColor::WHITE, 2.0),
            )),
        );
        let s = prepare(f);
        assert_eq!((s.width, s.height), (200, 100));
        assert_eq!(
            s.draws[0].shape,
            Shape::Line {
                start: RenderPoint::ZERO,
                end: RenderPoint::new(200.0, 100.0)
            }
        );
        assert_eq!(s.draws[0].stroke.unwrap().width, 2.0);
        assert_eq!(
            s.draws[1].shape,
            Shape::Circle {
                center: RenderPoint::new(100.0, 50.0),
                radius: 10.0
            }
        );
        assert_eq!(s.draws[1].fill.unwrap().a, 0.5);
    }

    #[test]
    fn overlapping_translucent_shapes_are_not_merged_and_text_is_left_to_overlay() {
        let mut f = frame();
        for c in [
            RenderColor::rgba(1.0, 0.0, 0.0, 0.5),
            RenderColor::rgba(0.0, 0.0, 1.0, 0.5),
        ] {
            f.push_primitive(
                0,
                RenderPrimitive::Circle(RenderCircle::filled(RenderPoint::ZERO, 10.0, c)),
            );
        }
        f.push_primitive(
            0,
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "Overlay")),
        );
        let s = prepare(f);
        assert_eq!(s.draws.len(), 2);
        assert_eq!(s.draws[0].fill.unwrap().r, 1.0);
        assert_eq!(s.draws[1].fill.unwrap().b, 1.0);
    }

    #[test]
    fn concave_paths_and_offscreen_points_are_preserved_for_gpu_clipping() {
        let points = vec![
            RenderPoint::new(-150.0, 40.0),
            RenderPoint::new(20.0, 40.0),
            RenderPoint::new(20.0, 20.0),
            RenderPoint::new(0.0, 20.0),
            RenderPoint::new(0.0, -80.0),
            RenderPoint::new(-150.0, -80.0),
        ];
        let mut f = frame();
        f.push_primitive(
            0,
            RenderPrimitive::Polygon(RenderPolygon::filled(points, RenderColor::GREEN)),
        );
        let s = prepare(f);
        let Shape::Polygon(points) = &s.draws[0].shape else {
            panic!("polygon lost");
        };
        assert_eq!(points.len(), 6);
        assert_eq!(points[0], RenderPoint::new(-50.0, 10.0));
        assert_eq!(points[4], RenderPoint::new(100.0, 130.0));
    }

    #[test]
    fn unsupported_layouts_and_invalid_numbers_fail_instead_of_dropping_geometry() {
        let mut f = frame();
        assert!(
            build(
                &[f.clone(), f.clone()],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                1.0
            )
            .is_err()
        );
        assert!(
            build(
                &[f.clone()],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                2.0
            )
            .is_err()
        );
        assert!(
            build(
                &[f.clone()],
                Viewport::new(200.0, 100.0),
                FrameLayout::PlayerViewsWithMinimaps,
                1.0
            )
            .is_err()
        );
        f.camera.height = 0.0;
        assert!(
            build(
                &[f],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                1.0
            )
            .is_err()
        );
        let mut f = frame();
        f.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::new(f32::NAN, 0.0),
                2.0,
                RenderColor::RED,
            )),
        );
        assert!(
            build(
                &[f],
                Viewport::new(200.0, 100.0),
                FrameLayout::EqualHorizontal,
                1.0
            )
            .is_err()
        );
    }
}
