use std::{
    env,
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use engine_core::Vec2;
use engine_gravity::{
    GravityBackend, GravityConfig, GravityErrorMetrics, GravityId, GravityParticipant,
    GravitySolver, GravityStepMetrics, compare_outputs,
};

const DEFAULT_SIZES: &[usize] = &[300, 1_000, 5_000, 10_000];
const DEFAULT_SCENARIOS: &[Scenario] = &[Scenario::Jittered, Scenario::Clustered];
const DEFAULT_SEED: u32 = 0x00C0_FFEE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Jittered,
    Uniform,
    Clustered,
    Coincident,
}

impl Scenario {
    const fn label(self) -> &'static str {
        match self {
            Self::Jittered => "jittered",
            Self::Uniform => "uniform",
            Self::Clustered => "clustered",
            Self::Coincident => "coincident",
        }
    }

    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "jittered" => Ok(Self::Jittered),
            "uniform" => Ok(Self::Uniform),
            "clustered" => Ok(Self::Clustered),
            "coincident" => Ok(Self::Coincident),
            _ => Err(CliError(format!(
                "unknown scenario {value:?}; expected jittered, uniform, clustered, or coincident"
            ))),
        }
    }
}

#[derive(Debug)]
struct Options {
    sizes: Vec<usize>,
    scenarios: Vec<Scenario>,
    samples: usize,
    warmup: usize,
    theta: f32,
    oracle_limit: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            sizes: DEFAULT_SIZES.to_vec(),
            scenarios: DEFAULT_SCENARIOS.to_vec(),
            samples: 20,
            warmup: 5,
            theta: 0.7,
            oracle_limit: 1_000,
        }
    }
}

#[derive(Debug)]
struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

