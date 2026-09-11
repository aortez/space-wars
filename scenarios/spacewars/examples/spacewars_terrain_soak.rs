//! Repeatable endurance workloads with an offline visual report.

#[path = "terrain_soak/report.rs"]
mod report;
#[path = "terrain_soak/workload.rs"]
mod workload;

use std::{
    error::Error,
    fs::{self, File},
    hint::black_box,
    io::{BufWriter, Write},
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use engine_common::Scenario;
use scenario_spacewars::{
    SpacewarsScenario, SpacewarsState, TerrainDiagnostics, TerrainMotionAnomaly, TerrainMotionPeaks,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use workload::{Case, Workload};

const HZ: u64 = 60;
const BUDGET_MS: f64 = 1000.0 / HZ as f64;

#[derive(Debug, Parser)]
#[command(
    about = "Run Spacewars terrain for 1–180 simulated seconds per case/seed and write an offline HTML report.",
    after_help = "Runs faster than real time when possible. Timing covers simulation and four draw lists, excluding rasterization, audits, snapshots, and report writing. Workloads inject real shells and log scripted edits; they are not autonomous player matches."
)]
struct Args {
    #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(u16).range(1..=180))]
    seconds: u16,
    /// Wall-clock budget per run, checked between completed ticks.
    #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(u16).range(1..=3600))]
    wall_seconds: u16,
    #[arg(long, value_delimiter = ',', default_value = "42,1337,9001")]
    seeds: Vec<u64>,
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "cannon,excavation,fragments,multi-planet"
    )]
    cases: Vec<Case>,
    /// New output directory; an existing directory is never overwritten.
    #[arg(long)]
    output: Option<PathBuf>,
    /// A machine/build label to retain in the report.
    #[arg(long, default_value = "local")]
    label: String,
}

#[derive(Debug, Clone, Default, Serialize)]
struct Timing {
    count: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    over_budget: usize,
}

