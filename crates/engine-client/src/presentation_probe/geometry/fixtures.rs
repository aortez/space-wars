use crate::{
    client_scenarios::{
        self, BenchmarkConfiguration, ClockBenchmarkCase, ClockBenchmarkConfig, RenderBackend,
        ScenarioStartMode,
    },
    gpu_scene::{
        plan,
        reusable::{Definition, Placement},
    },
    render::{FrameLayout, Viewport},
};
use engine_common::{
    Camera2, RenderCircle, RenderColor, RenderFrame, RenderLine, RenderPoint, RenderPolygon,
    RenderPrimitive, Settings, Stroke,
};
use scenario_pizza::PizzaBenchmarkConfig;
use std::{sync::Arc, time::Duration};

mod meshes;
mod pizza;
mod ships;

struct AuthoredMesh {
    draw: usize,
    definition: Arc<Definition>,
    position: RenderPoint,
    angle: f32,
}

pub(super) struct Fixture {
    pub name: String,
    pub seed: u64,
    pub frames: Vec<RenderFrame>,
    pub tick: u32,
    pub objects: usize,
    pub omitted_text: usize,
    pub checksum: u64,
    pub sequence: Vec<Vec<RenderFrame>>,
    meshes: Vec<Vec<AuthoredMesh>>,
    producer: Option<ships::Producer>,
}

impl Fixture {
    pub fn scene(&self, index: usize, viewport: Viewport) -> Result<plan::Scene, String> {
        self.scene_mode(index, viewport, true)
    }
    pub fn scene_mode(
        &self,
        index: usize,
        viewport: Viewport,
        reuse: bool,
    ) -> Result<plan::Scene, String> {
        let generated = self.generate(index, reuse);
        self.project(index, viewport, generated.as_ref())
    }
    pub fn generates_frames(&self) -> bool {
        self.producer.is_some()
    }
    pub fn local_geometry(&self) -> bool {
        self.producer.as_ref().is_some_and(|p| p.local)
    }
    pub fn reference_scene(&self, index: usize, viewport: Viewport) -> Result<plan::Scene, String> {
        // Independent ordinary world polygons, never the local input's own
        // expansion. Used outside timing to detect errors in that expansion.
        plan::build(self.at(index), viewport, FrameLayout::EqualHorizontal, 1.0)
    }
    pub fn generate(&self, index: usize, reuse: bool) -> Option<ships::Generated> {
        self.producer.as_ref().map(|p| p.generate(index, reuse))
    }
    pub fn project(
        &self,
        index: usize,
        viewport: Viewport,
        generated: Option<&ships::Generated>,
    ) -> Result<plan::Scene, String> {
        if let Some(frame) = generated.and_then(|g| g.local.as_ref()) {
            return plan::build_authored(frame, viewport);
        }
        let frames = generated.map_or_else(|| self.at(index), |g| g.frames.as_slice());
        let mut scene = plan::build(frames, viewport, FrameLayout::EqualHorizontal, 1.0)?;
        let meshes = generated.map(|g| g.meshes.as_slice()).or_else(|| {
            (!self.meshes.is_empty()).then(|| self.meshes[index % self.meshes.len()].as_slice())
        });
        if let Some(meshes) = meshes {
            let camera = &frames[0].camera;
            scene.reuse = meshes
                .iter()
                .map(|p| {
                    let normalized = camera.world_to_viewport(p.position, viewport.aspect_ratio());
                    Placement {
                        draw: p.draw,
                        polygon: crate::gpu_scene::reusable::Polygon {
                            definition: p.definition.clone(),
                            angle: p.angle,
                            scale: viewport.height / camera.height,
                            translation: RenderPoint::new(
                                normalized.x * viewport.width,
                                normalized.y * viewport.height,
                            ),
                        },
                    }
                })
                .collect();
        }
        Ok(scene)
    }
    pub fn at(&self, frame: usize) -> &[RenderFrame] {
        if self.sequence.is_empty() {
            &self.frames
        } else {
            &self.sequence[frame % self.sequence.len()]
        }
    }
}

