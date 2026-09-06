use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_spaceling_lab::{
    SpacelingLabAction, SpacelingLabConfig, SpacelingLabScenario, SpacelingLabState,
};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::{ClientInput, GameKey};
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spaceling-lab",
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
    controls_help: "Spaceling Lab: d-pad or left stick left/right walks; A jumps; B applies a test shove. Release A/B before repeating. Keyboard: A/D or arrows walk, Space jumps, X shoves. Start or Esc pauses; R restarts. Strong impacts disable control until physical support permits recovery. Cyan shows support; orange shows gravity.",
    create,
};

struct SpacelingLabClientScenario {
    state: SpacelingLabState,
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SpacelingLabClientScenario {
        state: SpacelingLabScenario::init(SpacelingLabConfig::default(), seed),
    }))
}

impl ClientScenario for SpacelingLabClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        SpacelingLabScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        SpacelingLabScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        let (walk, jump, shove) = spaceling_controls(input);
        vec![SpacelingLabAction::with_shove(walk, jump, shove)]
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![SpacelingLabScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
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

fn spaceling_controls(input: &ClientInput) -> (f32, bool, bool) {
    let (gamepad_walk, gamepad_jump, gamepad_shove) = input.spaceling_gamepad_input();
    let left = input.is_pressed(GameKey::P1TurnLeft) || input.is_pressed(GameKey::NesLeft);
    let right = input.is_pressed(GameKey::P1TurnRight) || input.is_pressed(GameKey::NesRight);
    let walk = match (left, right) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        (true, true) => 0.0,
        (false, false) => gamepad_walk,
    };
    (
        walk,
        input.is_pressed(GameKey::P1Laser) || gamepad_jump,
        input.is_pressed(GameKey::P1Reverse) || gamepad_shove,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GamepadInput, GamepadSeatInput};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn keyboard_walk_jump_and_opposites() {
        for (left, right) in [
            (GameKey::P1TurnLeft, GameKey::P1TurnRight),
            (GameKey::NesLeft, GameKey::NesRight),
        ] {
            let mut input = ClientInput::default();
            input.press(left);
            assert_eq!(spaceling_controls(&input), (-1.0, false, false));
            input.press(GameKey::P1Laser);
            assert_eq!(spaceling_controls(&input), (-1.0, true, false));
            input.press(GameKey::P1Reverse);
            assert_eq!(spaceling_controls(&input), (-1.0, true, true));
            input.release(GameKey::P1Reverse);
            input.press(right);
            assert_eq!(spaceling_controls(&input), (0.0, true, false));
            input.release(left);
            input.release(GameKey::P1Laser);
            assert_eq!(spaceling_controls(&input), (1.0, false, false));
        }
    }

    #[test]
    fn gamepad_axes_buttons_release_and_disconnect() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let input = ClientInput::new(Rc::clone(&pads));
        for direction in [-1.0, 1.0] {
            pads.borrow_mut().set_seat(
                0,
                GamepadSeatInput {
                    connected: true,
                    dpad_left: direction < 0.0,
                    dpad_right: direction > 0.0,
                    south: true,
                    east: true,
                    ..GamepadSeatInput::default()
                },
            );
            assert_eq!(spaceling_controls(&input), (direction, true, true));
            pads.borrow_mut().set_seat(
                0,
                GamepadSeatInput {
                    connected: true,
                    left_stick_x: direction,
                    ..GamepadSeatInput::default()
                },
            );
            assert_eq!(spaceling_controls(&input), (direction, false, false));
        }
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                ..GamepadSeatInput::default()
            },
        );
        assert_eq!(spaceling_controls(&input), (0.0, false, false));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: false,
                dpad_left: true,
                south: true,
                east: true,
                ..GamepadSeatInput::default()
            },
        );
        assert_eq!(spaceling_controls(&input), (0.0, false, false));
    }

    #[test]
    fn registry_creates_reproducible_lab_and_both_render_paths() {
        let viewport = Viewport::new(1280.0, 720.0);
        let mut lab = REGISTRATION
            .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
            .unwrap();
        let initial = lab.render_frames(RenderBackend::Vector, viewport);
        assert_eq!(initial, lab.render_frames(RenderBackend::Raster, viewport));
        for _ in 0..60 {
            lab.step(
                &[SpacelingLabAction::control(1.0, false)],
                Duration::from_secs_f64(1.0 / 60.0),
            );
        }
        assert_ne!(initial, lab.render_frames(RenderBackend::Vector, viewport));
        let fresh = REGISTRATION
            .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
            .unwrap();
        assert_eq!(
            initial,
            fresh.render_frames(RenderBackend::Vector, viewport)
        );
        assert!(
            REGISTRATION
                .create(
                    0,
                    &Settings::default(),
                    viewport,
                    ScenarioStartMode::Benchmark(super::super::BenchmarkConfiguration::default())
                )
                .is_err()
        );
    }

    #[test]
    fn knockback_frames_reach_both_renderers() {
        use crate::raster::{RasterOptions, RasterRenderer};

        let viewport = Viewport::new(1280.0, 720.0);
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        let mut lab = SpacelingLabClientScenario {
            state: SpacelingLabScenario::init(SpacelingLabConfig::default(), 0),
        };
        for _ in 0..120 {
            lab.step(&[SpacelingLabAction::control(0.0, false)], dt);
        }
        let mut captured = [false; 3];
        let mut raster = RasterRenderer::new();
        for tick in 0..1200 {
            lab.step(&[SpacelingLabAction::with_shove(0.0, false, tick == 0)], dt);
            let snapshot = lab.state.spaceling_snapshot();
            let index = if snapshot.recoveries > 0 {
                2
            } else if snapshot.recovery_progress > 0.35 {
                1
            } else if snapshot.knockdowns > 0 && snapshot.motion.angle.abs() > 0.9 {
                0
            } else {
                continue;
            };
            if captured[index] {
                continue;
            }
            let frames = lab.render_frames(RenderBackend::Vector, viewport);
            assert_eq!(frames, lab.render_frames(RenderBackend::Raster, viewport));
            let scene = crate::render::scene_primitives_from_frames(&frames, viewport);
            assert!(scene.len() > 20);
            let overlay = crate::render::raster_text_overlay(&frames, viewport, lab.frame_layout());
            let label = ["KNOCKED DOWN", "RECOVERING", "BALANCED"][index];
            assert!(
                overlay
                    .iter()
                    .any(|primitive| primitive.text.starts_with(label))
            );
            let image = raster.image_from_frames_with_layout(
                &frames,
                viewport,
                lab.frame_layout(),
                RasterOptions::default(),
            );
            let pixels = image.to_rgb8().unwrap();
            let suit_pixels = pixels
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(i, pixel)| {
                    let x = i % 1280;
                    let y = i / 1280;
                    (550..730).contains(&x)
                        && (260..460).contains(&y)
                        && pixel.r > 200
                        && pixel.b < 120
                })
                .count();
            assert!(suit_pixels > 60, "missing {label} pose: {suit_pixels}");
            if let Some(directory) = std::env::var_os("SPACEWARS_SPACELING_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                let name = ["knocked-down", "recovering", "balanced"][index];
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
            captured[index] = true;
            if captured.iter().all(|value| *value) {
                break;
            }
        }
        assert_eq!(captured, [true; 3]);
    }
}
