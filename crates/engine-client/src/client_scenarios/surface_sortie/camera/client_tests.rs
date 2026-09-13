use super::*;
use crate::{
    client_scenarios::{
        ClientScenario, RenderBackend, surface_sortie::SurfaceSortieClientScenario,
    },
    raster::{RasterOptions, RasterRenderer},
    render::{self, Viewport},
};
use engine_common::{RenderPrimitive, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieAction, SurfaceSortieScenario, SurfaceWingAction,
    },
};

const DT: Duration = Duration::from_nanos(16_666_667);

fn step(scenario: &mut SurfaceSortieClientScenario, input: SurfaceSortieAction) {
    scenario.step(&[input.encode(PlayerId::PLAYER_1)], DT);
}

#[test]
fn camera_render_cadence_does_not_change_physics_or_camera_history() {
    let mut shown = SurfaceSortieClientScenario::new(SurfaceSortieScenario::init_material(42, 1));
    let mut hidden = SurfaceSortieClientScenario::new(shown.state.clone());
    let mut reference = shown.state.clone();
    let viewport = Viewport::new(800.0, 480.0);
    for tick in 0..180 {
        let input = SurfaceSortieAction {
            interact_held: tick == 120,
            ..Default::default()
        };
        step(&mut shown, input);
        step(&mut hidden, input);
        SurfaceSortieScenario::step(&mut reference, &[input.encode(PlayerId::PLAYER_1)], DT);
        // 30 Hz, 60 Hz, then 120 Hz render requests over the same 60 Hz physics.
        let count = if tick < 60 {
            usize::from(tick % 2 == 0)
        } else if tick < 120 {
            1
        } else {
            2
        };
        for _ in 0..count {
            let frames = shown.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(frames, shown.render_frames(RenderBackend::Vector, viewport));
        }
        assert_eq!(shown.cameras, hidden.cameras);
    }
    assert_eq!(
        SurfaceSortieScenario::observe(&shown.state).payload,
        SurfaceSortieScenario::observe(&reference).payload
    );
    let before = shown.render_frames(RenderBackend::Raster, viewport);
    shown.step(
        &[SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        Duration::ZERO,
    );
    assert_eq!(before, shown.render_frames(RenderBackend::Raster, viewport));
    let fresh = SurfaceSortieClientScenario::new(SurfaceSortieScenario::init_material(42, 1));
    let target = SurfaceSortieScenario::camera_target(&fresh.state, 0, false);
    assert_eq!(fresh.cameras[0], PlayerCamera::new(target));
}

#[test]
fn camera_transition_uses_displayed_view_for_culling_maps_and_pointer_projection() {
    let mut scenario =
        SurfaceSortieClientScenario::new(SurfaceSortieScenario::init_material(42, 2));
    for _ in 0..90 {
        step(&mut scenario, SurfaceSortieAction::default());
    }
    let desired = SurfaceSortieScenario::camera_target(&scenario.state, 0, false);
    let mut wide = desired;
    wide.camera.center.x -= 100.0;
    wide.camera.height = 260.0;
    scenario.cameras[0] = PlayerCamera::new(wide);
    let mut culled_raster = RasterRenderer::new();
    let mut full_raster = RasterRenderer::new();
    for tick in 0..=90 {
        if [0, 12, 30, 90].contains(&tick) {
            for viewport in [Viewport::new(320.0, 240.0), Viewport::new(160.0, 320.0)] {
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                let reference = scenario.render_frames_reference(viewport).unwrap();
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                assert_eq!(frames[0].camera, reference[0].camera);
                let layout = scenario.frame_layout();
                let projections = render::frame_projections(&frames, viewport, layout);
                assert_eq!(projections.len(), 2);
                for seat in 0..2 {
                    assert_eq!(projections[seat].camera, frames[seat].camera);
                    let bounds = frames[seat]
                        .camera
                        .world_bounds(projections[seat].viewport.aspect_ratio());
                    let map = frames[seat + 2]
                        .ordered_layers()
                        .into_iter()
                        .find(|l| l.z == 0)
                        .unwrap();
                    let RenderPrimitive::Polygon(footprint) = &map.primitives[0] else {
                        panic!("footprint")
                    };
                    assert_eq!(footprint.points[0], bounds.min);
                    assert_eq!(footprint.points[2], bounds.max);
                }
                for scale in [1.0, 2.0] {
                    let internal = Viewport::new(viewport.width * scale, viewport.height * scale);
                    let full = full_raster.image_from_frames_with_layout(
                        &reference,
                        internal,
                        layout,
                        RasterOptions::for_scale(scale),
                    );
                    let culled = culled_raster.image_from_frames_with_layout(
                        &frames,
                        internal,
                        layout,
                        RasterOptions::for_scale(scale),
                    );
                    assert_eq!(
                        full.to_rgb8().unwrap().as_slice(),
                        culled.to_rgb8().unwrap().as_slice(),
                        "tick={tick}, viewport={viewport:?}, scale={scale}"
                    );
                }
            }
        }
        scenario.cameras[0].advance(desired, DT);
    }
}

#[test]
fn camera_transition_fixture_lands_exits_boards_and_takes_off() {
    use crate::MainWindow;
    use slint::{
        ComponentHandle,
        platform::{
            Platform, PlatformError, WindowAdapter,
            software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
        },
    };
    use std::rc::Rc;
    struct TestPlatform;
    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
        }
    }
    let output = std::env::var_os("SPACEWARS_CAMERA_ARTIFACTS").map(std::path::PathBuf::from);
    let window = output.as_ref().map(|directory| {
        std::fs::create_dir_all(directory).unwrap();
        slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
        let window = MainWindow::new().unwrap();
        window.show().unwrap();
        window
    });
    let capture = |name: &str, scenario: &SurfaceSortieClientScenario| {
        if let (Some(directory), Some(window)) = (&output, &window) {
            for (profile, viewport) in [
                ("picade", Viewport::new(1024.0, 768.0)),
                ("hyperpixel", Viewport::new(800.0, 480.0)),
            ] {
                window
                    .window()
                    .set_size(slint::LogicalSize::new(viewport.width, viewport.height));
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                crate::host::present_frames(
                    window,
                    frames,
                    scenario.frame_layout(),
                    RenderBackend::Raster,
                    2.0,
                    &mut RasterRenderer::new(),
                );
                let pixels = window.window().take_snapshot().unwrap();
                let file =
                    std::fs::File::create(directory.join(format!("{profile}-{name}.png"))).unwrap();
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
    };
    let mut scenario =
        SurfaceSortieClientScenario::new(SurfaceSortieScenario::init_material(42, 2));
    let mut landing_tick = None;
    for tick in 0..180 {
        step(&mut scenario, SurfaceSortieAction::default());
        let intent = SurfaceSortieScenario::camera_target(&scenario.state, 0, false);
        if scenario.state.observation(0).landing.phase == LandingPhase::Landed
            && landing_tick.is_none()
        {
            assert!(
                scenario.cameras[0].height > intent.camera.height + 1.0,
                "must ease, not snap at touchdown"
            );
            landing_tick = Some(tick);
        }
        if let Some(start) = landing_tick
            && [0, 12, 36, 60].contains(&(tick - start))
        {
            capture(&format!("landing-{:03}", tick - start), &scenario);
        }
    }
    assert!(landing_tick.is_some(), "physical touchdown required");
    let aboard = scenario.state.location(0);
    for (name, expected) in [("exit", PilotLocation::OnFoot), ("board", aboard)] {
        let before = scenario.cameras[0].displayed(1.0);
        step(
            &mut scenario,
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            },
        );
        assert_eq!(scenario.state.location(0), expected);
        let first = scenario.cameras[0].displayed(1.0);
        assert!((first.center.x - before.center.x).hypot(first.center.y - before.center.y) < 2.0);
        capture(&format!("{name}-000"), &scenario);
        for tick in 1..=90 {
            step(&mut scenario, SurfaceSortieAction::default());
            if [12, 60].contains(&tick) {
                capture(&format!("{name}-{tick:03}"), &scenario);
            }
        }
    }
    scenario.step(
        &[SurfaceWingAction { closed: true }.encode(PlayerId::PLAYER_1)],
        DT,
    );
    assert_ne!(
        scenario.state.observation(0).landing.phase,
        LandingPhase::Landed
    );
    let desired = SurfaceSortieScenario::camera_target(&scenario.state, 0, false);
    assert!(
        scenario.cameras[0].height < desired.camera.height - 1.0,
        "takeoff must ease out"
    );
    capture("takeoff-000", &scenario);
    for tick in 1..=60 {
        step(&mut scenario, SurfaceSortieAction::default());
        if [12, 60].contains(&tick) {
            capture(&format!("takeoff-{tick:03}"), &scenario);
        }
    }
}
