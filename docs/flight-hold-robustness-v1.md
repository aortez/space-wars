# Frozen hold robustness audit

**Status: completed bounded robustness audit, 2026-10-24.**
Experiment `learned-flight-hold-robustness-v1-20261024` audits all three final
128-update early-arm policies from the [successful hold experiment](flight-extended-hold-v1.md).
No training, parameter changes, checkpoint selection, native labels or fallback.
The prior 48/48 result remains a half-second hold from a perfectly settled start.

## Fixed cases and separate scopes

First reproduce all sixteen original development episodes for each seed 17/29/43,
requiring exact traces and thirty-interval successes. A mismatch stops that worker
before new probes. Copy the original policies/checkpoints and all 48 baseline traces.

New probes use **eight fresh anchors**, seed **4410001**, two per original heading
quadrant, with the existing per-index hold sampler. No policy-dependent replacement
or failure additions. Retain the practice world, altitude 140–150, zero gravity/planet
spin, fixed target, clear space and full open-wing ship. Training/development parent
seeds and reserved confirmation 4310003/4210003/4110003 are not used to generate probes.

Each anchor has **23 variants**, evaluated by all three models with native mean actions:

| Scope | Variants per anchor | Initial errors | Required outcome |
|---|---:|---|---|
| Nominal duration | 3 | Zero | Hold continuously for 30, 120, 300 ticks (0.5, 2, 5 seconds), respectively |
| Inside-tolerance disturbances | 10 | Position ±1.5 forward/right; speed ±0.5 forward/right; spin ±0.075 | Remain within every tolerance for 300 complete intervals |
| Recovery | 10 | Position ±4 forward/right; speed ±1.5 forward/right; spin ±0.275 | Enter tolerance and complete 30 consecutive intervals within 300 ticks |

Only one error component is changed per variant. Position means a target displacement
from the initial COM along the ship's configured forward/right axes; velocity uses
the same axes, with both signs. Spin has both signs. Spin is configured as ±0.1/±0.3
before the normal neutral setup tick, which removes 0.025 rad/s; the recorded errors are
therefore ±0.075/±0.275. Position/velocity are preserved. Assert actual errors and signed
directions after setup, within 0.002 numerical tolerance, without teleporting or applying
an external controller. Neutral setup is outside the evaluated horizon.

Physical limits stay **COM distance ≤3, speed ≤1, absolute spin≤native-f32 .2**; heading
is free. Version 2 task semantics, complete before/after intervals and physical-failure
precedence remain intact. Nominal/inside cases have deadlines equal to required dwell,
so leaving and later returning cannot pass a sustained-hold test. Recovery uses dwell 30,
deadline 300 and can earn dwell only after entering tolerance. No early-failure terminal
or reward is used in this audit. Longer-duration/recovery scores are separate from the
original thirty-interval result.

## Admission and frozen checks

New native dispatch admits a separately named `hold-robustness-v1` geometric envelope:
small displacement/speed/spin, unchanged spatial tolerances, declared horizons, clear
world and no hold-failure rule. Existing recipes are unchanged. All hull/start/goal,
orientation-independent clearance and route checks still apply. The old conservative
travel-time estimate includes a fixed travel/settling reserve, so it is retained as
advisory evidence rather than rejecting these prescribed short hold horizons. This
is explicitly geometric admission, not proof of dynamic feasibility. No physics,
observations, inference or production Python training code changes.

Per variant/seed, pass requires **at least 6/8 successes**, at least one per heading
quadrant, and zero physical failures. Report separate seed-level checks for fresh
half-second nominal holding, both longer nominal durations, all ten inside perturbations,
and all ten recovery variants. An across-seed scope passes only if at least two seeds
pass its complete check. Report every variant and sign; do not hide a failing direction
inside a combined average. These are diagnostic scope checks, not game deployment gates.

Reconstruct every outcome and dwell; retain first tolerance exit/entry, maxima/final
errors, steering sign changes and mean/absolute controls. Native/Python parity uses
one baseline and one initial-state fixture per variant per seed: 24×3=72 comparisons.
Verify export/checkpoint equality, model/RNG neutrality and unchanged policy/checkpoint
file hashes. No new model exports or checkpoints are produced.

## Budgets and retention