impl Timing {
    fn from_samples(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let percentile =
            |fraction: f64| sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize];
        Self {
            count: values.len(),
            p50_ms: percentile(0.5),
            p95_ms: percentile(0.95),
            p99_ms: percentile(0.99),
            max_ms: *sorted.last().unwrap(),
            over_budget: values.iter().filter(|&&v| v > BUDGET_MS).count(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Sample {
    second: f64,
    tick: u64,
    step: Timing,
    draw: Timing,
    combined: Timing,
    lifecycle: Timing,
    physics: Timing,
    contacts_peak: usize,
    active_bodies_peak: usize,
    injected_shells: u64,
    scripted_cuts: u64,
    live_ships: usize,
    material_hash: String,
    health: TerrainDiagnostics,
}

#[derive(Default)]
struct Window<'a> {
    steps: &'a [f64],
    draws: &'a [f64],
    combined: &'a [f64],
    lifecycle: &'a [f64],
    physics: &'a [f64],
    contacts_peak: usize,
    active_bodies_peak: usize,
}

#[derive(Debug, Serialize)]
struct Event {
    second: f64,
    message: String,
}

#[derive(Debug, Serialize)]
struct Snapshot {
    second: f64,
    svg: String,
    planets: Vec<[f32; 3]>,
}

#[derive(Debug, Serialize)]
struct Run {
    case: Case,
    description: &'static str,
    seed: u64,
    requested_seconds: u16,
    wall_budget_seconds: f64,
    stop_reason: Option<String>,
    completed_ticks: u64,
    initial_cells: u64,
    wall_seconds: f64,
    audit_ms: f64,
    snapshot_ms: f64,
    workload_ms: f64,
    motion_trace_ms: f64,
    motion_peaks: TerrainMotionPeaks,
    motion_anomaly: Option<TerrainMotionAnomaly>,
    peak_fragments: usize,
    peak_contacts: usize,
    step: Timing,
    draw: Timing,
    combined: Timing,
    samples: Vec<Sample>,
    snapshots: Vec<Snapshot>,
    events: Vec<Event>,
    warnings: Vec<String>,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    label: String,
    platform: String,
    profile: &'static str,
    binary_sha256: String,
    checkout_revision: Option<String>,
    checkout_dirty: Option<bool>,
    command: Vec<String>,
    created_unix_seconds: u64,
    requested_runs: usize,
    hz: u64,
    budget_ms: f64,
    runs: Vec<Run>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = Args::parse();
    args.seeds.sort_unstable();
    args.seeds.dedup();
    args.cases.dedup();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("target/terrain-soak/{now}")));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output)?;
    let checkout_revision = git(&["rev-parse", "HEAD"]);
    let checkout_dirty = git(&["status", "--porcelain"]).map(|s| !s.is_empty());
    let binary_sha256 = format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?));
    let mut report = Report {
        schema_version: 2,
        label: args.label.clone(),
        platform: format!("{} / {}", std::env::consts::ARCH, std::env::consts::OS),
        profile: if cfg!(debug_assertions) {
            "debug (not representative performance)"
        } else {
            "release"
        },
        binary_sha256,
        checkout_revision,
        checkout_dirty,
        command: std::env::args().collect(),
        created_unix_seconds: now,
        requested_runs: args.cases.len() * args.seeds.len(),
        hz: HZ,
        budget_ms: BUDGET_MS,
        runs: Vec::new(),
    };
    let mut csv = BufWriter::new(File::create(output.join("samples.csv"))?);
    writeln!(
        csv,
        "case,seed,second,tick,step_p95_ms,step_max_ms,draw_p95_ms,combined_max_ms,over_budget_ticks,fragments,terrain_colliders,physics_bodies,physics_colliders,contacts_peak,occupied_cells,removed_cells,cannon_hits,budget_skips,pending_edits,supported_bases,max_speed,max_spin,injected_shells,scripted_cuts,live_ships,material_hash,motion_hash"
    )?;
    eprintln!(
        "{} runs × {} simulated seconds; report: {}",
        report.requested_runs,
        args.seconds,
        output.join("report.html").display()
    );
    for case in &args.cases {
        for seed in &args.seeds {
            let run = run_case(
                *case,
                *seed,
                args.seconds,
                Duration::from_secs(u64::from(args.wall_seconds)),
                &mut csv,
            )?;
            eprintln!(
                "{} seed {}: {} ticks, {} errors, {} speed flags, step P95 {:.2} ms / max {:.2} ms, peak {} fragments{}",
                case.name(),
                seed,
                run.completed_ticks,
                run.errors.len(),
                run.warnings.len(),
                run.step.p95_ms,
                run.step.max_ms,
                run.peak_fragments,
                run.stop_reason
                    .as_ref()
                    .map_or(String::new(), |reason| format!("; INCOMPLETE: {reason}"))
            );
            report.runs.push(run);
            report::write(&output, &report)?;
        }
    }
    if report
        .runs
        .iter()
        .any(|run| !run.errors.is_empty() || run.stop_reason.is_some())
    {
        return Err(format!(
            "endurance runs failed or exhausted their wall budget; inspect {}",
            output.join("report.html").display()
        )
        .into());
    }
    eprintln!(
        "All runs completed and audited invariants passed; {} runs have speed flags. Open {}",
        report
            .runs
            .iter()
            .filter(|run| !run.warnings.is_empty())
            .count(),
        output.join("report.html").display()
    );
    Ok(())
}

