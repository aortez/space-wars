# Adaptive training workers

Practice and match episodes now use one persistent pool per training job.
Independent worlds run concurrently, with a dynamic queue rather than static
chunks. Every result is returned in input order; selection, mutation, seeds and
checkpoint writes stay on the coordinator. Match sources have a separate mutex
per case because their query/render caches are interior-mutable. Each episode
still forks its own physical world. Production physics and controllers are
unchanged.

The local Training screen uses this pool for stationary and mixed arrival
selection and scheduled validation. Its header switches resource policy while
a job is running; see [the UI guide](managed-training.md). The match research
harness uses the same implementation for its 157 precision and 15 match cases.
It finishes each phase before starting the next, without a second pool.

## Resource policy

- **Auto**, the default: target 75% total machine CPU, start at one worker,
  sample every two seconds and change the dispatch allowance by one when outside
  a ±5 percentage-point deadband. UI presets also offer 50% and 90%.
- **Fixed**: a requested count clamped to detected available parallelism. CPU
  readings remain visible but do not adjust dispatch.
- Capacity uses Rust's process-available parallelism and is capped at 64 workers.
  The headless Auto override can impose a lower maximum.
- Linux reads aggregate `/proc/stat` deltas. Idle and I/O wait are excluded from
  busy time; guest counters are not double-counted. First, missing, zero-delta,
  rolled-back, and invalid samples never mean zero load. Auto falls back to one
  worker when a usable sample is unavailable. Other platforms currently need
  Fixed for parallel evaluation.
- The target includes training, previews, foreground gameplay, and other
  applications. It is a sharing heuristic, not an OS-enforced CPU quota.
  Reducing the limit never cancels an episode. At least one remains allowed.
  After a pause of at least two seconds, sampling restarts and Auto ramps from one.
- Metrics report active/allowed/capacity, queue depth, the latest CPU reading,
  completed episodes, and aggregate episodes per second during pool batches.
  Resource settings and metrics do not enter manifests or saved fitness.

The design follows DirtSim's machine CPU target and deadband, with adjustment
at episode dispatch rather than waiting for a generation boundary. Missing CPU
samples have an explicit conservative fallback. Worker panics remain failures;
they propagate after worker cleanup rather than becoming fitness values.

## Headless use

For the existing `learned_guidance_match_training` harness:

```sh
# Default: Auto, 75% target, detected capacity.
export SPACEWARS_TRAINING_WORKERS=auto
export SPACEWARS_TRAINING_CPU_PERCENT=75
export SPACEWARS_TRAINING_MAX_WORKERS=0

# Fixed benchmarking example (clamped to this process's capacity).
export SPACEWARS_TRAINING_WORKERS=4
```

Pass these as environment variables along with the existing recipe, preflight,
mode and output arguments. Each saved candidate updates `workers.json`, and
generation progress includes the worker snapshot. Historical frozen plan bytes,
including their old `retention_workers` hint, are retained for provenance; the
new execution settings control concurrency. Changing workers within the same
binary can resume an experiment. Rebuilding still invalidates exact resume;
there is no relaxation of the runtime gate.

## Verification and measurement

Measured on the Ryzen 7 9800X3D (8 cores / 16 hardware threads), Rust 1.89.0,
optimized `ci` profile, using the historical finalist and the in-memory world
copy implementation. Each candidate scored 83,262 ticks across 172 episodes.

| Execution | Candidate wall time | Scored ticks / second | Gain over previous path |
| --- | ---: | ---: | ---: |
| Previous: four practice workers, serial match | 1.243 s | 66,985 | 1× |
| Fixed 1 | 2.353 s | 35,382 | 0.53× |
| Fixed 2 | 1.216 s | 68,489 | 1.02× |
| Fixed 4 | 0.648 s | 128,447 | 1.92× |
| Fixed 8 | 0.377 s | 220,643 | 3.29× |

Fixed rows are medians of three interleaved samples. The previous path controls
were 1.242 s before and 1.244 s after. Four-worker match time alone fell to
0.271 s, and eight-worker match time to 0.170 s. These are aggregate headless
rates, including world setup and host checks, with no rendering. They measure
one retained candidate, not the duration of a whole future training run.

