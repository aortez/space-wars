use super::{CliError, control_error, human_error, parse_timeout};
use clap::Subcommand;
use spacewars_control::{ClockState, ClockStatePredicate, ClockTriggerRequest, ControlClient};
use std::time::{Duration, Instant};

#[derive(Debug, Subcommand)]
pub enum ClockCommand {
    /// Inspect the active Clock's phase, time, schedule, and physics counts.
    State {
        #[arg(long)]
        json: bool,
    },
    /// Trigger a fall from idle (also works with the Off profile), then wait for it to start.
    Trigger {
        /// Reject a stale Clock instance; defaults to the current instance.
        #[arg(long)]
        expect_scenario_revision: Option<u64>,
        /// Reject a stale event; defaults to the current event ID.
        #[arg(long)]
        expect_event_id: Option<u64>,
        #[arg(long, default_value = "3s", value_parser = parse_timeout)]
        timeout: Duration,
        #[arg(long)]
        json: bool,
    },
    /// Wait for a phase in this Clock instance. Useful for synchronized screenshots.
    Wait {
        #[arg(long, value_parser = ["idle", "falling", "reforming", "cooldown"])]
        phase: String,
        #[arg(long)]
        event_id: Option<u64>,
        #[arg(long, default_value_t = 0)]
        min_phase_tick: u64,
        /// Fail if the instance changes; defaults to the current instance.
        #[arg(long)]
        expect_scenario_revision: Option<u64>,
        #[arg(long, default_value = "10s", value_parser = parse_timeout)]
        timeout: Duration,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(client: &ControlClient, command: ClockCommand) -> Result<(), CliError> {
    match command {
        ClockCommand::State { json } => {
            let state = client
                .clock_state_before(Instant::now() + Duration::from_secs(3))
                .map_err(|e| control_error(e, json))?;
            print_state(&state, json)
        }
        ClockCommand::Trigger {
            expect_scenario_revision,
            expect_event_id,
            timeout,
            json,
        } => {
            let deadline = deadline(timeout)?;
            let state = client
                .clock_state_before(deadline)
                .map_err(|e| control_error(e, json))?;
            let request = ClockTriggerRequest {
                expected_scenario_revision: expect_scenario_revision
                    .unwrap_or(state.scenario_revision),
                expected_event_id: expect_event_id.unwrap_or(state.event_id),
                ..ClockTriggerRequest::new(&state)
            };
            client
                .clock_trigger_before(&request, deadline)
                .map_err(|e| control_error(e, json))?;
            let state = client
                .wait_for_clock_state(
                    &ClockStatePredicate {
                        scenario_revision: request.expected_scenario_revision,
                        phase: "falling".into(),
                        event_id: Some(request.expected_event_id.saturating_add(1)),
                        min_phase_tick: 0,
                    },
                    deadline.saturating_duration_since(Instant::now()),
                )
                .map_err(|e| control_error(e, json))?;
            print_state(&state, json)
        }
        ClockCommand::Wait {
            phase,
            event_id,
            min_phase_tick,
            expect_scenario_revision,
            timeout,
            json,
        } => {
            let deadline = deadline(timeout)?;
            let revision = match expect_scenario_revision {
                Some(revision) => revision,
                None => {
                    client
                        .clock_state_before(deadline)
                        .map_err(|e| control_error(e, json))?
                        .scenario_revision
                }
            };
            let state = client
                .wait_for_clock_state(
                    &ClockStatePredicate {
                        scenario_revision: revision,
                        phase,
                        event_id,
                        min_phase_tick,
                    },
                    deadline.saturating_duration_since(Instant::now()),
                )
                .map_err(|e| control_error(e, json))?;
            print_state(&state, json)
        }
    }
}

fn deadline(timeout: Duration) -> Result<Instant, CliError> {
    Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| human_error("Clock timeout is too large"))
}

fn print_state(state: &ClockState, json: bool) -> Result<(), CliError> {
    if json {
        println!("{}", state.to_json().map_err(human_error)?);
    } else {
        println!(
            "Clock: {} / {} (instance {}, event {}, phase tick {}, paused={})\nTime: {:?}; target digits: {:?}\nPhysics: {} bodies, {} colliders; simulation tick {}, next event {:?}; can trigger={}",
            state.profile,
            state.phase,
            state.scenario_revision,
            state.event_id,
            state.phase_tick,
            state.paused,
            state.reading,
            state.display_digits,
            state.body_count,
            state.collider_count,
            state.simulation_tick,
            state.next_event_tick,
            state.can_trigger
        );
    }
    Ok(())
}
