use super::*;
use crate::{
    MainWindow,
    input::{SharedGamepadInput, SharedInput},
};
use engine_common::{ClockDuckLedgePhase as Phase, ClockDuckLedgeReason as Reason};
use slint::{
    ComponentHandle,
    platform::{Platform, PlatformError, WindowAdapter, WindowEvent},
};
use std::rc::Rc;

struct TestPlatform;
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
    }
}

struct Controls {
    input: SharedInput,
    pads: SharedGamepadInput,
    window: MainWindow,
    // Keyboard P1, logical gamepad P1, logical Picade P2.
    source: u8,
    held: (bool, bool),
}

impl Controls {
    fn new(source: u8, viewport: Viewport) -> Self {
        let (input, pads) = crate::input::new_shared_input();
        let window = MainWindow::new().unwrap();
        window
            .window()
            .set_size(slint::LogicalSize::new(viewport.width, viewport.height));
        crate::install_keyboard_navigation(&window, Rc::clone(&input));
        window.set_launcher_scenario("clock".into());
        window.set_launcher_visible(false);
        window.show().unwrap();
        Self {
            input,
            pads,
            window,
            source,
            held: (false, false),
        }
    }

    fn player(&self) -> u8 {
        if self.source == 2 { 2 } else { 1 }
    }

    fn buttons(&mut self, jump: bool, down: bool) {
        if self.source == 0 {
            for (text, old, held) in [
                (slint::SharedString::from(" "), self.held.0, jump),
                (slint::platform::Key::DownArrow.into(), self.held.1, down),
            ] {
                if old != held {
                    self.window.window().dispatch_event(if held {
                        WindowEvent::KeyPressed { text }
                    } else {
                        WindowEvent::KeyReleased { text }
                    });
                }
            }
        } else {
            self.pads.borrow_mut().set_seat(
                usize::from(self.player() - 1),
                GamepadSeatInput {
                    connected: true,
                    name: if self.source == 2 {
                        "Space-Wars Picade"
                    } else {
                        "Test gamepad"
                    }
                    .into(),
                    east: jump,
                    dpad_down: down,
                    ..Default::default()
                },
            );
        }
        self.held = (jump, down);
    }

    fn tick(&self, scene: &mut ClockClientScenario) {
        step(scene, &mut self.input.borrow_mut());
    }

    fn catch(&mut self, scene: &mut ClockClientScenario) {
        self.input
            .borrow_mut()
            .request_clock_player_duck(self.player());
        for _ in 0..60 {
            self.tick(scene);
            if scene
                .state
                .player_duck_state()
                .is_some_and(|p| p.phase != "opening" && p.duck.grounded)
            {
                break;
            }
        }
        self.buttons(true, false);
        for _ in 0..65 {
            self.tick(scene);
            if scene.state.player_duck_state().unwrap().duck.ledge.phase == Phase::Hanging {
                return;
            }
        }
        panic!(
            "source {} failed to catch: {:?}",
            self.source,
            scene.state.player_duck_state()
        );
    }
}

#[test]
fn keyboard_and_owning_gamepad_ledge_sequence_uses_real_client_actions() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    for viewport in [
        Viewport::new(800.0, 480.0),
        Viewport::new(1024.0, 768.0),
        Viewport::new(480.0, 800.0),
    ] {
        for source in 0..3 {
            for drop in [false, true] {
                let mut scene = scenario(viewport);
                let mut controls = Controls::new(source, viewport);
                controls.catch(&mut scene);
                for _ in 0..90 {
                    controls.tick(&mut scene);
                    let p = scene.state.player_duck_state().unwrap();
                    assert_eq!(p.duck.ledge.phase, Phase::Hanging);
                    assert_eq!(p.duck.jumps, 1);
                }
                if source == 0 && !drop {
                    capture(&scene, viewport, "hang");
                }
                controls.buttons(false, false);
                // Another seat cannot climb or drop this duck.
                controls.pads.borrow_mut().set_seat(
                    usize::from(2 - controls.player()),
                    GamepadSeatInput {
                        connected: true,
                        east: true,
                        dpad_down: true,
                        ..Default::default()
                    },
                );
                for _ in 0..90 {
                    controls.tick(&mut scene);
                    assert_eq!(
                        scene.state.player_duck_state().unwrap().duck.ledge.phase,
                        Phase::Hanging
                    );
                }
                controls.buttons(true, drop);
                controls.tick(&mut scene);
                let p = scene.state.player_duck_state().unwrap();
                assert_eq!(
                    p.duck.ledge.phase,
                    if drop {
                        Phase::Dropping
                    } else {
                        Phase::Climbing
                    }
                );
                if drop {
                    assert_eq!(p.duck.ledge.reason, Some(Reason::Released));
                    controls.buttons(true, false);
                } else if source == 0 {
                    capture(&scene, viewport, "climb");
                }
                for _ in 0..150 {
                    controls.tick(&mut scene);
                }
                let p = scene.state.player_duck_state().unwrap();
                assert_eq!(p.duck.ledge.catches, 1);
                assert_eq!(p.duck.ledge.climbs, u32::from(!drop));
                assert_eq!(p.duck.ledge.releases, u32::from(drop));
                assert_eq!(p.duck.jumps, 1, "held climb/drop never buffers a jump");
                assert!(p.duck.grounded);
            }
        }
    }
}

