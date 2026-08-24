//! Headless Spacewars episode runner and future training entry point.

use std::{io, process::ExitCode};

use clap::{Parser, ValueEnum};
use engine_agent::{
    BatchConfig, ComparedPolicyMetrics, ControllerKind, EvaluationSuite, PolicyComparisonProfile,
    PolicyComparisonReport, RunReport, SpacewarsPreset, run_batch, run_policy_comparison,
    run_suite, verify_suite_baseline,
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
            "player_2",
            "compare",
            "baseline",
            "candidate"
        ]
    )]
    suite: Option<SuiteArg>,

    /// Require a named suite to match its checked-in deterministic baseline.
    #[arg(long, requires = "suite", conflicts_with = "compare")]
    verify: bool,

    /// Run a fixed workload with baseline/candidate seats swapped per seed.
    #[arg(
        long,
        value_enum,
        requires = "candidate",
        conflicts_with_all = [
            "suite",
            "verify",
            "scenario",
            "preset",
            "seed",
            "seed_step",
            "episodes",
            "max_ticks",
            "player_1",
            "player_2",
            "trace_player"
        ]
    )]
    compare: Option<ComparisonArg>,

    /// Established controller used as the comparison baseline.
    #[arg(long, value_enum, requires = "compare")]
    baseline: Option<ControllerArg>,

    /// New controller being evaluated.
    #[arg(long, value_enum, requires = "compare")]
    candidate: Option<ControllerArg>,

    /// Override the comparison profile's first seed.
    #[arg(long, requires = "compare")]
    comparison_start_seed: Option<u64>,

    /// Override the comparison profile's number of paired seeds.
    #[arg(long, requires = "compare", value_parser = clap::value_parser!(u32).range(1..))]
    comparison_episodes: Option<u32>,

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
    #[arg(long, value_enum, default_value = "rule-v5")]
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
    StrategyV1,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ComparisonArg {
    NavigationV1,
    StrategyV1,
}

impl From<ComparisonArg> for PolicyComparisonProfile {
    fn from(value: ComparisonArg) -> Self {
        match value {
            ComparisonArg::NavigationV1 => Self::NavigationV1,
            ComparisonArg::StrategyV1 => Self::StrategyV1,
        }
    }
}

