//! Generic probes through the real vector/raster consumers; no Clock admission.
use crate::{
    raster::{RasterOptions, RasterRenderer},
    render::{self, FrameLayout, Viewport},
};
use engine_common::{
    Camera2, RenderCircle, RenderColor, RenderFrame, RenderLayer, RenderPoint, RenderPrimitive,
    Scenario,
};
use engine_core::{Color, Vec2, lighting::*};
use engine_rapier::world::{PhysicsWorld, PhysicsWorldConfig, physics_activity};

fn snapshot(points: &[Vec2]) -> LightingSnapshot {
    // Eight equal-energy points retain their actual locations, including holes.
    let sources: Vec<_> = points
        .chunks(MAX_FOOTPRINT)
        .enumerate()
        .map(|(id, points)| LocalSource {
            id: LightId(id as u64, 1),
            active: true,
            color: Color::RED,
            strength: points.len() as f32 * 10.0,
            range: 20.0,
            footprint: points
                .iter()
                .map(|&position| EmissionPoint {
                    position,
                    surface: None,
                })
                .collect(),
        })
        .collect();
    LightingSnapshot::new(
        5,
        1,
        DirectionalSource {
            toward_source: Vec2::Y,
            strength: 50.0,
            color: Color::WHITE,
        },
        &sources,
    )
    .unwrap()
}

#[test]
fn lighting_presentation_contract_consumers_cameras_cadence_pause_and_scale() {
    let mut state = scenario_clock::ClockScenario::init(Default::default(), 1);
    scenario_clock::ClockScenario::step(
        &mut state,
        &[scenario_clock::ClockAction::set_reading(
            scenario_clock::ClockReading::new(8, 24, 0).unwrap(),
        )],
        std::time::Duration::ZERO,
    );
    let normal = scenario_clock::lighting_fixture::clock_centers(state.display());
    assert!(!normal.is_empty());
    // Find the densest admitted glyph from the actual font, not an assumed
    // five-by-seven filled rectangle or a manually copied capacity constant.
    let (glyph, count) = (33u8..=126)
        .filter_map(|byte| {
            let text = char::from(byte).to_string();
            scenario_clock::lighting_fixture::text_centers(&text)
                .ok()
                .map(|points| (text, points.len()))
        })
        .max_by_key(|(_, count)| *count)
        .unwrap();
    let message = glyph.repeat(engine_common::MAX_CLOCK_MESSAGE_BYTES);
    let maximum = scenario_clock::lighting_fixture::text_centers(&message).unwrap();
    assert!(maximum.len() > normal.len());
    assert_eq!(
        maximum.len(),
        count * engine_common::MAX_CLOCK_MESSAGE_BYTES
    );
    assert!(maximum.len() <= scenario_clock::lighting_fixture::MAX_TEXT_CELLS);
    let transformed =
        scenario_clock::lighting_fixture::transformed_text_centers(&message, 0.37).unwrap();
    assert_eq!(transformed.len(), maximum.len());
    assert_ne!(transformed, maximum);
    let world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let geometry = world.query_snapshot();
    let visibility = geometry.lighting_visibility(&[], &[]).unwrap();
    let activity = physics_activity();
    for points in [&normal, &maximum, &transformed] {
        let snapshot = snapshot(points);
        assert_eq!(
            snapshot
                .sources()
                .iter()
                .map(|s| s.footprint.len())
                .sum::<usize>(),
            points.len()
        );
        let receivers = [Receiver {
            position: Vec2::new(2.0, 0.0),
            surface: None,
        }];
        let baseline = snapshot.query(5, 1, &receivers, &visibility).unwrap();
        let mut raster = RasterRenderer::new();
        // These are all configured software output scales, including clamping
        // boundaries. There is no graphics-quality/gamma/bloom energy input.
        for scale in [0.1, 0.5, 1.0, 2.0, 3.0] {
            for camera in [Vec2::ZERO, Vec2::new(200.0, 100.0)] {
                for repeats in [0, 1, 3] {
                    let mut frame =
                        RenderFrame::new(Camera2::new(RenderPoint::new(camera.x, camera.y), 40.0));
                    let mut layer = RenderLayer::new(0);
                    for source in snapshot.sources() {
                        for point in &source.footprint {
                            layer
                                .primitives
                                .push(RenderPrimitive::Circle(RenderCircle::filled(
                                    RenderPoint::new(point.position.x, point.position.y),
                                    0.3,
                                    RenderColor::rgba(
                                        source.color.r,
                                        source.color.g,
                                        source.color.b,
                                        1.0,
                                    ),
                                )));
                        }
                    }
                    frame.layers.push(layer);
                    // Scalar probe appearance uses exactly the published exposure.
                    frame
                        .layers
                        .push(RenderLayer::new(1).with_primitive(RenderPrimitive::Circle(
                            RenderCircle::filled(
                                RenderPoint::new(2.0, 0.0),
                                0.5,
                                RenderColor::rgba(baseline[0].intensity / FULL_SUN, 0.0, 0.0, 1.0),
                            ),
                        )));
                    for _ in 0..repeats {
                        let frames = [frame.clone()];
                        let viewport = Viewport::new(64.0, 48.0);
                        let _vector = render::scene_presentation_from_frames_with_layout(
                            &frames,
                            viewport,
                            FrameLayout::EqualHorizontal,
                        );
                        let _image = raster.image_from_frames_with_layout(
                            &frames,
                            viewport,
                            FrameLayout::EqualHorizontal,
                            RasterOptions::for_scale(scale),
                        );
                    }
                    assert_eq!(
                        snapshot.query(5, 1, &receivers, &visibility).unwrap(),
                        baseline
                    );
                    assert_eq!(snapshot.tick(), 5);
                    assert_eq!(physics_activity(), activity);
                }
            }
        }
    }
}
