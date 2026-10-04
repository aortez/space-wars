//! Recorded material experiments and a standalone ARM benchmark.
use std::{
    error::Error,
    fs,
    path::PathBuf,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use scenario_terrain_lab::soil_lab::{DT, Experiment, Fixture, LabConfig, Metrics, Model, SoilLab};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(about = "Pour, undermine, and blast soil on flat and moving planetary ground")]
struct Args {
    /// New directory. Existing results are never overwritten.
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=30))]
    seconds: u32,
    #[arg(long, default_value = "all", value_parser = ["all", "mpm", "frictionless", "grains"])]
    model: String,
    #[arg(long, default_value = "all", value_parser = ["all", "flat", "moving-planet"])]
    fixture: String,
    #[arg(long, default_value = "all", value_parser = ["all", "pour", "bank", "blasts"])]
    experiment: String,
    /// Timings and every-frame audits only; omit playback geometry and replay.
    #[arg(long)]
    benchmark: bool,
    /// Human-readable build identifier for a binary copied away from its checkout.
    #[arg(long)]
    build_label: Option<String>,
}

fn metric(m: Metrics) -> Value {
    json!({"particles":m.particles,"mass":m.mass,"reservoir_mass":m.reservoir_mass,
        "rms_speed":m.rms_speed,"height":m.height,"width":m.width,
        "below_platform_mass":m.below_platform_mass,"center_height":m.center_height,
        "kinetic_energy":m.kinetic_energy})
}

fn frame(lab: &SoilLab, m: Metrics) -> Value {
    let points: Vec<_> = lab
        .points()
        .into_iter()
        .flat_map(|p| [p.position.x, p.position.y])
        .map(|v| (v * 10000.0).round() / 10000.0)
        .collect();
    let (center, angle) = lab.config.fixture.pose(lab.tick as f64 * DT);
    json!({"tick":lab.tick,"points":points,"metrics":metric(m),"center":[center.x,center.y],
        "angle":angle,"support_removed":lab.ground.support_removed})
}

