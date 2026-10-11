//! Opt-in local-geometry input for the renderer lab. No GL handles or cache keys
//! belong here. The software adapter materializes an ordinary RenderFrame only
//! when requested; normal scenario serialization is unchanged.
use super::reusable::Definition;
use engine_common::{
    Camera2, Fill, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive, Stroke,
};
use engine_core::{Transform2, Vec2};
use std::sync::Arc;

pub(crate) struct Polygon {
    pub definition: Arc<Definition>,
    pub transform: Transform2,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

impl Polygon {
    pub fn materialize(&self) -> RenderPrimitive {
        RenderPrimitive::Polygon(RenderPolygon {
            points: self
                .definition
                .points()
                .iter()
                .map(|p| {
                    let p = self.transform.transform_point(Vec2::new(p.x, p.y));
                    RenderPoint::new(p.x, p.y)
                })
                .collect(),
            fill: self.fill,
            stroke: self.stroke,
        })
    }
}

pub(crate) enum Primitive {
    World(RenderPrimitive),
    Local(Polygon),
}

pub(crate) struct Layer {
    pub z: i32,
    pub primitives: Vec<Primitive>,
}

pub(crate) struct Frame {
    pub camera: Camera2,
    pub layers: Vec<Layer>,
}

impl Frame {
    pub fn new(camera: Camera2) -> Self {
        Self {
            camera,
            layers: Vec::new(),
        }
    }

    pub fn push(&mut self, z: i32, primitive: Primitive) {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.z == z) {
            layer.primitives.push(primitive);
        } else {
            self.layers.push(Layer {
                z,
                primitives: vec![primitive],
            });
        }
    }

    pub fn ordered_layers(&self) -> Vec<&Layer> {
        let mut layers: Vec<_> = self.layers.iter().collect();
        layers.sort_by_key(|l| l.z);
        layers
    }

    pub fn materialize(&self) -> RenderFrame {
        let mut frame = RenderFrame::new(self.camera);
        for layer in &self.layers {
            for primitive in &layer.primitives {
                frame.push_primitive(
                    layer.z,
                    match primitive {
                        Primitive::World(p) => p.clone(),
                        Primitive::Local(p) => p.materialize(),
                    },
                );
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gpu_scene::plan::{self, Shape},
        render::{FrameLayout, Viewport},
    };
    use engine_common::{RenderCircle, RenderColor, RenderText};

    fn polygon(transform: Transform2) -> Polygon {
        Polygon {
            definition: Arc::new(Definition::new(vec![
                RenderPoint::new(-3., -2.),
                RenderPoint::new(5., -1.),
                RenderPoint::new(1., 1.),
                RenderPoint::new(-2., 4.),
            ])),
            transform,
            fill: Some(Fill::new(RenderColor::rgba(1., 0.2, 0.4, 0.5))),
            stroke: Some(Stroke::new(RenderColor::rgba(0., 1., 0., 0.4), 1.7)),
        }
    }

    #[test]
    fn local_projection_and_software_materialization_preserve_layers_styles_and_pivots() {
        for scale in [
            Vec2::splat(1.),
            Vec2::splat(2.),
            Vec2::new(2., 0.5),
            Vec2::splat(-1.),
            Vec2::ZERO,
        ] {
            let mut frame = Frame::new(Camera2::new(RenderPoint::new(3., -4.), 64.));
            let p = polygon(Transform2 {
                translation: Vec2::new(8., 3.),
                pivot: Vec2::new(1., 2.),
                rotation_radians: 0.4,
                scale,
            });
            let definition = p.definition.clone();
            frame.push(9, Primitive::Local(p));
            frame.push(
                -2,
                Primitive::World(RenderPrimitive::Circle(RenderCircle::filled(
                    RenderPoint::ZERO,
                    2.,
                    RenderColor::BLUE,
                ))),
            );
            let mut hidden = polygon(Transform2::IDENTITY);
            hidden.fill = None;
            hidden.stroke = None;
            frame.push(-2, Primitive::Local(hidden));
            frame.push(
                9,
                Primitive::World(RenderPrimitive::Text(RenderText::new(
                    RenderPoint::ZERO,
                    "ignored",
                ))),
            );
            frame.push(
                9,
                Primitive::Local(polygon(Transform2::from_translation(Vec2::new(-9., 0.)))),
            );
            let materialized = frame.materialize();
            assert_eq!(materialized.layers[0].z, 9);
            for viewport in [Viewport::new(400., 300.), Viewport::new(200., 150.)] {
                let actual = plan::build_authored(&frame, viewport).unwrap();
                let expected = plan::build(
                    std::slice::from_ref(&materialized),
                    viewport,
                    FrameLayout::EqualHorizontal,
                    1.,
                )
                .unwrap();
                assert_eq!(actual.draws.len(), 3);
                assert_eq!(actual.draws.len(), expected.draws.len());
                assert!(
                    actual.reuse.is_empty(),
                    "local identity does not need parallel hints"
                );
                for (a, b) in actual.draws.iter().zip(&expected.draws) {
                    assert_eq!(a.fill, b.fill);
                    assert_eq!(a.stroke, b.stroke);
                    match &a.shape {
                        Shape::LocalPolygon(p) => assert!(p.matches(b)),
                        _ => assert_eq!(a.shape, b.shape),
                    }
                }
                assert_eq!(actual.draws[1].stroke.unwrap().width, 1.7);
                if scale.x == scale.y && scale.x > 0. {
                    let Shape::LocalPolygon(p) = &actual.draws[1].shape else {
                        panic!("lost local polygon")
                    };
                    assert!(Arc::ptr_eq(&definition, &p.definition));
                } else {
                    assert!(matches!(actual.draws[1].shape, Shape::Polygon(_)));
                }
            }
            assert_eq!(frame.materialize(), materialized);
        }
    }

    #[test]
    fn malformed_local_input_is_rejected_before_any_submission() {
        for mutation in 0..9 {
            let mut p = polygon(Transform2::IDENTITY);
            match mutation {
                0 => p.transform.translation.x = f32::INFINITY,
                1 => p.transform.rotation_radians = f32::NAN,
                2 => p.transform.pivot.x = f32::NAN,
                3 => p.transform.scale.y = f32::NAN,
                4 => p.stroke.as_mut().unwrap().width = f32::NAN,
                5 => p.fill.as_mut().unwrap().color.a = f32::INFINITY,
                6 => {
                    p.definition =
                        Arc::new(Definition::new(vec![RenderPoint::new(f32::NAN, 0.); 3]))
                }
                7 => p.transform.scale = Vec2::splat(f32::MAX),
                _ => {
                    p.definition =
                        Arc::new(Definition::new(vec![
                            RenderPoint::new(f32::MAX, f32::MAX);
                            3
                        ]))
                }
            }
            let mut frame = Frame::new(Camera2::default());
            frame.push(0, Primitive::Local(p));
            assert!(
                plan::build_authored(&frame, Viewport::new(400., 300.)).is_err(),
                "mutation {mutation}"
            );
        }
    }
}