pub(super) fn build(name: &str, viewport: Viewport) -> Result<Fixture, Box<dyn std::error::Error>> {
    if let Some(direction) = name.strip_prefix("pizza-event-") {
        return pizza::build(direction, viewport);
    }
    if matches!(
        name,
        "ships-pair"
            | "ships-fleet"
            | "ships-zoom"
            | "ships-pair-local"
            | "ships-fleet-local"
            | "ships-zoom-local"
    ) {
        return ships::build(name, viewport);
    }
    if matches!(
        name,
        "meshes-grouped" | "meshes-interleaved" | "meshes-churn"
    ) {
        return meshes::build(name, viewport);
    }
    if matches!(
        name,
        "mixed-grouped" | "mixed-interleaved" | "moving-circles"
    ) {
        let sequence: Vec<_> = (0..32)
            .map(|i| vec![mixed(name, viewport.aspect_ratio(), i)])
            .collect();
        return Ok(Fixture {
            name: name.into(),
            seed: 7,
            frames: sequence[0].clone(),
            tick: 0,
            objects: if name == "moving-circles" {
                1024
            } else {
                192 * 3
            },
            omitted_text: 0,
            checksum: checksum(&serde_json::to_vec(&sequence)?),
            sequence,
            meshes: Vec::new(),
            producer: None,
        });
    }
    let mut config = BenchmarkConfiguration::default();
    let (scenario_id, tick, objects) = match name {
        "clock-idle" | "clock-rain" => {
            config.clock = ClockBenchmarkConfig {
                case: if name == "clock-idle" {
                    ClockBenchmarkCase::Idle
                } else {
                    ClockBenchmarkCase::Rain
                },
                ..Default::default()
            };
            ("clock", 600, 0)
        }
        "pizza-300" | "pizza-1000" | "pizza-5000" | "pizza-10000" => {
            let count = name.strip_prefix("pizza-").unwrap().parse()?;
            config.pizza = PizzaBenchmarkConfig {
                ball_count: count,
                ..Default::default()
            };
            ("pizza", 120, count)
        }
        "overlap-1" | "overlap-8" | "overlap-32" | "overlap-128" => {
            ("", 0, name.strip_prefix("overlap-").unwrap().parse()?)
        }
        _ => return Err(format!("unknown geometry fixture {name}").into()),
    };
    let mut frames = if scenario_id.is_empty() {
        vec![overlap(objects, viewport.aspect_ratio())]
    } else {
        let mut scenario = client_scenarios::registration(scenario_id)
            .unwrap()
            .create_with_asset(
                7,
                &Settings::default(),
                viewport,
                ScenarioStartMode::Benchmark(config),
                &client_scenarios::ScenarioAsset::None,
            )?;
        for _ in 0..tick {
            scenario.step(&[], Duration::from_nanos(16_666_667));
        }
        scenario.render_frames(RenderBackend::Raster, viewport)
    };
    // Both renderers here draw scene geometry only. Slint text/UI is explicitly
    // excluded, rather than compared against a reference containing extra text.
    let mut omitted_text = 0;
    for frame in &mut frames {
        for layer in &mut frame.layers {
            layer.primitives.retain(|p| {
                if matches!(p, RenderPrimitive::Text(_)) {
                    omitted_text += 1;
                    false
                } else {
                    true
                }
            });
        }
    }
    let checksum = checksum(&serde_json::to_vec(&frames)?);
    Ok(Fixture {
        name: name.into(),
        seed: 7,
        frames,
        tick,
        objects,
        omitted_text,
        checksum,
        sequence: Vec::new(),
        meshes: Vec::new(),
        producer: None,
    })
}