fn git(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn run_case(
    case: Case,
    seed: u64,
    seconds: u16,
    wall_budget: Duration,
    csv: &mut impl Write,
) -> Result<Run, Box<dyn Error>> {
    let started = Instant::now();
    let (mut workload, mut state) = Workload::new(case, seed);
    state.enable_terrain_motion_trace();
    let initial = state.terrain_diagnostics();
    let mut run = Run {
        case,
        description: case.description(),
        seed,
        requested_seconds: seconds,
        wall_budget_seconds: wall_budget.as_secs_f64(),
        stop_reason: None,
        completed_ticks: 0,
        initial_cells: initial.occupied_cells,
        wall_seconds: 0.0,
        audit_ms: 0.0,
        snapshot_ms: 0.0,
        workload_ms: 0.0,
        motion_trace_ms: 0.0,
        motion_peaks: TerrainMotionPeaks::default(),
        motion_anomaly: None,
        peak_fragments: 0,
        peak_contacts: 0,
        step: Timing::default(),
        draw: Timing::default(),
        combined: Timing::default(),
        samples: Vec::new(),
        snapshots: Vec::new(),
        events: Vec::new(),
        warnings: Vec::new(),
        errors: initial.issues.clone(),
    };
    let mut steps = Vec::new();
    let mut draws = Vec::new();
    let mut combined = Vec::new();
    let mut lifecycle = Vec::new();
    let mut physics = Vec::new();
    let mut contacts_peak = 0;
    let mut active_peak = 0;
    let mut window_start = 0;
    snapshot(&state, &mut run);
    push_sample(&state, &workload, &mut run, initial, Window::default(), csv)?;
    for expected_tick in 1..=u64::from(seconds) * HZ {
        if !run.errors.is_empty() || run.stop_reason.is_some() {
            break;
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            let start = Instant::now();
            for message in workload.prepare(&mut state) {
                run.events.push(Event {
                    second: state.tick as f64 / HZ as f64,
                    message,
                });
            }
            let actions = workload.actions();
            run.workload_ms += ms(start.elapsed());
            let start = Instant::now();
            SpacewarsScenario::step(
                &mut state,
                &actions,
                Duration::from_secs_f64(1.0 / HZ as f64),
            );
            let step_ms = ms(start
                .elapsed()
                .saturating_sub(state.last_step_metrics.motion_diagnostics_time));
            let start = Instant::now();
            black_box(SpacewarsScenario::render_raster_local_play_frames(
                &state, 1.4,
            ));
            let draw_ms = ms(start.elapsed());
            (step_ms, draw_ms)
        }));
        collect_motion_diagnostics(&mut state, &mut run);
        let (step_ms, draw_ms) = match result {
            Ok(times) => times,
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic");
                run.errors.push(format!(
                    "panic while attempting tick {expected_tick}: {message}"
                ));
                break;
            }
        };
        run.completed_ticks = state.tick;
        run.motion_trace_ms += ms(state.last_step_metrics.motion_diagnostics_time);
        if state.tick != expected_tick {
            run.errors.push(format!(
                "simulation did not advance: expected tick {expected_tick}, got {}",
                state.tick
            ));
            break;
        }
        steps.push(step_ms);
        draws.push(draw_ms);
        combined.push(step_ms + draw_ms);
        lifecycle.push(ms(state.last_step_metrics.lifecycle_time));
        physics.push(ms(state.last_step_metrics.physics_time));
        contacts_peak = contacts_peak.max(state.last_step_metrics.rapier.contact_pairs);
        active_peak = active_peak.max(state.last_step_metrics.rapier.active_bodies);
        run.peak_contacts = run.peak_contacts.max(contacts_peak);
        run.peak_fragments = run.peak_fragments.max(state.terrain_fragments().count());
        if started.elapsed() >= wall_budget && state.tick < u64::from(seconds) * HZ {
            run.stop_reason = Some(format!(
                "Wall budget of {:.0} s exhausted after {:.3} simulated seconds",
                wall_budget.as_secs_f64(),
                state.tick as f64 / HZ as f64
            ));
        }
        if state.tick.is_multiple_of(HZ) || run.stop_reason.is_some() || !run.errors.is_empty() {
            let start = Instant::now();
            let health = state.terrain_diagnostics();
            if health.occupied_cells + health.removed_cells != run.initial_cells {
                run.errors.push(format!("material conservation failed at tick {}: {} remaining + {} removed != {} initial", state.tick, health.occupied_cells, health.removed_cells, run.initial_cells));
            }
            for issue in &health.issues {
                run.errors.push(format!("tick {}: {issue}", state.tick));
            }
            if run.warnings.is_empty()
                && health.max_speed > 2.0 * state.config.universe_radius as f32 * HZ as f32
            {
                let warning = format!(
                    "High-speed anomaly at {} s: {} moves at {:.0} world units/s, enough to cross the world diameter in one tick",
                    state.tick / HZ,
                    health.fastest_body.as_deref().unwrap_or("unknown body"),
                    health.max_speed
                );
                run.events.push(Event {
                    second: state.tick as f64 / HZ as f64,
                    message: warning.clone(),
                });
                run.warnings.push(warning);
            }
            run.audit_ms += ms(start.elapsed());
            push_sample(
                &state,
                &workload,
                &mut run,
                health,
                Window {
                    steps: &steps[window_start..],
                    draws: &draws[window_start..],
                    combined: &combined[window_start..],
                    lifecycle: &lifecycle[window_start..],
                    physics: &physics[window_start..],
                    contacts_peak,
                    active_bodies_peak: active_peak,
                },
                csv,
            )?;
            window_start = steps.len();
            contacts_peak = 0;
            active_peak = 0;
            if run.errors.is_empty()
                && (state.tick.is_multiple_of(HZ * 5)
                    || state.tick == u64::from(seconds) * HZ
                    || run.stop_reason.is_some())
            {
                snapshot(&state, &mut run);
                eprintln!(
                    "  {} seed {}: {}/{} s, {} fragments, {} removed cells",
                    case.name(),
                    seed,
                    state.tick / HZ,
                    seconds,
                    run.peak_fragments,
                    state.terrain_removed_cells()
                );
            }
        }
    }
    run.step = Timing::from_samples(&steps);
    run.draw = Timing::from_samples(&draws);
    run.combined = Timing::from_samples(&combined);
    run.wall_seconds = started.elapsed().as_secs_f64();
    Ok(run)
}

