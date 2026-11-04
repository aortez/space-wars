//! Re-run the real Spacewars ship-art producer inside the preparation timer.
//! Poses are prescribed outside timing, like the existing thruster visual lab;
//! this is not a complete match, simulation benchmark or fleet-size claim.
use super::*;
use crate::gpu_scene::authored;
use engine_common::{Fill, Stroke};
use engine_core::{Transform2, Vec2};
use scenario_spacewars::{
    ShipState,
    ship_geometry::{self, Part, Sink},
};

pub(super) struct Producer {
    poses: Vec<Vec<ShipState>>,
    cameras: Vec<Camera2>,
    definitions: Vec<Arc<Definition>>,
    pub local: bool,
}

pub(in crate::presentation_probe::geometry) struct Generated {
    pub(super) frames: Vec<RenderFrame>,
    pub(super) meshes: Vec<AuthoredMesh>,
    pub(super) local: Option<authored::Frame>,
}

impl Generated {
    pub fn materialize(&self) -> std::borrow::Cow<'_, [RenderFrame]> {
        match &self.local {
            Some(frame) => std::borrow::Cow::Owned(vec![frame.materialize()]),
            None => std::borrow::Cow::Borrowed(&self.frames),
        }
    }
}

struct LocalCollector<'a> {
    frame: authored::Frame,
    definitions: &'a [Arc<Definition>],
}

impl Sink for LocalCollector<'_> {
    fn push_primitive(&mut self, z: i32, primitive: RenderPrimitive) {
        self.frame.push(z, authored::Primitive::World(primitive));
    }

    fn rigid_polygon(
        &mut self,
        z: i32,
        part: Part,
        transform: Transform2,
        fill: RenderColor,
        outline: RenderColor,
    ) {
        self.frame.push(
            z,
            authored::Primitive::Local(authored::Polygon {
                definition: self.definitions[part as usize].clone(),
                transform,
                fill: Some(Fill::new(fill)),
                stroke: Some(Stroke::new(outline, 0.75)),
            }),
        );
    }
}

struct Collector<'a> {
    frame: RenderFrame,
    meshes: Vec<AuthoredMesh>,
    definitions: &'a [Arc<Definition>],
}

impl Sink for Collector<'_> {
    fn push_primitive(&mut self, z: i32, primitive: RenderPrimitive) {
        self.frame.push_primitive(z, primitive);
    }

    fn rigid_polygon(
        &mut self,
        z: i32,
        part: Part,
        t: Transform2,
        fill: RenderColor,
        outline: RenderColor,
    ) {
        // Keep the ordinary world polygon as a complete software/dynamic
        // fallback. BOTH its construction and the reuse metadata are timed.
        // No matching or hashing of transformed points is used to find identity.
        let draw = self.frame.layers.iter().map(|l| l.primitives.len()).sum();
        if t.scale == Vec2::splat(1.0) {
            let position = t.transform_point(Vec2::ZERO);
            self.meshes.push(AuthoredMesh {
                draw,
                definition: self.definitions[part as usize].clone(),
                position: RenderPoint::new(position.x, position.y),
                angle: t.rotation_radians,
            });
        }
        self.frame.rigid_polygon(z, part, t, fill, outline);
    }
}

impl Producer {
    pub fn generate(&self, index: usize, reuse: bool) -> Generated {
        if self.local {
            let i = index % self.poses.len();
            let mut output = LocalCollector {
                frame: authored::Frame::new(self.cameras[i]),
                definitions: &self.definitions,
            };
            for ship in &self.poses[i] {
                ship_geometry::render(&mut output, ship);
            }
            return Generated {
                frames: Vec::new(),
                meshes: Vec::new(),
                local: Some(output.frame),
            };
        }
        self.generate_world(index, reuse)
    }

    fn generate_world(&self, index: usize, reuse: bool) -> Generated {
        let i = index % self.poses.len();
        let mut output = Collector {
            frame: RenderFrame::new(self.cameras[i]),
            meshes: Vec::new(),
            definitions: &self.definitions,
        };
        for ship in &self.poses[i] {
            if reuse {
                ship_geometry::render(&mut output, ship);
            } else {
                ship_geometry::render(&mut output.frame, ship);
            }
        }
        // This trial deliberately renders one ship-art layer. Adding a world
        // producer requires resolving layer order, not reusing insertion indices.
        assert_eq!(output.frame.layers.len(), 1);
        Generated {
            frames: vec![output.frame],
            meshes: output.meshes,
            local: None,
        }
    }
}