// Camera pan/zoom, changing positions/radii/styles, rotated/deforming concave
// polygons and lines. Precomputed outside timing; every mode replays identical
// inputs. Grouping is a property of the fixture, NEVER a renderer reordering.
fn mixed(name: &str, aspect: f32, frame_index: u32) -> RenderFrame {
    let t = frame_index as f32 * std::f32::consts::TAU / 32.0;
    let mut frame = RenderFrame::new(Camera2::new(
        RenderPoint::new(4.0 * t.sin(), 3.0 * t.cos()),
        200.0 + 20.0 * t.sin(),
    ));
    let only_circles = name == "moving-circles";
    let count = if only_circles { 1024 } else { 192 };
    let columns = if only_circles { 32 } else { 16 };
    let rows = count / columns;
    for i in 0..count {
        let phase = t + i as f32 * 0.73;
        let x = ((i % columns) as f32 / (columns - 1) as f32 - 0.5) * 190.0 * aspect
            + 3.0 * phase.sin();
        let y = ((i / columns) as f32 / (rows - 1) as f32 - 0.5) * 190.0 + 3.0 * phase.cos();
        let color = RenderColor::rgba(0.4 + 0.3 * phase.sin(), 0.5 + 0.4 * phase.cos(), 0.8, 0.55);
        let mut circle = RenderCircle::filled(
            RenderPoint::new(x, y),
            0.4 + (i % 9) as f32 * 0.65 + 0.2 * phase.cos(),
            color,
        );
        if i % 3 != 0 {
            circle.stroke = Some(Stroke::new(
                RenderColor::rgba(0.9, 0.3, 0.1, 0.6),
                0.25 + (i % 5) as f32 * 0.7,
            ));
        }
        if i % 3 == 2 {
            circle.fill = None;
        }
        frame.push_primitive(0, RenderPrimitive::Circle(circle));
        if only_circles {
            continue;
        }
        let layer = if name == "mixed-grouped" { 1 } else { 0 };
        let points = [
            (0.0, -6.0),
            (5.0, 4.0),
            (0.0, 1.0 + phase.sin()),
            (-5.0, 4.0),
        ]
        .map(|(px, py)| {
            RenderPoint::new(
                x + 2.0 + px * phase.cos() - py * phase.sin(),
                y + px * phase.sin() + py * phase.cos(),
            )
        });
        let mut polygon =
            RenderPolygon::filled(points.to_vec(), RenderColor::rgba(0.8, 0.2, 0.5, 0.45));
        polygon.stroke = Some(Stroke::new(RenderColor::rgba(0.9, 0.8, 0.1, 0.5), 0.8));
        frame.push_primitive(layer, RenderPrimitive::Polygon(polygon));
        frame.push_primitive(
            layer,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::new(x - 4.0, y),
                RenderPoint::new(x + 4.0, y + 2.0 * phase.sin()),
                Stroke::new(RenderColor::WHITE, 0.5),
            )),
        );
    }
    frame
}

fn overlap(layers: usize, aspect: f32) -> RenderFrame {
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 2.0));
    for i in 0..layers {
        let color = match i % 3 {
            0 => RenderColor::rgba(1.0, 0.15, 0.05, 0.15),
            1 => RenderColor::rgba(0.05, 1.0, 0.15, 0.15),
            _ => RenderColor::rgba(0.15, 0.05, 1.0, 0.15),
        };
        frame.push_primitive(
            i as i32,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(-aspect, -1.0),
                    RenderPoint::new(aspect, -1.0),
                    RenderPoint::new(aspect, 1.0),
                    RenderPoint::new(-aspect, 1.0),
                ],
                color,
            )),
        );
    }
    frame
}

pub(super) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures_are_deterministic_and_text_free() {
        let viewport = Viewport::new(128.0, 96.0);
        for name in ["clock-idle", "clock-rain", "pizza-300", "overlap-8"] {
            let a = build(name, viewport).unwrap();
            let b = build(name, viewport).unwrap();
            assert_eq!(a.frames, b.frames, "{name}");
            assert_eq!(a.checksum, b.checksum);
            assert!(
                a.frames
                    .iter()
                    .flat_map(|f| &f.layers)
                    .flat_map(|l| &l.primitives)
                    .all(|p| !matches!(p, RenderPrimitive::Text(_)))
            );
            if name == "pizza-300" {
                assert_eq!(a.objects, 300);
                assert_eq!(a.omitted_text, 1);
            }
        }
        assert!(build("invalid", viewport).is_err());
    }
    #[test]
    fn overlap_scales_layers_without_changing_coverage_or_alpha() {
        let a = overlap(8, 4.0 / 3.0);
        assert_eq!(a.layers.len(), 8);
        for (i, layer) in a.layers.iter().enumerate() {
            assert_eq!(layer.z, i as i32);
            let RenderPrimitive::Polygon(p) = &layer.primitives[0] else {
                panic!()
            };
            assert_eq!(p.points.len(), 4);
            assert_eq!(p.fill.unwrap().color.a, 0.15);
        }
    }

    #[test]
    fn moving_inputs_are_deterministic_and_change_geometry_camera_and_style() {
        let viewport = Viewport::new(128.0, 96.0);
        for name in ["mixed-grouped", "mixed-interleaved", "moving-circles"] {
            let a = build(name, viewport).unwrap();
            let b = build(name, viewport).unwrap();
            assert_eq!(a.checksum, b.checksum);
            assert_eq!(a.sequence.len(), 32);
            assert_eq!(a.at(0), a.at(32));
            assert_ne!(a.at(0), a.at(1));
            assert_ne!(a.at(0)[0].camera, a.at(1)[0].camera);
            assert_ne!(a.at(0)[0].layers, a.at(1)[0].layers);
        }
    }
}
