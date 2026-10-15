# First material ship-training experiment

`material_arrival_learning_v1` is a bounded local measurement using the
[arrival fixture](ship-arrival-diagnostic.md). Its settings and case partitions
are fixed before training. The existing `material_arrival_fixed_v1` diagnostic
keeps its original eight cases and identity.

## Recipe

| Setting | Value |
| --- | --- |
| Contract | `arrival_local_9_v1` → `turn_tanh_thrust_brake_v1`, once per physics tick |
| Network | 9 inputs → 8 tanh hidden units → 3 linear outputs; 107 f32 weights/biases, no recurrent memory |
| Initial population | Every parameter independently uniform in [-0.5, 0.5); no seed genomes or teacher initialization |
| Independent seeds | 1, 2, 3; report all three |
| Budget per seed | 32 candidates × 48 generations × 4 training cases = 6,144 episodes maximum |
| Selection | 2 unchanged elites; tournament size 3, with replacement |
| Mutation | Each parameter independently has probability 0.1 of a uniform [-0.35, 0.35) addition, clamped to [-8, 8] |
| Ranking | Completed cases, summed final position points, summed final relative-speed points, then spare successful ticks; descending lexicographic order |
| Ties | Genome hash ascending, then population slot |
| Finalist | Best training fitness in the last generation, separately for each seed |

Initialization and mutations use the shared runner's counter-based deterministic
draws. There is no crossover, topology search, reference imitation, or extra
learned-policy control assistance. The reference retains its documented thrust
accumulator; learned outputs use the same ordinary controls and physics through
the declared stateless decoder.

Every episode retains the calibrated 20-second deadline, 2-unit position and
1-unit/s relative-speed tolerances, 30-tick continuous dwell, and the original
contact, assistance, health, ownership, and altitude gates. Failures other than
timeouts earn zero. Timeout shaping and success ordering are unchanged. These
are exploratory settings, not recommended production training defaults.

## Partitions and acceptance

Training uses `stationary_radial`, `stationary_right`, `stationary_left`, and
`stationary_rotated` from the fixed diagnostic. Only these four cases enter
`TrainingTask::evaluate`, parent selection, and finalist selection.

Stationary validation is four separate physical starts:

| Case | Bearing | Heading offset | Target offset | Initial radial / lateral speed |
| --- | ---: | ---: | --- | --- |
| `validation_radial_close` | 0 | 0 | (0, 12) | 2 / 0 |
| `validation_right_drift` | 0 | -0.2 | (14, 8) | 0 / -2 |
| `validation_left_rotated` | -π/2 | 0.2 | (-14, 8) | 0 / 2 |
| `stationary_reversed` | π | π | (0, 20) | 0 / 0 |

All use the same material build, seed 42, altitude 90, and physics as training.
Angles are radians; positions/speeds use the initial radial frame described in
the fixture. This is limited geometric variation in one world/build, not broad
world or organism coverage.

Moving transfer uses the original `moving_right`, `moving_left`, and
`moving_rotated` cases. These test a task absent from training and are reported
separately from stationary validation.

The frozen reference is checked on all 11 cases before training. Each seed's
last-generation finalist is then evaluated once on all three partitions. No
validation result enters the run or picks a winning seed. A candidate needs
4/4 training and 4/4 stationary validation arrivals before advancing to a
moving-target curriculum; transfer results are informative at this stage.
Failure to meet that gate is retained, not followed by an automatic budget
increase. Inspecting these results makes them development validation; a future
qualification claim needs fresh cases after architecture and recipe decisions.

## Commands and artifacts

```sh
cargo +1.89.0 build --offline --locked --profile ci \
  -p engine-agent --example ship_training
mkdir -p target/ship-training
target/ci/examples/ship_training manifest > target/ship-training/manifest.json
target/ci/examples/ship_training evaluate --policy reference \
  > target/ship-training/reference.json

# Repeat with --seed 2 and --seed 3 and separate output paths.
target/ci/examples/ship_training train --seed 1 \
  --checkpoint target/ship-training/seed-1.checkpoint.json \
  --best-genome target/ship-training/seed-1.genome.json \
  > target/ship-training/seed-1.training.json
target/ci/examples/ship_training evaluate --policy model \
  --genome target/ship-training/seed-1.genome.json \
  > target/ship-training/seed-1.evaluation.json
target/ci/examples/ship_training episode --case stationary_right --policy model \
  --genome target/ship-training/seed-1.genome.json \
  > target/ship-training/seed-1.right.trace.json
```

Training reports checkpoint progress as JSON on stderr and a final summary on
stdout. The checkpoint records every candidate's fitness and each generation's
winner. It is saved atomically before evaluation and after every candidate
batch. `--steps N` pauses after N batches; `--steps 0` saves only the initial
state. Continue with `resume --checkpoint PATH` and the optional `--best-genome
PATH`. Resume requires the same executable and immutable task/configuration;
the CLI rejects altered budgets and model architectures. A rebuilt executable
can still load saved compatible genomes for evaluation.

An export after a partial run is the latest **completed** generation's winner;
before the first generation completes there is no export. For the declared
experiment, evaluate finalists only after all 48 generations finish. Exporting
or evaluating a genome does not change the population. Existing checkpoints
cannot be overwritten by `train`; checkpoint and genome output paths must
differ. This remains a single-writer local command, without a managed job
service, Training view, genome browser, or live-bot assignment.