pub(super) fn build(name: &str, viewport: Viewport) -> Result<Fixture, Box<dyn std::error::Error>> {
    let case = name.strip_suffix("-local").unwrap_or(name);
    let count = if case == "ships-pair" { 2 } else { 96 };
    let columns = if count == 2 { 2 } else { 12 };
    let rows = count / columns;
    let mut producer = Producer {
        poses: Vec::new(),
        cameras: Vec::new(),
        local: name.ends_with("-local"),
        definitions: Part::ALL
            .into_iter()
            .map(|p| {
                Arc::new(Definition::new(
                    p.points()
                        .iter()
                        .map(|v| RenderPoint::new(v.x, v.y))
                        .collect(),
                ))
            })
            .collect(),
    };
    for i in 0..32 {
        let t = i as f32 * std::f32::consts::TAU / 32.0;
        let height = if count == 2 { 36. } else { 192. };
        producer.cameras.push(Camera2::new(
            RenderPoint::new(t.sin(), t.cos()),
            height
                + if case == "ships-zoom" {
                    (i / 8) as f32 * 10.
                } else {
                    0.
                },
        ));
        producer.poses.push(
            (0..count)
                .map(|j| {
                    let phase = t + j as f32 * 0.7;
                    let position = Vec2::new(
                        ((j % columns) as f32 + 0.5 - columns as f32 / 2.)
                            * height
                            * viewport.aspect_ratio()
                            / columns as f32
                            * 0.85,
                        ((j / columns) as f32 + 0.5 - rows as f32 / 2.) * height / rows as f32
                            * 0.85,
                    ) + Vec2::new(phase.sin(), phase.cos());
                    ship_geometry::specimen(
                        if count == 2 && j == 1 { 3 } else { j },
                        phase,
                        position,
                    )
                })
                .collect(),
        );
    }
    let sequence: Vec<_> = (0..32)
        .map(|i| producer.generate_world(i, false).frames)
        .collect();
    let fixture = Fixture {
        name: name.into(),
        seed: 7,
        frames: sequence[0].clone(),
        tick: 0,
        objects: count,
        omitted_text: 0,
        checksum: checksum(&serde_json::to_vec(&sequence)?),
        sequence,
        meshes: Vec::new(),
        producer: Some(producer),
    };
    for i in 0..32 {
        let generated = fixture.generate(i, true).unwrap();
        if generated.materialize().as_ref() != fixture.at(i) {
            return Err("ship reuse producer changed ordinary RenderFrame output".into());
        }
        for scale in [1., 0.5] {
            let scene = fixture.scene(
                i,
                Viewport::new(viewport.width * scale, viewport.height * scale),
            )?;
            if !scene.reuse.iter().all(|p| p.matches(&scene.draws[p.draw])) {
                return Err("ship authoring transform differs from world polygon".into());
            }
            let ordinary = plan::build(
                fixture.at(i),
                Viewport::new(viewport.width * scale, viewport.height * scale),
                FrameLayout::EqualHorizontal,
                1.0,
            )?;
            if scene.draws.len() != ordinary.draws.len()
                || !scene.draws.iter().zip(&ordinary.draws).all(|(a, b)| {
                    a.fill == b.fill
                        && a.stroke == b.stroke
                        && match &a.shape {
                            plan::Shape::LocalPolygon(p) => p.matches(b),
                            _ => a.shape == b.shape,
                        }
                })
            {
                return Err("local ship geometry differs from ordinary scene".into());
            }
        }
    }
    Ok(fixture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_transform_keeps_the_complete_ordinary_polygon() {
        let definitions: Vec<_> = Part::ALL
            .into_iter()
            .map(|p| {
                Arc::new(Definition::new(
                    p.points()
                        .iter()
                        .map(|v| RenderPoint::new(v.x, v.y))
                        .collect(),
                ))
            })
            .collect();
        let mut collector = Collector {
            frame: RenderFrame::default(),
            meshes: Vec::new(),
            definitions: &definitions,
        };
        let transform = Transform2 {
            scale: Vec2::new(2., 1.),
            ..Transform2::IDENTITY
        };
        collector.rigid_polygon(
            0,
            Part::Hull,
            transform,
            RenderColor::WHITE,
            RenderColor::BLACK,
        );
        let mut reference = RenderFrame::default();
        reference.rigid_polygon(
            0,
            Part::Hull,
            transform,
            RenderColor::WHITE,
            RenderColor::BLACK,
        );
        assert!(collector.meshes.is_empty());
        assert_eq!(collector.frame, reference);
    }

    #[test]
    fn production_ship_parts_stay_explicit_and_dynamic_art_stays_streamed() {
        let viewport = Viewport::new(1024., 768.);
        for name in ["ships-pair", "ships-fleet", "ships-zoom"] {
            let f = build(name, viewport).unwrap();
            assert_eq!(f.checksum, build(name, viewport).unwrap().checksum);
            let a = f.generate(0, true).unwrap();
            let b = f.generate(1, true).unwrap();
            assert!(Arc::ptr_eq(
                &a.meshes[0].definition,
                &b.meshes[0].definition
            ));
            assert!(f.generate(0, false).unwrap().meshes.is_empty());
            assert_eq!(a.frames, f.at(0));
            assert_ne!(a.frames, b.frames);
            let scene = f.scene(0, viewport).unwrap();
            assert_eq!(
                scene.reuse.len(),
                if name == "ships-pair" { 10 } else { 576 }
            );
            assert!(scene.draws.len() > scene.reuse.len());
        }
    }

    #[test]
    fn local_ship_input_omits_fixed_world_points_but_keeps_exact_software_output() {
        let viewport = Viewport::new(1024., 768.);
        for name in ["ships-pair", "ships-fleet", "ships-zoom"] {
            let world = build(name, viewport).unwrap();
            let local = build(&format!("{name}-local"), viewport).unwrap();
            assert_eq!(local.checksum, world.checksum);
            let generated = local.generate(0, true).unwrap();
            assert!(generated.frames.is_empty());
            assert!(generated.meshes.is_empty());
            assert_eq!(generated.materialize().as_ref(), world.at(0));
            let scene = local.scene(0, viewport).unwrap();
            assert!(scene.reuse.is_empty());
            assert_eq!(
                scene.reusable_polygons(),
                if name == "ships-pair" { 10 } else { 576 }
            );
            let source = generated.local.as_ref().unwrap();
            assert_eq!(
                source
                    .layers
                    .iter()
                    .flat_map(|l| &l.primitives)
                    .filter(|p| matches!(p, authored::Primitive::Local(_)))
                    .count(),
                scene.reusable_polygons()
            );
            let next = local.scene(1, viewport).unwrap();
            let definitions = |s: &plan::Scene| {
                s.draws
                    .iter()
                    .filter_map(|d| match &d.shape {
                        plan::Shape::LocalPolygon(p) => Some(Arc::as_ptr(&p.definition)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(definitions(&scene), definitions(&next));
        }
    }
}
