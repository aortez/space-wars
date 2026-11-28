use super::{CliError, control_error, human_error, parse_timeout};
use clap::Subcommand;
use engine_common::{
    ClockArenaOwner, ClockArenaState, ClockArenaUnavailable, ClockDigitSurfaceState,
    ClockLampBrainState, ClockLampMode, ClockLampPercepts, ClockLampsState, ClockSurfaceChangeKind,
    ClockSurfaceRetirement, ClockThemeChoice, ClockThemeState,
};
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
        /// Event ID: falling, color-cycle, meltdown, duck, marquee, digit-slide, rain, crow, explosion, aquarium, cell-swap, grass, slot-machine, spider, coins or tanks.
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
        #[arg(long, value_parser = ["falling", "reforming", "cycling", "melting", "draining", "opening", "running", "exiting", "resetting", "presenting", "sliding", "raining", "clearing", "entering", "perched", "hopping", "flying", "leaving", "warning", "exploding", "filling", "swimming", "spinning", "descending", "crossing", "climbing", "crawling", "swinging", "recovering", "minting", "cascading", "gathering", "raising", "beam-down", "settling", "fighting", "finishing-shots", "blowing-up", "aftermath", "beam-up", "withdrawing"])]
        phase: Option<String>,
        #[arg(long, value_parser = parse_event)]
        event: Option<ClockEventKind>,
        /// Admission ID; with --event crow or spider, matches its stable visit ID.
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
                        "{} — {} ({}, {} ticks; trigger={}, automatic={}, reuse delay={} ticks, ready at={}, blocked by duck={}, blocked by crow={}, blocked by tanks={}, blocked by spider={}, blocked by arena={}, blocked by unknown time={})",
                        event.kind.as_str(),
                        event.label,
                        event.effect,
                        event.duration_ticks,
                        event.trigger.as_str(),
                        event.enabled,
                        event.cooldown_ticks,
                        event.automatic_ready_at_tick,
                        event.blocked_by_duck,
                        event.blocked_by_crow,
                        event.blocked_by_tanks,
                        event.blocked_by_spider,
                        event
                            .blocked_by_arena
                            .map_or("none", ClockArenaUnavailable::as_str),
                        event.blocked_by_unknown_time
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
                    &request.started_predicate(),
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

