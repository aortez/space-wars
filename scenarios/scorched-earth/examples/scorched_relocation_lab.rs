//! Same-state incoming-hit continuations with unchanged opponent combat policy.
use clap::Parser;
use engine_rapier::terrain::GrainShape;
use scenario_scorched_earth::{
    BankStrength, CpuPolicy, ScorchedConfig, bot_lab::compare_relocation,
};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "angular", value_parser = ["round", "angular"])]
    shape: String,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=20))]
    opportunity: u32,
    #[arg(long, default_value_t = 45, value_parser = clap::value_parser!(u32).range(1..=120))]
    search_seconds: u32,
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=60))]
    seconds: u32,
    #[arg(long, default_value = "off", value_parser = ["off", "weak", "strong"])]
    cohesion: String,
    #[arg(long)]
    replay: bool,
}

fn main() {
    let args = Args::parse();
    let config = ScorchedConfig {
        shape: if args.shape == "round" {
            GrainShape::Round
        } else {
            GrainShape::Hexagon
        },
        demo: true,
        cpu_driving: true,
        cpu_footing: [true; 2],
        cpu_policy: [CpuPolicy::Reactive; 2],
        suspension: true,
        slumping: args.cohesion != "off",
        breakup: args.cohesion != "off",
        cohesion: match args.cohesion.as_str() {
            "weak" => BankStrength::Weak,
            "strong" => BankStrength::Strong,
            _ => BankStrength::Off,
        },
        ..Default::default()
    };
    let run = || {
        compare_relocation(
            config,
            args.seed,
            args.opportunity,
            args.search_seconds,
            args.seconds,
        )
    };
    let mut report = run();
    if args.replay {
        assert_eq!(report, run());
    }
    report["replay_checked"] = args.replay.into();
    println!("{report}");
}
