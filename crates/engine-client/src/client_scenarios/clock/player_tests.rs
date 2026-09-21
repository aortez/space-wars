use super::*;
use crate::input::{GameKey, GamepadSeatInput};
use engine_common::ClockEventProfile;

fn scenario(viewport: Viewport) -> ClockClientScenario {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(None),
        benchmark: None,
    }
}

fn step(scenario: &mut ClockClientScenario, input: &mut ClientInput) {
    let actions = scenario.map_input(input, false);
    scenario.step(&actions, Duration::from_nanos(16_666_667));
}

#[test]
fn player_input_needs_neutral_after_spawn_clear_and_new_session_and_respects_seats() {
    let mut scenario = scenario(Viewport::new(800.0, 480.0));
    let (input, gamepads) = crate::input::new_shared_input();
    let pad = |axis, jump| GamepadSeatInput {
        connected: true,
        left_stick_x: axis,
        east: jump,
        ..Default::default()
    };
    gamepads.borrow_mut().set_seat(1, pad(1.0, true));
    input.borrow_mut().request_clock_player_duck(2);
    step(&mut scenario, &mut input.borrow_mut());
    for _ in 0..60 {
        step(&mut scenario, &mut input.borrow_mut());
    }
    let current = scenario.state.player_duck_state().unwrap();
    assert_eq!(current.player, 2);
    assert_eq!(current.move_milli, 0);
    assert_eq!(current.duck.jumps, 0);
    gamepads.borrow_mut().set_seat(1, pad(0.0, false));
    step(&mut scenario, &mut input.borrow_mut());
    input.borrow_mut().press(GameKey::NesRight);
    gamepads.borrow_mut().set_seat(0, pad(1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(
        scenario.state.player_duck_state().unwrap().move_milli,
        0,
        "P1 does not steer P2"
    );
    gamepads.borrow_mut().set_seat(1, pad(-1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    let current = scenario.state.player_duck_state().unwrap();
    assert_eq!(current.move_milli, -1000);
    assert_eq!(current.duck.jumps, 1, "Picade East jumps");
    input.borrow_mut().clear();
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(scenario.state.player_duck_state().unwrap().move_milli, 0);
    assert!(!scenario.state.player_duck_state().unwrap().jump_held);
    gamepads.borrow_mut().disconnect_seat(1);
    step(&mut scenario, &mut input.borrow_mut());
    assert_eq!(scenario.state.player_duck_state().unwrap().move_milli, 0);
    scenario.state.set_aspect_ratio(0.6);
    input.borrow_mut().request_clock_player_duck(2);
    gamepads.borrow_mut().set_seat(1, pad(1.0, true));
    step(&mut scenario, &mut input.borrow_mut());
    step(&mut scenario, &mut input.borrow_mut());
    let new_visit = scenario.state.player_duck_state().unwrap();
    assert_ne!(new_visit.session_id, current.session_id);
    assert_eq!(new_visit.move_milli, 0);
    assert!(!new_visit.jump_held);
}

#[test]
fn player_duck_is_observable_and_renders_through_both_production_adapters() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, viewport) in [
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("picade", Viewport::new(1024.0, 768.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        let mut scenario = scenario(viewport);
        scenario.step(&[ClockAction::toggle_player_duck(1)], Duration::ZERO);
        for _ in 0..90 {
            scenario.step(&[], Duration::from_nanos(16_666_667));
        }
        let current = scenario.clock_state().unwrap();
        assert_eq!(current.event_kind, None);
        assert!(current.automatic_events_suspended);
        assert_eq!(current.player_duck.unwrap().player, 1);
        assert!(current.duck.is_none());
        assert!(!current.can_trigger);
        assert!(current.body_count <= 9 && current.collider_count <= 9);
        let frames = scenario.render_frames(RenderBackend::Raster, viewport);
        assert_eq!(
            frames,
            scenario.render_frames(RenderBackend::Vector, viewport)
        );
        let pixels = raster(&frames[0], viewport);
        let vector = svg(&frames[0], viewport);
        assert!(!vector.contains("NaN") && !vector.contains("inf"));
        // Text is composited by Slint over the raster image. The geometry-only
        // SVG exporter omits text, so inspect the production adapter directly.
        assert!(
            crate::render::scene_primitives_from_frames(&frames, viewport)
                .iter()
                .any(|item| item.text == "P1 DUCK")
        );
        assert_eq!(pixels.width(), viewport.width as u32);
        assert!(
            pixels
                .as_slice()
                .iter()
                .any(|p| p.r > 230 && p.g > 130 && p.b < 100),
            "visible duck/course"
        );
        if let Some(output) = &output {
            write_png(&output.join(format!("player-duck-{name}.png")), &pixels);
            std::fs::write(output.join(format!("player-duck-{name}.svg")), vector).unwrap();
        }
    }
}
