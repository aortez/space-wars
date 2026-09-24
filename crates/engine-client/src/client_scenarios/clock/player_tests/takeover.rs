use super::*;
use engine_common::ClockEventKind;

#[test]
fn takeover_requires_neutral_and_only_the_requesting_seat_can_move_or_jump() {
    let mut scene = scenario(Viewport::new(800.0, 480.0));
    scene.step(
        &[ClockAction::preview_event(ClockEventKind::Duck)],
        Duration::ZERO,
    );
    for _ in 0..36 {
        scene.step(&[], Duration::from_nanos(16_666_667));
    }
    let (input, pads) = crate::input::new_shared_input();
    let pad = |axis, jump| GamepadSeatInput {
        connected: true,
        left_stick_x: axis,
        east: jump,
        ..Default::default()
    };
    pads.borrow_mut().set_seat(1, pad(1.0, true));
    input.borrow_mut().request_clock_player_duck(2);
    step(&mut scene, &mut input.borrow_mut());
    for _ in 0..60 {
        step(&mut scene, &mut input.borrow_mut());
    }
    let player = scene.state.player_duck_state().unwrap();
    assert_eq!(player.player, 2);
    assert_eq!(player.move_milli, 0);
    assert!(!player.jump_held);
    assert_eq!(
        player.duck.jumps, 0,
        "neither AI nor held input jumps after takeover"
    );
    assert!(player.duck.grounded);
    pads.borrow_mut().set_seat(1, pad(0.0, false));
    step(&mut scene, &mut input.borrow_mut());
    pads.borrow_mut().set_seat(0, pad(1.0, true));
    input.borrow_mut().press(GameKey::NesRight);
    input.borrow_mut().request_clock_player_duck(1);
    step(&mut scene, &mut input.borrow_mut());
    assert_eq!(
        scene.state.player_duck_session(),
        Some((player.session_id, 2))
    );
    assert_eq!(scene.state.player_duck_state().unwrap().move_milli, 0);
    pads.borrow_mut().set_seat(1, pad(1.0, true));
    step(&mut scene, &mut input.borrow_mut());
    let controlled = scene.state.player_duck_state().unwrap();
    assert_eq!(controlled.move_milli, 1000);
    assert_eq!(controlled.duck.jumps, 1);
    assert!(
        controlled.velocity_milli.unwrap()[0] > 0,
        "right means screen-right"
    );
    pads.borrow_mut().disconnect_seat(1);
    step(&mut scene, &mut input.borrow_mut());
    assert_eq!(scene.state.player_duck_state().unwrap().move_milli, 0);
    assert!(!scene.state.player_duck_state().unwrap().jump_held);
}

#[test]
fn automatic_duck_takeover_is_visually_continuous_through_both_render_adapters() {
    use crate::thruster_visual_tests::{raster, svg, write_png};
    let output = std::env::var_os("SPACEWARS_CLOCK_PLAYER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (device, viewport) in [
        ("picade", Viewport::new(1024.0, 768.0)),
        ("hyperpixel", Viewport::new(800.0, 480.0)),
        ("portrait", Viewport::new(480.0, 800.0)),
    ] {
        for elapsed in [70, 220] {
            let mut scene = scenario(viewport);
            scene.step(
                &[ClockAction::preview_event(ClockEventKind::Duck)],
                Duration::ZERO,
            );
            for _ in 0..elapsed {
                scene.step(&[], Duration::from_nanos(16_666_667));
            }
            let before = scene.render_frames(RenderBackend::Raster, viewport);
            let before_pixels = raster(&before[0], viewport);
            // Exercise client action mapping, but retain the fixture's fixed
            // reading rather than sampling the workstation clock mid-capture.
            let (input, _) = crate::input::new_shared_input();
            input.borrow_mut().request_clock_player_duck(1);
            let actions: Vec<_> = scene
                .map_input(&mut input.borrow_mut(), false)
                .into_iter()
                .filter(|a| {
                    matches!(
                        ClockAction::decode(a),
                        Some(ClockAction::TogglePlayerDuck(1))
                    )
                })
                .collect();
            assert_eq!(actions.len(), 1);
            scene.step(&actions, Duration::ZERO);
            let after = scene.render_frames(RenderBackend::Raster, viewport);
            assert_eq!(after, scene.render_frames(RenderBackend::Vector, viewport));
            assert!(
                before_pixels.as_slice() == raster(&after[0], viewport).as_slice(),
                "no duck/course/door jump on {device} at {elapsed}"
            );
            assert!(
                crate::render::scene_primitives_from_frames(&after, viewport)
                    .iter()
                    .any(|item| item.text == "P1 DUCK")
            );
            scene.step(&[], Duration::from_nanos(16_666_667));
            let advanced = scene.render_frames(RenderBackend::Raster, viewport);
            for (stage, frames) in [("before", before), ("owned", after), ("after-1", advanced)] {
                let vector = svg(&frames[0], viewport);
                assert!(!vector.contains("NaN") && !vector.contains("inf"));
                if let Some(output) = &output {
                    let file = format!("takeover-{device}-{elapsed}-{stage}");
                    write_png(
                        &output.join(format!("{file}.png")),
                        &raster(&frames[0], viewport),
                    );
                    std::fs::write(output.join(format!("{file}.svg")), vector).unwrap();
                }
            }
        }
    }
}
