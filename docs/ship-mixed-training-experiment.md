# Seeded mixed ship-arrival experiment

`material_arrival_mixed_learning_v1` continues from the retained successful
[stationary genome](../crates/engine-agent/fixtures/ship-arrival-learning-v1/seed-3.json),
ID `6c0ef53fb27e190243c84903855544a2cb9b97f4f32c92499dd37e541bbc41fb`.
The previous [qualification](ship-arrival-qualification.md) identified
moving-target precision and lower-altitude inward arrival as coverage gaps.
This experiment changes the training distribution and selection priority;
the nine inputs, 107-parameter network, controls, physics, deadline, and arrival
and failure criteria remain those of the [arrival fixture](ship-arrival-diagnostic.md).

## Frozen recipe

| Setting | Value |
| --- | --- |
| Initial population | One unchanged seed genome, then 31 independently randomized genomes, parameters uniform in [-0.5, 0.5) |
| Independent run seeds | 1, 2, 3; retain and report all three |
| Budget per seed | 32 candidates × 32 generations × 12 cases = 12,288 training episodes |
| Total budget | 36,864 training episodes; no automatic extension |
| Selection | 2 unchanged elites; tournament size 3 with replacement |
| Mutation | Each parameter independently has probability 0.05 of a uniform [-0.15, 0.15) addition, clamped to [-8, 8] |
| Ranking | Original four stationary training arrivals first, then total arrivals, summed final position points, summed relative-speed points, spare successful ticks |
| Ties | Genome hash ascending, then population slot |
| Finalist | Best training fitness in the last generation, independently for every run seed |

The smaller mutation steps explore around the retained policy. Initial random
candidates still provide alternatives; they are not perturbations of the seed.
The shared runner records seed ancestry, deterministic draws, every candidate's
fitness, and each generation's winner. No crossover, imitation, new input,
or hidden control assistance is added.

Prioritizing the original four arrivals prevents selection from trading them
away for new successes. This guarantees retention only on those four training
cases. Broader stationary behavior is measured separately and can still regress.
This is a distinct recipe, not a change to either previous experiment.

## Training and evaluation

Training contains 12 cases, always evaluated together:

- The four original stationary training cases.
- Four inspected target-motion cases: `qualification_target_motion_b0_p0`,
  `b2_p1`, `b0_p4`, and `b2_p5`. These cover right/left lateral motion at 3 units/s
  and diagonal motion, including observed failures and successful contrasts.
- Both failed low-altitude inward cases, `qualification_combined_a0_b0_p2` and
  `a0_b1_p2`, plus a stationary inward approach at altitude 70 and a mirrored
  moving inward approach at altitude 75.

The original 11 cases and the previous 96-case qualification are development
checks now that their outcomes have been inspected. Four training cases overlap
the original set and six overlap the 96-case set. Reports keep the partitions
separate and do not treat their counts as independent samples.

The new evaluation schedule has 48 cases: 16 stationary, 16 moving, and 16
inward. Each crosses four fixed approach profiles with bearings π/6, 2π/3,
−π/3, and −5π/6. Profiles vary initial motion and heading; moving targets use
2.75-unit/s lateral or 1.75/0.75 diagonal motion; inward profiles use altitudes
68–82. All are physically distinct from training and previous evaluation.
The executable's `manifest` supplies every resolved case and the exact settings.

Freeze that manifest and source/executable identity before training. Preflight
checks use idle controls to validate start geometry and marker paths, without
consulting fresh learned or reference outcomes. Finish all three training runs
before evaluating any finalist, the original seed, or the handwritten reference
on the fresh schedule. Report all outcomes, including reference failures; do
not select a seed or revise a budget using evaluation results.

The acceptance gate requires every training, original, development, and fresh
case to arrive. Failure remains a result, not a trigger for more generations.
Passing would qualify this small arrival component on the declared fixture;
it would not authorize a live match-bot replacement. After inspection, these
48 cases also become development evidence for later decisions.

## Commands and artifacts

```sh
cargo +1.89.0 build --offline --locked --profile ci \
  -p engine-agent --example ship_curriculum
mkdir -p target/ship-curriculum
target/ci/examples/ship_curriculum manifest > target/ship-curriculum/manifest.json
# Repeat with seeds 2 and 3, using separate paths.
target/ci/examples/ship_curriculum train --seed 1 \
  --checkpoint target/ship-curriculum/seed-1.checkpoint.json \
  --best-genome target/ship-curriculum/seed-1.genome.json \
  > target/ship-curriculum/seed-1.training.json

# Run evaluations only after all three training runs finish.
target/ci/examples/ship_curriculum evaluate --policy baseline \
  > target/ship-curriculum/baseline.evaluation.json
target/ci/examples/ship_curriculum evaluate --policy reference \
  > target/ship-curriculum/reference.evaluation.json
target/ci/examples/ship_curriculum evaluate --policy model \
  --genome target/ship-curriculum/seed-1.genome.json \
  > target/ship-curriculum/seed-1.evaluation.json
```

