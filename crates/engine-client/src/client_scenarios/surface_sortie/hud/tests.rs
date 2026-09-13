use super::*;
use crate::render::{self, FrameLayout};
use scenario_spacewars::surface_sortie::hud::{HudMeter, HudPrompt};

fn readout(player: usize) -> PlayerHud {
    PlayerHud {
        player,
        color: if player == 0 {
            RenderColor::RED
        } else {
            RenderColor::GREEN
        },
        mode: if player == 0 { "ON FOOT" } else { "SHIP" },
        health: HudMeter {
            label: if player == 0 { "Pilot" } else { "Hull" },
            fraction: if player == 0 { 0.96 } else { 1.0 },
        },
        resource: Some(HudMeter {
            label: if player == 0 { "Jetpack" } else { "Energy" },
            fraction: if player == 0 { 0.72 } else { 1.0 },
        }),
        rounds_loaded: (player == 1).then_some(2),
        note: if player == 0 {
            "Ship lost"
        } else {
            "OPEN 34 u/s"
        }
        .into(),
        prompt: (player == 0).then(|| HudPrompt {
            title: "Rebuild blocked".into(),
            detail: "Move to clear ground".into(),
            progress: Some(1.0),
            warning: true,
        }),
    }
}

#[test]
fn instruments_mirror_beside_the_radars_and_leave_the_center_clear() {
    for viewport in [
        Viewport::new(1024.0, 768.0),
        Viewport::new(800.0, 480.0),
        Viewport::new(480.0, 800.0),
    ] {
        for count in [1, 2] {
            for (player, pane) in viewport.split_horizontally(count).into_iter().enumerate() {
                let layout = player_hud_layout(pane, player);
                for r in [layout.radar, layout.vitals, layout.prompt] {
                    assert!(r.x >= pane.x && r.x + r.width <= pane.x + pane.width);
                    assert!(r.y >= pane.y && r.y + r.height <= pane.y + pane.height);
                }
                assert!(layout.radar.y >= pane.height * 0.75);
                if player == 0 {
                    assert!(layout.radar.x + layout.radar.width < layout.vitals.x);
                } else {
                    assert!(layout.vitals.x + layout.vitals.width < layout.radar.x);
                }
            }
        }
    }
}