/// The spaceling a Tanks fight's hills left behind, the flag one planted and
/// what became of those the run has had.
fn print_stranded(stranded: &engine_common::ClockStrandedState) {
    let side = |slot: u8| ["left", "right"][usize::from(slot.min(1))];
    if let Some(spaceling) = &stranded.spaceling {
        let leaving = spaceling
            .leaving
            .map(|why| format!(", leaving ({})", why.as_str()))
            .unwrap_or_default();
        println!(
            "Stranded spaceling: {} tank's, from fight {}, {}{leaving}, age={} phase tick={}, position={:?}, facing right={}, perch={:?}, flags={} flights={} knockdowns={} dodges={}",
            side(spaceling.slot),
            spaceling.fight_id,
            spaceling.phase.as_str(),
            spaceling.age_ticks,
            spaceling.phase_tick,
            spaceling.position_milli,
            spaceling.facing_right,
            spaceling.perch,
            spaceling.flags,
            spaceling.flights,
            spaceling.knockdowns,
            spaceling.dodges
        );
    }
    if let Some(flag) = &stranded.flag {
        println!(
            "Stranded flag: {} tank's, on block {:?}, age={}",
            side(flag.slot),
            flag.block,
            flag.age_ticks
        );
    }
    let accounting = &stranded.accounting;
    println!(
        "Stranded so far: {} left behind; beamed out time-up={} event={} no-place={}; cleared={} lost={} died={}",
        accounting.stranded,
        accounting.time_up,
        accounting.event,
        accounting.no_place,
        accounting.cleared,
        accounting.lost,
        accounting.died
    );
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
            "Clock: {} / {} / {} / {} (instance {}, event {}, phase tick {}, paused={})\nTime: {}; target digits: {:?}\nPhysics: {} bodies, {} colliders; simulation tick {}, next event {:?}; can trigger={}",
            state.profile,
            state.lifecycle,
            state.event_kind.map_or("none", ClockEventKind::as_str),
            state.phase.as_deref().unwrap_or("none"),
            state.scenario_revision,
            state.event_id,
            state.phase_tick,
            state.paused,
            if state.time_unknown {
                "unknown, showing --:--".to_owned()
            } else {
                format!("{:?}", state.reading)
            },
            state.display_digits,
            state.body_count,
            state.collider_count,
            state.simulation_tick,
            state.next_event_tick,
            state.can_trigger
        );
        println!(
            "Date: {:?}; label: {:?}; show_date={}\nFloor: {}\nConfigured marquee: {} / {:?}; settings pending={}",
            state.date,
            state.date_label,
            state.settings.show_date,
            state.floor.as_str(),
            state.settings.marquee_preset.label(),
            state.settings.marquee_message.as_str(),
            state.settings_pending
        );
        if let Some(error) = &state.settings_error {
            println!("Settings warning: {error}");
        }
        if let Some(theme) = &state.theme {
            println!("{}", theme_line(theme, state.settings.theme));
        }
        if let Some(arena) = &state.arena {
            print_arena(arena);
        }
        if let Some(surfaces) = &state.digit_surfaces {
            print_digit_surfaces(surfaces);
        }
        if let Some(explosion) = &state.explosion {
            println!(
                "Explosion: {} cells, {} live (cap {}), shared arena={}",
                explosion.cells, explosion.live_cells, explosion.max_cells, explosion.shared_arena
            );
        }
        if let Some(tanks) = &state.tanks {
            println!(
                "Tanks: tick={}, combat={}, health milli={:?}, shells={}, shots={}, result={}, converted={} cells ({:.2} area)",
                tanks.tick,
                tanks.combat_ticks,
                tanks.health_milli,
                tanks.shells,
                tanks.shots,
                tanks.result.as_deref().unwrap_or("pending"),
                tanks.converted_cells,
                tanks.conversion_area_milli as f32 / 1000.0,
            );
            if let Some(reason) = &tanks.entry_failure {
                println!("Tanks entry: {reason}");
            }
            for (side, survivor) in ["Left", "Right"].into_iter().zip(&tanks.survivors) {
                if let Some(survivor) = survivor {
                    // An ending that fell back says which it drew, and why.
                    let fallback = survivor
                        .fallback
                        .map(|why| format!(" (drew {}, {})", survivor.drawn.as_str(), why.as_str()))
                        .unwrap_or_default();
                    println!(
                        "{side} spaceling: {}, got out {}, ending {}{fallback}, position={:?}, stood up={}, knockdowns={}",
                        survivor.phase.as_str(),
                        survivor.ejection.as_str(),
                        survivor.ending.as_str(),
                        survivor.position_milli,
                        survivor.stood_up,
                        survivor.knockdowns
                    );
                }
            }
            if let Some(rebuild) = &tanks.rebuild {
                println!(
                    "Tanks rebuild: {} tank {}, first result {}, winner fired={}, sudden death={} ticks",
                    ["left", "right"][usize::from(rebuild.slot.min(1))],
                    rebuild.stage.as_str(),
                    rebuild.first_result,
                    rebuild.fired,
                    rebuild.sudden_death_ticks,
                );
            }
        }
        if let Some(coins) = &state.coins {
            println!(
                "Coins: {} cells, {} live (cap {}), shared arena={}, taken={}",
                coins.cells, coins.live_cells, coins.max_cells, coins.shared_arena, coins.taken
            );
            if let Some(magpie) = &coins.magpie {
                println!(
                    "Magpie: {}, phase tick={}, position={:?}, facing right={}, home right={}, target={:?}, carrying={}, trips={}, abandoned={}",
                    magpie.phase.as_str(),
                    magpie.phase_tick,
                    magpie.position_milli,
                    magpie.facing_right,
                    magpie.home_right,
                    magpie.target,
                    magpie.carrying,
                    magpie.trips,
                    magpie.abandoned
                );
            }
        }
        if let Some(lamps) = &state.lamps {
            println!("{}", lamps_line(lamps, state.settings.lamps));
            println!("{}", lamp_line("Left", &lamps.left));
            println!("{}", lamp_line("Right", &lamps.right));
            println!("{}", perceived_line(&lamps.perceived));
        }
        for crow in state.visiting_crows() {
            println!(
                "Crow #{} member {}: {}, age={} phase tick={}, position={:?}, target={:?}, hops={}, escapes={}, ground={:?} visits={} pecks={}, water={} wet={}/1000 departures={}",
                crow.visit_id,
                crow.member,
                crow.phase.as_str(),
                crow.age_ticks,
                crow.phase_tick,
                crow.position_milli,
                crow.target,
                crow.hops,
                crow.escapes,
                crow.ground_target_milli,
                crow.ground_visits,
                crow.pecks,
                crow.water_tolerance.label(),
                crow.wetness_milli,
                crow.wet_departures
            );
        }
        if let Some(stranded) = &state.stranded {
            print_stranded(stranded);
        }
        if let Some(spider) = &state.spider {
            println!(
                "Spider #{} (seed {}): {}, age={} phase tick={} remaining={}, {} at {:?} going {:?} up {:?}, holds {:?}, tied to {:?}, shooting={} line={:?}, climbs={} swings={} falls={} recoveries={}, proxies={}, failed exit={}",
                spider.visit_id,
                spider.seed,
                spider.phase.as_str(),
                spider.age_ticks,
                spider.phase_tick,
                spider.remaining_ticks,
                spider.mode.as_str(),
                spider.position_milli,
                spider.velocity_milli,
                spider.up_milli,
                spider.support,
                spider.anchor,
                spider.shooting,
                spider.line_length_milli,
                spider.climbs,
                spider.swings,
                spider.falls,
                spider.recoveries,
                spider.proxies,
                spider.failed_exit
            );
        }
        if let Some(chess) = &state.chess {
            for line in chess_lines(chess) {
                println!("{line}");
            }
        }
        let world_vector =
            |value: Option<[i32; 2]>| value.map(|[x, y]| [x as f64 / 1000.0, y as f64 / 1000.0]);
        if let Some(rain) = state.rain {
            println!(
                "Rain arena: {}; visiting duck has joined={}",
                if rain.duck_course {
                    "shared duck course (fixed)"
                } else {
                    "responsive floor"
                },
                rain.duck_joined
            );
            println!(
                "Rain: {} / {:?}, spawns={}, entry depth={:.1}/{:.1}; position={:?}, velocity={:?}",
                rain.amount.label(),
                rain.duck_phase,
                rain.duck_spawns,
                rain.entry_depth_milli as f64 / 1000.0,
                rain.required_depth_milli as f64 / 1000.0,
                world_vector(rain.duck_position_milli),
                world_vector(rain.duck_velocity_milli)
            );
            println!(
                "Water (micro-units): injected={} pooled={} falling={} drained={} reclaimed={}; parcels={}, source-limited ticks={}, outlet-limited ticks={}",
                rain.injected_microunits,
                rain.pooled_microunits,
                rain.in_flight_microunits,
                rain.drained_microunits,
                rain.reclaimed_microunits,
                rain.parcels,
                rain.source_limited_ticks,
                rain.water_limited_ticks
            );
            if rain.strikes_planned > 0 {
                println!("{}", lightning_line(&rain));
            }
        }
        if let Some(material) = state.meltdown {
            // Older hosts omit these fields and only have uniform full-size cells.
            let (initial, solid) = if material.initial_microunits == 0 {
                (
                    (material.initial_cells as u64).saturating_mul(1_000_000),
                    (material
                        .waiting_cells
                        .saturating_add(material.airborne_cells) as u64)
                        .saturating_mul(1_000_000),
                )
            } else {
                (material.initial_microunits, material.solid_microunits)
            };
            println!(
                "Meltdown: {} waiting, {} airborne, {} wet columns; initial {:.3}, solid {:.3}, water {:.3}, spilling {:.3} ({} parcels), drained {:.3}, reclaimed {:.3} cell-volumes; capacity-limited ticks={}; displaced body space={:.3} (not water)",
                material.waiting_cells,
                material.airborne_cells,
                material.water_columns,
                initial as f64 / 1_000_000.0,
                solid as f64 / 1_000_000.0,
                material.pooled_microunits as f64 / 1_000_000.0,
                material.spilling_microunits as f64 / 1_000_000.0,
                material.spill_parcels,
                material.drained_microunits as f64 / 1_000_000.0,
                material.reclaimed_microunits as f64 / 1_000_000.0,
                material.capacity_limited_ticks,
                material.displaced_microunits as f64 / 1_000_000.0
            );
            println!(
                "Meltdown floor: {:.1}% open, load={:.3} world units, motion deferrals={}; {:.3} of drained cell-volumes exited as solid blocks",
                material.floor_open_milli as f64 / 10.0,
                material.floor_load_milli as f64 / 1000.0,
                material.floor_motion_deferrals,
                material.exited_solid_microunits as f64 / 1_000_000.0,
            );
        }
        if let Some(player) = &state.player_duck {
            println!(
                "Player duck: P{} session={} phase={} move={} jump-held={} run-held={} dive-held={} grounded={} jumps={} position={:?} outcome={:?}; automatic-events-suspended={}",
                player.player,
                player.session_id,
                player.phase,
                player.move_milli,
                player.jump_held,
                player.run_held,
                player.dive_held,
                player.duck.grounded,
                player.duck.jumps,
                world_vector(player.duck.position_milli),
                player.duck.outcome,
                state.automatic_events_suspended,
            );
            let ledge = &player.duck.ledge;
            println!(
                "Player ledge: {:?}, support={:?}, anchor={:?}, rearm={}, catches={}, climbs={}, releases={}, reason={:?}",
                ledge.phase,
                ledge.support_body,
                world_vector(ledge.anchor_milli),
                ledge.rearm_required,
                ledge.catches,
                ledge.climbs,
                ledge.releases,
                ledge.reason,
            );
            println!(
                "Ledge controls: hold Jump to catch; release then press Jump to climb; Down drops. Release Jump before re-catching."
            );
            println!(
                "Player water: {:.1}% submerged, velocity={:?}, strokes={}, stroke cooldown={} ticks, dive remaining={} ticks, dive recharging={}",
                player.submerged_milli as f32 / 10.0,
                world_vector(player.velocity_milli),
                player.swim_strokes,
                player.swim_cooldown_ticks,
                player.dive_ticks_left,
                player.dive_recharging,
            );
            if let Some(opening) = player.floor_open_milli {
                println!(
                    "Player arena: responsive floor, {:.1}% open",
                    opening as f32 / 10.0
                );
            }
            let blocked: Vec<_> = state
                .events
                .iter()
                .filter(|event| event.blocked_by_duck)
                .map(|event| event.label.as_str())
                .collect();
            println!("Waiting for duck arena: {}", blocked.join(", "));
        }
        if let Some(duck) = state.duck {
            if let Some(visit) = duck.visit {
                println!(
                    "Automatic visit: phase={} tick={} phase-tick={}, {:.1}% submerged, velocity={:?}",
                    visit.phase.as_str(),
                    visit.tick,
                    visit.phase_tick,
                    visit.submerged_milli as f32 / 10.0,
                    world_vector(visit.velocity_milli),
                );
            }
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
                    "Duck water: {} interruptions, {} recoveries, {} paddling ticks, {} recovering ticks, target bank={:?}",
                    navigation.water.interruptions,
                    navigation.water.recoveries,
                    navigation.water.paddling_ticks,
                    navigation.water.recovering_ticks,
                    navigation.water.target_surface,
                );
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