fn collect_motion_diagnostics(state: &mut SpacewarsState, run: &mut Run) {
    run.motion_peaks = state.terrain_motion_peaks().expect("motion trace enabled");
    if let Some(anomaly) = state.take_terrain_motion_anomaly() {
        let message = format!(
            "Motion anomaly at {:.4} s, {:?}: body {}/{} speed {:.1}; {} history frames retained in report.json",
            anomaly.tick as f64 / HZ as f64,
            anomaly.stage,
            anomaly.entity,
            anomaly.role,
            anomaly.speed,
            anomaly.history.len(),
        );
        if anomaly.non_finite {
            run.errors.push(format!("Non-finite {message}"));
        } else {
            run.warnings.push(message.clone());
        }
        run.events.push(Event {
            second: anomaly.tick as f64 / HZ as f64,
            message,
        });
        run.motion_anomaly = Some(anomaly);
    }
}

fn snapshot(state: &SpacewarsState, run: &mut Run) {
    let start = Instant::now();
    run.snapshots.push(Snapshot {
        second: state.tick as f64 / HZ as f64,
        svg: report::svg(&SpacewarsScenario::render_frame(state)),
        planets: state
            .planets
            .iter()
            .map(|p| [p.position.x, p.position.y, p.radius])
            .collect(),
    });
    run.snapshot_ms += ms(start.elapsed());
}