#[test]
fn overlay_has_one_shared_clock_no_vitals_panels_and_optional_diagnostics() {
    let viewport = Viewport::new(1024.0, 768.0);
    let overlay = compose(&[readout(0), readout(1)], Some("Time 02:06"), viewport);
    let labels: Vec<_> = overlay
        .layers
        .iter()
        .flat_map(|l| &l.primitives)
        .filter_map(|p| match p {
            RenderPrimitive::Text(t) if t.color != SHADOW => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(labels.iter().filter(|t| t.text == "Time 02:06").count(), 1);
    assert!(!labels.iter().any(|t| t.text.starts_with("AI:")));
    assert!(
        labels
            .iter()
            .any(|t| t.text == "Rebuild blocked" && -t.position.y < 100.0)
    );
    assert!(
        labels
            .iter()
            .any(|t| t.text == "P1 · ON FOOT" && -t.position.y > 600.0)
    );
    for p in overlay.layers.iter().flat_map(|l| &l.primitives) {
        if let RenderPrimitive::Polygon(p) = p {
            let width = p.points[1].x - p.points[0].x;
            let height = p.points[0].y - p.points[2].y;
            // Bottom shapes are meter tracks or missile pips, never panels.
            if -p.points[0].y > 200.0 {
                assert!(height <= 6.0 || width <= 1.0);
            }
        }
    }
    let world = RenderFrame::default();
    let mut frames = vec![world.clone(), world.clone(), world.clone(), world, overlay];
    append_bot_diagnostic(&mut frames, 1, "Testing optional diagnostics");
    assert!(frames[4].layers.iter().flat_map(|l| &l.primitives).any(
        |p| matches!(p, RenderPrimitive::Text(t) if t.text == "AI: Testing optional diagnostics")
    ));
    let presentation = render::scene_presentation_from_frames_with_layout(
        &frames,
        viewport,
        FrameLayout::PlayerViewsWithMinimaps,
    );
    assert_eq!(presentation.minimaps.len(), 2);
    assert_eq!(
        render::frame_projections(&frames, viewport, FrameLayout::PlayerViewsWithMinimaps).len(),
        2
    );
    let rows = render::raster_text_overlay(&frames, viewport, FrameLayout::PlayerViewsWithMinimaps);
    let vector_rows: Vec<_> = presentation
        .main_primitives
        .into_iter()
        .filter(|p| p.kind == crate::PrimitiveKind::Text)
        .collect();
    assert_eq!(rows, vector_rows);
}

#[test]
fn hud_visual_fixture_captures_production_ui_at_device_sizes() {
    use crate::{
        MainWindow, client_scenarios::ClientScenario, host::RenderBackend, raster::RasterRenderer,
    };
    use scenario_spacewars::{
        PlayerId,
        surface_sortie::{SurfaceSortieAction, SurfaceSortieScenario},
    };
    use slint::{
        ComponentHandle,
        platform::{
            Platform, PlatformError, WindowAdapter,
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        },
    };
    use std::{rc::Rc, time::Duration};
    struct TestPlatform;
    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
        }
    }
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let window = MainWindow::new().unwrap();
    window.show().unwrap();
    window.set_performance_overlay_enabled(true);
    window.set_performance_overlay_text("FPS 60 | UPS 60".into());
    window.set_autostart_running(true);
    let output = std::env::var_os("SPACEWARS_HUD_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &output {
        std::fs::create_dir_all(path).unwrap();
    }
    let dt = Duration::from_nanos(16_666_667);
    let mut sortie = super::super::SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init_expedition(0, 2),
    };
    for _ in 0..240 {
        sortie.step(&[], dt);
    }
    sortie.step(
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        dt,
    );
    for _ in 0..90 {
        sortie.step(
            &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
            dt,
        );
    }
    assert_eq!(sortie.state.player_hud(0).mode, "ON FOOT");
    let mut flight = super::super::SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init_material_match(42),
    };
    flight.step(&[], dt);
    for (profile, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
    ] {
        window
            .window()
            .set_size(slint::LogicalSize::new(viewport.width, viewport.height));
        for (name, scenario) in [("on-foot", &sortie), ("flight", &flight)] {
            for (backend, scale, suffix) in [
                (RenderBackend::Raster, 1.0, "raster-1x"),
                (RenderBackend::Raster, 2.0, "raster-2x"),
            ] {
                let frames = scenario.render_frames(backend, viewport);
                crate::host::present_frames(
                    &window,
                    frames,
                    scenario.frame_layout(),
                    backend,
                    scale,
                    &mut RasterRenderer::new(),
                );
                let pixels = window.window().take_snapshot().unwrap();
                assert_eq!(
                    (pixels.width(), pixels.height()),
                    (viewport.width as u32, viewport.height as u32)
                );
                if let Some(path) = &output {
                    let file =
                        std::fs::File::create(path.join(format!("{profile}-{name}-{suffix}.png")))
                            .unwrap();
                    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.as_bytes())
                        .unwrap();
                }
            }
        }
        // A prescribed blocked prompt exercises dense/critical UI without
        // waiting for a random match to happen to produce a blocked rebuild.
        let mut frames = sortie.render_frames(RenderBackend::Raster, viewport);
        *frames.last_mut().unwrap() =
            compose(&[readout(0), readout(1)], Some("Time 02:06"), viewport);
        crate::host::present_frames(
            &window,
            frames,
            sortie.frame_layout(),
            RenderBackend::Raster,
            1.0,
            &mut RasterRenderer::new(),
        );
        let pixels = window.window().take_snapshot().unwrap();
        if let Some(path) = &output {
            let file =
                std::fs::File::create(path.join(format!("{profile}-blocked-fixture.png"))).unwrap();
            let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels.as_bytes())
                .unwrap();
        }
    }
}