#[derive(Debug, Clone, Copy, Default)]
struct Sample {
    total_ms: f64,
    build_ms: f64,
    aggregate_ms: f64,
    traverse_ms: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Summary {
    median: f64,
    p95: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let Some(options) = parse_options(env::args().skip(1).collect())? else {
        return Ok(());
    };
    println!(
        "scenario,bodies,theta,samples,total_median_ms,total_p95_ms,build_median_ms,aggregate_median_ms,traverse_median_ms,nodes,exact_sources,approximations,applied_sources,normalized_rms,p95_relative,maximum_relative"
    );
    for &scenario in &options.scenarios {
        for &body_count in &options.sizes {
            run_case(&options, scenario, body_count)?;
        }
    }
    Ok(())
}

fn run_case(
    options: &Options,
    scenario: Scenario,
    body_count: usize,
) -> Result<(), Box<dyn Error>> {
    let participants = create_scenario(scenario, body_count, DEFAULT_SEED);
    let config = GravityConfig {
        backend: GravityBackend::BarnesHut {
            theta: options.theta,
        },
        softening: 1.0e-6,
        interaction_scale: 0.0005,
    };
    let mut solver = GravitySolver::new();
    solver.reserve(body_count, body_count.saturating_mul(3));
    for _ in 0..options.warmup {
        solver.solve(&participants, config)?;
    }

    let mut samples = Vec::with_capacity(options.samples);
    let mut final_metrics = GravityStepMetrics::default();
    for _ in 0..options.samples {
        let started = Instant::now();
        solver.solve(&participants, config)?;
        let total = started.elapsed();
        final_metrics = solver.metrics();
        samples.push(Sample {
            total_ms: milliseconds(total),
            build_ms: milliseconds(final_metrics.build_time),
            aggregate_ms: milliseconds(final_metrics.aggregation_time),
            traverse_ms: milliseconds(final_metrics.traversal_time),
        });
    }

    let error = if body_count <= options.oracle_limit {
        let candidate = solver.solve(&participants, config)?.to_vec();
        let mut exact = GravitySolver::new();
        let reference = exact
            .solve(
                &participants,
                GravityConfig {
                    backend: GravityBackend::Exact,
                    ..config
                },
            )?
            .to_vec();
        compare_outputs(&reference, &candidate)?
    } else {
        GravityErrorMetrics {
            normalized_rms: f64::NAN,
            p95_relative: f64::NAN,
            maximum_relative: f64::NAN,
        }
    };

    let total = summarize(samples.iter().map(|sample| sample.total_ms));
    let build = summarize(samples.iter().map(|sample| sample.build_ms));
    let aggregate = summarize(samples.iter().map(|sample| sample.aggregate_ms));
    let traverse = summarize(samples.iter().map(|sample| sample.traverse_ms));
    println!(
        "{},{},{:.3},{},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{:.9},{:.9},{:.9}",
        scenario.label(),
        body_count,
        options.theta,
        options.samples,
        total.median,
        total.p95,
        build.median,
        aggregate.median,
        traverse.median,
        final_metrics.node_count,
        final_metrics.exact_interactions,
        final_metrics.approximations,
        final_metrics.applied_sources,
        error.normalized_rms,
        error.p95_relative,
        error.maximum_relative,
    );
    Ok(())
}

fn summarize(values: impl Iterator<Item = f64>) -> Summary {
    let mut values = values.collect::<Vec<_>>();
    values.sort_unstable_by(f64::total_cmp);
    Summary {
        median: percentile(&values, 0.5),
        p95: percentile(&values, 0.95),
    }
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let index = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn parse_options(arguments: Vec<String>) -> Result<Option<Options>, CliError> {
    let mut options = Options::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "--full" => options.sizes = vec![1_000, 5_000, 10_000, 25_000, 50_000],
            "--sizes" => {
                index += 1;
                options.sizes = parse_list(
                    arguments
                        .get(index)
                        .ok_or_else(|| CliError("--sizes requires a value".into()))?,
                    "--sizes",
                    |value| {
                        value.parse::<usize>().map_err(|_| {
                            CliError(format!("--sizes contains invalid body count {value:?}"))
                        })
                    },
                )?;
                if options.sizes.contains(&0) {
                    return Err(CliError("--sizes values must be at least one".into()));
                }
                options.sizes.sort_unstable();
            }
            "--scenarios" | "--scenario" => {
                index += 1;
                options.scenarios = parse_list(
                    arguments
                        .get(index)
                        .ok_or_else(|| CliError(format!("{argument} requires a value")))?,
                    argument,
                    Scenario::parse,
                )?;
            }
            "--samples" => {
                index += 1;
                options.samples = parse_usize_argument(&arguments, index, "--samples")?;
                if options.samples == 0 {
                    return Err(CliError("--samples must be at least one".into()));
                }
            }
            "--warmup" => {
                index += 1;
                options.warmup = parse_usize_argument(&arguments, index, "--warmup")?;
            }
            "--theta" => {
                index += 1;
                options.theta = arguments
                    .get(index)
                    .ok_or_else(|| CliError("--theta requires a value".into()))?
                    .parse()
                    .map_err(|_| {
                        CliError("--theta requires a finite non-negative number".into())
                    })?;
                if !options.theta.is_finite() || options.theta < 0.0 {
                    return Err(CliError(
                        "--theta requires a finite non-negative number".into(),
                    ));
                }
            }
            "--oracle-limit" => {
                index += 1;
                options.oracle_limit = parse_usize_argument(&arguments, index, "--oracle-limit")?;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(None);
            }
            _ => return Err(CliError(format!("unknown option {argument:?}"))),
        }
        index += 1;
    }
    Ok(Some(options))
}

fn parse_usize_argument(
    arguments: &[String],
    index: usize,
    option: &str,
) -> Result<usize, CliError> {
    arguments
        .get(index)
        .ok_or_else(|| CliError(format!("{option} requires a value")))?
        .parse()
        .map_err(|_| CliError(format!("{option} requires a non-negative integer")))
}

fn parse_list<T>(
    value: &str,
    option: &str,
    mut parse: impl FnMut(&str) -> Result<T, CliError>,
) -> Result<Vec<T>, CliError> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(&mut parse)
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err(CliError(format!("{option} requires at least one value")));
    }
    Ok(values)
}