fn push_sample(
    state: &SpacewarsState,
    workload: &Workload,
    run: &mut Run,
    health: TerrainDiagnostics,
    window: Window<'_>,
    csv: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let material_hash = if health.issues.is_empty() {
        let hash = SpacewarsScenario::observe(state)
            .payload
            .into_iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        format!("{hash:016x}")
    } else {
        "unavailable".into()
    };
    let sample = Sample {
        second: state.tick as f64 / HZ as f64,
        tick: state.tick,
        step: Timing::from_samples(window.steps),
        draw: Timing::from_samples(window.draws),
        combined: Timing::from_samples(window.combined),
        lifecycle: Timing::from_samples(window.lifecycle),
        physics: Timing::from_samples(window.physics),
        contacts_peak: window.contacts_peak,
        active_bodies_peak: window.active_bodies_peak,
        injected_shells: workload.injected_shells,
        scripted_cuts: workload.cuts,
        live_ships: state
            .ships
            .iter()
            .filter(|s| !s.dead && s.form == scenario_spacewars::ShipForm::Ship)
            .count(),
        material_hash,
        health,
    };
    let h = &sample.health;
    writeln!(
        csv,
        "{},{},{:.3},{},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{},{},{},{},{}",
        run.case.name(),
        run.seed,
        sample.second,
        sample.tick,
        sample.step.p95_ms,
        sample.step.max_ms,
        sample.draw.p95_ms,
        sample.combined.max_ms,
        sample.combined.over_budget,
        h.fragments,
        h.terrain_colliders,
        h.physics_bodies,
        h.physics_colliders,
        sample.contacts_peak,
        h.occupied_cells,
        h.removed_cells,
        h.cannon_hits,
        h.budget_skips,
        h.pending_edits,
        h.supported_bases,
        h.max_speed,
        h.max_spin,
        sample.injected_shells,
        sample.scripted_cuts,
        sample.live_ships,
        sample.material_hash,
        h.motion_hash
    )?;
    csv.flush()?;
    if let Some(previous) = run.samples.last() {
        if previous.health.supported_bases != sample.health.supported_bases {
            run.events.push(Event {
                second: sample.second,
                message: format!(
                    "Supported bases: {} → {}",
                    previous.health.supported_bases, sample.health.supported_bases
                ),
            });
        }
        if previous.live_ships != sample.live_ships {
            run.events.push(Event {
                second: sample.second,
                message: format!(
                    "Live ships: {} → {} (scheduled shell workload continues)",
                    previous.live_ships, sample.live_ships
                ),
            });
        }
    }
    run.samples.push(sample);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_durations_outside_the_supported_simulation_window() {
        for seconds in ["0", "181"] {
            assert!(Args::try_parse_from(["soak", "--seconds", seconds]).is_err());
        }
        assert_eq!(
            Args::try_parse_from(["soak", "--seconds", "180"])
                .unwrap()
                .seconds,
            180
        );
    }

    #[test]
    fn grid_cut_and_rover_remain_bounded_through_the_original_runaway_window() {
        let run = run_case(
            Case::Fragments,
            42,
            7,
            Duration::from_secs(180),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(run.completed_ticks, 420);
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        assert!(run.motion_anomaly.is_none(), "{:?}", run.warnings);
        assert!(run.peak_fragments >= 80);
        assert!(run.motion_peaks.speed < 1_000.0, "{:?}", run.motion_peaks);
        assert!(run.samples.iter().all(|sample| sample.health.occupied_cells
            + sample.health.removed_cells
            == run.initial_cells));
    }

    #[test]
    fn motion_history_does_not_change_the_workload_motion() {
        let (mut workload, mut plain) = Workload::new(Case::Fragments, 42);
        let (mut traced_workload, mut traced) = Workload::new(Case::Fragments, 42);
        traced.enable_terrain_motion_trace();
        for _ in 0..420 {
            workload.prepare(&mut plain);
            traced_workload.prepare(&mut traced);
            let dt = Duration::from_secs_f64(1.0 / HZ as f64);
            SpacewarsScenario::step(&mut plain, &workload.actions(), dt);
            SpacewarsScenario::step(&mut traced, &traced_workload.actions(), dt);
        }
        assert_eq!(
            plain.terrain_diagnostics().motion_hash,
            traced.terrain_diagnostics().motion_hash
        );
        assert_eq!(
            SpacewarsScenario::observe(&plain).payload,
            SpacewarsScenario::observe(&traced).payload
        );
    }

    #[test]
    fn repeated_seed_keeps_material_and_motion_samples_identical() {
        let first = run_case(
            Case::Excavation,
            42,
            6,
            Duration::from_secs(180),
            &mut Vec::new(),
        )
        .unwrap();
        let second = run_case(
            Case::Excavation,
            42,
            6,
            Duration::from_secs(180),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(first.completed_ticks, 360);
        assert!(first.samples.last().unwrap().scripted_cuts > 0);
        assert!(first.peak_fragments > 0);
        assert!(first.samples.last().unwrap().health.cannon_hits > 0);
        for (a, b) in first.samples.iter().zip(&second.samples) {
            assert_eq!(a.material_hash, b.material_hash);
            assert_eq!(a.health.motion_hash, b.health.motion_hash);
        }
        assert_eq!(first.snapshots.last().unwrap().second, 6.0);
    }

    #[test]
    fn exhausted_wall_budget_preserves_partial_measurements_without_claiming_a_pass() {
        let mut csv = Vec::new();
        let run = run_case(Case::Cannon, 42, 180, Duration::ZERO, &mut csv).unwrap();
        assert!(run.errors.is_empty());
        assert!(run.stop_reason.is_some());
        assert_eq!(run.completed_ticks, 1);
        assert_eq!(run.step.count, 1);
        assert_eq!(run.samples.last().unwrap().tick, 1);
        assert_eq!(run.snapshots.last().unwrap().second, 1.0 / HZ as f64);
        assert_eq!(String::from_utf8(csv).unwrap().lines().count(), 2);
    }
}
