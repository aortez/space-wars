use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_spacewars::surface_sortie::{
    SurfaceMotionPreset, SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode, spaceling_lab::spaceling_controls,
};
use crate::{
    input::ClientInput,
    render::{FrameLayout, Viewport},
};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Surface Sortie: left/right or A/D turns aboard, walks on foot. A / Space thrusts aboard and jumps on foot. Down / S brakes. Face away from the planet for landing assist; settle on both rear feet. B / X exits or boards at the cyan hatch. Release controls after transferring. Stand beside the amber terminal for 3 seconds to capture; leaving, jumping, or knockdown resets progress. A friendly outpost repairs a nearby landed ship at 5%/s. The ship starts at 75% health. Minimap: red ship, orange spaceling, cyan planet, square outpost (owner color). Start / Esc pauses; R restarts. No weapons or rebuilding.",
    create,
};

/// Another selectable preset of the same scenario, input mapping and renderer.
/// Keeping a distinct registry ID makes the choice persist and remain reachable
/// through ordinary keyboard/gamepad launcher navigation without new buttons.
pub(super) const ORBIT_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie-orbit",
    create: create_orbit,
    ..REGISTRATION
};

struct SurfaceSortieClientScenario {
    state: SurfaceSortieState,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::default(), seed),
    }))
}

fn create_orbit(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, seed),
    }))
}