CI checks partition isolation, exact fitness aggregation, reference reachability,
recipe compatibility, and replay of a retained learned genome. It does not
assert that random training improves.
Shared model, physics, and checkpoint tests cover the underlying adapter and
runner.

## Results

Measured locally on Linux x86_64 with Rust 1.89.0 and the `ci` profile. All three
declared runs completed their original budget: 18,432 training episodes total,
plus separate reference, verification, and finalist evaluations. Every finalist
was selected by training fitness before its validation was run.

| Controller | Training arrivals | Stationary validation | Moving transfer |
| --- | ---: | ---: | ---: |
| Frozen handwritten reference | 4/4 | 4/4 | 3/3 |
| Seed 1 finalist | 1/4 | 0/4 | 1/3 |
| Seed 2 finalist | 1/4 | 0/4 | 1/3 |
| Seed 3 finalist | 4/4 | 4/4 | 3/3 |

All initial-generation winners had zero training arrivals. The first arrival
appeared in completed generations 11, 33, and 2 respectively. Seed 3 first
completed all four training cases at generation 46; its generation-48 finalist
also passed every retained case. It meets this experiment's stationary-stage
gate. One successful seed out of three does not establish a robust training
default, and 11 small development cases are not general-flight qualification.

Seeds 1 and 2 learned the stationary-left and moving-left approaches. Their
other cases timed out. Retained training traces show that the remaining
stationary approaches never entered the 2-unit position tolerance; these were
not arrivals rejected solely by the dwell counter. Seed 3 learned effective
arrival across the declared approaches, including moving targets absent from
training. None of the 33 finalist evaluation episodes clipped any input, hit
a physical-failure gate, or produced invalid controls; every outcome was an
arrival or timeout. That constrains the diagnosis of this small experiment,
without proving the encoder sufficient for broader tasks.

Paired completion times for the successful finalist, in scored physics ticks:

| Case | Reference | Seed 3 |
| --- | ---: | ---: |
| `stationary_radial` | 752 | 504 |
| `stationary_right` | 887 | 937 |
| `stationary_left` | 757 | 331 |
| `stationary_rotated` | 863 | 955 |
| `validation_radial_close` | 572 | 720 |
| `validation_right_drift` | 746 | 725 |
| `validation_left_rotated` | 650 | 299 |
| `stationary_reversed` | 844 | 416 |
| `moving_right` | 944 | 935 |
| `moving_left` | 816 | 440 |
| `moving_rotated` | 911 | 924 |

Seed 3 is faster on seven cases and slower on four. These comparisons share
the same physics, ordinary controls, failure gates, and arrival tolerances;
they do not justify replacing a general match bot.

Trainer wall times, including checkpoint writes, were 138.9, 136.5, and 148.5
seconds respectively: about 7.1 minutes total, or 41–45 episode evaluations per
second per run on this host. This is a local prototype measurement, not a
supported-device throughput target.

All three [saved genomes](../crates/engine-agent/fixtures/ship-arrival-learning-v1/README.md)
and their provenance are included as small fixtures. The successful artifact is
`seed-3.json`, genome ID
`6c0ef53fb27e190243c84903855544a2cb9b97f4f32c92499dd37e541bbc41fb`.
A deterministic test replays its saved weights on the 11 declared cases. The
experiment itself remains random-initialized and does not use these fixtures
as seeds.

The local evidence directory is
`target/ship-training/learning-v1-fgvfe2by/`. It contains the manifest saved
before training, all checkpoints, exported genomes, per-generation fitness,
partition reports, training traces, executable, source snapshot, checksums,
and reproduction scripts. Runtime identity:
`diagnostic_v1/linux/x86_64/3daf5cfd8268c0dbdc35a6ba730b132f9468d0e2bdbfe00586010b81d4ae3b87`.
The pre-run source archive has SHA-256
`ce82c92a93d104466779bcbdffbc3eb51d96af100119d9c3512cc35bf30e1664`.
The retained-genome fixtures and their regression test were added afterward.
The complete local evidence is also archived outside Cargo's build directory
at `data/training-experiments/material-arrival-learning-v1-20261013.tar.gz`,
with a sibling SHA-256 file and an internal `SHA256SUMS` inventory.

Verification includes all 60 engine-agent tests, strict AI/runner lint, reload of
each finalist reproducing its recorded training fitness, and a cross-process
pause/load/advance checkpoint identical to uninterrupted evaluation. CLI checks
also cover an initial checkpoint with no fake winner, output-path collisions,
existing-checkpoint preservation, and undeclared seeds. A Python verifier
initially stopped after seed 1 because one JSON report expands f32 numbers to
f64 decimals; comparison at the declared f32 precision confirmed identical
settings. The completed run was retained unchanged, and seeds 2 and 3 continued
with the frozen recipe. No budget or scoring change followed the results.

The [fresh-case qualification](ship-arrival-qualification.md) is now complete:
seed 3 passed 85/96 new cases and retained all 11 original arrivals, while the
reference passed every case. The misses identify moving-target precision and
lower-altitude inward arrival as the next training gaps. The original 11-case
results above are unchanged. The subsequent
[seeded mixed experiment](ship-mixed-training-experiment.md) closes these measured
gaps on its frozen gate: all three descendants pass the 107 previous and 48 new
cases. The planned managed-job and Training/Genomes workflows can use the
retained artifacts from both experiments.
