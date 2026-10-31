# Candidate learned-flight execution contract v1

**Status: historical binary-control contract, superseded for new work by
[continuous controls v2](../flight-continuous-controls-v2.md), 2026-10-24.** The current
runner rejects this v1 decoder; the descriptions and commands below document the
retained v1 source/runtime only. Do not use them with the v2 bridge or learner.

The first [v2 learning experiment](../flight-continuous-approach-v1.md) has now produced
0/48 mean and 0/192 sampled final arrivals. Both modes approach but fail to slow
adequately. A [saved-policy response probe](../flight-goal-response-v1.md) finds
almost no distance/speed-dependent command changes. This does not reinstate the
historical binary decoder below. The [paired scaling experiment](../flight-input-scaling-v1.md)
uses unchanged v2 continuous execution with explicitly folded inference exports.

**Original status: candidate available in routine evaluation, 2026-10-23.** This makes the mixed
action decoder explicit; it does not replace production defaults or silently revise
qualification. See the [learning-gain comparison](../flight-learning-gain-v1.md),
[physical contract](learned-flight-contract-v1.md) and [current handoff](learned-flight-controller-v1.md).

Machine-readable identity: `mean-steering-sampled-buttons-v1`, version 1, in
[`mixed-execution-v1.json`](../../tools/flight_ppo/mixed-execution-v1.json), copied
byte-for-byte from the original diagnostic descriptor. The runner validates its pinned
hash before inference. Source and descriptor identity belong to each new checkpoint's
source/runtime contract; integration does not relax exact-resume checks for old runs.

At every **60 Hz** decision:

1. Obtain the network's steering mean, thrust logit and brake logit from the current
   observation. All three outputs remain learned and state-dependent.
2. Execute `tanh(steering_mean)` with no steering noise applied.
3. Convert each button logit to `clamp(sigmoid(logit), 1e-6, 1-1e-6)`. Draw independent
   unit-variance Gaussian auxiliary button latents with means given by the inverse
   standard-normal CDF of those probabilities. A button is on iff its latent is ≥0.
4. Use the existing independent latent sampler (rho=0). Draw its complete three-value
   vector in steering/thrust/brake order, including the unused steering draw. Its
   steering standard deviation is `exp(clamp(saved_log_std,-5,2))`; the draw is retained
   for audit but does not affect executed steering. This preserves comparison streams.

The implementation uses the pinned PyTorch CPU generator with an explicit
per-episode seed. Reset the RNG once at episode start and retain its stream across
decisions; do not reseed per tick. Distinct episode seeds and reset conditions belong
to each frozen experiment. Repeatability requires the recorded source/runtime contract;
this is not a portable RNG guarantee for other languages or library versions.

Physics stepping and outcome calculation remain native Rust. The current mixed decoder
runs in the Python diagnostic and routine evaluation tools; native gameplay integration and an interoperable
randomness specification are separate work. No native controller, labels, fallback,
rescue, forced straight-line steering or success shortcut is introduced.

This is an **execution** contract, not a change to PPO's training distribution.
The retained models were trained with noise in all three channels. This comparison
changes neither their training objective nor likelihood. Future training recipes must
state their exploration and evaluation modes separately and qualify against the intended
execution behavior. The model architecture, observations, physical arrival tolerances,
60 Hz cadence and ownership of the entire maneuver remain unchanged.

Qualification is deliberately unresolved here. Existing deterministic gates remain
historical requirements for their original experiments. An acceptance test for this
candidate must declare physical success, initial-versus-trained gain, seed/repeat
coverage, failure limits and task scope before using independent confirmation. A
positive execution or learning diagnostic alone does not qualify general navigation.

## Current evidence

The [steering ablation](../flight-steering-ablation-v1.md) measured 137/192 arrivals
under this mode versus 20/192 with fully sampled steering. The subsequent
[learning-gain comparison](../flight-learning-gain-v1.md) measured 0/192 at initialization
versus the same 137/192 after training, using paired tasks and random streams. Both
are development diagnostics. At update 128, only one trained seed exceeded 75% success overall;
coverage remains uneven and no general-flight qualification or independent confirmation
has occurred. The execution mode remains a candidate.
The [routine integration verification](../flight-evaluation-integration-v1.md) confirmed
that moving this behavior into the evaluator preserves those archived trajectories.
The later [duration experiment](../flight-mixed-duration-v1.md) froze explicit repeated
trial criteria but regressed to 84/192 arrivals after 256 updates. Its progression
criteria failed; none of those models reached 75%. Reserved confirmation remains
unused. The subsequent [duration-by-execution audit](../flight-duration-execution-v1.md)
measured 20/192 then 21/192 fully sampled arrivals. Seed 29 improved sampled execution
while losing mixed arrivals, but seeds 17/43 declined under both modes. The execution
mismatch alone does not explain those regressions. Goal entries decreased in both
modes. The [update-attribution audit](../flight-update-attribution-v1.md) exactly replayed
three seed-43 updates and found the selected thrust declines persisted without
immediate value loss. No execution rule changed. The next proposed offline comparison
examines auxiliary button-latent versus executed-button learning likelihoods at rho=0;
it is not a promoted training fix or an execution-contract change.

## Routine evaluation use

Training configurations may explicitly add:

```json
"evaluation": {
  "mode": "mean-steering-sampled-buttons-v1",
  "repeats": 4,
  "seed_base": 5310000
}
```

Omitting this field preserves the original deterministic evaluation. Initial and final
exports use identical tasks, repeat counts and episode seeds. Best-export selection
still uses physical success, fewer failures and lower final distance, with initial
winning ties; it is a development comparison, not qualification. PPO collection still
uses its separately configured exploration distribution.

Evaluate an existing export without training or checkpoint continuation:

```sh
data/python-envs/flight-ppo/bin/python -m tools.flight_ppo.evaluate \
  --bridge path/to/flight_ppo_bridge \
  --policy path/to/policy-0128.json \
  --admission path/to/development-admission.json \
  --out path/to/new-evaluation-directory \
  --mode mean-steering-sampled-buttons-v1 \
  --repeats 4 --seed-base 5310000 --learner-seed 17
```

The output directory must be new. CPU inference uses one thread and deterministic
PyTorch algorithms through both CLIs. Episode seeds are
`seed_base + learner_seed * 10000 + repeat * 1000 + candidate.index`. The version
supports 1–10 repeats and unique integer task indices 0–999; learner seeds are
nonnegative integers and derived seeds must be below 2⁶³. Keep seed bases and task
identities explicit in experiment specifications. These bounds prevent overlapping
seed blocks within one evaluation specification, not between unrelated experiments.

Mixed results live under `repeat-N/`, with full physics traces and compressed policy
rows recording actor outputs, probabilities, draws and controls. Root `results.json`
contains all attempts; `metadata/` holds execution identity and summaries by repeat
and recipe. Failures write an interruption record instead of complete root results.
The evaluator restores the caller's CPU RNG even on timeout; task order cannot change
an episode's stream. Run concurrent evaluations in separate processes, not threads
sharing PyTorch's global CPU RNG.

Legacy result rows and trace filenames remain compatible with their original
reporters. Historical sixteen-task qualification gates are not valid for flattened
mixed repeats. New acceptance criteria must explicitly account for repeats, model
seeds and coverage. Old checkpoints require archived source for exact continuation;
their exported weights can be evaluated read-only by the current evaluator.