fn percentile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn command(args: &[&str]) -> Option<String> {
    let output = Command::new(args[0]).args(&args[1..]).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run(config: LabConfig, args: &Args) -> Result<Value, Box<dyn Error>> {
    let mut lab = SoilLab::new(config)?;
    let initial = lab.audit()?;
    let mut frames = if args.benchmark {
        Vec::new()
    } else {
        vec![frame(&lab, initial)]
    };
    let mut hashes = if args.benchmark {
        Vec::new()
    } else {
        vec![lab.state_hash()]
    };
    let mut ms = Vec::new();
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut substeps = 0;
    let mut max_substeps = 0;
    let mut max_nodes = 0;
    let mut yields = 0;
    let mut corrections = 0;
    let mut max_correction = 0.0_f32;
    for _ in 0..args.seconds * 60 {
        let start = Instant::now();
        lab.step().map_err(|e| {
            format!(
                "{} / {} / {} tick {}: {e}",
                config.fixture.name(),
                config.experiment.name(),
                config.model.name(),
                lab.tick
            )
        })?;
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let audit = lab.audit()?;
        substeps += lab.last_step.substeps;
        max_substeps = max_substeps.max(lab.last_step.substeps);
        max_nodes = max_nodes.max(lab.last_step.peak_active_nodes);
        yields += lab.last_step.yield_events;
        corrections += lab.last_step.contact_corrections;
        max_correction = max_correction.max(lab.last_step.max_correction);
        if lab.last_blast_hits > 0 {
            events.push(json!({"tick":lab.tick-1,"hits":lab.last_blast_hits,"center":lab.ground.blast_center((lab.tick-1) as f64 * DT)}));
        }
        if lab.tick % 60 == 0 || lab.tick == 119 || lab.tick == 299 {
            samples.push(json!({"tick":lab.tick,"metrics":metric(audit)}));
        }
        if !args.benchmark {
            hashes.push(lab.state_hash());
            if lab.tick % 3 == 0 {
                frames.push(frame(&lab, audit));
            }
        }
    }
    let mut replay_passed = false;
    if !args.benchmark {
        let mut replay = SoilLab::new(config)?;
        if replay.state_hash() != hashes[0] {
            return Err("initial replay mismatch".into());
        }
        for hash in hashes.iter().skip(1) {
            replay.step()?;
            if replay.state_hash() != *hash {
                return Err(format!(
                    "replay mismatch at {} / {} / {} tick {}",
                    config.fixture.name(),
                    config.experiment.name(),
                    config.model.name(),
                    replay.tick
                )
                .into());
            }
        }
        replay_passed = true;
    }
    let final_metrics = lab.audit()?;
    let materials: Vec<_> = lab.points().iter().map(|p| p.material.0).collect();
    let hashes: Vec<_> = hashes.iter().map(|h| format!("{h:016x}")).collect();
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    let p95 = percentile(&ms, 0.95);
    println!(
        "{} / {} / {}: n={} rms={:.4} height={:.3} width={:.3} center={:.3}; mean={mean:.3}ms p95={p95:.3}ms, corrections={corrections}, replay={replay_passed}",
        config.fixture.name(),
        config.experiment.name(),
        config.model.name(),
        final_metrics.particles,
        final_metrics.rms_speed,
        final_metrics.height,
        final_metrics.width,
        final_metrics.center_height
    );
    Ok(
        json!({"fixture":config.fixture.name(),"experiment":config.experiment.name(),"model":config.model.name(),
        "seed":config.seed,"initial":metric(initial),"final":metric(final_metrics),"frames":frames,"materials":materials,
        "events":events,"samples":samples,"replay_passed":replay_passed,"hashes":hashes,
        "step_ms":{"mean":mean,"p50":percentile(&ms,0.5),"p95":p95,"max":percentile(&ms,1.0),"all":ms},
        "solver":{"substeps":substeps,"max_substeps":max_substeps,"peak_active_nodes":max_nodes,
        "yield_events":yields,"contact_corrections":corrections,"max_correction":max_correction}}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("target/soil-mpm-lab/{timestamp}")));
    if output.exists() {
        return Err(format!("output already exists: {}", output.display()).into());
    }
    let build = json!({"label":args.build_label,"revision":command(&["git","rev-parse","HEAD"]),
        "status":command(&["git","status","--short"]),"arch":std::env::consts::ARCH,"os":std::env::consts::OS,
        "executable_sha256":format!("{:x}",Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "machine":command(&["uname","-a"]),"cpu_model":fs::read_to_string("/proc/device-tree/model").ok().map(|s|s.trim_end_matches('\0').to_owned()),
        "temperature_before_millidegrees":fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok(),
        "cpu_khz_before":fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").ok(),
        "load_before":fs::read_to_string("/proc/loadavg").ok()});
    let mut cases = Vec::new();
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        if args.fixture != "all" && args.fixture != fixture.name() {
            continue;
        }
        for experiment in [Experiment::Pour, Experiment::Bank, Experiment::Blasts] {
            if args.experiment != "all" && args.experiment != experiment.name() {
                continue;
            }
            for model in [Model::Mpm, Model::Frictionless, Model::Grains] {
                if args.model != "all" && args.model != model.name() {
                    continue;
                }
                cases.push(run(
                    LabConfig {
                        fixture,
                        experiment,
                        model,
                        seed: args.seed,
                    },
                    &args,
                )?);
            }
        }
    }
    let report = json!({"schema":1,"created_unix":timestamp,"build":build,"seconds":args.seconds,"seed":args.seed,
        "benchmark_only":args.benchmark,"timing_scope":"emission, events, environment preparation and solver; excludes audit, hash, recording, replay, rendering",
        "temperature_after_millidegrees":fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok(),"load_after":fs::read_to_string("/proc/loadavg").ok(),
        "parameters":{"grid_cell_size":0.25,"particle_spacing":0.125,"young_modulus":2000,"density":1,"poisson_ratio":0.2,
        "friction_angle_degrees":35,"surface_friction":0.6,"blast_radius":1.5,"blast_speed":12,"blast_ticks":[120,300],"support_tick":120},
        "cases":cases});
    let data = serde_json::to_string(&report)?;
    fs::create_dir_all(output.parent().ok_or("missing output parent")?)?;
    fs::create_dir(&output)?;
    fs::write(output.join("report.json"), &data)?;
    if !args.benchmark {
        fs::write(
            output.join("report.html"),
            include_str!("soil_mpm_lab/report.html")
                .replace("__REPORT_DATA__", &data.replace('<', "\\u003c")),
        )?;
    }
    println!("Saved {}", output.display());
    Ok(())
}