#[test]
fn hanging_player_pause_disconnect_and_stale_inputs_preserve_ownership_and_neutral_gate() {
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let viewport = Viewport::new(800.0, 480.0);
    let mut scene = scenario(viewport);
    let mut controls = Controls::new(2, viewport);
    controls.catch(&mut scene);
    let before = scene.state.player_duck_state().unwrap();
    let frame = scene.render_frames(RenderBackend::Raster, viewport);
    for _ in 0..120 {
        scene.step(&[], Duration::ZERO);
    }
    assert_eq!(scene.state.player_duck_state().unwrap(), before);
    assert_eq!(scene.render_frames(RenderBackend::Raster, viewport), frame);
    controls.input.borrow_mut().clear();
    for _ in 0..60 {
        controls.tick(&mut scene);
        let p = scene.state.player_duck_state().unwrap();
        assert_eq!(p.duck.ledge.phase, Phase::Hanging);
        assert!(!p.jump_held, "held pad cannot climb after resume");
    }
    controls.pads.borrow_mut().disconnect_seat(1);
    controls.tick(&mut scene);
    for (session_id, player) in [(before.session_id + 1, 2), (before.session_id, 1)] {
        scene.step(
            &[ClockAction::player_duck_input(
                scenario_clock::ClockDuckInput {
                    session_id,
                    player,
                    move_milli: 1000,
                    jump: true,
                    run: true,
                    dive: true,
                },
            )],
            Duration::from_nanos(16_666_667),
        );
        assert_eq!(
            scene.state.player_duck_state().unwrap().duck.ledge.phase,
            Phase::Hanging
        );
    }
    controls.buttons(false, false);
    controls.tick(&mut scene);
    controls.buttons(true, false);
    controls.tick(&mut scene);
    assert_eq!(
        scene.state.player_duck_state().unwrap().duck.ledge.phase,
        Phase::Climbing
    );
}

fn capture(scene: &ClockClientScenario, viewport: Viewport, stage: &str) {
    let Some(output) = std::env::var_os("SPACEWARS_PLAYER_LEDGE_ARTIFACTS") else {
        return;
    };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    let frames = scene.render_frames(RenderBackend::Raster, viewport);
    assert_eq!(frames, scene.render_frames(RenderBackend::Vector, viewport));
    crate::thruster_visual_tests::write_png(
        &output.join(format!(
            "player-ledge-{}x{}-{stage}.png",
            viewport.width, viewport.height
        )),
        &crate::thruster_visual_tests::raster(&frames[0], viewport),
    );
}

#[test]
fn ledge_dismissal_keeps_shared_events_and_resize_releases_the_visit() {
    use engine_common::ClockEventKind;
    slint::platform::set_platform(Box::new(TestPlatform)).unwrap();
    let viewport = Viewport::new(800.0, 480.0);
    for event in [
        ClockEventKind::Rain,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        let mut scene = scenario(viewport);
        let mut controls = Controls::new(2, viewport);
        controls.catch(&mut scene);
        scene.step(&[ClockAction::preview_event(event)], Duration::ZERO);
        let event_id = scene.state.event_id();
        assert_eq!(
            scene.state.player_duck_state().unwrap().duck.ledge.phase,
            Phase::Hanging
        );
        controls.input.borrow_mut().request_clock_player_duck(2);
        controls.tick(&mut scene);
        let player = scene.state.player_duck_state().unwrap();
        assert_eq!(player.phase, "resetting");
        assert_eq!(player.duck.ledge, Default::default());
        for _ in 0..35 {
            controls.tick(&mut scene);
        }
        assert!(scene.state.player_duck_state().is_none());
        assert_eq!(scene.state.event_id(), event_id);
        assert_eq!(scene.state.event_kind(), Some(event));
        if event == ClockEventKind::Rain {
            // Rain retains course geometry/water without an unoccupied Rapier world.
            assert!(scene.clock_state().unwrap().rain.unwrap().duck_course);
        } else {
            assert!(scene.state.body_count() > 0);
        }
        scene.state.set_aspect_ratio(0.6);
        assert_eq!(scene.state.body_count(), 0);
        assert_eq!(scene.state.collider_count(), 0);
    }
    let mut scene = scenario(viewport);
    let mut controls = Controls::new(2, viewport);
    controls.catch(&mut scene);
    controls.buttons(false, false);
    controls.tick(&mut scene);
    controls.buttons(true, false);
    controls.tick(&mut scene);
    assert_eq!(
        scene.state.player_duck_state().unwrap().duck.ledge.phase,
        Phase::Climbing
    );
    scene.state.set_aspect_ratio(0.6);
    assert!(scene.state.player_duck_state().is_none());
    assert_eq!(scene.state.body_count(), 0);
    assert_eq!(scene.state.collider_count(), 0);
}