Checkpoint progress is JSON on stderr. `--steps N` pauses after N candidate
batches; `resume --checkpoint PATH` continues using the same executable and
fixed recipe. Resume validates the original seed artifact as well as task,
budget, and architecture. Exports use the last completed generation; an initial
checkpoint has no winner. `episode --case NAME --policy baseline|reference|model`
retains a full trace, with `--genome PATH` required for `model`.

Keep all finalists, checkpoint histories, paired evaluations, the frozen
manifest, executable, source snapshot, reproduction commands, and checksums.
Routine tests check isolation, retention ranking, seeded initialization, recipe
compatibility, and physical setup; they do not require stochastic improvement.

## Results

All three declared runs finished their fixed budgets and passed the acceptance
gate. Each finalist was selected before any fresh evaluation. The original
seed and handwritten reference were evaluated on the identical cases and
initial physical states using the same frozen executable.

| Controller | Training | Original | Previous 96 | Fresh stationary | Fresh moving | Fresh inward |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Original seed genome | 6/12 | 11/11 | 85/96 | 16/16 | 13/16 | 4/16 |
| Handwritten reference | 12/12 | 11/11 | 96/96 | 16/16 | 16/16 | 16/16 |
| Run seed 1 finalist | 12/12 | 11/11 | 96/96 | 16/16 | 16/16 | 16/16 |
| Run seed 2 finalist | 12/12 | 11/11 | 96/96 | 16/16 | 16/16 | 16/16 |
| Run seed 3 finalist | 12/12 | 11/11 | 96/96 | 16/16 | 16/16 | 16/16 |

The new 48-case schedule improves from 33/48 for the parent to 48/48 for every
descendant. No descendant loses a previously successful case. All three also
recover the parent's two lower-altitude boundary failures and nine timeouts on
the previous 96-case schedule. Replaying the parent and reference reproduces
their original 107 outcomes, completion ticks, and final arrival measurements
exactly. No controller clips an input or emits invalid controls in this
evaluation. All five reports retain every case, including the parent's failures.

The full gate contains 167 reported episodes per controller but 157 distinct
physical starts, because ten training cases also appear in the old partitions.
These are deterministic cases in one world/build, not independent random world
samples. Three successful continuations from the same parent are encouraging
evidence for this recipe; they are not three successful runs from scratch.
The fresh cases become development evidence after this inspection.

The initial-generation winner in each run is the unchanged parent, at 6/12.
All 12 training cases first arrive at completed generations 3, 10, and 7 for
run seeds 1, 2, and 3. Every generation's winner retains the four stationary
training arrivals. All runs continue to generation 32; evaluation results do
not extend the budget or select one seed over another.

The experiment evaluates 36,864 training episodes, with separate checkpoint
verification, finalist evaluation, and trace replay. Three independent training
processes ran concurrently on this Linux x86_64 host with Rust 1.89.0, `ci`
profile. Their elapsed times, including checkpoint writes, were 209.5, 219.7,
and 203.2 seconds; the concurrent training phase took 219.7 seconds overall.
These timings are not directly comparable with the earlier sequential runs
or a supported-device throughput target.

Twenty full traces retain all five policies on four representative cases: a
known lateral tracking failure, a known lower-altitude boundary failure, and
the first parent failure in each new moving/inward group. Every traced replay
matches its scored episode exactly after removing trace samples. These paired
examples show arrivals under the same physical rules; no extra recovery reset,
deadline extension, or model-specific assistance was introduced.

All three [genomes and provenance](../crates/engine-agent/fixtures/ship-arrival-mixed-v1/README.md)
are retained. The repository also contains the
[pretraining plan](data/ship-arrival-mixed-v1-plan.json) and
[complete compact results](data/ship-arrival-mixed-v1-results.json), including
every reported outcome, per-generation winners, and trace diagnostics.

Local evidence is in `target/ship-curriculum/mixed-v1-pxqc6156/`, with a durable
archive at
`data/training-experiments/material-arrival-mixed-learning-v1-20261014.tar.gz`.
It includes the executable, source snapshot, manifest, every checkpoint and
finalist, evaluation reports, traces, reproduction scripts, and SHA-256 inventory.
Runtime identity:
`diagnostic_v1/linux/x86_64/423faedc2d2f58b29f6fa687132df5381eabf91e240223a9f4854601cf87430d`.
Source archive SHA-256:
`9dfb502f609ced1c2bc1ceca012af5806bdd64e36433dfef8c92b8ccb63582be`.
The frozen-plan test, retained descendants, replay test, and result documentation
were added after the freeze; execution code and recipe remained unchanged.

All 71 engine-agent tests pass, including deterministic replay of every saved
descendant against the complete gate. Strict AI/runner lint, formatting, and
whitespace checks pass. A seeded run paused across a generation boundary and
resumed in separate processes produces a byte-identical checkpoint to the
uninterrupted control. Six CLI checks also verify rejected arguments, output
collisions, and preservation of existing checkpoints.

The first [managed run and Training setup/progress view](managed-training.md)
now uses these retained recipes, followed next by genome browsing and compatible
scenario assignment. This measured arrival component supplies concrete fixtures
for that work. Broad match play and live bot replacement require their own task
coverage and integration checks.