/// The visit to the chess board, and each of its birds that is in the
/// scene. The birds are the visit's own, so they are listed under it.
fn chess_lines(chess: &engine_common::ClockChessState) -> Vec<String> {
    let mut lines = vec![format!(
        "Chess visit #{}: match {}, {}, phase tick={}, actions={}, board={:?}",
        chess.visit_id,
        chess.match_number,
        chess.phase.as_str(),
        chess.phase_tick,
        chess.actions,
        chess.board_milli
    )];
    for actor in &chess.actors {
        lines.push(format!(
            "Chess visit #{} {}: plays {}, {}, position={:?}, facing right={}, carrying={}",
            chess.visit_id,
            actor.bird.as_str(),
            if actor.white { "White" } else { "Black" },
            actor.pose.as_str(),
            actor.position_milli,
            actor.facing_right,
            actor.carrying
        ));
    }
    lines
}

fn print_arena(arena: &ClockArenaState) {
    let (accounting, limits) = (arena.accounting, arena.limits);
    println!(
        "Arena: {} claims, {} residents (at most {} and {}); {} bodies, {} colliders, {} joints (at most {}, {}, {}); floor {} bodies",
        accounting.claims,
        accounting.residents,
        limits.claims,
        limits.residents,
        accounting.bodies,
        accounting.colliders,
        accounting.joints,
        limits.capacity.bodies,
        limits.capacity.colliders,
        limits.capacity.joints,
        arena.floor.bodies
    );
    let claims: Vec<_> = arena
        .claims
        .iter()
        .map(|claim| {
            let owner = match claim.owner {
                ClockArenaOwner::Visit => "visit".into(),
                ClockArenaOwner::Event { event } => format!("event {}", event.as_str()),
                ClockArenaOwner::Soil => "soil".into(),
                ClockArenaOwner::Fixture { index } => format!("fixture {index}"),
                ClockArenaOwner::Spider => "spider".into(),
                ClockArenaOwner::Crow => "crow".into(),
                ClockArenaOwner::Stranded => "stranded".into(),
                ClockArenaOwner::Chess => "chess".into(),
            };
            format!(
                "{owner} {}/{} bodies, {}/{} colliders",
                claim.population.bodies,
                claim.budget.bodies,
                claim.population.colliders,
                claim.budget.colliders
            )
        })
        .collect();
    println!(
        "Arena claims: {}",
        if claims.is_empty() {
            "none".into()
        } else {
            claims.join("; ")
        }
    );
    println!(
        "Arena run: {} worlds, {} steps; granted={} released={} refused={}; layout incompatible={}, last remap={:?}",
        accounting.worlds,
        accounting.steps,
        accounting.granted,
        accounting.released,
        accounting.refused,
        accounting.layout_incompatible,
        accounting.last_remap
    );
    if let Some(step) = arena.last_step {
        println!(
            "Arena step: {} us (collision detection {} us, solver {} us); bodies active={} sleeping={}; pairs candidate={} contact={}, contacts={}, CCD substeps={}",
            step.wall_us,
            step.collision_detection_us,
            step.solver_us,
            step.active_bodies,
            step.sleeping_bodies,
            step.candidate_pairs,
            step.contact_pairs,
            step.contacts,
            step.ccd_substeps
        );
    }
}

