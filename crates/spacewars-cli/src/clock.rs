use super::{CliError, control_error, human_error, parse_timeout};
use clap::Subcommand;
use spacewars_control::{
    ClockEventKind, ClockMarqueeMessage, ClockMessageRequest, ClockState, ClockStatePredicate,
    ClockTriggerRequest, ControlClient,
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
    /// Save text for future marquee events. Requires a paused Clock.
    Message {
        /// 1-32 ASCII characters: letters, digits, spaces and . , : - ! ? / '
        /// Lowercase is displayed and saved as uppercase; all-space text is invalid.
        text: ClockMarqueeMessage,
        /// Reject a stale Clock instance; defaults to the current instance.
        #[arg(long)]
        expect_scenario_revision: Option<u64>,
        #[arg(long, default_value = "3s", value_parser = parse_timeout)]
        timeout: Duration,
        #[arg(long)]
        json: bool,
    },
    /// Preview an event from idle, including with Off or that event disabled.
    Trigger {
        /// Event ID: falling, color-cycle, meltdown, duck, marquee or digit-slide.
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
        #[arg(long, value_parser = ["falling", "reforming", "cycling", "melting", "draining", "opening", "running", "exiting", "resetting", "presenting", "sliding"])]
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
        .ok_or_else(|| {
            format!(
                "Unknown Clock event {value:?}; choose {}",
                ClockEventKind::ALL.map(ClockEventKind::as_str).join(", ")
            )
        })
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
                        "{} — {} ({}, {} ticks; trigger={}, automatic={}, reuse delay={} ticks, ready at={})",
                        event.kind.as_str(),
                        event.label,
                        event.effect,
                        event.duration_ticks,
                        event.trigger.as_str(),
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
        ClockCommand::Message {
            text,
            expect_scenario_revision,
            timeout,
            json,
        } => {
            let deadline = deadline(timeout)?;
            let state = client
                .clock_state_before(deadline)
                .map_err(|e| control_error(e, json))?;
            let request = ClockMessageRequest {
                expected_scenario_revision: expect_scenario_revision
                    .unwrap_or(state.scenario_revision),
                ..ClockMessageRequest::new(&state, text)
            };
            client
                .clock_message_before(&request, deadline)
                .map_err(|e| control_error(e, json))?;
            let state = client
                .wait_for_clock_message(
                    &request,
                    deadline.saturating_duration_since(Instant::now()),
                )
                .map_err(|e| control_error(e, json))?;
            if json {
                print_state(&state, true)
            } else {
                println!(
                    "Saved marquee message: {:?}. Used by the next text event; active content is unchanged.",
                    state.settings.marquee_message.as_str()
                );
                Ok(())
            }
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
        println!(
            "Configured marquee: {} / {:?}; settings pending={}",
            state.settings.marquee_preset.label(),
            state.settings.marquee_message.as_str(),
            state.settings_pending
        );
        if let Some(error) = &state.settings_error {
            println!("Settings warning: {error}");
        }
        if let Some(material) = state.meltdown {
            println!(
                "Meltdown: {} waiting, {} airborne, {} wet columns; water {:.3}, spilling {:.3} ({} parcels), drained {:.3}, reclaimed {:.3} cell-volumes; capacity-limited ticks={}",
                material.waiting_cells,
                material.airborne_cells,
                material.water_columns,
                material.pooled_microunits as f64 / 1_000_000.0,
                material.spilling_microunits as f64 / 1_000_000.0,
                material.spill_parcels,
                material.drained_microunits as f64 / 1_000_000.0,
                material.reclaimed_microunits as f64 / 1_000_000.0,
                material.capacity_limited_ticks
            );
        }
        if let Some(duck) = state.duck {
            println!(
                "Duck: {}; {} jumps, {}/{} obstacles cleared, grounded={}; outcome {:?}",
                if duck.left_to_right {
                    "left to right"
                } else {
                    "right to left"
                },
                duck.jumps,
                duck.cleared_obstacles,
                duck.obstacle_count,
                duck.grounded,
                duck.outcome
            );
            if let Some(navigation) = duck.navigation {
                println!(
                    "Duck controller: {:?} / {:?}, facing {}; wall tags L/R={:?}, target obstacle={:?}, exit visible={}; course seed={}",
                    navigation.jump_profile,
                    navigation.behavior,
                    if navigation.facing_right {
                        "right"
                    } else {
                        "left"
                    },
                    navigation.wall_tags,
                    navigation.target_obstacle,
                    navigation.exit_visible,
                    navigation.course_seed
                );
                println!(
                    "Duck measured: {} warm-up jumps, {} speed samples; run={:?} units/s, jump={:?} units, flight={:?} ticks",
                    navigation.calibrated_jumps,
                    navigation.speed_samples,
                    navigation.run_speed_milli.map(|v| v as f32 / 1000.0),
                    navigation.jump_height_milli.map(|v| v as f32 / 1000.0),
                    navigation.flight_ticks
                );
                if let Some(planning) = navigation.planning {
                    println!(
                        "Duck movement: {:?} course; {} running jumps, {} moving landings, {} careful fallbacks, {} platforms skipped",
                        planning.pattern,
                        planning.running_jumps,
                        planning.moving_landings,
                        planning.flowing_fallbacks,
                        planning.skipped_platforms
                    );
                    println!(
                        "Duck landings: {} confirmed, {} short, {} long, {} wrong-surface; support={:?}/{}, rejected={} ({:?}), fallback={}",
                        planning.confirmed_landings,
                        planning.undershoots,
                        planning.overshoots,
                        planning.wrong_surface_landings,
                        planning.support,
                        planning.surface_count,
                        planning.rejected_plans,
                        planning.rejection,
                        planning.fallback_course
                    );
                    if let Some(plan) = planning.plan {
                        println!(
                            "Duck plan: {} -> {}, takeoff={:?}, landing={:?} (milliunits), flight={} ticks, cruise={:.3} units/s, running={}, next={:?}",
                            plan.source,
                            plan.target,
                            plan.takeoff_milli,
                            plan.landing_milli,
                            plan.flight_ticks,
                            plan.cruise_speed_milli as f32 / 1000.0,
                            plan.running_takeoff,
                            plan.next_target
                        );
                    }
                }
            }
        }
        if let Some(marquee) = &state.marquee {
            println!(
                "marquee: {} content={:?} cells={} groups={} progress={:.1}% scroll={} wave={} rotation={} lighting={}",
                marquee.preset.label(),
                marquee.content,
                marquee.cell_count,
                marquee.group_count,
                marquee.progress_milli as f32 / 10.0,
                marquee.scrolling,
                marquee.waving,
                marquee.rotation_target.as_deref().unwrap_or("none"),
                marquee.lighting
            );
        }
        if let Some(slide) = &state.digit_slide {
            println!(
                "Digit Slide: {:?} -> {:?}; changed={:?}, progress={:.1}%, preview={}",
                slide.from_digits,
                slide.to_digits,
                slide.changed_slots,
                slide.progress_milli as f32 / 10.0,
                slide.preview
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Args, Command};
    use clap::Parser;

    #[test]
    fn every_catalog_event_is_a_valid_cli_trigger_and_sliding_is_a_known_phase() {
        for kind in ClockEventKind::ALL {
            assert!(
                Args::try_parse_from(["spacewars-cli", "clock", "trigger", kind.as_str()]).is_ok()
            );
        }
        assert!(
            Args::try_parse_from([
                "spacewars-cli",
                "clock",
                "wait",
                "--event",
                "digit-slide",
                "--phase",
                "sliding"
            ])
            .is_ok()
        );
    }

    #[test]
    fn message_cli_validates_before_contacting_the_client() {
        let args =
            Args::try_parse_from(["spacewars-cli", "clock", "message", "Hello, pi!", "--json"])
                .unwrap();
        let Command::Clock {
            command: ClockCommand::Message { text, json, .. },
        } = args.command
        else {
            panic!()
        };
        assert!(json);
        assert_eq!(text.as_str(), "HELLO, PI!");
        for message in ["", "   ", "A\nB", "é", "A_B", &"A".repeat(33)] {
            assert!(Args::try_parse_from(["spacewars-cli", "clock", "message", message]).is_err());
        }
    }
}
