use super::{CliError, UiScreenArg, control_error, human_error};
use clap::{Subcommand, ValueEnum};
use spacewars_control::{ControlClient, InputButton, InputPressRequest, InputProfile};
use std::time::{Duration, Instant};

#[derive(Debug, Subcommand)]
pub enum InputCommand {
    /// Press a virtual controller button, wait for its automatic release.
    /// Uses application routing, not physical switches or the OS input backend.
    Press {
        #[arg(value_parser = parse_button)]
        button: InputButton,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
        player: u8,
        #[arg(long, value_enum, default_value_t = Profile::Standard)]
        profile: Profile,
        /// App-owned hold; released early on menu/scenario changes. No sticky down/up mode.
        #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(50..=2000))]
        hold_ms: u64,
        /// Defaults to the observed screen; stale UI revisions are always rejected.
        #[arg(long, value_enum)]
        expect_screen: Option<UiScreenArg>,
        #[arg(long)]
        expect_revision: Option<u64>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Profile {
    Standard,
    Picade,
}

fn parse_button(value: &str) -> Result<InputButton, String> {
    InputButton::ALL
        .into_iter()
        .find(|button| button.as_str() == value)
        .ok_or_else(|| {
            format!(
                "Choose {}",
                InputButton::ALL.map(InputButton::as_str).join(", ")
            )
        })
}

pub fn run(client: &ControlClient, command: InputCommand) -> Result<(), CliError> {
    let InputCommand::Press {
        button,
        player,
        profile,
        hold_ms,
        expect_screen,
        expect_revision,
        json,
    } = command;
    let deadline = Instant::now() + Duration::from_millis(hold_ms + 3000);
    let state = client
        .ui_state_before(deadline)
        .map_err(|error| control_error(error, json))?;
    let mut request = InputPressRequest::new(&state, button);
    request.player = player;
    request.profile = match profile {
        Profile::Standard => InputProfile::Standard,
        Profile::Picade => InputProfile::Picade,
    };
    request.hold_ms = hold_ms;
    request.expected_screen = expect_screen.map(Into::into).unwrap_or(state.screen);
    request.expected_revision = expect_revision.unwrap_or(state.revision);
    let result = client
        .input_press_before(&request, deadline)
        .map_err(|error| control_error(error, json))?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(human_error)?
        );
    } else {
        println!(
            "P{} {} released ({:?}); screen={}",
            result.player,
            result.button.as_str(),
            result.release_reason,
            result.state.screen
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn parses_bounded_named_input_and_rejects_unsafe_arguments() {
        assert!(
            crate::Args::try_parse_from([
                "cli",
                "input",
                "press",
                "right-shoulder",
                "--hold-ms",
                "1500",
                "--player",
                "2",
                "--profile",
                "picade"
            ])
            .is_ok()
        );
        for args in [
            vec!["power"],
            vec!["308"],
            vec!["south", "--hold-ms", "0"],
            vec!["south", "--hold-ms", "2001"],
            vec!["south", "--player", "3"],
        ] {
            assert!(
                crate::Args::try_parse_from(["cli", "input", "press"].into_iter().chain(args))
                    .is_err()
            );
        }
        for button in InputButton::ALL {
            assert_eq!(parse_button(button.as_str()), Ok(button));
        }
    }
}
