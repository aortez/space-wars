use super::{CliError, control_error, human_error, parse_timeout};
use clap::Subcommand;
use spacewars_control::{
    ClockEventKind, ClockState, ClockStatePredicate, ClockTriggerRequest, ControlClient,
};
use std::time::{Duration, Instant};

#[derive(Debug, Subcommand)]
pub enum ClockCommand {
    /// Inspect the active Clock's event, time, schedule, and resource counts.
    State {
        #[arg(long)]
        json: bool,
    },
    /// List named events, their effects, automatic enablement, and cooldowns.
    Events {
        #[arg(long)]
        json: bool,
    },
    /// Preview an event from idle, including with Off or that event disabled.
    Trigger {
        /// Event ID: falling or color-cycle.
        #[arg(value_parser = parse_event)]
        event: ClockEventKind,
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
    /// Wait for matching event state in this Clock instance; synchronizes captures.
    #[command(group(clap::ArgGroup::new("condition").required(true).multiple(true)
        .args(["lifecycle", "phase", "event", "event_id"])))]
    Wait {
        #[arg(long, value_parser = ["idle", "active", "cooldown"])]
        lifecycle: Option<String>,
        #[arg(long, value_parser = ["falling", "reforming", "cycling"])]
        phase: Option<String>,
        #[arg(long, value_parser = parse_event)]
        event: Option<ClockEventKind>,
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

fn parse_event(value: &str) -> Result<ClockEventKind, String> {
    ClockEventKind::ALL
        .into_iter()
        .find(|kind| kind.as_str() == value)
        .ok_or_else(|| format!("Unknown Clock event {value:?}; choose falling or color-cycle"))
}

pub fn run(client: &ControlClient, command: ClockCommand) -> Result<(), CliError> {
    match command {
        ClockCommand::State { json } => {
            let state = client
                .clock_state_before(Instant::now() + Duration::from_secs(3))
                .map_err(|e| control_error(e, json))?;
            print_state(&state, json)
        }
        ClockCommand::Events { json } => {
            let state = client
                .clock_state_before(Instant::now() + Duration::from_secs(3))
                .map_err(|e| control_error(e, json))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&state.events).map_err(human_error)?
                );
            } else {
                for event in state.events {
                    println!(
                        "{} — {} ({}, {} ticks; automatic={}, reuse delay={} ticks, ready at={})",
                        event.kind.as_str(),
                        event.label,
                        event.effect,
                        event.duration_ticks,
                        event.enabled,
                        event.cooldown_ticks,
                        event.automatic_ready_at_tick
                    );
                }
            }
            Ok(())
        }
        ClockCommand::Trigger {
            event,
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
                ..ClockTriggerRequest::new(&state, event)
            };
            client
                .clock_trigger_before(&request, deadline)
                .map_err(|e| control_error(e, json))?;
            let state = client
                .wait_for_clock_state(
                    &ClockStatePredicate {
                        scenario_revision: request.expected_scenario_revision,
                        lifecycle: Some("active".into()),
                        event_kind: Some(event),
                        phase: None,
                        event_id: Some(request.expected_event_id.saturating_add(1)),
                        min_phase_tick: 0,
                    },
                    deadline.saturating_duration_since(Instant::now()),
                )
                .map_err(|e| control_error(e, json))?;
            print_state(&state, json)
        }
        ClockCommand::Wait {
            lifecycle,
            phase,
            event,
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
                        lifecycle,
                        event_kind: event,
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
            "Clock: {} / {} / {} / {} (instance {}, event {}, phase tick {}, paused={})\nTime: {:?}; target digits: {:?}\nPhysics: {} bodies, {} colliders; simulation tick {}, next event {:?}; can trigger={}",
            state.profile,
            state.lifecycle,
            state.event_kind.map_or("none", ClockEventKind::as_str),
            state.phase.as_deref().unwrap_or("none"),
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
