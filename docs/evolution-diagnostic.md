# Evolution diagnostic prototype

This local developer tool implements the first diagnostic slice of the
[evolutionary training plan](design/evolutionary-training.md): a tiny organism,
loadable feed-forward models, exact episode traces, seeded evolution, and saved
continuation. The separate [material ship-arrival fixture](ship-arrival-diagnostic.md)
now uses the same model and evaluator interfaces. General complete/hybrid brain
composition, the managed training service, Training UI, and genome browser remain
planned work.

## Frozen line-mover contract

`scenario-line-mover` implements the ordinary `Scenario` interface. The runner
reads `Scenario::observe`, decodes model outputs, and submits ordinary scenario
actions. Rendering and trace retention do not advance the world.

| Item | V1 meaning |
| --- | --- |
| Organism / component role | `diagnostic_line_mover` / `movement_v1` |
| World | `line_mover_v1`; integer positions from -4 through +4 |
| Observation | One f32: `(target - position) / 8`, fixed normalization |
| Outputs | Three logits in left, stay, right order |
| Decoding | Highest logit wins; ties prefer stay, then left, then right |
| Actions | Move one integer cell or stay; clamp at world bounds |
| Cadence | One decision and movement per fixed 60 Hz tick |
| Success | Two consecutive post-step states at the target by default |
| Deadline | 16 ticks by default; arrival wins an exact-deadline tie |
| Limits | Configurable dwell and deadline with `1 <= dwell <= deadline <= 256` |

Starting at the target still requires dwell. A zero-duration step does nothing;
terminal worlds cannot advance. Malformed actions terminate explicitly. The
episode adapter distinguishes a model inference failure from a host error:
non-finite output receives an `invalid_output` outcome and zero fitness;
incompatible artifacts fail before an episode starts.

The known oracle is a single linear 1-to-3 layer with weights `[-1, 0, 1]` and
biases `[0, 1/16, 0]`. For the default start -2 and target +1:

| Tick | Position before | Input | Raw outputs | Action | Position after / dwell |
| --- | --- | --- | --- | --- | --- |
| 1 | -2 | 0.375 | [-0.375, 0.0625, 0.375] | right | -1 / 0 |
| 2 | -1 | 0.25 | [-0.25, 0.0625, 0.25] | right | 0 / 0 |
| 3 | 0 | 0.125 | [-0.125, 0.0625, 0.125] | right | 1 / 1 |
| 4 | 1 | 0 | [0, 0.0625, 0] | stay | 1 / 2, arrived |

Idle and wrong-direction models supply known failures. Independent rule
controllers check all three models over all 81 start/target pairs. The oracle
completes 81; idle and wrong-direction each complete the nine coincident cases.

## Run it

Build once, then keep that executable for checkpoint continuation. Output
directories must exist. These examples use fresh checkpoint paths; `train`
rejects an existing checkpoint, while `resume` updates it atomically.

```sh
cargo build --profile ci -p engine-agent --example evolution_diagnostic
mkdir -p target/evolution-diagnostic

target/ci/examples/evolution_diagnostic genome --policy oracle \
  --out target/evolution-diagnostic/oracle.json
target/ci/examples/evolution_diagnostic episode \
  --genome target/evolution-diagnostic/oracle.json --start -2 --target 1 \
  > target/evolution-diagnostic/oracle-trace.json

target/ci/examples/evolution_diagnostic train \
  --seed 1 --population 16 --generations 8 --steps 5 \
  --checkpoint target/evolution-diagnostic/run.json
target/ci/examples/evolution_diagnostic resume \
  --checkpoint target/evolution-diagnostic/run.json \
  --best-genome target/evolution-diagnostic/best.json
target/ci/examples/evolution_diagnostic episode \
  --genome target/evolution-diagnostic/best.json --start 4 --target -4
```

`--steps` counts complete candidate evaluation batches, each containing all 81
cases. Zero saves the initial population. Omitting it runs to the finite
generation limit. Each batch is checkpointed; stderr reports checkpoint progress
at the first batch and generation boundaries, and stdout contains a final JSON
summary. `--best-genome` exports the best model of the latest completed
generation; if no generation has completed, the summary reports no export.
The genome and checkpoint must have different output paths.

Repeat `--seed-genome PATH` when starting a new run to copy selected compatible
models into the initial population. Each occurrence fills one slot unchanged;
remaining slots get random parameters. All seeds are evaluated afresh. The
prototype CLI fixes the template to the diagnostic linear 1-to-3 network, even
though the model loader supports other bounded dense architectures.

