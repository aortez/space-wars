//! Headless Spacewars episode runner and future training entry point.

use std::{io, process::ExitCode};

use clap::{Parser, ValueEnum};
use engine_agent::{
    BatchConfig, ControllerKind, EvaluationSuite, RunReport, SpacewarsPreset, run_batch, run_suite,
};
use scenario_spacewars::PlayerId;

#[derive(Parser, Debug)]
#[command(
    name = "engine-agent",
    about = "Run deterministic Spacewars controller episodes without rendering"
)]
struct Args {
    /// Run a fixed, named evaluation suite instead of a custom batch.
    #[arg(
        long,
        value_enum,
        conflicts_with_all = [
            "scenario",
            "preset",
            "seed",
            "seed_step",
            "episodes",
            "max_ticks",
            "player_1",
            "player_2"
        ]
    )]
    suite: Option<SuiteArg>,

    /// Scenario to evaluate.
    #[arg(long, value_enum, default_value = "spacewars")]
    scenario: ScenarioArg,

    /// Spacewars world preset.
    #[arg(long, value_enum, default_value = "standard")]
    preset: PresetArg,

    /// First episode seed.
    #[arg(long, default_value_t = 0)]
    seed: u64,

    /// Amount added to the seed after each episode.
    #[arg(long, default_value_t = 1)]
    seed_step: u64,

    /// Number of episodes to run.
    #[arg(long, default_value_t = 1)]
    episodes: u32,

    /// Maximum fixed simulation ticks per episode.
    #[arg(long, default_value_t = 36_000)]
    max_ticks: u64,

    /// Controller installed in player seat 1.
    #[arg(long, value_enum, default_value = "idle")]
    player_1: ControllerArg,

    /// Controller installed in player seat 2.
    #[arg(long, value_enum, default_value = "rule")]
    player_2: ControllerArg,

    /// Record navigation events for player 1 or 2 in a custom batch.
    #[arg(long, value_parser = parse_trace_player, conflicts_with = "suite")]
    trace_player: Option<PlayerId>,

    /// Human-readable summary or versioned JSON report.
    #[arg(long, value_enum, default_value = "text")]
    output: OutputArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScenarioArg {
    Spacewars,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PresetArg {
    Standard,
    StandardNoAsteroids,
    Navigation,
    Deathmatch,
}

impl From<PresetArg> for SpacewarsPreset {
    fn from(value: PresetArg) -> Self {
        match value {
            PresetArg::Standard => Self::Standard,
            PresetArg::StandardNoAsteroids => Self::StandardNoAsteroids,
            PresetArg::Navigation => Self::Navigation,
            PresetArg::Deathmatch => Self::Deathmatch,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SuiteArg {
    NavigationV1,
}

impl From<SuiteArg> for EvaluationSuite {
    fn from(value: SuiteArg) -> Self {
        match value {
            SuiteArg::NavigationV1 => Self::NavigationV1,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ControllerArg {
    Idle,
    Rule,
}

impl From<ControllerArg> for ControllerKind {
    fn from(value: ControllerArg) -> Self {
        match value {
            ControllerArg::Idle => Self::Idle,
            ControllerArg::Rule => Self::Rule,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputArg {
    Text,
    Json,
}

fn parse_trace_player(value: &str) -> Result<PlayerId, String> {
    match value {
        "1" => Ok(PlayerId::PLAYER_1),
        "2" => Ok(PlayerId::PLAYER_2),
        _ => Err("trace player must be 1 or 2".to_owned()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("engine-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let report = if let Some(suite) = args.suite {
        run_suite(suite.into())?
    } else {
        match args.scenario {
            ScenarioArg::Spacewars => {}
        }
        run_batch(BatchConfig {
            start_seed: args.seed,
            seed_step: args.seed_step,
            episodes: args.episodes,
            preset: args.preset.into(),
            controllers: [args.player_1.into(), args.player_2.into()],
            max_ticks: args.max_ticks,
            trace_player: args.trace_player,
        })?
    };

    match args.output {
        OutputArg::Text => print_text_report(&report),
        OutputArg::Json => {
            let stdout = io::stdout();
            serde_json::to_writer_pretty(stdout.lock(), &report)?;
            println!();
        }
    }
    Ok(())
}

fn print_text_report(report: &RunReport) {
    if let Some(suite_id) = report.suite_id {
        println!("suite={suite_id}");
    }
    for episode in &report.episodes {
        println!(
            "seed={} outcome={} ticks={} sim={:.1}s captures={:?} losses={:?} rebuilds={:?} contacts(body/ship)={:?}/{:?} impacts(debris/laser)={:?}/{:?} docks={:?} departures(port/safe-capture/safe-rebuild)={:?}/{:?}/{:?} planets={:?} actions={} trace={}",
            episode.seed,
            episode.outcome,
            episode.ticks,
            episode.simulated_seconds,
            episode.captures,
            episode.ship_losses,
            episode.rebuilds,
            episode.body_contacts,
            episode.ship_contacts,
            episode.debris_impacts,
            episode.laser_hits_received,
            episode.port_dockings,
            episode.port_departures,
            episode.safe_capture_departures,
            episode.safe_rebuild_departures,
            episode.final_planet_counts,
            episode.actions_emitted,
            episode
                .trace_sha256
                .get(..12)
                .unwrap_or(&episode.trace_sha256),
        );
        for event in &episode.navigation_trace {
            println!(
                "  nav tick={} p={} reason={:?} goal={:?} phase={:?} target={:?} docked={:?} pending={:?} age={} focus={:?} clearance={} outward={} port-omega={} velocity=({:.1},{:.1}) speed={:.1} rotation={:.2} port-rotation={} omega(control/measured)={:.2}/{:.2} heading={:.2} desired={:.1} relative={:.1} contact(body/ship)={}/{} intent(turn={:.2} thrust={:.1} brake={:.1} wings={} laser={} cannon={})",
                event.tick,
                event.player.index() + 1,
                event.reasons,
                event.goal,
                event.port_phase,
                event.target_planet,
                event.docked_planet,
                event.pending_capture_planet,
                optional_u64(event.pending_capture_ticks),
                event.focus_planet,
                optional_f32(event.surface_clearance),
                optional_f32(event.outward_speed),
                optional_f32(event.spaceport_angular_speed),
                event.world_velocity[0],
                event.world_velocity[1],
                event.world_speed,
                event.world_rotation,
                optional_f32(event.spaceport_rotation),
                event.angular_velocity,
                event.measured_angular_speed,
                event.heading_error,
                event.desired_speed,
                event.relative_speed,
                event.body_contact,
                event.ship_contact,
                event.intent.turn,
                event.intent.thrust,
                event.intent.brake,
                event.intent.wings_closed,
                event.intent.laser,
                event.intent.cannon,
            );
        }
    }
    let summary = &report.summary;
    println!(
        "episodes={} winners={:?} tick_limits={} ticks={} wall={:.3}s ticks/s={:.0} realtime={:.1}x captures={:?} losses={:?} rebuilds={:?} contacts(body/ship)={:?}/{:?} impacts(debris/laser)={:?}/{:?} docks={:?} departures(port/safe-capture/safe-rebuild)={:?}/{:?}/{:?}",
        summary.episodes,
        summary.winner_counts,
        summary.tick_limits,
        summary.total_ticks,
        summary.wall_seconds,
        summary.ticks_per_wall_second,
        summary.simulated_seconds_per_wall_second,
        summary.captures,
        summary.ship_losses,
        summary.rebuilds,
        summary.body_contacts,
        summary.ship_contacts,
        summary.debris_impacts,
        summary.laser_hits_received,
        summary.port_dockings,
        summary.port_departures,
        summary.safe_capture_departures,
        summary.safe_rebuild_departures,
    );
}

fn optional_f32(value: Option<f32>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.1}"))
}

fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}