/// The theme's setting, the theme it wears now, the one Auto would, and how
/// far the fade to it has come, in thousandths of its half second.
fn theme_line(theme: &ClockThemeState, choice: ClockThemeChoice) -> String {
    format!(
        "Theme: {}; wears={} auto={} fade={}",
        choice.label(),
        theme.theme.label(),
        theme.auto.label(),
        theme.fade_milli
    )
}

/// The lamps' setting, how much of their light they give at this time of
/// day, and how many times the run's lamps have done each thing.
fn lamps_line(lamps: &ClockLampsState, mode: ClockLampMode) -> String {
    format!(
        "Lamps: {}; output={} notices={} tracks={} handoffs={} startles={} outages={} presents={} fidgets={} dips={}",
        mode.label(),
        lamps.output_milli,
        lamps.notices,
        lamps.tracks,
        lamps.handoffs,
        lamps.startles,
        lamps.outages,
        lamps.presents,
        lamps.fidgets,
        lamps.dips
    )
}

/// What one lamp is doing, on one line: its temperament, its activity and
/// for how many ticks, the mover it holds, where its light points, how
/// narrow and bright it is, and its drives. A mover is `world:entity/role`,
/// as the perceive line names it. The aim is in thousandths of a view unit,
/// and the rest in thousandths.
fn lamp_line(side: &str, lamp: &ClockLampBrainState) -> String {
    let target = lamp.target.map_or(String::new(), |target| {
        format!(" {}:{}/{}", target.world, target.entity, target.role)
    });
    format!(
        "{side} lamp ({}): {}{target} for {} ticks; aim={:?} narrow={} brightness={}; interest={} boredom={} heat={}",
        lamp.temperament.as_str(),
        lamp.state.as_str(),
        lamp.state_ticks,
        lamp.aim_milli,
        lamp.narrow_milli,
        lamp.brightness_milli,
        lamp.drives.interest_milli,
        lamp.drives.boredom_milli,
        lamp.drives.heat_milli
    )
}