A subsequent [shared terrain cache optimization](learned-guidance-training-performance.md#shared-terrain-caches)
reduced candidate wall time by a further 19–24% at four to eight workers. The
table above preserves the original worker-pool comparison, before that change.

Auto rose from one to eight workers before the competing load. Eight bounded
background threads pushed measured machine CPU to 91.5%; Auto reduced its
allowance to four by second 29. After the load stopped at second 36, it rose
again, reaching ten by second 49. Every replay retained exactly the same
complete episode results. Auto's target is intentionally gradual and may remain
below its target when the remaining case queue cannot use more parallelism.

Both Fixed 4 and Auto also replayed all **276 historical match episodes** across
the mission, reference, parent and finalist controllers. All **20,479 trace
rows per mode were byte-identical**, including controls, observations, handoffs
and world hashes; endpoint results matched. These replay checks do not resume
an old executable's checkpoint. The [compact results](data/adaptive-training-workers.json)
link to raw measurements and source/binary provenance in
`data/training-experiments/training-workers-20261017/`.

Pool tests compare full practice episode traces and serialized evolution
checkpoints across serial, Fixed and Auto scheduling, including resume after
changing policy. Other tests cover draining active episodes after a reduction,
CPU counter failures, deadband/bounds, ordered errors, and worker panic cleanup.
Managed-job tests retain candidate-boundary saves and validation provenance.
Final local checks passed: 97 agent tests, eight guidance fixtures, five Training
control/history/replay tests, and the rendered Training test at 800×480,
1024×768 and 480×800. The final executable repeated both historical replay
checks successfully. Formatting and whitespace checks passed.

## Varied-population training trial

A subsequent trial ran three generations of the unchanged v3 recipe with
Auto at 75%: 16 candidates per generation, 172 episodes per candidate, **8,256
episodes** total. The first population contained the retained parent and 15
independently randomized genomes. All three populations had 16 distinct genome
IDs, with precision completions ranging from 0 to 157. This exercises weak
candidates as well as the retained controller.

| Generation | Execution | Evaluation wall time |
| --- | --- | ---: |
| 1 | New run, Auto starts at one worker | 17.0 s |
| 2 | Same process | 12.2 s |
| 3 | Resume after saving generation 2; Auto starts at one again | 21.0 s |

These times include candidate saves and exclude approximately **34 seconds of
source reconstruction per process**. The separate preflight also took 34
seconds. External checkpoint polling captured every candidate boundary at
100 ms intervals; timings have approximately 0.1 second sampling uncertainty.
Peak process RSS was **489 MiB**, including source preparation and evaluation.

Auto reached an allowance of ten workers after the first two generations and
seven after the resumed generation. Mean total machine CPU during evaluation
was 32% and 30%, respectively. This short trial did not reach the 75% target;
the gradual ramp and available queued work govern dispatch. It used ordinary
background load. The separate benchmark above covers reduction under imposed
CPU contention.

Every population ID, fitness value, selected genome and generation history
entry matched the first three generations of the historical run exactly.
Fresh physical source receipts matched apart from the executable runtime ID.
Both bounded invocations passed the actual checkpoint reload checks, and the
resumed generation preserved the preceding history. The historical checkpoint
and finalist remained unchanged.

This verifies scheduling and checkpoint continuation with an evolving
population; the saved run remains incomplete at **3 of 24 generations**. It
does not qualify a new learned controller. The [compact trial results](data/adaptive-training-worker-trial.json)
record the code commit, binary and recipe hashes, timings, resource observations
and checks. Raw logs, observer scripts, all candidate checkpoints and the
resumable run are retained in
`data/training-experiments/training-worker-trial-20261017/`.

## Reproduce the scaling benchmark

`learned_guidance_worker_scaling` replays the historical finalist on all 172
training episodes. It checks source hashes, saved fitness, and complete episode
results. Fixed 1/2/4/8 each run three times in interleaved order. It then runs
Auto for 56 seconds, introducing up to eight bounded CPU-load threads between
seconds 20 and 36. The original four-practice-worker/serial-match path runs
before and after as a control. No evolutionary training is started.

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_TRAINING_PROFILE_INPUT="$PWD/data/training-experiments/recovery-guidance-match-learning-v3-20261016" \
SPACEWARS_TRAINING_PROFILE_OUT=/tmp/spacewars-worker-scaling-NEW \
cargo +1.89.0 test -p engine-client --bin engine-client \
  --locked --offline --profile ci learned_guidance_worker_scaling \
  -- --ignored --nocapture
```

Use a fresh output directory. To reproduce all historical match traces through
the pool, use `learned_guidance_clone_trace_parity` with
`SPACEWARS_TRAINING_WORKERS=4` (or `auto`), the same input, and another fresh
output directory. With that variable omitted, the checker retains its serial
path. Stage profiling remains explicitly serial; the worker benchmark measures
complete elapsed wall time without silently losing thread-local stage timers.
