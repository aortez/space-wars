//! Presentation-only, tick-driven camera sweep shared by every renderer.
//! The scenario's camera history, physics and saved settings are not modified.
use std::cell::RefCell;

use engine_common::Camera2;
use slint::ComponentHandle;

const PERIOD_TICKS: u64 = 480;
const STOP_TICK: u64 = 4 * PERIOD_TICKS;
const MIN_HEIGHT: f32 = 44.0;
const MAX_HEIGHT: f32 = 640.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ZoomMode {
    #[default]
    Off,
    Sweep,
    SweepStop,
}

impl ZoomMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Sweep => "sweep",
            Self::SweepStop => "sweep-stop",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "off" => Ok(Self::Off),
            "sweep" => Ok(Self::Sweep),
            "sweep-stop" => Ok(Self::SweepStop),
            _ => Err("camera-zoom requires off, sweep, or sweep-stop"),
        }
    }

    pub fn camera(self, mut camera: Camera2, player: usize, tick: u64) -> Camera2 {
        if self != Self::Off && player < 2 {
            // Opposite-phase log sweeps exercise both near and far geometry.
            // Absolute simulation ticks make restart repeatable and pause static;
            // render cadence and policy changes cannot advance the sweep.
            let tick = if self == Self::SweepStop {
                tick.min(STOP_TICK)
            } else {
                tick
            };
            let phase = (tick % PERIOD_TICKS + player as u64 * PERIOD_TICKS / 2) % PERIOD_TICKS;
            let progress = phase.min(PERIOD_TICKS - phase) as f32 / (PERIOD_TICKS / 2) as f32;
            camera.height = MIN_HEIGHT * (MAX_HEIGHT / MIN_HEIGHT).powf(progress);
        }
        camera
    }
}

