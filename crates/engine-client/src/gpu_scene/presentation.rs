//! Ordered render passes for live camera layouts. World layers keep their order;
//! minimaps are opaque intermediate scenes, clipped/faded once as a group.
pub(crate) mod scratch;
#[cfg(test)]
mod storage_tests;
use super::plan::{self, Scene};
use crate::render::{self, FrameLayout, Viewport};
use engine_common::RenderFrame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Clip {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Clip {
    pub fn new(v: Viewport, size: (u32, u32)) -> Option<Self> {
        // Same pixel coverage as the software adapter, including fractional panes.
        let x = (v.x.floor() as i32).max(0);
        let y = (v.y.floor() as i32).max(0);
        let right = ((v.x + v.width).ceil() as i32).min(size.0 as i32);
        let bottom = ((v.y + v.height).ceil() as i32).min(size.1 as i32);
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

#[derive(Debug)]
pub(crate) enum Pass {
    Direct {
        scene: Scene,
        clip: Clip,
    },
    Minimap {
        scene: Scene,
        destination: Viewport,
        opacity: f32,
    },
}

impl Pass {
    pub fn clip(&self) -> Clip {
        match self {
            Self::Direct { clip, .. } => *clip,
            Self::Minimap { scene, .. } => Clip {
                x: 0,
                y: 0,
                width: scene.width as i32,
                height: scene.height as i32,
            },
        }
    }

    pub fn scene(&self) -> &Scene {
        match self {
            Self::Direct { scene, .. } | Self::Minimap { scene, .. } => scene,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Plan {
    pub width: u32,
    pub height: u32,
    pub passes: Vec<Pass>,
}

impl Plan {
    pub fn shapes(&self) -> usize {
        self.passes.iter().map(|p| p.scene().draws.len()).sum()
    }

    pub fn single_scene(&self) -> Option<&Scene> {
        match self.passes.as_slice() {
            [Pass::Direct { scene, clip }]
                if *clip
                    == (Clip {
                        x: 0,
                        y: 0,
                        width: self.width as i32,
                        height: self.height as i32,
                    }) =>
            {
                Some(scene)
            }
            _ => None,
        }
    }
}

pub(crate) fn build(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
) -> Result<Plan, String> {
    build_with_retained(frames, viewport, layout, scale, false)
}

pub(crate) fn build_with_retained(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
    retain: bool,
) -> Result<Plan, String> {
    build_with_storage(
        frames,
        viewport,
        layout,
        scale,
        retain,
        super::PlanMode::Reference,
    )
}

pub(crate) fn build_with_storage(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
    retain: bool,
    mode: super::PlanMode,
) -> Result<Plan, String> {
    build_reusing(frames, viewport, layout, scale, retain, mode, None)
}

pub(crate) fn build_reusing(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
    scale: f32,
    retain: bool,
    mode: super::PlanMode,
    storage: Option<Plan>,
) -> Result<Plan, String> {
    if scale != 1.0
        || viewport.x != 0.0
        || viewport.y != 0.0
        || ![viewport.width, viewport.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0 && *v <= 16384.0)
        || frames.is_empty()
        || frames.len() > 8
    {
        return Err(
            "GPU presentation requires 1x, a valid full-window viewport and 1..8 frames".into(),
        );
    }
    let size = (viewport.width.ceil() as u32, viewport.height.ceil() as u32);
    // Slots identify storage, not scene contents. Every pass, clip and point is
    // rebuilt even when the layout/camera/primitive types changed completely.
    let mut reusable = storage.into_iter().flat_map(|p| p.passes);
    let mut passes = Vec::with_capacity(frames.len());
    for (index, (frame, pane)) in frames
        .iter()
        .zip(render::frame_viewports(viewport, frames.len(), layout))
        .enumerate()
    {
        let Some(clip) = Clip::new(pane, size) else {
            continue;
        };
        let storage = reusable.next().map(|pass| match pass {
            Pass::Direct { scene, .. } | Pass::Minimap { scene, .. } => scene,
        });
        if layout.is_minimap(frames.len(), index) {
            let map_size = (
                pane.width.ceil().max(1.0) as u32,
                pane.height.ceil().max(1.0) as u32,
            );
            let local = Viewport::new(map_size.0 as f32, map_size.1 as f32);
            let scene = plan::frame_scene(
                frame,
                local,
                map_size,
                (layout == FrameLayout::SpacewarsLocalPlay)
                    .then_some(render::MIN_SPACEWARS_OVERVIEW_OBJECT_DIAMETER),
                retain,
                mode,
                storage,
            )?;
            passes.push(Pass::Minimap {
                scene,
                destination: Viewport::with_origin(
                    pane.x.round(),
                    pane.y.round(),
                    local.width,
                    local.height,
                ),
                opacity: render::SPACEWARS_MINIMAP_OPACITY,
            });
        } else {
            passes.push(Pass::Direct {
                scene: plan::frame_scene(frame, pane, size, None, retain, mode, storage)?,
                clip,
            });
        }
    }
    Ok(Plan {
        width: size.0,
        height: size.1,
        passes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_scene::plan::Shape;
    use engine_common::{Camera2, RenderCircle, RenderColor, RenderPoint, RenderPrimitive};

    /// Diagnostic only: fully tessellate before checking the actual output.
    /// This measures removable work, not a proposed source-bounds culling rule.
    #[cfg(feature = "pi-kiosk-gpu")]
    #[test]
    #[ignore = "production replay visibility audit; run explicitly with --nocapture"]
    fn audit_viewport_work() {
        use crate::client_scenarios::{self, RenderBackend, ScenarioAsset, ScenarioStartMode};
        use crate::gpu_scene::mesh::MeshBuilder;
        use engine_common::{Settings, SpacewarsController};
        let viewport = Viewport::new(1024., 768.);
        for (name, seed, start, human) in [
            ("bots-early", 42, 120, false),
            ("bots-late", 42, 1800, false),
            ("close-up-zoom", 0, 1500, true),
        ] {
            let mut settings = Settings::default();
            settings.spacewars.player_1_controller = if human {
                SpacewarsController::Human
            } else {
                SpacewarsController::RuleBot
            };
            settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
            let mut scenario = client_scenarios::registration("spacewars")
                .unwrap()
                .create_with_asset(
                    seed,
                    &settings,
                    viewport,
                    ScenarioStartMode::Normal,
                    &ScenarioAsset::None,
                )
                .unwrap();
            for tick in 0..start + 65 {
                scenario.step(&[], std::time::Duration::from_nanos(16_666_667));
                if tick != start && tick != start + 64 {
                    continue;
                }
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                let plan =
                    build_with_retained(&frames, viewport, scenario.frame_layout(), 1., true)
                        .unwrap();
                for (view, pass) in plan.passes.iter().enumerate() {
                    let scene = pass.scene();
                    let clip = match pass {
                        Pass::Direct { clip, .. } => *clip,
                        Pass::Minimap { .. } => Clip {
                            x: 0,
                            y: 0,
                            width: scene.width as i32,
                            height: scene.height as i32,
                        },
                    };
                    let mut mesh = MeshBuilder::default();
                    let mut counts = [[0_usize; 4]; 2];
                    for draw in &scene.draws {
                        for (style, count) in counts.iter_mut().enumerate() {
                            mesh.clear();
                            mesh.append_draw_with_styles(
                                draw,
                                (style == 0).then_some(draw.fill).flatten(),
                                (style == 1).then_some(draw.stroke).flatten(),
                            )
                            .unwrap();
                            let vertices = &mesh.buffers.vertices;
                            if vertices.is_empty() {
                                continue;
                            }
                            let outside = vertices.iter().all(|v| v[0] < clip.x as f32 - 1.)
                                || vertices.iter().all(|v| v[1] < clip.y as f32 - 1.)
                                || vertices
                                    .iter()
                                    .all(|v| v[0] > (clip.x + clip.width) as f32 + 1.)
                                || vertices
                                    .iter()
                                    .all(|v| v[1] > (clip.y + clip.height) as f32 + 1.);
                            count[0] += 1;
                            count[1] += usize::from(outside);
                            count[2] += vertices.len();
                            count[3] += usize::from(outside) * vertices.len();
                        }
                    }
                    eprintln!(
                        "visibility fixture={name} tick={tick} view={view} fill={:?} stroke={:?} [pieces,outside,vertices,outside_vertices]",
                        counts[0], counts[1]
                    );
                }
            }
        }
    }

    #[cfg(feature = "pi-kiosk-gpu")]
    #[test]
    fn close_up_seed_zero_terrain_hints_survive_projection() {
        use crate::client_scenarios::{self, RenderBackend, ScenarioAsset, ScenarioStartMode};
        use engine_common::{Settings, SpacewarsController};
        let viewport = Viewport::new(1024., 768.);
        let mut settings = Settings::default();
        settings.spacewars.player_1_controller = SpacewarsController::Human;
        settings.spacewars.player_2_controller = SpacewarsController::RuleBot;
        let mut scenario = client_scenarios::registration("spacewars")
            .unwrap()
            .create_with_asset(
                0,
                &settings,
                viewport,
                ScenarioStartMode::Normal,
                &ScenarioAsset::None,
            )
            .unwrap();
        for _ in 0..180 {
            scenario.step(&[], std::time::Duration::from_nanos(16_666_667));
        }
        let frames = scenario.render_frames(RenderBackend::Raster, viewport);
        let plan =
            build_with_retained(&frames, viewport, scenario.frame_layout(), 1.0, true).unwrap();
        assert_eq!(frames[0].camera.height, 44.);
        assert!(!frames[0].geometry_hints.is_empty());
        // The old projected-point test rejected all three groups: equivalent
        // f32 projection orders differed by 0.0031..0.0037 logical pixels.
        assert_eq!(
            plan.passes[0].scene().retained.len(),
            frames[0].geometry_hints.len()
        );
    }

    #[test]
    fn retained_hints_respect_layers_skipped_draws_and_mutated_frames() {
        use engine_common::{PolygonGeometry, PolygonGeometryHint, RenderPolygon, RenderText};
        use std::sync::Arc;
        let mut f = frame();
        let polygon = RenderPolygon::filled(
            vec![
                RenderPoint::ZERO,
                RenderPoint::new(4., 0.),
                RenderPoint::new(0., 4.),
            ],
            RenderColor::RED,
        );
        f.push_primitive(0, RenderPrimitive::Polygon(polygon.clone()));
        let layer = f.layers.iter().position(|l| l.z == 0).unwrap();
        f.geometry_hints.push(PolygonGeometryHint {
            layer,
            first: 0,
            geometry: Arc::new(PolygonGeometry::new(vec![polygon]).unwrap()),
            position: RenderPoint::ZERO,
            angle: 0.0,
        });
        let v = Viewport::new(640., 360.);
        let project = |f: &RenderFrame| {
            build_with_retained(
                &[f.clone(), f.clone(), f.clone(), f.clone()],
                v,
                FrameLayout::SpacewarsLocalPlay,
                1.0,
                true,
            )
            .unwrap()
        };
        let plan = project(&f);
        assert_eq!(plan.passes.len(), 4);
        for p in &plan.passes {
            assert_eq!(p.scene().retained.len(), 1);
            assert!(p.scene().retained[0].matches(p.scene()));
        }
        // Even a one-ULP source edit invalidates the hint, regardless of whether
        // projection rounds it back onto the same screen point.
        if let RenderPrimitive::Polygon(p) = &mut f.layers[layer].primitives[0] {
            p.points[0].x = p.points[0].x.next_up();
        }
        assert!(
            project(&f)
                .passes
                .iter()
                .all(|p| p.scene().retained.is_empty())
        );
        // A bogus range or non-finite transform never poisons the ordinary plan.
        f.geometry_hints[0].first = usize::MAX;
        f.geometry_hints[0].position.x = f32::NAN;
        assert!(
            project(&f)
                .passes
                .iter()
                .all(|p| p.scene().retained.is_empty())
        );
        f.geometry_hints[0].first = 0;
        f.geometry_hints[0].position = RenderPoint::ZERO;
        f.layers[layer].primitives[0] =
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "overlay"));
        assert!(
            project(&f)
                .passes
                .iter()
                .all(|p| p.scene().retained.is_empty())
        );
    }

    fn frame() -> RenderFrame {
        let mut f = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        f.push_primitive(
            9,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                300.0,
                RenderColor::RED,
            )),
        );
        f.push_primitive(
            -2,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                10.0,
                RenderColor::BLUE,
            )),
        );
        f
    }

    #[test]
    fn split_cameras_use_their_actual_aspect_and_do_not_reorder_layers() {
        let plan = build(
            &[frame(), frame()],
            Viewport::new(641.0, 360.0),
            FrameLayout::EqualHorizontal,
            1.0,
        )
        .unwrap();
        assert!(plan.single_scene().is_none());
        for (i, pass) in plan.passes.iter().enumerate() {
            let Pass::Direct { scene, clip } = pass else {
                panic!()
            };
            assert_eq!(clip.x, if i == 0 { 0 } else { 320 });
            assert_eq!(clip.width, 321);
            assert_eq!(scene.draws[0].fill, Some(RenderColor::BLUE));
            assert_eq!(
                scene.draws[0].shape,
                Shape::Circle {
                    center: RenderPoint::new(160.25 + 320.5 * i as f32, 180.0),
                    radius: 36.0
                }
            );
        }
    }

    #[test]
    fn cameras_maps_and_full_window_overlay_keep_their_group_order() {
        for count in [2, 3, 4, 5] {
            let frames = vec![frame(); count];
            let before = frames.clone();
            let plan = build(
                &frames,
                Viewport::new(1280.0, 720.0),
                FrameLayout::PlayerViewsWithMinimaps,
                1.0,
            )
            .unwrap();
            let players = count / 2;
            assert_eq!(plan.passes.len(), count);
            for (i, pass) in plan.passes.iter().enumerate() {
                assert_eq!(
                    matches!(pass, Pass::Minimap { .. }),
                    (players..players * 2).contains(&i)
                );
                if let Pass::Minimap {
                    scene,
                    opacity,
                    destination,
                } = pass
                {
                    assert_eq!(*opacity, 0.74);
                    assert_eq!(scene.draws[0].fill.unwrap().a, 1.0); // Group alpha, not per shape.
                    assert_eq!(scene.width as f32, destination.width);
                }
            }
            assert_eq!(frames, before);
        }
    }

    #[test]
    fn invisible_legacy_frames_and_small_debris_match_raster_policy() {
        let mut debris = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        debris.push_primitive(
            1,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                0.01,
                RenderColor::RED,
            )),
        );
        let plan = build(
            &[frame(), frame(), debris.clone(), debris, frame()],
            Viewport::new(640.0, 360.0),
            FrameLayout::SpacewarsLocalPlay,
            1.0,
        )
        .unwrap();
        assert_eq!(plan.passes.len(), 4);
        assert!(plan.passes[2].scene().draws.is_empty());
    }

    #[test]
    fn invalid_input_is_rejected_instead_of_displaying_a_partial_frame() {
        let v = Viewport::new(640.0, 360.0);
        for frames in [vec![], vec![frame(); 9]] {
            assert!(build(&frames, v, FrameLayout::EqualHorizontal, 1.0).is_err());
        }
        for scale in [0.5, 2.0, f32::NAN] {
            assert!(build(&[frame()], v, FrameLayout::EqualHorizontal, scale).is_err());
        }
        let mut invalid = frame();
        invalid.camera.height = f32::NAN;
        assert!(build(&[frame(), invalid], v, FrameLayout::EqualHorizontal, 1.0).is_err());
    }
}
