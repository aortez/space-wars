//! Replayable native-sandbox workload; timings exclude replay and rendering to
//! pixels. Use spacewars-cli status for actual application/display performance.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use clap::{Parser, ValueEnum};
use engine_common::{Action, Scenario};
use scenario_terrain_lab::{
    blast_lab::Fixture,
    granular_lab::{
        Command, GrainPreset, GranularLabAction, GranularLabConfig, GranularLabScenario,
        GranularLabState,
    },
};
use serde_json::{Value, json};

#[derive(Clone, Copy, ValueEnum)]
enum Ground {
    All,
    Flat,
    Slope,
    MovingPlanet,
}
#[derive(Clone, Copy, ValueEnum)]
enum Preset {
    All,
    Round,
    Grippy,
    Angular,
}

#[derive(Parser)]
struct Args {
    #[arg(long, value_enum, default_value = "all")]
    fixture: Ground,
    #[arg(long, value_enum, default_value = "all")]
    preset: Preset,
    #[arg(long, default_value_t = 192, value_parser = clap::value_parser!(u32).range(1..=512))]
    limit: u32,
    #[arg(long, default_value_t = 0.5)]
    cell_size: f32,
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u32).range(480..=3600))]
    ticks: u32,
    #[arg(long)]
    verify_replay: bool,
}

fn stats(mut ms: Vec<f64>) -> Value {
    ms.sort_by(f64::total_cmp);
    json!({"samples":ms.len(), "mean_ms":ms.iter().sum::<f64>()/ms.len() as f64,
        "p95_ms":ms[(ms.len()*95/100).min(ms.len()-1)], "max_ms":ms[ms.len()-1]})
}

fn run(config: GranularLabConfig, ticks: u32, verify: bool) -> Value {
    let mut state = GranularLabScenario::init(config, 42);
    let dt = Duration::from_secs_f64(1.0 / 60.0);
    let mut commands = Vec::<Vec<Action>>::new();
    let mut hashes = Vec::new();
    let mut steps = Vec::new();
    let mut events = Vec::new();
    let mut frames = Vec::new();
    let mut max_bodies = 0;
    let mut box_supported_frames = 0;
    for tick in 0..ticks {
        let actions = match tick {
            0 => vec![GranularLabAction::Command(Command::Fire).encode()],
            180 => vec![GranularLabAction::Command(Command::Drop).encode()],
            360 => {
                let position = state.lab.probe_snapshot().unwrap().motion.position;
                vec![
                    GranularLabAction::Aim(position - state.lab.up_at(position) * 1.0).encode(),
                    GranularLabAction::Command(Command::Fire).encode(),
                ]
            }
            450 => vec![
                GranularLabAction::Aim(state.lab.surface_point(6.0, 0.75)).encode(),
                GranularLabAction::Command(Command::Fire).encode(),
            ],
            _ => vec![],
        };
        let start = Instant::now();
        GranularLabScenario::step(&mut state, &actions, dt);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if actions.is_empty() {
            steps.push(elapsed);
        } else {
            events.push(elapsed);
        }
        let start = Instant::now();
        black_box(GranularLabScenario::render_frame(&state));
        frames.push(start.elapsed().as_secs_f64() * 1000.0);
        max_bodies = max_bodies.max(state.lab.loose_body_count());
        if state
            .lab
            .probe_snapshot()
            .is_some_and(|p| p.grain_contacts > 0 && p.relative_speed < 0.2)
        {
            box_supported_frames += 1;
        }
        state
            .lab
            .audit()
            .expect("conserved material, finite motion and matching colliders");
        if verify {
            commands.push(actions);
            hashes.push(GranularLabScenario::observe(&state).payload);
        }
    }
    if verify {
        let mut replay = GranularLabState::new(config, 42);
        for (actions, hash) in commands.iter().zip(hashes) {
            GranularLabScenario::step(&mut replay, actions, dt);
            assert_eq!(
                GranularLabScenario::observe(&replay).payload,
                hash,
                "same-build replay"
            );
        }
    }
    let balance = state.lab.audit().unwrap();
    json!({"fixture":config.fixture.name(),"preset":config.preset.name(),"cell_size":config.cell_size,
        "body_limit":config.max_loose_bodies,"ticks":ticks,"peak_loose_bodies":max_bodies,
        "blasts":state.lab.blasts,"rejected_blasts":state.lab.rejected_blasts,
        "box_supported_frames":box_supported_frames,"ground_cells":balance.ground,
        "loose_cells":balance.loose,"conserved_cells":balance.initial,"removed_cells":balance.removed,
        "native_update":stats(steps),"event_update":stats(events),"frame_construction":stats(frames),
        "same_build_replay_verified":verify,"final_hash":format!("{:016x}",state.lab.content_motion_hash())})
}

fn main() {
    let args = Args::parse();
    assert!(
        args.cell_size.is_finite() && (0.25..=1.0).contains(&args.cell_size),
        "cell size must be in 0.25..=1"
    );
    let fixtures = match args.fixture {
        Ground::All => vec![Fixture::Flat, Fixture::Slope, Fixture::MovingPlanet],
        Ground::Flat => vec![Fixture::Flat],
        Ground::Slope => vec![Fixture::Slope],
        Ground::MovingPlanet => vec![Fixture::MovingPlanet],
    };
    let presets = match args.preset {
        Preset::All => vec![
            GrainPreset::Round,
            GrainPreset::Grippy,
            GrainPreset::Angular,
        ],
        Preset::Round => vec![GrainPreset::Round],
        Preset::Grippy => vec![GrainPreset::Grippy],
        Preset::Angular => vec![GrainPreset::Angular],
    };
    let mut cases = Vec::new();
    for fixture in fixtures {
        for &preset in &presets {
            eprintln!(
                "{} / {} / cells {} / limit {}",
                fixture.name(),
                preset.name(),
                args.cell_size,
                args.limit
            );
            cases.push(run(
                GranularLabConfig {
                    fixture,
                    preset,
                    cell_size: args.cell_size,
                    max_loose_bodies: args.limit as usize,
                },
                args.ticks,
                args.verify_replay,
            ));
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"schema":1,"architecture":std::env::consts::ARCH,
        "fixed_hz":60,"seed":42,"timing_scope":"native scenario update and RenderFrame construction; excludes pixels, host, input transport, replay and post-step audits",
        "workload":"blast at 0; box at 180; blast below box at 360; fresh ground at 450", "cases":cases})).unwrap());
}