/// Rain's lightning: how many of its planned strikes have struck, this tick's
/// flash, and the latest strike, its points in world units.
fn lightning_line(rain: &engine_common::ClockRainState) -> String {
    let mut line = format!(
        "Lightning: struck {} of {}; flash={}/1000",
        rain.strikes, rain.strikes_planned, rain.flash_milli
    );
    if let Some(strike) = rain.last_strike {
        let point =
            |[x, y]: [i32; 2]| format!("[{:.1}, {:.1}]", x as f64 / 1000.0, y as f64 / 1000.0);
        let target = match strike.target {
            engine_common::ClockStrikeTarget::Digit => "a digit",
            engine_common::ClockStrikeTarget::Water => "the water",
            engine_common::ClockStrikeTarget::Floor => "the floor",
        };
        line += &format!(
            "; last at tick {}, {} strokes, struck {target} from {} to {}",
            strike.tick,
            strike.strokes,
            point(strike.top_milli),
            point(strike.ground_milli)
        );
    }
    line
}

/// What the lamps perceive this tick, on one line: how many bodies they
/// looked at, the most salient mover, the strongest impact and what changed.
/// A body is `world:entity/role`, its world 0 for the arena and 1 for an
/// event's own.
/// Positions and sizes are in thousandths of a view unit, and velocities in
/// thousandths of one a second.
fn perceived_line(seen: &ClockLampPercepts) -> String {
    let mut line = format!(
        "Lamps perceive: bodies={} movers={} impacts={}",
        seen.bodies_visited,
        seen.movers.len(),
        seen.impacts.len()
    );
    if let Some(mover) = seen.movers.first() {
        line += &format!(
            "; top mover {}:{}/{} position={:?} velocity={:?} size={} age={} salience={}",
            mover.world,
            mover.entity,
            mover.role,
            mover.position_milli,
            mover.velocity_milli,
            mover.size_milli,
            mover.age_ticks,
            mover.salience_milli
        );
    }
    if let Some(impact) = seen.impacts.first() {
        line += &format!(
            "; top impact {}:{}/{} position={:?} strength={}",
            impact.world, impact.entity, impact.role, impact.position_milli, impact.strength_milli
        );
    }
    if let Some(event) = seen.event.filter(|_| seen.event_began) {
        line += &format!("; {} began", event.as_str());
    }
    if let Some(event) = seen.event_ended {
        line += &format!("; {} ended", event.as_str());
    }
    if !seen.changed_digits.is_empty() {
        line += &format!("; digits changed={:?}", seen.changed_digits);
    }
    if seen.ceiling_open {
        line += "; ceiling open";
    }
    if let Some(day) = seen.time_of_day_milli {
        line += &format!("; day={day}/1000");
    }
    if let Some([top, ground]) = seen.bolt_milli {
        line += &format!("; lightning from {top:?} to {ground:?}");
    }
    line
}

