//! Reproducible flat/planetary blast comparisons with offline geometry playback.
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::PathBuf,
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use scenario_terrain_lab::{
    FIXED_HZ,
    blast_lab::{Blast, BlastLab, BlastLabConfig, BlastMode, BlastResult, Fixture, MotionStats},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    about = "Compare crater removal, rigid pieces, and rounded grains on flat, sloped, and moving planetary ground"
)]
struct Args {
    /// New output directory; defaults to target/dirt-blast-lab/<timestamp>.
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Experimental patch width in cells; not terrain processing chunk size.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(2..=8))]
    patch_cells: u32,
    #[arg(long, alias = "max-fragments", default_value_t = 128, value_parser = clap::value_parser!(u32).range(1..=512))]
    max_loose_bodies: u32,
    #[arg(long, default_value_t = 0.35)]
    friction: f32,
    /// Grain-pulse duration at 60 Hz; the total velocity-change budget stays fixed.
    #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u32).range(1..=30))]
    pulse_ticks: u32,
    #[arg(long, default_value_t = 3.0)]
    radius: f32,
    /// Maximum radial velocity change in world units per second.
    #[arg(long, default_value_t = 18.0)]
    speed: f32,
    /// Horizontal coordinate in the original ground frame, -12..12.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    x: f32,
    /// Depth below the original surface; a negative value puts the charge above it.
    #[arg(long, default_value_t = 0.5, allow_hyphen_values = true)]
    depth: f32,
    /// Seconds after initialization, before the first blast.
    #[arg(long, default_value_t = 1.0)]
    first_at: f32,
    /// Seconds after initialization, before repeating the blast at the same ground location.
    #[arg(long, default_value_t = 5.0)]
    second_at: f32,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(2..=60))]
    seconds: u32,
}

#[derive(Clone, Copy)]
struct RecordedBlast {
    tick: u64,
    blast: Blast,
    result: BlastResult,
}

#[derive(Default)]
struct Capture {
    indices: BTreeMap<String, usize>,
    shapes: Vec<Value>,
    frames: Vec<Value>,
}

fn rounded(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}

impl Capture {
    fn frame(&mut self, lab: &BlastLab) -> Result<MotionStats, Box<dyn Error>> {
        let balance = lab.audit()?;
        let motion_stats = lab.motion_stats();
        let mut poses = Vec::new();
        for (body, motion) in lab.bodies() {
            let key = format!("{}-{}", body.id.value(), body.terrain.revision());
            let index = *self.indices.entry(key).or_insert_with(|| {
                let index = self.shapes.len();
                let rects: Vec<_> = body
                    .geometry
                    .chunks()
                    .iter()
                    .flat_map(|chunk| &chunk.rectangles)
                    .map(|rect| {
                        let center = rect.local_center(&body.terrain);
                        let half = rect.half_extents(&body.terrain);
                        json!([
                            center.x,
                            center.y,
                            half.x * 2.0,
                            half.y * 2.0,
                            rect.material.0
                        ])
                    })
                    .collect();
                self.shapes
                    .push(json!({"id":body.id.value(),"rects":rects}));
                index
            });
            poses.push(json!([
                index,
                rounded(motion.position.x),
                rounded(motion.position.y),
                rounded(motion.angle)
            ]));
        }
        for (grain, motion) in lab.grains() {
            let key = format!("grain-{}", grain.id().value());
            let index = *self.indices.entry(key).or_insert_with(|| {
                let index = self.shapes.len();
                self.shapes.push(json!({"id":grain.id().value(),"rects":[],
                    "circles":[[0.0,0.0,grain.radius(),grain.cell().material.0]]}));
                index
            });
            poses.push(json!([
                index,
                rounded(motion.position.x),
                rounded(motion.position.y),
                rounded(motion.angle)
            ]));
        }
        self.frames.push(json!({"tick":lab.tick,"poses":poses,"ground":balance.ground,"loose":balance.loose,
            "removed":balance.removed,"fragments":lab.fragment_count(),"contacts":lab.last_physics.contact_pairs,
            "grains":lab.grains().count(),"loose_bodies":lab.loose_body_count(),
            "above_surface_cells":motion_stats.above_surface_cells,"supported_slow_cells":motion_stats.supported_slow_cells,
            "max_clearance":motion_stats.max_clearance,"active_pulses":lab.active_pulses(),
            "sleeping":lab.last_physics.sleeping_bodies}));
        Ok(motion_stats)
    }
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize]
}