impl From<SuiteArg> for EvaluationSuite {
    fn from(value: SuiteArg) -> Self {
        match value {
            SuiteArg::NavigationV1 => Self::NavigationV1,
            SuiteArg::StrategyV1 => Self::StrategyV1,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ControllerArg {
    Idle,
    #[value(name = "rule-v5", alias = "rule")]
    RuleV5,
    #[value(name = "rule-v6")]
    RuleV6,
    #[value(name = "rule-v7")]
    RuleV7,
}

impl From<ControllerArg> for ControllerKind {
    fn from(value: ControllerArg) -> Self {
        match value {
            ControllerArg::Idle => Self::Idle,
            ControllerArg::RuleV5 => Self::RuleV5,
            ControllerArg::RuleV6 => Self::RuleV6,
            ControllerArg::RuleV7 => Self::RuleV7,
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
    if let Some(comparison) = args.compare {
        let baseline = args.baseline.unwrap_or(ControllerArg::RuleV5).into();
        let candidate = args
            .candidate
            .expect("clap requires a candidate when comparison is selected")
            .into();
        let profile = PolicyComparisonProfile::from(comparison);
        let mut config = profile.config(baseline, candidate);
        if let Some(seed) = args.comparison_start_seed {
            config.start_seed = seed;
        }
        if let Some(episodes) = args.comparison_episodes {
            config.episodes = episodes;
        }
        let report = run_policy_comparison(config)?;
        match args.output {
            OutputArg::Text => print_policy_comparison_report(&report),
            OutputArg::Json => {
                let stdout = io::stdout();
                serde_json::to_writer_pretty(stdout.lock(), &report)?;
                println!();
            }
        }
        return Ok(());
    }

    let suite = args.suite.map(EvaluationSuite::from);
    let report = if let Some(suite) = suite {
        run_suite(suite)?
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

    let verification = if args.verify {
        Some(verify_suite_baseline(
            suite.expect("clap requires a suite when verification is selected"),
            &report,
        )?)
    } else {
        None
    };

    match args.output {
        OutputArg::Text => print_text_report(&report),
        OutputArg::Json => {
            let stdout = io::stdout();
            serde_json::to_writer_pretty(stdout.lock(), &report)?;
            println!();
        }
    }
    if let Some(verification) = verification {
        eprintln!(
            "baseline={} status=match episodes={}",
            verification.suite_id, verification.episodes
        );
    }
    Ok(())
}

fn print_policy_comparison_report(report: &PolicyComparisonReport) {
    print_text_report(&report.run);
    println!(
        "comparison={} seed-pairs={} episode-runs={}",
        report.workload_id.unwrap_or("custom"),
        report.summary.seed_pairs,
        report.summary.episode_runs,
    );
    print_compared_policy("baseline", &report.summary.baseline);
    print_compared_policy("candidate", &report.summary.candidate);
}

fn print_compared_policy(role: &str, metrics: &ComparedPolicyMetrics) {
    println!(
        "{role}={} wins={} tick-limits={} captures={} losses={} losses(planet/sun)={}/{} rebuilds={} contacts(body/ship)={}/{} docks={} departures(port/safe-capture/safe-rebuild)={}/{}/{} planets-sum={} health={:?} strategy={:?}",
        metrics.controller.policy_id,
        metrics.wins,
        metrics.tick_limits,
        metrics.captures,
        metrics.ship_losses,
        metrics.planet_impact_losses,
        metrics.sun_impact_losses,
        metrics.rebuilds,
        metrics.body_contacts,
        metrics.ship_contacts,
        metrics.port_dockings,
        metrics.port_departures,
        metrics.safe_capture_departures,
        metrics.safe_rebuild_departures,
        metrics.final_planet_count_sum,
        metrics.health,
        metrics.strategy,
    );
}

fn print_text_report(report: &RunReport) {
    if let Some(suite_id) = report.suite_id {
        println!("suite={suite_id}");
    }
    for episode in &report.episodes {
        println!(
            "seed={} controllers={:?} outcome={} ticks={} sim={:.1}s captures={:?} losses={:?} losses(planet/sun)={:?}/{:?} rebuilds={:?} contacts(body/ship)={:?}/{:?} impacts(debris/laser)={:?}/{:?} docks={:?} departures(port/safe-capture/safe-rebuild)={:?}/{:?}/{:?} planets={:?} health={:?} strategy={:?} actions={} trace={}",
            episode.seed,
            episode.controllers,
            episode.outcome,
            episode.ticks,
            episode.simulated_seconds,
            episode.captures,
            episode.ship_losses,
            episode.planet_impact_losses,
            episode.sun_impact_losses,
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
            episode.health,
            episode.strategy,
            episode.actions_emitted,
            episode
                .trace_sha256
                .get(..12)
                .unwrap_or(&episode.trace_sha256),
        );
        for event in &episode.navigation_trace {
            println!(
                "  nav tick={} p={} reason={:?} strategy={:?} strategy-target={:?}/{:?} strategy-score={} strategy-reason={:?} goal={:?} avoid={:?} avoid-clearance={} avoid-outward={} predictive={} avoid-closest={} avoid-predicted-clearance={} avoid-age={}/{} assist={}/{} phase={:?} target={:?} attempt(age/stalled/obstructed)={}/{}/{} replans={} cooldown={:?}/{} multi(active/age/bodies/activations)={}/{}/{}/{} docked={:?} pending={:?} age={} focus={:?} clearance={} outward={} port-distance={} port-omega={} velocity=({:.1},{:.1}) speed={:.1} rotation={:.2} port-rotation={} omega(control/measured)={:.2}/{:.2} heading={:.2} desired={:.1} relative={:.1} contact(body/ship)={}/{} intent(turn={:.2} thrust={:.1} brake={:.1} wings={} laser={} cannon={})",
                event.tick,
                event.player.index() + 1,
                event.reasons,
                event.strategy.goal,
                event.strategy.target,
                event.strategy.target_planet,
                optional_f32(event.strategy.selected_score),
                event.strategy.selection_reason,
                event.goal,
                event.avoided_body,
                optional_f32(event.avoidance_surface_clearance),
                optional_f32(event.avoidance_outward_speed),
                event.avoidance_predictive,
                optional_f32(event.avoidance_seconds_until_closest),
                optional_f32(event.avoidance_predicted_surface_clearance),
                event.avoidance_age_ticks,
                event.avoidance_stalled_ticks,
                event.avoidance_escape_assist,
                event.avoidance_emergency_escape_assist,
                event.port_phase,
                event.target_planet,
                event.port_attempt_age_ticks,
                event.port_attempt_stalled_ticks,
                event.port_attempt_obstructed_ticks,
                event.port_replan_count,
                event.cooled_port_planet,
                event.port_cooldown_remaining_ticks,
                event.multi_body_escape_active,
                event.multi_body_escape_age_ticks,
                event.multi_body_escape_body_count,
                event.multi_body_escape_activations,
                event.docked_planet,
                event.pending_capture_planet,
                optional_u64(event.pending_capture_ticks),
                event.focus_planet,
                optional_f32(event.surface_clearance),
                optional_f32(event.outward_speed),
                optional_f32(event.spaceport_distance),
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
        "episodes={} winners={:?} tick_limits={} ticks={} wall={:.3}s ticks/s={:.0} realtime={:.1}x captures={:?} losses={:?} losses(planet/sun)={:?}/{:?} rebuilds={:?} contacts(body/ship)={:?}/{:?} impacts(debris/laser)={:?}/{:?} docks={:?} departures(port/safe-capture/safe-rebuild)={:?}/{:?}/{:?} health={:?} strategy={:?} policies={:?}",
        summary.episodes,
        summary.winner_counts,
        summary.tick_limits,
        summary.total_ticks,
        summary.wall_seconds,
        summary.ticks_per_wall_second,
        summary.simulated_seconds_per_wall_second,
        summary.captures,
        summary.ship_losses,
        summary.planet_impact_losses,
        summary.sun_impact_losses,
        summary.rebuilds,
        summary.body_contacts,
        summary.ship_contacts,
        summary.debris_impacts,
        summary.laser_hits_received,
        summary.port_dockings,
        summary.port_departures,
        summary.safe_capture_departures,
        summary.safe_rebuild_departures,
        summary.health,
        summary.strategy,
        summary.policy_metrics,
    );
}

fn optional_f32(value: Option<f32>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.1}"))
}

fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_alias_and_explicit_v5_select_the_same_concrete_controller() {
        for value in ["rule", "rule-v5"] {
            let args = Args::try_parse_from(["engine-agent", "--player-2", value]).unwrap();

            assert!(matches!(args.player_2, ControllerArg::RuleV5));
            assert_eq!(ControllerKind::from(args.player_2), ControllerKind::RuleV5);
        }
    }

    #[test]
    fn comparison_cli_captures_explicit_roles_and_profile() {
        let args = Args::try_parse_from([
            "engine-agent",
            "--compare",
            "strategy-v1",
            "--baseline",
            "rule-v5",
            "--candidate",
            "rule-v6",
            "--comparison-start-seed",
            "100",
            "--comparison-episodes",
            "20",
        ])
        .unwrap();

        assert!(matches!(args.compare, Some(ComparisonArg::StrategyV1)));
        assert!(matches!(args.baseline, Some(ControllerArg::RuleV5)));
        assert!(matches!(args.candidate, Some(ControllerArg::RuleV6)));
        assert_eq!(args.comparison_start_seed, Some(100));
        assert_eq!(args.comparison_episodes, Some(20));
    }

    #[test]
    fn navigation_comparison_and_explicit_v6_are_selectable() {
        let args = Args::try_parse_from([
            "engine-agent",
            "--compare",
            "navigation-v1",
            "--candidate",
            "rule-v6",
        ])
        .unwrap();

        assert!(matches!(args.compare, Some(ComparisonArg::NavigationV1)));
        assert!(matches!(args.candidate, Some(ControllerArg::RuleV6)));
        assert_eq!(
            ControllerKind::from(args.candidate.unwrap()),
            ControllerKind::RuleV6
        );
    }

    #[test]
    fn explicit_v7_is_selectable() {
        let args = Args::try_parse_from(["engine-agent", "--player-2", "rule-v7"]).unwrap();

        assert!(matches!(args.player_2, ControllerArg::RuleV7));
        assert_eq!(ControllerKind::from(args.player_2), ControllerKind::RuleV7);
    }

    #[test]
    fn comparison_cli_requires_a_candidate() {
        let error = Args::try_parse_from(["engine-agent", "--compare", "strategy-v1"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }
}