fn print_digit_surfaces(surfaces: &ClockDigitSurfaceState) {
    let accounting = surfaces.accounting;
    let sources: Vec<_> = surfaces
        .sources
        .iter()
        .map(|count| format!("{} {}", count.source.as_str(), count.live))
        .collect();
    println!(
        "Digit surfaces: {} live, {} departing (peak {}, at most {}); sources: {}",
        accounting.live,
        accounting.departing,
        accounting.peak_live,
        surfaces.limits.live,
        if sources.is_empty() {
            "none".into()
        } else {
            sources.join(", ")
        }
    );
    let reasons: Vec<_> = ClockSurfaceRetirement::ALL
        .into_iter()
        .filter(|reason| accounting.retired.get(*reason) > 0)
        .map(|reason| format!("{} {}", reason.as_str(), accounting.retired.get(reason)))
        .collect();
    println!(
        "Digit surface run: admitted={} handed off={} departed={} retired={}{}; last change #{}",
        accounting.admitted,
        accounting.handed_off,
        accounting.departed,
        accounting.retired.total(),
        if reasons.is_empty() {
            String::new()
        } else {
            format!(" ({})", reasons.join(", "))
        },
        accounting.sequence
    );
    let milli = |value: i32| value as f64 / 1000.0;
    for surface in &surfaces.sample {
        let [min_x, min_y, max_x, max_y] = surface.footprint_milli.map(milli);
        println!(
            "Digit surface {}: slot {} cell {:?}, {}{}, origin ({:.3}, {:.3}), angle {:.1} degrees, scale {:.3}, footprint ({min_x:.2}, {min_y:.2})..({max_x:.2}, {max_y:.2}), opacity {:.3}{}{}",
            surface.id,
            surface.slot,
            surface.cell,
            surface.source.as_str(),
            if surface.departing { " departing" } else { "" },
            milli(surface.origin_milli[0]),
            milli(surface.origin_milli[1]),
            milli(surface.angle_millidegrees),
            milli(surface.scale_milli),
            f64::from(surface.opacity_milli) / 1000.0,
            surface
                .clip_milli
                .map_or(String::new(), |[low, high]| format!(
                    ", clipped to y {:.3}..{:.3}",
                    milli(low),
                    milli(high)
                )),
            surface
                .body
                .map_or(String::new(), |body| format!(", body {body}"))
        );
    }
    let changes: Vec<_> = surfaces
        .recent
        .iter()
        .map(|change| {
            let what = match change.change {
                ClockSurfaceChangeKind::Admitted { slot, cell, source } => {
                    format!("admitted in slot {slot} at {cell:?} by {}", source.as_str())
                }
                ClockSurfaceChangeKind::HandedOff { from, to } => {
                    format!("handed off {} to {}", from.as_str(), to.as_str())
                }
                ClockSurfaceChangeKind::Departing => "departing".into(),
                ClockSurfaceChangeKind::Retired { reason } => {
                    format!("retired {}", reason.as_str())
                }
            };
            format!(
                "#{} tick {} surface {} {what}",
                change.sequence, change.tick, change.id
            )
        })
        .collect();
    println!(
        "Digit surface changes: {}",
        if changes.is_empty() {
            "none".into()
        } else {
            changes.join("; ")
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Args, Command};
    use clap::Parser;
    use engine_common::{
        ClockLampActivity, ClockLampDrives, ClockLampImpact, ClockLampMover, ClockLampTarget,
        ClockLampTemperament,
    };

    #[test]
    fn the_theme_line_shows_the_setting_what_it_wears_what_auto_would_and_the_fade() {
        use engine_common::ClockTheme;
        let theme = ClockThemeState {
            theme: ClockTheme::Normal,
            auto: ClockTheme::Christmas,
            fade_milli: 400,
        };
        assert_eq!(
            theme_line(&theme, ClockThemeChoice::Normal),
            "Theme: Normal; wears=Normal auto=Christmas fade=400"
        );
    }

    #[test]
    fn the_chess_lines_show_the_visit_and_each_of_its_birds_under_it() {
        use engine_common::{
            ClockChessActor, ClockChessBird, ClockChessPhase, ClockChessPose, ClockChessState,
        };
        let mut chess = ClockChessState {
            visit_id: 3,
            match_number: 2,
            phase: ClockChessPhase::Arriving,
            phase_tick: 120,
            actions: 17,
            board_milli: [-45_067, -201_600, 45_067, -111_467],
            actors: vec![
                ClockChessActor {
                    bird: ClockChessBird::Crow,
                    white: true,
                    pose: ClockChessPose::Arriving,
                    position_milli: [78_402, -187_310],
                    facing_right: false,
                    carrying: false,
                },
                ClockChessActor {
                    bird: ClockChessBird::Duck,
                    white: false,
                    pose: ClockChessPose::Arriving,
                    position_milli: [-250_683, -194_660],
                    facing_right: true,
                    carrying: true,
                },
            ],
        };
        assert_eq!(
            chess_lines(&chess),
            [
                "Chess visit #3: match 2, arriving, phase tick=120, actions=17, board=[-45067, -201600, 45067, -111467]",
                "Chess visit #3 crow: plays White, arriving, position=[78402, -187310], facing right=false, carrying=false",
                "Chess visit #3 duck: plays Black, arriving, position=[-250683, -194660], facing right=true, carrying=true",
            ]
        );
        // A visit whose birds have gone is still a visit until its door has
        // closed.
        chess.phase = ClockChessPhase::Leaving;
        chess.actors.clear();
        assert_eq!(
            chess_lines(&chess),
            [
                "Chess visit #3: match 2, leaving, phase tick=120, actions=17, board=[-45067, -201600, 45067, -111467]"
            ]
        );
    }

    #[test]
    fn the_lamp_lines_show_the_setting_the_counts_and_each_lamps_mind() {
        let mut lamps = ClockLampsState {
            output_milli: 30,
            notices: 4,
            tracks: 5,
            handoffs: 1,
            outages: 2,
            presents: 3,
            fidgets: 6,
            ..ClockLampsState::default()
        };
        assert_eq!(
            lamps_line(&lamps, ClockLampMode::Lively),
            "Lamps: Lively; output=30 notices=4 tracks=5 handoffs=1 startles=0 outages=2 presents=3 fidgets=6 dips=0"
        );
        assert_eq!(
            lamp_line("Right", &lamps.right),
            "Right lamp (lazy): rest for 0 ticks; aim=[0, 0] narrow=0 brightness=0; \
             interest=0 boredom=0 heat=0"
        );
        lamps.left = ClockLampBrainState {
            temperament: ClockLampTemperament::Eager,
            state: ClockLampActivity::Track,
            state_ticks: 84,
            target: Some(ClockLampTarget {
                world: 0,
                entity: 7001,
                role: 0,
            }),
            aim_milli: [-1500, 250],
            narrow_milli: 1000,
            brightness_milli: 1150,
            drives: ClockLampDrives {
                interest_milli: 1200,
                boredom_milli: 340,
                heat_milli: 520,
            },
        };
        assert_eq!(
            lamp_line("Left", &lamps.left),
            "Left lamp (eager): track 0:7001/0 for 84 ticks; aim=[-1500, 250] narrow=1000 \
             brightness=1150; interest=1200 boredom=340 heat=520"
        );
    }

    #[test]
    fn the_perceive_line_names_the_top_mover_and_impact_and_what_changed() {
        let mut seen = ClockLampPercepts {
            bodies_visited: 3,
            ..ClockLampPercepts::default()
        };
        assert_eq!(
            perceived_line(&seen),
            "Lamps perceive: bodies=3 movers=0 impacts=0"
        );
        seen.movers = vec![
            ClockLampMover {
                world: 1,
                entity: 7,
                role: 0,
                position_milli: [-1500, 250],
                velocity_milli: [4000, -10],
                size_milli: 120,
                age_ticks: 30,
                salience_milli: 2600,
            };
            2
        ];
        seen.impacts = vec![ClockLampImpact {
            world: 0,
            entity: 9,
            role: 1,
            position_milli: [300, -2000],
            strength_milli: 7500,
        }];
        seen.event = Some(ClockEventKind::Rain);
        seen.event_began = true;
        seen.changed_digits = vec![2, 3];
        seen.ceiling_open = true;
        seen.time_of_day_milli = Some(527);
        seen.bolt_milli = Some([[-120_000, 201_600], [-95_500, 105_600]]);
        assert_eq!(
            perceived_line(&seen),
            "Lamps perceive: bodies=3 movers=2 impacts=1; top mover 1:7/0 \
             position=[-1500, 250] velocity=[4000, -10] size=120 age=30 salience=2600; \
             top impact 0:9/1 position=[300, -2000] strength=7500; rain began; \
             digits changed=[2, 3]; ceiling open; day=527/1000; \
             lightning from [-120000, 201600] to [-95500, 105600]"
        );
        // An event under way that did not begin this tick goes unsaid.
        seen.event_began = false;
        seen.event_ended = Some(ClockEventKind::Duck);
        assert!(perceived_line(&seen).contains("; duck ended; digits"));
    }

    #[test]
    fn the_lightning_line_counts_strikes_and_places_the_last_one() {
        let state: engine_common::ClockRainState = serde_json::from_value(serde_json::json!({
            "seed": 7, "amount": "heavy", "requested_microunits": 0,
            "scheduled_microunits": 0, "injected_microunits": 0, "pooled_microunits": 0,
            "in_flight_microunits": 0, "drained_microunits": 0, "reclaimed_microunits": 0,
            "parcels": 0, "source_limited_ticks": 0, "water_limited_ticks": 0,
            "entry_depth_milli": 0, "required_depth_milli": 0, "duck_phase": "waiting",
            "duck_spawns": 0, "duck_position_milli": null, "duck_velocity_milli": null,
            "duck_angle_milli": null, "submerged_milli": 0, "door_open_milli": 0,
            "strikes_planned": 4, "strikes": 2, "flash_milli": 720,
            "last_strike": {"tick": 640, "strokes": 3, "top_milli": [-120000, 201600],
                "ground_milli": [-95500, 105600], "target": "digit"}
        }))
        .unwrap();
        assert_eq!(
            lightning_line(&state),
            "Lightning: struck 2 of 4; flash=720/1000; last at tick 640, 3 strokes, \
             struck a digit from [-120.0, 201.6] to [-95.5, 105.6]"
        );
        let before = engine_common::ClockRainState {
            strikes: 0,
            last_strike: None,
            flash_milli: 0,
            ..state
        };
        assert_eq!(
            lightning_line(&before),
            "Lightning: struck 0 of 4; flash=0/1000"
        );
    }

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
        for phase in ["raining", "draining", "clearing"] {
            assert!(
                Args::try_parse_from([
                    "spacewars-cli",
                    "clock",
                    "wait",
                    "--event",
                    "rain",
                    "--phase",
                    phase
                ])
                .is_ok()
            );
        }
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