fn run(args: &Args, fixture: Fixture, mode: BlastMode) -> Result<Value, Box<dyn Error>> {
    let config = BlastLabConfig {
        fixture,
        mode,
        seed: args.seed,
        patch_cells: args.patch_cells,
        max_loose_bodies: args.max_loose_bodies as usize,
        friction: args.friction,
        pulse_ticks: args.pulse_ticks,
        ..BlastLabConfig::default()
    };
    let mut lab = BlastLab::new(config)?;
    let mut capture = Capture::default();
    capture.frame(&lab)?;
    let initial = lab.audit()?.initial;
    let mut fingerprints = vec![lab.content_motion_hash()];
    let mut recorded = Vec::<RecordedBlast>::new();
    let mut events = Vec::new();
    let mut times = Vec::new();
    let mut capture_ms = 0.0;
    let mut peak_fragments = 0;
    let mut peak_loose_bodies = 0;
    let mut peak_contacts = 0;
    let mut peak_above_surface_cells = 0;
    let mut peak_clearance: f32 = 0.0;
    let mut failure = None;
    let ticks = args.seconds * FIXED_HZ;
    let blast_ticks =
        [args.first_at, args.second_at].map(|seconds| (seconds * FIXED_HZ as f32).round() as u64);
    for _ in 0..ticks {
        let mut blast_ms = 0.0;
        if blast_ticks.contains(&lab.tick) {
            let blast = Blast {
                center: lab.surface_point(args.x, args.depth),
                radius: args.radius,
                speed: args.speed,
            };
            let started = Instant::now();
            let result = lab.blast(blast)?;
            blast_ms = started.elapsed().as_secs_f64() * 1000.0;
            recorded.push(RecordedBlast {
                tick: lab.tick,
                blast,
                result,
            });
            events.push(json!({"tick":lab.tick,"center":[blast.center.x,blast.center.y],"radius":blast.radius,"speed":blast.speed,
                "admitted":result.admitted,"selected_cells":result.selected_cells,"loose_bodies_hit":result.loose_bodies_hit,
                "spawned_fragments":result.spawned_fragments,"accelerated_bodies":result.accelerated_bodies,
                "spawned_grains":result.spawned_grains,
                "rejection":result.rejection,
                "blast_ms":blast_ms}));
        }
        let started = Instant::now();
        lab.step();
        times.push(blast_ms + started.elapsed().as_secs_f64() * 1000.0);
        peak_fragments = peak_fragments.max(lab.fragment_count());
        peak_loose_bodies = peak_loose_bodies.max(lab.loose_body_count());
        peak_contacts = peak_contacts.max(lab.last_physics.contact_pairs);
        let started = Instant::now();
        match capture.frame(&lab) {
            Ok(stats) => {
                peak_above_surface_cells = peak_above_surface_cells.max(stats.above_surface_cells);
                peak_clearance = peak_clearance.max(stats.max_clearance);
            }
            Err(error) => {
                failure = Some(format!("tick {}: {error}", lab.tick));
                break;
            }
        }
        capture_ms += started.elapsed().as_secs_f64() * 1000.0;
        fingerprints.push(lab.content_motion_hash());
    }
    // Replay the recorded world-space shots, not a newly chosen target. Audit
    // and replay work are outside the recorded simulation-step measurements.
    if failure.is_none() {
        let mut replay = BlastLab::new(config)?;
        for (tick, expected) in fingerprints.iter().enumerate() {
            if replay.content_motion_hash() != *expected {
                failure = Some(format!("same-build replay diverged at tick {tick}"));
                break;
            }
            let replay_tick = replay.tick;
            for event in recorded.iter().filter(|event| event.tick == replay_tick) {
                if replay.blast(event.blast)? != event.result {
                    failure = Some(format!("blast replay diverged at tick {tick}"));
                    break;
                }
            }
            if failure.is_some() {
                break;
            }
            if tick + 1 < fingerprints.len() {
                replay.step();
            }
        }
    }
    println!(
        "{} / {}: {} ticks, {} peak bodies, {} peak cells above surface, p95 {:.3} ms, max {:.3} ms, {}",
        fixture.name(),
        mode.name(),
        lab.tick,
        peak_loose_bodies,
        peak_above_surface_cells,
        percentile(&times, 0.95),
        percentile(&times, 1.0),
        failure
            .as_deref()
            .unwrap_or("audits and recorded-shot replay passed")
    );
    Ok(
        json!({"fixture":fixture.name(),"mode":mode.name(),"initial":initial,"events":events,
        "shapes":capture.shapes,"frames":capture.frames,"failure":failure,"replay_passed":failure.is_none(),
        "simulation_ms":{"p50":percentile(&times,0.5),"p95":percentile(&times,0.95),"max":percentile(&times,1.0)},
        "capture_and_audit_ms":capture_ms,"peak_fragments":peak_fragments,"peak_contacts":peak_contacts,
        "peak_loose_bodies":peak_loose_bodies,"peak_above_surface_cells":peak_above_surface_cells,"peak_clearance":peak_clearance,
        "rejected_blasts":lab.rejected_blasts,"fingerprints":fingerprints.iter().map(|hash|format!("{hash:016x}")).collect::<Vec<_>>() }),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    if !args.x.is_finite()
        || !(-12.0..=12.0).contains(&args.x)
        || !args.depth.is_finite()
        || !(-2.0..=5.0).contains(&args.depth)
        || !args.radius.is_finite()
        || !(0.5..=5.0).contains(&args.radius)
        || !args.speed.is_finite()
        || !(0.0..=30.0).contains(&args.speed)
        || !args.friction.is_finite()
        || !(0.0..=1.5).contains(&args.friction)
        || !args.first_at.is_finite()
        || !args.second_at.is_finite()
        || args.first_at < 0.0
        || args.second_at <= args.first_at
        || args.second_at >= args.seconds as f32
        || (args.second_at * FIXED_HZ as f32).round() >= (args.seconds * FIXED_HZ) as f32
        || (args.first_at * FIXED_HZ as f32).round() == (args.second_at * FIXED_HZ as f32).round()
    {
        return Err("require x -12..12, depth -2..5, radius 0.5..5, speed 0..30, friction 0..1.5, and two distinct blast ticks within the run".into());
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("target/dirt-blast-lab/{stamp}")));
    if output.exists() {
        return Err(format!("output already exists: {}", output.display()).into());
    }
    fs::create_dir_all(&output)?;
    let mut cases = Vec::new();
    for fixture in [Fixture::Flat, Fixture::Slope, Fixture::MovingPlanet] {
        for mode in [
            BlastMode::Remove,
            BlastMode::Release,
            BlastMode::Grains,
            BlastMode::GrainPulse,
        ] {
            cases.push(run(&args, fixture, mode)?);
        }
    }
    let git = |arguments: &[&str]| {
        Command::new("git")
            .args(arguments)
            .output()
            .ok()
            .filter(|result| result.status.success())
            .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
    };
    let executable_sha256 = std::env::current_exe()
        .and_then(fs::read)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
    let report = json!({"schema":2,"hz":FIXED_HZ,"model":"Rigid patch and cell-sized round contact proxies; conserved cell mass/inertia and radial velocity kicks",
        "settings":{"seed":args.seed,"patch_cells":args.patch_cells,"max_loose_bodies":args.max_loose_bodies,"friction":args.friction,"pulse_ticks":args.pulse_ticks,
            "radius":args.radius,"speed":args.speed,"x":args.x,"depth":args.depth,"first_at":args.first_at,"second_at":args.second_at,"seconds":args.seconds},
        "build":{"arch":std::env::consts::ARCH,"os":std::env::consts::OS,"debug_assertions":cfg!(debug_assertions),
            "executable_sha256":executable_sha256,
            "checkout":git(&["rev-parse","HEAD"]),"working_tree":git(&["status","--porcelain"]),"command":std::env::args().collect::<Vec<_>>()},
        "cases":cases});
    let json = serde_json::to_string(&report)?;
    fs::write(output.join("report.json"), &json)?;
    fs::write(
        output.join("report.html"),
        include_str!("dirt_blast_lab/report.html")
            .replace("__DATA__", &json.replace('<', "\\u003c")),
    )?;
    println!("Open {}", output.join("report.html").display());
    if report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|case| !case["failure"].is_null())
    {
        return Err("an experiment failed; retained reports include the failure".into());
    }
    Ok(())
}
