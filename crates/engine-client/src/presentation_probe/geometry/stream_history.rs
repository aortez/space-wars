//! Bounded queued submission on prepared production histories. Throughput
//! includes the final drain; fence-observed age is not exact GPU/display latency.
use super::{Context, Error, Pending, Stamp, save_png, stream_input as input};
use crate::{
    gpu_scene::{
        CompositePresenter, Mode,
        presentation::Plan,
        streaming::{self, Policy},
        submit_profile::balanced_order,
    },
    presentation_probe::{Options, geometry},
    render::Viewport,
};
use glow::HasContext;
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, fs::File, io::Write, time::Instant};

#[path = "stream_capture.rs"]
mod capture;

#[derive(Clone, Copy)]
struct Config {
    name: &'static str,
    mode: Mode,
    stream: Policy,
}
const CONFIGS: [Config; 4] = [
    Config {
        name: "ranges-exact",
        mode: Mode::RetainedRanges,
        stream: Policy::PassExact,
    },
    Config {
        name: "culled-exact",
        mode: Mode::RetainedCulled,
        stream: Policy::PassExact,
    },
    Config {
        name: "ranges-bucketed",
        mode: Mode::RetainedRanges,
        stream: Policy::PassBucketed,
    },
    Config {
        name: "culled-bucketed",
        mode: Mode::RetainedCulled,
        stream: Policy::PassBucketed,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Counts([u64; 9]);
impl Counts {
    fn from(output: &crate::gpu_scene::DrawOutput) -> Self {
        let s = &output.mesh;
        Self([
            s.vertices as u64,
            s.indices as u64,
            s.upload_bytes,
            s.draws,
            s.retained.admissions,
            s.retained.promotions,
            s.retained.build_work,
            s.retained.entries as u64,
            s.retained.bytes as u64,
        ])
    }
}
struct Expected {
    hash: [u8; 32],
    counts: Counts,
}
struct Sample {
    draw_ms: f64,
    encode_ms: f64,
    submit_ms: f64,
    admission_wait_ms: f64,
    observed_ms: f64,
    counts: Counts,
    streams: streaming::Stats,
    slots: usize,
    capacity: usize,
}
struct Queued {
    pending: Pending,
    frame: usize,
    began: Instant,
}
struct Block {
    samples: Vec<Sample>,
    wall_ms: f64,
    cpu_ms: f64,
    drain_ms: f64,
    peak_pending: usize,
}

fn policy_at(config: Config, step: usize, validate: bool) -> Policy {
    if validate && step == 20 {
        Policy::SharedExact
    } else {
        config.stream
    }
}

fn prewarm(context: &Context, plan: &Plan, config: Config) -> Result<CompositePresenter, Error> {
    let mut r = CompositePresenter::new(context.gl.clone())?;
    r.set_stream_policy(config.stream);
    r.draw_with_mode(plan, Mode::Triangles)?;
    unsafe {
        context.gl.finish();
    }
    r.clear_retained();
    Ok(r)
}

fn references(context: &Context, plans: &[Plan], config: Config) -> Result<Vec<Expected>, Error> {
    let mut r = prewarm(
        context,
        &plans[0],
        Config {
            stream: Policy::SharedExact,
            ..config
        },
    )?;
    let mut out = Vec::new();
    for (step, plan) in plans.iter().enumerate() {
        if step == 40 {
            r.retire();
        }
        let drawn = r.draw_with_mode(plan, config.mode)?;
        out.push(Expected {
            hash: Sha256::digest(r.readback()?).into(),
            counts: Counts::from(&drawn),
        });
        r.collect_retired();
    }
    Ok(out)
}

struct Validation<'a> {
    expected: &'a [Expected],
    captures: Vec<capture::Capture>,
    output: Option<&'a std::path::Path>,
    fixture: &'a str,
    config: Config,
    depth: usize,
    foreign: &'a super::submit_profile::ForeignStreams<'a>,
    pack: &'a capture::ForeignPack,
}

fn complete(
    queue: &mut VecDeque<Queued>,
    samples: &mut [Sample],
    plans: &[Plan],
    validation: Option<&Validation<'_>>,
) -> Result<(), Error> {
    let q = queue.pop_front().ok_or("empty stream completion queue")?;
    q.pending.wait()?;
    samples[q.frame].observed_ms = q.began.elapsed().as_secs_f64() * 1000.;
    if let Some(v) = validation {
        let plan = &plans[q.frame];
        let pixels = v.captures[q.pending.slot].pixels(plan.width, plan.height)?;
        v.pack.check()?;
        v.foreign.check(&q.pending.gl)?;
        let hash: [u8; 32] = Sha256::digest(&pixels).into();
        if hash != v.expected[q.frame].hash || samples[q.frame].counts != v.expected[q.frame].counts
        {
            return Err(format!(
                "queued image/count mismatch: {} {} depth={} frame={}",
                v.fixture, v.config.name, v.depth, q.frame
            )
            .into());
        }
        if [16, 48, 72, 79].contains(&q.frame)
            && let Some(path) = v.output
        {
            save_png(
                &path.join(format!(
                    "{}-{}-q{}-{}.png",
                    v.fixture, v.config.name, v.depth, q.frame
                )),
                plan.width,
                plan.height,
                png::ColorType::Rgba,
                &pixels,
            )?;
        }
    }
    Ok(())
}

fn block(
    context: &Context,
    plans: &[Plan],
    config: Config,
    depth: usize,
    validation: Option<Validation<'_>>,
) -> Result<Block, Error> {
    let mut r = prewarm(context, &plans[0], config)?;
    let mut samples = Vec::with_capacity(plans.len());
    let mut queue = VecDeque::with_capacity(depth);
    let mut peak_pending = 0;
    let total = Stamp::now()?;
    for (step, plan) in plans.iter().enumerate() {
        let wait = Instant::now();
        if queue.len() == depth {
            complete(&mut queue, &mut samples, plans, validation.as_ref())?;
        }
        let admission_wait_ms = wait.elapsed().as_secs_f64() * 1000.;
        let began = Instant::now();
        if validation.is_some() {
            if step == 40 {
                r.retire();
            }
            r.set_stream_policy(policy_at(config, step, true));
        }
        let output = r.draw_with_mode(plan, config.mode)?;
        let draw_ms = began.elapsed().as_secs_f64() * 1000.;
        let (slots, capacity) = r.stream_usage();
        if slots > streaming::MAX_SLOTS
            || capacity > streaming::MAX_BYTES + 36_000_000
            || output.mesh.retained.entries > 128
            || output.mesh.retained.bytes > 16 * 1024 * 1024
            || output.mesh.submission.0.iter().any(|d| !d.is_zero())
        {
            return Err("queued stream bounds or disabled-clock invariant failed".into());
        }
        if validation.is_some() && step == 23 && output.mesh.streaming.0[4] == 0 {
            return Err("queued history missed stream overflow fallback".into());
        }
        if validation.is_some() && step == 20 && slots != 0 {
            return Err("stream policy toggle retained pool".into());
        }
        let slot = step % depth;
        if let Some(v) = &validation {
            r.enqueue_readback(v.captures[slot].buffer)?;
            v.pack.check()?;
            v.foreign.check(&context.gl)?;
        }
        let fence = unsafe { context.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0)? };
        queue.push_back(Queued {
            pending: Pending {
                gl: context.gl.clone(),
                fence,
                slot,
            },
            frame: step,
            began,
        });
        unsafe {
            context.gl.flush();
        }
        peak_pending = peak_pending.max(queue.len());
        samples.push(Sample {
            draw_ms,
            encode_ms: output.encode.as_secs_f64() * 1000.,
            submit_ms: output.submit.as_secs_f64() * 1000.,
            admission_wait_ms,
            observed_ms: 0.,
            counts: Counts::from(&output),
            streams: output.mesh.streaming,
            slots,
            capacity,
        });
    }
    let drain = Instant::now();
    while !queue.is_empty() {
        complete(&mut queue, &mut samples, plans, validation.as_ref())?;
    }
    let drain_ms = drain.elapsed().as_secs_f64() * 1000.;
    let (wall_ms, cpu_ms) = total.elapsed()?;
    r.collect_retired();
    r.retire();
    r.collect_retired();
    if r.stream_usage().0 != 0 {
        return Err("queued retirement retained stream objects".into());
    }
    if unsafe { context.gl.get_error() } != glow::NO_ERROR {
        return Err("queued stream GL error".into());
    }
    Ok(Block {
        samples,
        wall_ms,
        cpu_ms,
        drain_ms,
        peak_pending,
    })
}

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let depth = options.geometry_in_flight as usize;
    let pack_bytes = input::limits(options.presentation_frames, width, height, depth)?;
    if options
        .geometry_stream_cases
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != options.geometry_stream_cases.len()
    {
        return Err("duplicate stream history fixture".into());
    }
    let depths = if depth == 1 { vec![1] } else { vec![1, depth] };
    let viewport = Viewport::new(width as f32, height as f32);
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
    }
    let metadata = serde_json::json!({"version":1,"kind":"stream-history","gl":context.identity,
        "width":width,"height":height,"frames":options.presentation_frames,"repeats":options.presentation_repeats,
        "modes":CONFIGS.map(|c|c.name),"depths":depths,"fixtures":options.geometry_stream_cases,
        "timing":"prepared plans: CPU encode/submission, bounded fence waits and final drain; no simulation/source/projection/text/UI/scanout",
        "throughput":"whole completed block elapsed divided by all frames, including cold-cache startup and final drain",
        "latency":"CPU observation of fence completion since draw start; upper bound, NOT exact GPU completion or input/display latency",
        "pending":"submitted fences not yet reaped by CPU, not a count of unsignalled GPU frames",
        "first_use":"ordinary geometry prewarms targets and stream capacity, then clears geometry cache; not process startup",
        "ordering":"four-mode Williams order within each depth; depth order alternates per repeat; cross-depth/row history uncontrolled",
        "complete_order_cycle":options.presentation_repeats.is_multiple_of(4),"complete_terrain_history":options.presentation_frames>=80,
        "prepared_input_limit_bytes":256*1024*1024,"capture_limit_bytes":64*1024*1024,
        "source_lifetime":"prepared definitions remain alive for the sequence; not a live producer/cache-GC memory test",
        "validation":"80 states per fixture and policy/depth; fenced PBO SHA256 RGBA and geometry/cache counts versus synchronous shared buffer reference; resize, empty, pool overflow, policy toggle, retirement, foreign vertex and pixel-pack state; captures never timed"});
    eprintln!("stream_history={metadata}");
    if let Some(path) = &options.geometry_output {
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    let mut trace = options
        .geometry_output
        .as_ref()
        .map(|p| File::create_new(p.join("frames.csv")))
        .transpose()?;
    if let Some(trace) = &mut trace {
        writeln!(
            trace,
            "fixture,mode,depth,repeat,order,frame,phase,startup,draw_ms,encode_ms,submit_ms,admission_wait_ms,observed_completion_ms,vertices,indices,upload_bytes,draws,admissions,promotions,build_work,cache_entries,cache_bytes,stream_slots,stream_capacity_bytes,{}",
            streaming::NAMES.join(",")
        )?;
    }
    println!(
        "fixture,mode,depth,repeat,order,frames,completed_ms_per_frame,cpu_ms_per_frame,draw_avg_ms,draw_p95_ms,draw_max_ms,observed_avg_ms,observed_p95_ms,observed_max_ms,first_observed_ms,admission_wait_total_ms,final_drain_ms,peak_pending,stream_capacity_bytes,upload_bytes,admissions,build_work"
    );
    let mut checks = 0;
    for name in &options.geometry_stream_cases {
        {
            let foreign = super::submit_profile::ForeignStreams::new(&context.gl)?;
            let pack = capture::ForeignPack::new(context.gl.clone())?;
            let sequence = input::prepare(name, viewport, 80, true)?;
            let references = [
                references(context, &sequence.frames, CONFIGS[0])?,
                references(context, &sequence.frames, CONFIGS[1])?,
            ];
            pack.check()?;
            foreign.check(&context.gl)?;
            for &depth in &depths {
                for config in CONFIGS {
                    let captures = (0..depth)
                        .map(|_| capture::Capture::new(context.gl.clone(), pack_bytes))
                        .collect::<Result<Vec<_>, _>>()?;
                    block(
                        context,
                        &sequence.frames,
                        config,
                        depth,
                        Some(Validation {
                            expected: &references[usize::from(config.mode == Mode::RetainedCulled)],
                            captures,
                            output: options.geometry_output.as_deref(),
                            fixture: name,
                            config,
                            depth,
                            foreign: &foreign,
                            pack: &pack,
                        }),
                    )?;
                    checks += sequence.frames.len();
                    pack.check()?;
                    foreign.check(&context.gl)?;
                }
            }
            eprintln!(
                "validated stream_history fixture={name} cumulative_queued_image_count_checks={checks}"
            );
        }
        let sequence = input::prepare(name, viewport, options.presentation_frames, false)?;
        eprintln!(
            "prepared stream_history fixture={name} accounted_payload_bytes={}",
            sequence.payload_bytes
        );
        for repeat in 0..options.presentation_repeats {
            let mut depths = depths.clone();
            if repeat % 2 == 1 {
                depths.reverse();
            }
            for depth in depths {
                for (order, index) in balanced_order(CONFIGS.len(), repeat as usize)
                    .into_iter()
                    .enumerate()
                {
                    let config = CONFIGS[index];
                    let b = block(context, &sequence.frames, config, depth, None)?;
                    let n = b.samples.len() as f64;
                    let draw: Vec<_> = b.samples.iter().map(|s| s.draw_ms).collect();
                    let observed: Vec<_> = b.samples.iter().map(|s| s.observed_ms).collect();
                    let total_wait: f64 = b.samples.iter().map(|s| s.admission_wait_ms).sum();
                    println!(
                        "{name},{},{depth},{repeat},{order},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{total_wait:.6},{:.6},{},{},{},{},{}",
                        config.name,
                        b.samples.len(),
                        b.wall_ms / n,
                        b.cpu_ms / n,
                        draw.iter().sum::<f64>() / n,
                        geometry::percentile(&draw, 95),
                        geometry::percentile(&draw, 100),
                        observed.iter().sum::<f64>() / n,
                        geometry::percentile(&observed, 95),
                        geometry::percentile(&observed, 100),
                        observed[0],
                        b.drain_ms,
                        b.peak_pending,
                        b.samples.iter().map(|s| s.capacity).max().unwrap_or(0),
                        b.samples.iter().map(|s| s.counts.0[2]).sum::<u64>(),
                        b.samples.iter().map(|s| s.counts.0[4]).sum::<u64>(),
                        b.samples.iter().map(|s| s.counts.0[6]).sum::<u64>()
                    );
                    if let Some(trace) = &mut trace {
                        for (step, s) in b.samples.iter().enumerate() {
                            writeln!(
                                trace,
                                "{name},{},{depth},{repeat},{order},{step},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{}",
                                config.name,
                                step / 16 % 4,
                                step < 8,
                                s.draw_ms,
                                s.encode_ms,
                                s.submit_ms,
                                s.admission_wait_ms,
                                s.observed_ms,
                                s.counts.0.map(|v| v.to_string()).join(","),
                                s.slots,
                                s.capacity,
                                s.streams.0.map(|v| v.to_string()).join(",")
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