**552 new episodes** (184 per model), at most **154,800 action ticks**. Baseline
reproduction adds 48 episodes, expected 1,440 ticks and bounded by the original 86,400
worst-case ticks before a mismatch aborts. Total 600 evaluation episodes; no sampled
execution or extra probes. All declared probe cases must be admitted; rejection aborts
instead of substituting an easier case.

Admission: 200 neutral setup ticks (16 old +184 new), cap 180 seconds. Parity: 72 neutral
setup ticks. Evaluation: 600 neutral setup ticks. Three workers, one Torch thread each,
1,800 seconds per worker/external 1,820; report cap 180/external 200. Five subprocesses
must exit zero. No budget extension. Retain all traces, admissions, parity fixtures,
per-seed results, process status and independently reconstructed metrics.

Freeze source, native runtime, spec and parent-input hashes before scored evaluation.
Retain 1,549 sources and 59 copied inputs: three checkpoints, three exports, three parent
summaries, one admission file, 48 original traces and one parent result. Verify all 29
previous manifests unchanged. Native baseline including the two new admission tests:
420 passed/two ignored. Eight focused Python tests verify setup, direction, admission,
300-interval success, recovery entry and scope gates. Local retention is not off-machine
backup. No confirmation use, model promotion, training or broader navigation claim.

## Results

The original 48/48 short holds reproduce, but sustained holding and general small-error recovery remain unqualified.

All 48 original baseline traces reproduced exactly, including every action and frame. All 552 new cases completed under the frozen criteria. Success counts below are out of eight anchors per model.

| Variant | Seed 17 | Seed 29 | Seed 43 |
|---|---:|---:|---:|
| nominal-30 | 8/8 | 8/8 | 8/8 |
| nominal-120 | 0/8 | 0/8 | 8/8 |
| nominal-300 | 0/8 | 0/8 | 0/8 |
| inside-position-forward-positive | 0/8 | 0/8 | 8/8 |
| inside-position-forward-negative | 0/8 | 0/8 | 0/8 |
| inside-position-right-positive | 0/8 | 0/8 | 0/8 |
| inside-position-right-negative | 0/8 | 0/8 | 0/8 |
| inside-velocity-forward-positive | 0/8 | 0/8 | 0/8 |
| inside-velocity-forward-negative | 0/8 | 0/8 | 0/8 |
| inside-velocity-right-positive | 0/8 | 0/8 | 0/8 |
| inside-velocity-right-negative | 0/8 | 0/8 | 0/8 |
| inside-spin-angular-positive | 0/8 | 0/8 | 0/8 |
| inside-spin-angular-negative | 0/8 | 0/8 | 0/8 |
| recovery-position-forward-positive | 0/8 | 3/8 | 8/8 |
| recovery-position-forward-negative | 0/8 | 0/8 | 0/8 |
| recovery-position-right-positive | 0/8 | 0/8 | 0/8 |
| recovery-position-right-negative | 0/8 | 0/8 | 0/8 |
| recovery-velocity-forward-positive | 0/8 | 1/8 | 8/8 |
| recovery-velocity-forward-negative | 8/8 | 8/8 | 8/8 |
| recovery-velocity-right-positive | 8/8 | 7/8 | 8/8 |
| recovery-velocity-right-negative | 8/8 | 6/8 | 8/8 |
| recovery-spin-angular-positive | 8/8 | 7/8 | 8/8 |
| recovery-spin-angular-negative | 8/8 | 0/8 | 8/8 |

Across-seed scope checks: `{"fresh_half_second": true, "nominal_long_hold": false, "inside_perturbations": false, "recovery": false}`. Every directional variant is included; no failed case was removed or replaced.

### Interpretation and task-conditioning limitation

The fresh short-deadline half-second probes pass 24/24. At two seconds, seed 43 passes 8/8 and seeds 17/29 pass 0/8. At five seconds, all three pass 0/8. The ten inside-tolerance five-second disturbance variants yield 0/80, 0/80, 8/80 by seed; the only passing variant is a goal 1.5 units forward for seed 43. No model passes the complete sustained-hold or inside-disturbance check. There are no physical failures in any of the 552 new probes; failures are deadlines after tolerance violations or unsuccessful recovery.

The nominal failure mechanisms are specific. Seed 17 first exceeds speed tolerance at ticks 53–56 (about 0.9 seconds) and reaches about 1.12 units/s; it continues applying about 6.13% throttle. Seed 29 first violates speed at ticks 58–119 in the two-second request. In the five-second request it first violates speed in seven cases and position in one, at ticks 84–240. Seed 43 remains below speed/spin limits, but crosses the position radius at ticks 238–242 (about 4 seconds), ending around 3.84 units from the goal at roughly 0.83 units/s. Its throttle stays near 4.67%. Continuing to drift slowly can satisfy the original short window without establishing sustained station keeping.

