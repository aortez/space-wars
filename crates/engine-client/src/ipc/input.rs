use super::*;
use crate::gamepad::SimulatedInput;
use crate::input::{SharedGamepadInput, SharedInput};
use spacewars_control::{INPUT_SCHEMA_VERSION, InputPressRequest, InputPressResult};
use std::time::Instant;

pub(super) struct InputAutomation {
    driver: SimulatedInput,
    response: Option<ResponseWriter>,
}

impl InputAutomation {
    pub(super) fn new(input: SharedInput, gamepads: SharedGamepadInput) -> Self {
        Self {
            driver: SimulatedInput::new(input, gamepads),
            response: None,
        }
    }

    pub(super) fn press(
        &mut self,
        window: &MainWindow,
        request: InputPressRequest,
        response: ResponseWriter,
        tracker: &mut UiStateTracker,
    ) {
        let state = match ui_state(window, tracker) {
            Ok(state) => state,
            Err(error) => {
                response.error(error.to_string());
                return;
            }
        };
        if let Err(failure) = validate_ui_preconditions(
            Some(request.expected_screen),
            Some(request.expected_revision),
            &state,
        ) {
            response.control_failure(*failure);
            return;
        }
        if state.scenario_revision != request.expected_scenario_revision {
            response.control_failure(ControlFailure::new(
                ControlFailureCode::StaleRevision,
                "scenario instance changed before the press",
                Some(state),
            ));
            return;
        }
        match self.driver.press(window, &state, request, Instant::now()) {
            Ok(()) => self.response = Some(response),
            Err(error) => response.control_failure(ControlFailure::new(
                ControlFailureCode::ActionUnavailable,
                error,
                Some(state),
            )),
        }
    }

    pub(super) fn tick(&mut self, window: &MainWindow, tracker: &mut UiStateTracker) {
        if !self.driver.active() {
            return;
        }
        // Build the UI inventory only while a bounded input request is active.
        let state = match ui_state(window, tracker) {
            Ok(state) => state,
            Err(error) => {
                self.driver.cancel();
                if let Some(response) = self.response.take() {
                    response.error(error.to_string());
                }
                return;
            }
        };
        if let Some((request, release_reason)) = self.driver.tick(window, &state, Instant::now()) {
            let result = InputPressResult {
                schema_version: INPUT_SCHEMA_VERSION,
                player: request.player,
                button: request.button,
                release_reason,
                state,
            };
            if let Some(response) = self.response.take() {
                match result.to_json() {
                    Ok(json) => response.ok(json),
                    Err(error) => response.error(error.to_string()),
                }
            }
        }
    }
}
