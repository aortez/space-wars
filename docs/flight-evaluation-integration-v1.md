# Routine flight evaluation integration v1

**Status: completed and verified, 2026-10-23.** The original specification was frozen before full replay. This implements
LF-021's next action: make the candidate mixed decoder available to routine evaluation.
It is an implementation check, not a new learning or capability experiment.

## Changes and fixed scope

The routine evaluator and training runner accept explicit evaluation mode
`mean-steering-sampled-buttons-v1`: network-mean steering and sampled thrust/brake
buttons at 60 Hz. An absent evaluation setting preserves `deterministic-v1` and its
native Rust inference, result rows and trace filenames. The mixed implementation uses
PyTorch CPU inference and native Rust physics, with the exact candidate descriptor.
No neural architecture, collector exploration, PPO objective or physical tolerance
changes. Native gameplay integration remains separate.

Mixed episodes isolate the caller's CPU random state, seed once per episode, and
retain all action probabilities, auxiliary draws and executed controls. Records include
the decoder identity, descriptor hash, seed, runtime and per-repeat/per-recipe outcomes.
The training runner evaluates initial/final exports under the same declared mode and
includes the setting in checkpoint identity. Existing source/runtime resume checks
remain strict: old exports can be evaluated, but old checkpoints require their archived
source for exact continuation. Invalid modes, duplicate task indices and incompatible
exports fail explicitly. Incomplete evaluations are marked and cannot appear complete.

## Predeclared verification

Use the same three final update-128 models, seeds 17/29/43, and sixteen development
tasks from the aligned-approach experiment. Reuse the steering-ablation streams:
seed base 5,310,000, four repeats, episode seed = base + learner seed × 10,000 +
repeat × 1,000 + task index. No reserved confirmation tasks or additional task search.

- Run the **routine CLI** in mixed mode for 192 episodes. Require exact decoded
  physics trajectories and every policy/action row against the archived ablation.
- Run the same CLI with its legacy default for 48 episodes. Require exact physics
  trajectories and result rows against the original aligned deterministic evaluation.
- Cap full replay at 240 episodes / 432,000 physics ticks; each CLI process has a
  1,200-second evaluation wall budget and 1,260-second process timeout.
- Require the existing 45 production tests and seven new integration tests to pass.
  New tests check export identity, mode/seed guards, archive parity, episode-order
  independence, RNG restoration on success/timeout, checkpoint identity, and complete
  training-state equality with evaluation skipped or enabled and after exact resume.
- The training check uses two updates × eight ticks × four slots, once with evaluation,
  once without, then resumes from update one. Its purpose is correctness only; retain
  every resulting checkpoint and trace. These tiny checks are outside the full-replay
  tick budget and cannot support a learning claim.

Freeze selected workspace source, this specification, inputs and unchanged bridge
before full replay. Retain logs, exact commands, all replay traces/probability rows,
tests and correctness-run checkpoints. Verify all seventeen earlier sealed manifests
and copied parent inputs remain unchanged; seal a new local archive. Local retention
is not off-machine backup. No commit or clean checkout is implied.

## Interpretation and next action

Exact replay would reproduce 137/192 mixed arrivals and 0/48 deterministic arrivals.
Those are existing development results, not fresh evidence of generalization. Old
sixteen-task qualification reporters remain tied to their historical experiments;
they must not be applied to flattened repeated trials. This evaluator summarizes
physical outcomes and selects between initial/final exports; it does not declare
qualification. Separately freeze a bounded aligned-training recipe and repeat-based
progression criteria before further substantive training or broader difficulty.

## Results

All **52 production tests passed**, including seven new integration tests. A preceding
seven-test integration invocation also passed; both sets of new correctness-run
artifacts are retained. Evaluation-enabled, evaluation-skipped and resumed runs had
bitwise-identical complete final checkpoints, including optimizer, collector and RNG
state. Timeout and reversed task order checks passed. Compilation and whitespace
checks passed. The unchanged old-source checkpoint contract correctly refused exact
continuation under the changed implementation.

All **240 full CLI replays matched exactly**: 48 legacy physics traces/result rows and
192 mixed physics traces plus every recorded actor/probability/draw/action row.
The replay used 264,317 physics ticks: 86,400 legacy and 177,917 mixed, within the
432,000 cap. The tiny correctness runs and regression tests are separate from this
replay accounting. No full replay was incomplete or had an infrastructure failure.

| Learner seed | Legacy arrivals | Mixed arrivals |
|---|---:|---:|
| 17 | 0/16 | 56/64 |
| 29 | 0/16 | 43/64 |
| 43 | 0/16 | 38/64 |
| Total | 0/48 | 137/192 |

Both modes had zero physical failures; unsuccessful episodes reached the deadline.
These are exact reproductions of the previous results. They verify implementation
parity and add no new evidence of model competence or generalization.

The new archive retains 1,526 selected source files, 441 copied parent inputs,
all full-replay outputs, new correctness-run checkpoints, logs, commands and frozen
specification. All seventeen prior manifests and referenced parent inputs remain
unchanged. See [compact results](data/flight-evaluation-integration-v1-results.json),
[archive receipt](data/flight-evaluation-integration-v1-archive.json) and
[configuration/CLI usage](design/learned-flight-execution-v1.md#routine-evaluation-use).

Routine integration is complete. The next decision is a separately frozen, bounded
aligned-training recipe with repeat-based acceptance criteria. Model coverage remains
uneven; no new substantive training, native gameplay integration, qualification or
reserved confirmation use occurred here.
