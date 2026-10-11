use super::*;
use crate::raster::{RasterOptions, RasterRenderer, primitive_count};
use scenario_spacewars::surface_sortie::{comparison::TerrainSurface, impact::RecoveryDisruption};

#[test]
fn source_point_pool_preserves_frames_and_hints_through_edits_zoom_and_replacement() {
    let mut pool =
        engine_common::RenderPointPool::new(crate::gpu_scene::source_control::LIMIT_BYTES);
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let mut scenario = SurfaceSortieClientScenario::new(
            SurfaceSortieScenario::init_material_arena_surface_trial(42, false, 1.7, surface),
        );
        scenario.step(&[], Duration::from_nanos(16_666_667));
        for edited in [false, true] {
            if edited {
                assert!(scenario.state.queue_recovery_disruption(
                    0,
                    RecoveryDisruption::GroundRouteNode { node: 0, radius: 8 }
                ));
                for _ in 0..12 {
                    scenario.step(&[], Duration::from_nanos(16_666_667));
                }
                assert!(scenario.state.mining_observation(0).unwrap().removed_cells > 0);
            }
            for viewport in [
                Viewport::new(1024., 768.),
                Viewport::new(641., 361.),
                Viewport::new(361., 641.),
            ] {
                let reference = scenario.render_frames(RenderBackend::Raster, viewport);
                let pooled = scenario
                    .render_frames_pooled(RenderBackend::Raster, viewport, &mut pool)
                    .unwrap();
                assert_eq!(reference, pooled);
                for (a, b) in reference.iter().zip(&pooled) {
                    assert_eq!(a.geometry_hints.len(), b.geometry_hints.len());
                    for (a, b) in a.geometry_hints.iter().zip(&b.geometry_hints) {
                        assert_eq!(
                            (a.layer, a.first, a.position, a.angle),
                            (b.layer, b.first, b.position, b.angle)
                        );
                        assert_eq!(a.geometry.polygons(), b.geometry.polygons());
                    }
                }
                pool.recycle(pooled);
            }
            let mut camera = SurfaceSortieScenario::camera_target(&scenario.state, 0, false).camera;
            for height in [44., 640., 44.] {
                camera.height = height;
                for center in [
                    camera.center,
                    engine_common::RenderPoint::new(-1.0e6, 1.0e6),
                ] {
                    let camera = engine_common::Camera2::new(center, height);
                    let viewport = Some(engine_common::RenderPoint::new(512., 768.));
                    let reference = SurfaceSortieScenario::player_frame_with_camera(
                        &scenario.state,
                        0,
                        camera,
                        viewport,
                    );
                    let pooled = SurfaceSortieScenario::player_frame_with_storage(
                        &scenario.state,
                        0,
                        camera,
                        viewport,
                        &mut pool,
                    );
                    assert_eq!(reference, pooled);
                    pool.recycle(vec![pooled]);
                }
            }
        }
    }
    assert!(pool.reuses > 1000);
    assert_eq!(pool.discards, 0);
    assert!(pool.high_water_bytes <= crate::gpu_scene::source_control::LIMIT_BYTES);
}

#[test]
fn chunk_culling_matches_reference_pixels_across_surfaces_edits_and_resizes() {
    let mut removed_primitives = 0;
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        for (case, state) in [
            (
                "arena0",
                SurfaceSortieScenario::init_material_arena_surface_trial(42, false, 0.0, surface),
            ),
            (
                "arena1.7",
                SurfaceSortieScenario::init_material_arena_surface_trial(42, false, 1.7, surface),
            ),
            (
                "single",
                SurfaceSortieScenario::init_surface_comparison(42, 1, surface),
            ),
        ] {
            let mut scenario = SurfaceSortieClientScenario::new(state);
            scenario.step(&[], Duration::from_nanos(16_666_667));
            let players = scenario.state.player_count();
            let mut full_raster = RasterRenderer::new();
            let mut culled_raster = RasterRenderer::new();
            for edited in [false, true] {
                if edited {
                    assert!(scenario.state.queue_recovery_disruption(
                        0,
                        RecoveryDisruption::GroundRouteNode { node: 0, radius: 8 }
                    ));
                    for _ in 0..12 {
                        scenario.step(&[], Duration::from_nanos(16_666_667));
                    }
                    assert!(scenario.state.mining_observation(0).unwrap().removed_cells > 0);
                }
                for viewport in [
                    Viewport::new(320.0, 240.0),
                    Viewport::new(321.0, 117.0),
                    Viewport::new(117.0, 321.0),
                ] {
                    let full = scenario.render_frames_reference(viewport).unwrap();
                    let culled = scenario.render_frames(RenderBackend::Raster, viewport);
                    assert_eq!(
                        culled,
                        scenario.render_frames(RenderBackend::Vector, viewport)
                    );
                    assert_eq!(&full[players..], &culled[players..], "minimaps changed");
                    assert_eq!(
                        crate::render::raster_text_overlay(
                            &full,
                            viewport,
                            scenario.frame_layout()
                        ),
                        crate::render::raster_text_overlay(
                            &culled,
                            viewport,
                            scenario.frame_layout()
                        )
                    );
                    removed_primitives += primitive_count(&full) - primitive_count(&culled);
                    for scale in [crate::MIN_RASTER_SCALE, 1.0, 2.0, 3.0] {
                        let internal =
                            Viewport::new(viewport.width * scale, viewport.height * scale);
                        let options = RasterOptions::for_scale(scale);
                        let a = full_raster.image_from_frames_with_layout(
                            &full,
                            internal,
                            scenario.frame_layout(),
                            options,
                        );
                        let b = culled_raster.image_from_frames_with_layout(
                            &culled,
                            internal,
                            scenario.frame_layout(),
                            options,
                        );
                        assert!(
                            a.to_rgb8().unwrap().as_slice() == b.to_rgb8().unwrap().as_slice(),
                            "pixels changed: surface={surface:?}, case={case}, edited={edited}, viewport={viewport:?}, scale={scale}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        removed_primitives > 10000,
        "must exercise meaningful culling"
    );
}