#[derive(Default)]
struct State {
    mode: ZoomMode,
    source_tick: Option<u64>,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn select(mode: ZoomMode) {
    STATE.with_borrow_mut(|s| {
        if s.mode != mode {
            s.mode = mode;
            s.source_tick = None;
        }
    });
}

pub(crate) fn request(window: &crate::MainWindow, mode: ZoomMode) {
    select(mode);
    window.window().request_redraw();
}

/// Called before source culling, minimap footprints and text projection.
pub(crate) fn camera(camera: Camera2, player: usize, tick: u64) -> Camera2 {
    STATE.with_borrow_mut(|s| {
        if s.mode != ZoomMode::Off && player < 2 {
            s.source_tick = Some(tick);
        }
        s.mode.camera(camera, player, tick)
    })
}

pub(crate) fn end_session() {
    STATE.with_borrow_mut(|s| *s = State::default());
}

pub(crate) fn diagnostics() -> String {
    STATE.with_borrow(|s| {
        let mode = s.mode.name();
        let tick = s.source_tick.map_or_else(|| "unavailable".into(), |tick| tick.to_string());
        // Keep the old GPU-prefixed keys for existing experiment scripts.
        format!("\ncamera_zoom_mode={mode}\ncamera_zoom_source_tick={tick}\ngpu_scene_zoom_mode={mode}\ngpu_scene_zoom_source_tick={tick}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::RenderPoint;

    #[test]
    fn requests_are_idempotent_and_outlive_gpu_resources_but_not_the_session() {
        std::thread::spawn(|| {
            let original = Camera2::new(RenderPoint::ZERO, 100.0);
            assert_eq!(camera(original, 0, 10), original);
            select(ZoomMode::Sweep);
            let expected = camera(original, 0, 120);
            select(ZoomMode::Sweep);
            assert_eq!(STATE.with_borrow(|s| s.source_tick), Some(120));
            // Vector presentation, GPU fallback and resource retirement must
            // not turn off a renderer-independent camera experiment.
            crate::gpu_scene::end_session();
            assert_eq!(camera(original, 0, 120), expected);
            assert!(diagnostics().contains("camera_zoom_mode=sweep"));
            assert!(diagnostics().contains("gpu_scene_zoom_source_tick=120"));
            select(ZoomMode::Off);
            assert_eq!(camera(original, 0, 120), original);
            assert_eq!(STATE.with_borrow(|s| s.source_tick), None);
            select(ZoomMode::Sweep);
            camera(original, 2, 123);
            assert_eq!(STATE.with_borrow(|s| s.source_tick), None);
            camera(original, 0, 123);
            end_session();
            assert_eq!(camera(original, 0, 123), original);
            assert!(diagnostics().contains("camera_zoom_mode=off"));
            assert!(diagnostics().contains("camera_zoom_source_tick=unavailable"));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn sweep_keeps_all_renderers_and_minimaps_consistent_without_changing_simulation() {
        std::thread::spawn(|| {
            use crate::client_scenarios::{self, RenderBackend, ScenarioAsset, ScenarioStartMode};
            use crate::render::Viewport;
            use engine_common::{RenderPrimitive, Settings};
            use std::time::Duration;
            let viewport = Viewport::new(320.0, 240.0);
            let create = || {
                client_scenarios::registration("spacewars")
                    .unwrap()
                    .create_with_asset(
                        0,
                        &Settings::default(),
                        viewport,
                        ScenarioStartMode::Normal,
                        &ScenarioAsset::None,
                    )
                    .unwrap()
            };
            let (mut shown, mut hidden) = (create(), create());
            let mut culled_raster = crate::raster::RasterRenderer::new();
            let mut full_raster = crate::raster::RasterRenderer::new();
            select(ZoomMode::Sweep);
            for tick in 1..=480 {
                let dt = Duration::from_nanos(16_666_667);
                shown.step(&[], dt);
                hidden.step(&[], dt);
                if ![1, 120, 240, 360, 480].contains(&tick) {
                    continue;
                }
                let frames = shown.render_frames(RenderBackend::Raster, viewport);
                // Pause and repeated rendering cannot advance the sweep.
                assert_eq!(frames, shown.render_frames(RenderBackend::Raster, viewport));
                let vector = shown.render_frames(RenderBackend::Vector, viewport);
                let reference = shown.render_frames_reference(viewport).unwrap();
                for ((raster, vector), reference) in frames.iter().zip(&vector).zip(&reference) {
                    assert_eq!(raster.camera, vector.camera);
                    assert_eq!(raster.camera, reference.camera);
                }
                let options = crate::raster::RasterOptions::for_scale(1.0);
                let culled = culled_raster.image_from_frames_with_layout(
                    &frames,
                    viewport,
                    shown.frame_layout(),
                    options,
                );
                let full = full_raster.image_from_frames_with_layout(
                    &reference,
                    viewport,
                    shown.frame_layout(),
                    options,
                );
                assert_eq!(
                    culled.to_rgb8().unwrap().as_slice(),
                    full.to_rgb8().unwrap().as_slice()
                );
                let projections =
                    crate::render::frame_projections(&frames, viewport, shown.frame_layout());
                for player in 0..2 {
                    assert_eq!(
                        frames[player].camera,
                        ZoomMode::Sweep.camera(frames[player].camera, player, tick)
                    );
                    assert_eq!(projections[player].camera, frames[player].camera);
                    let bounds = frames[player]
                        .camera
                        .world_bounds(projections[player].viewport.aspect_ratio());
                    let map = frames[player + 2]
                        .ordered_layers()
                        .into_iter()
                        .find(|layer| layer.z == 0)
                        .unwrap();
                    let RenderPrimitive::Polygon(footprint) = &map.primitives[0] else {
                        panic!("missing camera footprint");
                    };
                    assert_eq!(footprint.points[0], bounds.min);
                    assert_eq!(footprint.points[2], bounds.max);
                }
            }
            end_session();
            // Identical normal frames prove rendering changed neither physics
            // nor camera smoothing; no GPU feature or GL context is required.
            assert_eq!(
                shown.render_frames(RenderBackend::Raster, viewport),
                hidden.render_frames(RenderBackend::Raster, viewport)
            );
        })
        .join()
        .unwrap();
    }

    #[test]
    fn sweep_stop_uses_the_same_path_then_holds_only_camera_height() {
        let original = Camera2::new(RenderPoint::new(20., -30.), 123.);
        for player in 0..3 {
            for tick in (0..=STOP_TICK + PERIOD_TICKS).chain([u64::MAX]) {
                let held = ZoomMode::SweepStop.camera(original, player, tick);
                assert_eq!(
                    held,
                    ZoomMode::Sweep.camera(original, player, tick.min(STOP_TICK))
                );
                assert_eq!(held.center, original.center);
            }
        }
        let moved = Camera2::new(RenderPoint::new(35., 40.), 55.);
        assert_eq!(
            ZoomMode::SweepStop.camera(moved, 0, STOP_TICK + 1).center,
            moved.center
        );
    }

    #[test]
    fn sweep_is_explicit_bounded_periodic_and_preserves_camera_center() {
        assert_eq!(ZoomMode::default(), ZoomMode::Off);
        for mode in [ZoomMode::Off, ZoomMode::Sweep, ZoomMode::SweepStop] {
            assert_eq!(ZoomMode::parse(mode.name()), Ok(mode));
        }
        for value in ["", "on", "auto", "sweep\nstatus"] {
            assert!(ZoomMode::parse(value).is_err());
        }
        let original = Camera2::new(RenderPoint::new(-50.0, 120.0), 123.0);
        for tick in (0..PERIOD_TICKS * 3).chain([u64::MAX]) {
            assert_eq!(ZoomMode::Off.camera(original, 0, tick), original);
            assert_eq!(ZoomMode::Sweep.camera(original, 2, tick), original);
            for player in 0..2 {
                let camera = ZoomMode::Sweep.camera(original, player, tick);
                assert_eq!(camera.center, original.center);
                assert!((MIN_HEIGHT..=MAX_HEIGHT).contains(&camera.height));
                assert_eq!(
                    camera,
                    ZoomMode::Sweep.camera(original, player, tick % PERIOD_TICKS)
                );
            }
        }
        assert_eq!(ZoomMode::Sweep.camera(original, 0, 0).height, MIN_HEIGHT);
        assert_eq!(ZoomMode::Sweep.camera(original, 1, 0).height, MAX_HEIGHT);
        assert_eq!(ZoomMode::Sweep.camera(original, 0, 240).height, MAX_HEIGHT);
        assert_eq!(ZoomMode::Sweep.camera(original, 1, 240).height, MIN_HEIGHT);
    }
}
