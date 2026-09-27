use super::*;

#[test]
fn run_jump_and_dive_belong_to_the_visiting_pad_and_all_require_neutral() {
    let (input, gamepads) = crate::input::new_shared_input();
    let neutral = GamepadSeatInput {
        connected: true,
        ..Default::default()
    };
    for player in 1..=2 {
        for button in 0..3 {
            let held = GamepadSeatInput {
                south: button == 0,
                east: button == 1,
                dpad_down: button == 2,
                ..neutral.clone()
            };
            let seat = usize::from(player - 1);
            gamepads
                .borrow_mut()
                .replace_seats([neutral.clone(), neutral.clone()]);
            gamepads.borrow_mut().set_seat(seat, held.clone());
            let session = Some((10 + button, player));
            let command = input.borrow_mut().clock_duck_input(session).unwrap();
            assert!(!command.run && !command.jump && !command.dive);
            gamepads.borrow_mut().set_seat(seat, neutral.clone());
            input.borrow_mut().clock_duck_input(session);
            gamepads.borrow_mut().set_seat(1 - seat, held.clone());
            let command = input.borrow_mut().clock_duck_input(session).unwrap();
            assert!(
                !command.run && !command.jump && !command.dive,
                "other player ignored"
            );
            gamepads.borrow_mut().set_seat(seat, held.clone());
            let command = input.borrow_mut().clock_duck_input(session).unwrap();
            assert_eq!(
                (command.run, command.jump, command.dive),
                (button == 0, button == 1, button == 2)
            );
            // Pause/focus clear cannot leak a still-held control on resume.
            input.borrow_mut().clear();
            let command = input.borrow_mut().clock_duck_input(session).unwrap();
            assert!(!command.run && !command.jump && !command.dive);
            gamepads.borrow_mut().disconnect_seat(seat);
            let command = input.borrow_mut().clock_duck_input(session).unwrap();
            assert!(!command.run && !command.jump && !command.dive);
            input.borrow_mut().clock_duck_input(None);
        }
    }
}

#[test]
fn run_and_jump_can_be_held_together_and_down_accepts_stick_or_dpad() {
    let (input, gamepads) = crate::input::new_shared_input();
    let session = Some((1, 1));
    input.borrow_mut().clock_duck_input(session);
    for (up, down, y, dive) in [
        (false, false, -1.0, true),
        (false, false, 1.0, false),
        (false, true, 0.0, true),
        (true, false, -1.0, false), // retro axis Up must not be misread as Dive
        (true, true, 0.0, false),
    ] {
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                south: true,
                east: true,
                left_stick_y: y,
                dpad_up: up,
                dpad_down: down,
                ..Default::default()
            },
        );
        let command = input.borrow_mut().clock_duck_input(session).unwrap();
        assert!(command.jump && command.run);
        assert_eq!(command.dive, dive);
    }
}

#[test]
fn keyboard_run_and_dive_are_p1_only_and_both_shifts_release_independently() {
    let mut input = ClientInput::default();
    input.clock_duck_input(Some((1, 1)));
    for key in [
        GameKey::ClockRunLeft,
        GameKey::ClockRunRight,
        GameKey::NesDown,
        GameKey::NesA,
    ] {
        input.press(key);
    }
    let command = input.clock_duck_input(Some((1, 1))).unwrap();
    assert!(command.run && command.dive && command.jump);
    input.release(GameKey::ClockRunLeft);
    assert!(input.clock_duck_input(Some((1, 1))).unwrap().run);
    input.release(GameKey::ClockRunRight);
    assert!(!input.clock_duck_input(Some((1, 1))).unwrap().run);
    let command = input.clock_duck_input(Some((2, 2))).unwrap();
    assert!(!command.run && !command.dive && !command.jump);
}