## Selection and continuation

The generic runner in `engine-agent::evolution` asks an `EvaluationTask` for its
manifest and ordered fitness. The line adapter owns the world and scoring; the
runner contains no line-mover or Space Wars state.

Every candidate runs all 81 cases from a clean state. Fitness is a lexicographic
tuple of total completed cases, total terminal proximity points
(`8 - abs(target - position)`), then unused ticks on successful cases. The oracle
scores `(81, 648, 966)`. The same cases select every generation; this is a
framework diagnostic, with no held-out qualification claim.

The initial recipe uses 16 candidates, eight generations, two unchanged elites,
and tournaments of three draws with replacement. Remaining candidates inherit
one selected parent's parameters. Each parameter mutates with probability 0.2
by adding a uniform offset in `[-0.5, 0.5)`, clamped to `[-8, 8]`; random initial
parameters use `[-1, 1)`. These are prototype settings, not ship-training defaults.

Selection breaks fitness ties by ascending genome hash, then population slot.
The versioned SHA-256 counter stream keys draws by seed, generation, candidate,
initialization/offspring domain, and local draw counter. Unit draws use the high
24 bits; tournament indices use integer remainder. No live controller state is
inherited. Mutation/layout settings and the task manifest are immutable within
one run.

Genomes contain the organism/role, observation/action versions, dimensions,
cadence, memory declaration, dense layers, and f32 parameters. Linear and tanh
activations are supported, with no recurrent state. Per-layer weights are
row-major, followed by biases. Typed JSON serialization and its versioned hash
identify the artifact. Loading checks the contract and finite parameters;
seeding additionally requires the same architecture and parameter bounds.
Artifacts are bounded to eight layers, widths of 4096, 65,536 parameters, and a
2 MiB input file.

A separate versioned checkpoint stores the immutable recipe, runtime identity,
initial seeds, current population and lineage, completed fitness slots, and each
generation's best model and scores. It has a checksum and a 32 MiB file limit;
population, generations, and stored parameter copies are also bounded. Saves
write and sync a temporary file before replacing the destination. The previous
checkpoint remains usable if a write fails before replacement.

This prototype resumes only at candidate-batch boundaries and requires the
same executable hash, OS, and architecture. Rebuilding may invalidate a
checkpoint; compatible genome artifacts can still seed a new run. It does not
claim cross-platform numerical identity, retain mid-episode state, or implement
worker pools or the planned Pause / Save and stop service lifecycle.

## Verification

```sh
cargo test --profile ci -p scenario-line-mover -p engine-agent -p spacewars-ai --lib
cargo test --profile ci -p engine-agent --example evolution_diagnostic
```

Checks cover exact world transitions, known rule/model equivalence, f32 artifact
round trips, interface rejection, invalid outputs, reset and trace isolation,
seeded mutation and elitism, evaluator failures, and corrupt/incompatible
checkpoints. Initial, partial-generation, generation-boundary, and completed
checkpoints must resume to the same population, history, and serialized final
checkpoint as an uninterrupted control. None of these assertions depends on a
random population discovering a better controller.

### Initial learning smoke results

On Linux x86_64 with Rust 1.89.0 and the `ci` profile, three preselected seeds
(1, 2, 3) used 16 random candidates, 64 generations, the operator settings above,
and no seed genomes. All reached the oracle's `(81, 648, 966)` score:

| Seed | Completed cases in first generation | First generation completing all 81 (zero-based) |
| --- | --- | --- |
| 1 | 9 | 39 |
| 2 | 9 | 7 |
| 3 | 45 | 12 |

Reproduce each by running `train --seed N --population 16 --generations 64
--checkpoint PATH --best-genome PATH`, with separate output paths and the same
build. The shorter default eight-generation run with seed 1 improved proximity
but remained at nine completions. These results demonstrate learning in this
finite diagnostic; they are observations, not CI thresholds or ship settings.

A separate eight-generation seed-1 run was stopped after five candidate batches
and resumed in a fresh process. Its final checkpoint and exported best artifact
were byte-for-byte identical to the uninterrupted control. Command-line checks
also rejected an existing checkpoint for a new run and a genome export aimed
at the checkpoint path without modifying the saved job.

The separate [material-ship arrival fixture](ship-arrival-diagnostic.md) now
provides ordinary flight controls and a reachable reference controller. The
[Guidance v1 proposal](design/guidance-component-v1.md) describes its wider scope.