Recovery successes are 40/80, 32/80, 56/80, but their direction matters. All three recover all eight backward-velocity cases; lateral velocity and spin recovery are mixed, with seed 29 failing every negative-spin recovery case. Position recovery is poor: all models fail every backward and lateral target-displacement case; forward-target recovery is 0/8, 3/8, 8/8. None passes every recovery variant/heading criterion. These recovery counts require thirty complete intervals after entering tolerance, not indefinite holding.

The original steering oscillation is not the first nominal failure in this audit: those first exits are speed or position. Existing parity fixtures also provide matched-sign position-response probes at one physical anchor, without additional inference. Switching a target from 4 units ahead to 4 behind changes native steering by only 0.00000147/0.00022991/−0.00000081 across seeds; throttle changes by 0.00000176/−0.00000724/0.00000021. Lateral reversals are similarly small. This is evidence of weak goal-position conditioning in those specific states, not proof of an architectural incapacity or a global response bound. Full outputs are retained in validation/paired-initial-responses.json.

The task request is also part of the observation. Nominal duration and disturbance probes change requested dwell/deadline features, so this audit jointly tests task-conditioning and physical robustness. It does not isolate a unique causal explanation for every failure. The original baseline traces remain exactly reproduced. All source/model hashes, admission checks, outcome reconstruction and parity checks passed; no scored case was removed or changed after freezing.

The network observes required_dwell, dwell_fraction and remaining_time. The probe
horizons/deadlines therefore change conditioning inputs as well as the physical task.
Fresh nominal-30 also shortens the deadline from 1,800 to 30 ticks. These are joint
request/duration/perturbation generalization results, not a pure duration intervention
or fresh-anchor confirmation under the original time request. This limitation was
recorded during execution in validation/interpretation-scope.json; no frozen case,
budget, source, gate or model was changed. Per-horizon initial outputs on identical
physical states are retained in the parity fixtures. No causal attribution to one
input or training component follows from this audit alone.

### Verification and retention

Native surface tests: 420 passed/two ignored. Eight Python tests passed. A preparation
check exposed neutral-tick spin damping; the final specification and assertions use
actual signed post-setup errors. The earlier failed preparation check is retained.
All 200 geometric admissions passed. The native change is limited to diagnostic
admission/dispatch; baseline exact reproduction verifies unchanged policy/physics
behavior on all original evaluated trajectories.

All 72 native/Python parity comparisons passed. Copied checkpoint/export tensors match,
parity preserves model tensors and RNG, and checkpoint/export file hashes remain
unchanged. Every saved outcome and complete-interval dwell was reconstructed.

Maximum parity raw error 3.8147e-06; action error 8.9407e-08.

Budget used: **122,371 new-probe action ticks**, plus **1,440 reproduction ticks**. Neutral setups: 200 admission + 72 parity + 600 evaluation = 872. No optimizer steps, new checkpoints or exports. Worker times 104.56–120.88 seconds; all five subprocesses exited zero within their guards.

Archive `data/training-experiments/learned-flight-hold-robustness-v1-20261024` retains
1,549 source files, rebuilt runtime, 59 copied parent inputs, all 600 traces, 200 admissions,
72 parity fixtures, reconstructed metrics, logs and scope clarification. All 29 prior
manifests and referenced inputs remain unchanged. No training, confirmation use,
model promotion or broader flight claim. See
results (`docs/data/flight-hold-robustness-v1-results.json`, in git history) and
[archive receipt](data/flight-hold-robustness-v1-archive.json).

## Next decision

Specify the next learning task before running more training: a systematic near-goal stabilization/recovery distribution, with signed position, velocity and spin coverage and a separately declared sustained-hold criterion. Keep training, development and confirmation partitions explicit, and use fixed case families rather than appending these failed examples. Include matched task-request controls so duration/deadline conditioning is measured separately. Before a larger run, check the reward/return incentives and whether sampled rollouts actually reach the states needed to learn position correction and sustained low velocity. Preserve the existing short-hold result as a regression check. Keep the current network while designing that controlled test, without native teachers or handoffs. This is the next design decision; no new training recipe is implemented or run.
