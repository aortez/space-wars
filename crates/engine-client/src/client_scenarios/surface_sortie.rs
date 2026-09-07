use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_spacewars::surface_sortie::{
    SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
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
    controls_help: "Surface Sortie (experimental): left/right or A/D turns aboard, walks on foot. A / Space thrusts aboard and jumps on foot. Down / S brakes. Point the nose away from the planet for landing assist; settle on both rear feet. B / X exits a landed ship or boards at its cyan hatch. Release controls after transferring. Start / Esc pauses; R restarts. North-up minimap: red ship, orange spaceling, cyan planet. No weapons or capture.",
    create,
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
        state: SurfaceSortieScenario::init((), seed),
    }))
}

impl ClientScenario for SurfaceSortieClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
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
            state: SurfaceSortieScenario::init((), 0),
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
    fn registered_fixture_renders_and_restarts_in_both_backends() {
        let viewport = Viewport::new(1280.0, 720.0);
        let mut scenario = REGISTRATION
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
        let fresh = REGISTRATION
            .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
            .unwrap();
        assert_eq!(
            initial,
            fresh.render_frames(RenderBackend::Vector, viewport)
        );
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
            let file = std::fs::File::create(directory.join(format!("{name}.png"))).unwrap();
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