impl ClientScenario for SurfaceSortieClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        match self.state.motion_preset() {
            SurfaceMotionPreset::Orbit => &ORBIT_REGISTRATION,
            _ => &REGISTRATION,
        }
    }
    fn tick_model(&self) -> TickModel {
        SurfaceSortieScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        SurfaceSortieScenario::step(&mut self.state, actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark: bool) -> Vec<Action> {
        let (horizontal, primary_held, interact_held) = spaceling_controls(input);
        vec![
            SurfaceSortieAction {
                horizontal,
                primary_held,
                interact_held,
                brake_held: input.surface_sortie_brake_held(),
            }
            .encode(),
        ]
    }
    fn render_frames(&self, _renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        vec![
            SurfaceSortieScenario::render_frame(&self.state),
            SurfaceSortieScenario::minimap_frame(&self.state, viewport.aspect_ratio()),
        ]
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::SinglePlayerWithMinimap
    }
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GameKey, GamepadInput, GamepadSeatInput};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn keyboard_and_nes_pad_produce_the_same_context_neutral_intent() {
        let scenario = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 0),
        };
        let mut keyboard = ClientInput::default();
        for key in [
            GameKey::P1TurnLeft,
            GameKey::P1Laser,
            GameKey::P1Reverse,
            GameKey::P1Brake,
        ] {
            keyboard.press(key);
        }
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_left: true,
                dpad_down: true,
                south: true,
                east: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(Rc::clone(&pads));
        assert_eq!(
            scenario.map_input(&mut keyboard, false),
            scenario.map_input(&mut input, false)
        );
        pads.borrow_mut().set_seat(0, GamepadSeatInput::default());
        assert_eq!(
            scenario.map_input(&mut input, false),
            vec![SurfaceSortieAction::default().encode()]
        );
    }

    #[test]
    fn outpost_capture_progress_and_owner_flag_render_in_both_backends() {
        for preset in [SurfaceMotionPreset::Stationary, SurfaceMotionPreset::Orbit] {
            let mut scenario = SurfaceSortieClientScenario {
                state: SurfaceSortieScenario::init(preset, 0),
            };
            let dt = Duration::from_secs_f64(1.0 / 60.0);
            for frame in 0..120 {
                scenario.step(
                    &[SurfaceSortieAction {
                        interact_held: frame == 60,
                        ..SurfaceSortieAction::default()
                    }
                    .encode()],
                    dt,
                );
            }
            for _ in 0..600 {
                let observation = scenario.state.observation();
                if observation
                    .position
                    .distance_to(observation.outpost.position)
                    < 2.35
                {
                    break;
                }
                scenario.step(
                    &[SurfaceSortieAction {
                        horizontal: 1.0,
                        ..SurfaceSortieAction::default()
                    }
                    .encode()],
                    dt,
                );
            }
            for _ in 0..60 {
                scenario.step(&[SurfaceSortieAction::default().encode()], dt);
            }
            let partial = scenario.state.observation().outpost;
            assert!(partial.capture_progress > 0.0 && partial.capture_progress < 1.0);
            let viewport = Viewport::new(1280.0, 720.0);
            check_render(&scenario, viewport, "on-foot-capturing");
            for _ in 0..180 {
                scenario.step(&[SurfaceSortieAction::default().encode()], dt);
            }
            let observation = scenario.state.observation();
            assert_eq!(observation.outpost.owner, Some(observation.owner));
            assert!(observation.outpost.repaired_health > 0.0);
            for viewport in [viewport, Viewport::new(1280.0, 1400.0)] {
                let frames = scenario.render_frames(RenderBackend::Vector, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Raster, viewport)
                );
                let overlay =
                    crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
                assert!(
                    overlay
                        .iter()
                        .any(|primitive| primitive.text.starts_with("OUTPOST P1"))
                );
                assert!(
                    overlay
                        .iter()
                        .any(|primitive| primitive.text == "P1 OUTPOST")
                );
                let name = if viewport.height > viewport.width {
                    "on-foot-secured-portrait"
                } else {
                    "on-foot-secured"
                };
                check_render(&scenario, viewport, name);
                // Check the actual owner-colored flag, not just a text/model entry.
                let up = observation.outpost.surface_normal;
                let flag = observation.outpost.position
                    + up * 3.15
                    + engine_core::Vec2::new(up.y, -up.x) * 1.3;
                let projected = frames[0].camera.world_to_viewport(
                    engine_common::RenderPoint::new(flag.x, flag.y),
                    viewport.aspect_ratio(),
                );
                let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                let x = (projected.x * pixels.width() as f32) as usize;
                let y = (projected.y * pixels.height() as f32) as usize;
                let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
                assert!(
                    pixel.r > 200 && pixel.g < 80 && pixel.b < 80,
                    "missing owner flag: {pixel:?}"
                );
            }
        }
    }

    #[test]
    fn registered_fixture_renders_and_restarts_in_both_backends() {
        for registration in [&REGISTRATION, &ORBIT_REGISTRATION] {
            let viewport = Viewport::new(1280.0, 720.0);
            let mut scenario = registration
                .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
                .unwrap();
            let initial = scenario.render_frames(RenderBackend::Vector, viewport);
            for tick in 0..90 {
                scenario.step(
                    &[SurfaceSortieAction {
                        interact_held: tick == 60,
                        ..SurfaceSortieAction::default()
                    }
                    .encode()],
                    Duration::from_secs_f64(1.0 / 60.0),
                );
                if tick == 59 {
                    check_render(&*scenario, viewport, "aboard");
                    check_render(&*scenario, Viewport::new(1280.0, 1400.0), "aboard-portrait");
                }
            }
            let frames = scenario.render_frames(RenderBackend::Vector, viewport);
            assert_eq!(
                frames,
                scenario.render_frames(RenderBackend::Raster, viewport)
            );
            assert_ne!(frames, initial);
            check_render(&*scenario, viewport, "on-foot");
            check_render(
                &*scenario,
                Viewport::new(1280.0, 1400.0),
                "on-foot-portrait",
            );
            assert!(matches!(
                scenario
                    .as_any()
                    .downcast_ref::<SurfaceSortieClientScenario>()
                    .unwrap()
                    .state
                    .location(),
                scenario_spacewars::surface_sortie::PilotLocation::OnFoot
            ));
            let fresh = registration
                .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
                .unwrap();
            assert_eq!(
                initial,
                fresh.render_frames(RenderBackend::Vector, viewport)
            );
        }
    }

    fn check_render(scenario: &dyn ClientScenario, viewport: Viewport, name: &str) {
        use crate::raster::{RasterOptions, RasterRenderer};
        let frames = scenario.render_frames(RenderBackend::Vector, viewport);
        let presentation = crate::render::scene_presentation_from_frames_with_layout(
            &frames,
            viewport,
            scenario.frame_layout(),
        );
        assert!(!presentation.main_primitives.is_empty());
        assert_eq!(presentation.minimaps.len(), 1);
        assert!(!presentation.minimaps[0].primitives.is_empty());
        let overlay =
            crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
        let label = if name.starts_with("on-foot") {
            "ON FOOT"
        } else {
            "ABOARD"
        };
        assert!(overlay.iter().any(|p| p.text.starts_with(label)));
        let image = RasterRenderer::new().image_from_frames_with_layout(
            &frames,
            viewport,
            scenario.frame_layout(),
            RasterOptions::default(),
        );
        let pixels = image.to_rgb8().unwrap();
        let ship_pixels = pixels
            .as_slice()
            .iter()
            .filter(|p| p.r > 200 && p.g < 80 && p.b < 80)
            .count();
        assert!(ship_pixels > 100, "missing ship in {name}: {ship_pixels}");
        if name.starts_with("on-foot") {
            let position = scenario
                .as_any()
                .downcast_ref::<SurfaceSortieClientScenario>()
                .unwrap()
                .state
                .observation()
                .position;
            let projected = frames[0].camera.world_to_viewport(
                engine_common::RenderPoint::new(position.x, position.y),
                viewport.aspect_ratio(),
            );
            let center_x = projected.x * pixels.width() as f32;
            let center_y = projected.y * pixels.height() as f32;
            let suit_pixels = pixels
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    let x = i % pixels.width() as usize;
                    let y = i / pixels.width() as usize;
                    (x as f32 - center_x).abs() < 60.0
                        && (y as f32 - center_y).abs() < 60.0
                        && p.r > 200
                        && p.g > 80
                        && p.g < 190
                        && p.b < 100
                })
                .count();
            assert!(suit_pixels > 20, "missing on-foot actor: {suit_pixels}");
        }
        if let Some(directory) = std::env::var_os("SPACEWARS_SORTIE_ARTIFACTS") {
            let directory = std::path::PathBuf::from(directory);
            let directory = if directory.is_absolute() {
                directory
            } else {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(directory)
            };
            std::fs::create_dir_all(&directory).unwrap();
            let file = std::fs::File::create(
                directory.join(format!("{name}-{}.png", scenario.registration().id)),
            )
            .unwrap();
            let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels.as_bytes())
                .unwrap();
        }
    }
}
