//! One app-owned virtual-controller lease, independent of hardware/OS devices.
use super::*;
use spacewars_control::{
    InputButton, InputPressRequest, InputProfile, InputReleaseReason, UiState,
};

pub(crate) struct SimulatedInput {
    input: SharedInput,
    gamepads: SharedGamepadInput,
    active: Option<Lease>,
    repeat: UiRepeat,
}

struct Lease {
    request: InputPressRequest,
    until: Instant,
    selected_scenario: String,
}

impl SimulatedInput {
    pub(crate) fn new(input: SharedInput, gamepads: SharedGamepadInput) -> Self {
        Self {
            input,
            gamepads,
            active: None,
            repeat: UiRepeat::default(),
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn press(
        &mut self,
        window: &MainWindow,
        state: &UiState,
        request: InputPressRequest,
        now: Instant,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        if self.active.is_some() {
            return Err("another simulated press is still active".into());
        }
        if window.get_launcher_busy() || state.benchmark_active {
            return Err("input is unavailable while launching or benchmarking".into());
        }
        let seat = usize::from(request.player - 1);
        if self
            .gamepads
            .borrow()
            .physical_seat(seat)
            .is_some_and(|pad| !is_neutral(pad))
        {
            return Err("release the player's physical controller before simulating input".into());
        }
        let until = now + Duration::from_millis(request.hold_ms);
        self.gamepads
            .borrow_mut()
            .simulate(seat, request.button, until);
        self.repeat.reset();
        let button = gilrs_button(request.button);
        let name = match request.profile {
            InputProfile::Standard => "CLI virtual controller",
            InputProfile::Picade => "Space-Wars Picade",
        };
        let route = button_route(
            window,
            button,
            name,
            request.button == InputButton::Start,
            request.button == InputButton::Select,
        );
        self.active = Some(Lease {
            request,
            until,
            selected_scenario: state.selected_scenario.clone(),
        });
        window.global::<UserActivity>().invoke_notify();
        apply_button_route(window, &self.input, route);
        self.repeat_direction(window, now);
        Ok(())
    }

    pub(crate) fn tick(
        &mut self,
        window: &MainWindow,
        state: &UiState,
        now: Instant,
    ) -> Option<(InputPressRequest, InputReleaseReason)> {
        let lease = self.active.as_ref()?;
        let reason = if state.screen != lease.request.expected_screen
            || state.scenario_revision != lease.request.expected_scenario_revision
            || state.selected_scenario != lease.selected_scenario
        {
            Some(InputReleaseReason::ContextChanged)
        } else if now >= lease.until {
            Some(InputReleaseReason::Elapsed)
        } else if !self.gamepads.borrow().has_simulated() {
            Some(InputReleaseReason::InputCleared)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.gamepads.borrow_mut().clear_simulated();
            self.repeat.reset();
            return Some((self.active.take().unwrap().request, reason));
        }
        window.global::<UserActivity>().invoke_notify();
        self.repeat_direction(window, now);
        None
    }

    pub(crate) fn cancel(&mut self) {
        self.gamepads.borrow_mut().clear_simulated();
        self.active = None;
        self.repeat.reset();
    }

    fn repeat_direction(&mut self, window: &MainWindow, now: Instant) {
        if !is_ui_mode(window) {
            return;
        }
        let direction = match self.active.as_ref().map(|lease| lease.request.button) {
            Some(InputButton::Up) => Some(UiDirection::Up),
            Some(InputButton::Down) => Some(UiDirection::Down),
            Some(InputButton::Left) => Some(UiDirection::Left),
            Some(InputButton::Right) => Some(UiDirection::Right),
            _ => None,
        };
        if let Some(action) = self.repeat.update(direction, now) {
            window.invoke_ui_action(action.code());
        }
    }
}

impl Drop for SimulatedInput {
    fn drop(&mut self) {
        self.gamepads.borrow_mut().clear_simulated();
    }
}

fn gilrs_button(button: InputButton) -> Button {
    match button {
        InputButton::Up => Button::DPadUp,
        InputButton::Down => Button::DPadDown,
        InputButton::Left => Button::DPadLeft,
        InputButton::Right => Button::DPadRight,
        InputButton::South => Button::South,
        InputButton::East => Button::East,
        InputButton::North => Button::North,
        InputButton::West => Button::West,
        InputButton::LeftShoulder => Button::LeftTrigger,
        InputButton::RightShoulder => Button::RightTrigger,
        InputButton::Start => Button::Start,
        InputButton::Select => Button::Select,
    }
}