fn print_help() {
    println!(
        "\
Usage: cargo run --release -p engine-gravity --example gravity_benchmark -- [options]

Options:
  --full                Include 25,000 and 50,000 bodies.
  --sizes LIST          Comma-separated body counts (default: 300,1000,5000,10000).
  --scenarios LIST      jittered, uniform, clustered, coincident.
  --samples NUMBER      Recorded solves per case (default: 20).
  --warmup NUMBER       Warmup solves per case (default: 5).
  --theta NUMBER        Barnes-Hut opening angle (default: 0.7).
  --oracle-limit NUMBER Largest case checked against exact gravity (default: 1000).
  --help                Show this help.

Every recorded solve validates participants, rebuilds and aggregates the tree,
then calculates gravity for all bodies. Accuracy checks run outside the timed samples."
    );
}

fn create_scenario(scenario: Scenario, count: usize, seed: u32) -> Vec<GravityParticipant> {
    let mut random = Mulberry32::new(seed ^ hash_label(scenario.label()) ^ count as u32);
    let side = (count as f64).sqrt().ceil() as usize;
    let positions = match scenario {
        Scenario::Jittered => (0..count)
            .map(|index| {
                let jitter_x = (random.next_f32() - 0.5) * 0.4;
                let jitter_y = (random.next_f32() - 0.5) * 0.4;
                Vec2::new(
                    (index % side) as f32 + 0.5 + jitter_x,
                    (index / side) as f32 + 0.5 + jitter_y,
                ) / side.max(1) as f32
            })
            .collect::<Vec<_>>(),
        Scenario::Uniform => (0..count)
            .map(|_| Vec2::new(random.next_f32(), random.next_f32()))
            .collect(),
        Scenario::Clustered => {
            let centers = [
                Vec2::new(0.25, 0.25),
                Vec2::new(0.75, 0.25),
                Vec2::new(0.25, 0.75),
                Vec2::new(0.75, 0.75),
            ];
            (0..count)
                .map(|index| {
                    let center = centers[index % centers.len()];
                    Vec2::new(
                        (center.x + random.next_normal() * 0.045).clamp(0.0, 1.0),
                        (center.y + random.next_normal() * 0.045).clamp(0.0, 1.0),
                    )
                })
                .collect()
        }
        Scenario::Coincident => vec![Vec2::new(0.371, 0.619); count],
    };

    positions
        .into_iter()
        .enumerate()
        .map(|(index, position)| {
            GravityParticipant::dynamic(GravityId::new(index as u64 + 1), position, 1.0)
        })
        .collect()
}

fn hash_label(value: &str) -> u32 {
    value.bytes().fold(2_166_136_261, |mut hash, byte| {
        hash ^= u32::from(byte);
        hash.wrapping_mul(16_777_619)
    })
}

#[derive(Debug)]
struct Mulberry32 {
    state: u32,
    spare_normal: Option<f32>,
}

impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self {
            state: seed,
            spare_normal: None,
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let mut value = self.state;
        value = (value ^ (value >> 15)).wrapping_mul(value | 1);
        value ^= value.wrapping_add((value ^ (value >> 7)).wrapping_mul(value | 61));
        (value ^ (value >> 14)) as f32 / 4_294_967_296.0
    }

    fn next_normal(&mut self) -> f32 {
        if let Some(spare) = self.spare_normal.take() {
            return spare;
        }
        loop {
            let x = self.next_f32() * 2.0 - 1.0;
            let y = self.next_f32() * 2.0 - 1.0;
            let magnitude_squared = x * x + y * y;
            if magnitude_squared > 0.0 && magnitude_squared < 1.0 {
                let scale = (-2.0 * magnitude_squared.ln() / magnitude_squared).sqrt();
                self.spare_normal = Some(y * scale);
                return x * scale;
            }
        }
    }
}
