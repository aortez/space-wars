# Frozen continuous-policy persistence probe

**Status: complete; training-candidate gate failed, 2026-10-24.** Experiment
`learned-flight-persistent-probe-v1-20261024` follows LF-048's unsuccessful family
centering pilot. Test whether temporally correlated exploration supplies useful
maneuver experience while preserving opportunities to complete holds. This is a
no-training diagnostic, not an attempt to qualify a learned navigation policy.

## Frozen models and case matrix

Use LF-047's original residual-architecture update-0 policies and checkpoints,
seeds 17/29/43, with the zero-output goal branch. Keep every tensor and learned
deviation unchanged. Production source and runtime remain unchanged from LF-048.
No optimizer, new checkpoint/export, teacher label, fallback, handoff or promotion.
The earlier binary-action persistence experiment remains a negative result; this
probe addresses the current continuous controls and quarter-amplitude noise.

Two existing development suites, with no replacements or confirmation cases:

- Position: all four signed position cases from recovery seed 4510002, indices
  1/5/9/13 (forward/left/backward/right), deadline 900 ticks.
- Original hold: all sixteen cases from seed 4310002, four heading strata, deadline
  1,800 ticks. Holding begins at the goal with zero velocity/spin.

Both retain COM distance ≤3, speed ≤1, absolute spin ≤native-f32 .2 and thirty
complete consecutive intervals of dwell. Decisions and physics remain **60 Hz**.
Admit all sixty policy/suite cases without replacement. Copy their native baseline
traces and require exact reproduction by the unchanged native evaluator.

For each policy/case run native deterministic and Python mean once, then independent
noise (rho 0) and persistent noise (rho .98) four times each. Both sampled conditions
use quarter marginal latent-residual scale; do not alter action transforms or clamp
executed controls. Use Python mean for noise attribution, keeping native/Python
outcome or timing differences explicit. Closed-loop trajectories can change actor
means despite fixed weights.

## Conditional law, pairing and evidence

Use the production `continuous-gaussian-scaled-ar1-v3` conditional law:

`z[t] = mu[t] + rho*(z[t-1]-mu[t-1]) + sigma*.25*sqrt(1-rho²)*epsilon[t]`.

At every episode's first action, set effective rho to zero and use the full
quarter marginal scale. Previous actor/latent state never crosses episode resets.
Because weights are frozen, the saved previous actor equals recomputation on the
previous observation. Keep 240 private-generator float32 `[deadline,3]` Gaussian
streams: `4910000 + model_seed*100000 + suite*10000 + repeat*100 + case_index`, with
position=0, hold=1. The two conditions share each stream, consuming common prefixes
until their respective terminals. Require exact equality of their first samples.
The theoretical marginal residual scale is fixed; observed variance can differ in
finite, outcome-truncated trajectories.

Save every actor mean, latent, decoded action and conditional transformed-action
log density, along with physical traces and Gaussian streams. Regenerate every
stream, independently reconstruct each conditional latent exactly, verify log
density within 1e-5 absolute/relative, and decoded actions within 1e-7 for vectorized
transcendental rounding. Logged and executed actions must match exactly. Require
unchanged weights/checkpoint/export hashes and global RNG, and native/Python parity
on all sixty initial observations (raw 1e-5 absolute/relative, controls 1e-5).

Reconstruct outcomes, dwell, first tolerance exits, eligible rows, radius entries,
distance reduction, absolute goal bearing and realized rewards. Use the unchanged
parent reward and gamma, with both discounted and undiscounted terms; retain paired
return differences against independent noise and the corresponding Python mean case.

Measure actual signed/absolute heading change with angle wrapping. Consecutive
same-sign turn runs use a descriptive 1e-5-radian-per-tick neutral threshold. A
descriptive useful-turn event requires, at the **same frame**, goal-bearing error
improvement ≥20° and distance improvement ≥1 unit relative to the start. Bearing
can improve through translation as well as rotation; this event is not proof of
causal steering or successful arrival. Holding has no goal-bearing metric.

Measure standardized steering-residual lag-one correlation and sign-run lengths.
Pool only within-episode pairs; never connect separate episodes. Report empirical
means/standard deviations alongside the prescribed theoretical noise scale.

## Frozen opportunity gates

These are descriptive gates for considering a subsequent training experiment,
not model qualification or statistical significance. Each seed has sixteen sampled
position attempts per condition (twelve nonforward) and sixty-four sampled holds.

For **at least two seeds jointly**, require:

- Persistence manipulation: pooled steering-residual lag-one correlation ≥.90
  and increase ≥.70 over independent noise in both suites. Also report whether
  this manipulation passes in all three seeds.
- Maneuver exposure: ≥4/12 nonforward radius entries, gain ≥2 versus independent;
  ≥2/12 nonforward settled successes, gain ≥1; and radius entry in at least two
  distinct nonforward directions.
- Holding retention: ≥48/64 eventual holds, ≥8/16 in every heading stratum;
  eventual and immediate hold counts each fall by no more than 8/64 compared
  with independent noise.
- Zero physical failures in both persistent-noise suites.

Turning more, forward-only success, better rewards or holding alone cannot pass
the joint gate. Inspect every case and paired repeat regardless of these gates.
No coefficient grid, favorable-repeat selection, retraining or automatic extension.

## Budgets and retention

Six single-threaded Torch CPU workers (policy × suite), each ≤1800 seconds with
1820-second external watchdog. One verifier ≤600 seconds, external 620. Stop on
failure; no scored retry. Position workers have forty episodes each; hold workers
have 160 each. Total **600 episodes**, at most **972,000 action ticks** (120×900 +
480×1800). Episode setup plus sixty admissions and sixty parity fixtures totals
**720 neutral setup ticks**. Training ticks and optimizer steps are zero.

Freeze 1,579 sources, 73 copied inputs, specification, constants, unchanged runtime
and dependencies, and all 44 previous manifest hashes before experiment simulation.
Inputs include three checkpoints/exports/configs, two admissions, sixty native
baseline traces and two parent reports. Thirty-three Python tests pass, covering
conditional law/densities, changing actor means, stationary scale, episode resets,
stream/RNG identity, movement/return reconstruction and joint gate separation.
Retain the unchanged 421-pass/two-ignored native baseline; do not claim a new run.

Retain sixty native traces, 540 compressed Python traces and policy records, 240
Gaussian streams, all summaries, tests, commands and process timings. No new model
checkpoints or exports. Local retention is not off-machine backup. No confirmation
use, model promotion or general-navigation claim follows this probe.

## Results

Persistent noise changes rotation but adds no sideways/backward arrivals; the frozen training-candidate gate fails.

| Condition | Position successes | Sideways/backward radius entries | Sideways/backward successes | Eventual holds | Immediate holds |
|---|---:|---:|---:|---:|---:|
| native | 2/12 | 0/9 | 0/9 | 48/48 | 48/48 |
| mean | 2/12 | 0/9 | 0/9 | 48/48 | 48/48 |
| independent | 8/48 | 0/36 | 0/36 | 151/192 | 61/192 |
| persistent | 9/48 | 0/36 | 0/36 | 149/192 | 83/192 |

The persistence manipulation passes in all three seeds. Nevertheless, both
sampled conditions have **0/36 sideways/backward radius entries and 0/36 settled
arrivals**. Persistent noise adds one forward success, giving **9/48 position
successes versus 8/48** with independent noise. Native and Python deterministic
execution each retain **2/12 position successes**, both forward.

Eventual sampled holds change from **151/192 to 149/192**; immediate holds change
from **61/192 to 83/192**. This aggregate immediate-hold improvement is concentrated
in seed 29: 31→59. Seed 17 loses three immediate and three eventual holds, and seed
43 also loses three of each. Holding retention passes for seeds 29/43 and fails
for seed 17. No seed passes maneuver exposure, so no seed passes the joint gate.
All conditions have zero physical failures; every unsuccessful trial ends at its
declared deadline. Deterministic holding remains **48/48 immediate holds** in both
implementations. No model has been trained or improved by this probe.

| Seed | Position successes, independent → persistent /16 | Eventual holds /64 | Immediate holds /64 | Persistent hold successes by heading, each /16 | Holding gate | Joint gate |
|---|---:|---:|---:|---|---|---|
| 17 | 0 → 1 | 30 → 27 | 17 → 14 | 7 / 7 / 5 / 8 | fail | fail |
| 29 | 4 → 4 | 60 → 64 | 31 → 59 | 16 / 16 / 16 / 16 | pass | fail |
| 43 | 4 → 4 | 61 → 58 | 13 → 10 | 14 / 14 / 15 / 15 | pass | fail |

### Persistence, movement and action coverage

| Seed | Suite | Steering residual lag-one correlation, independent → persistent | Residual empirical std, independent → persistent | Mean residual sign run, ticks | Mean physical turn run, ticks |
|---|---|---|---|---|---|
| 17 | position | 0.0042 → 0.9788 | 0.9974 → 0.9686 | 2.0171 → 16.7672 | 3.0667 → 16.8667 |
| 17 | hold | 0.0014 → 0.9808 | 0.9980 → 1.0137 | 1.9873 → 16.2128 | 3.0826 → 16.2041 |
| 29 | position | -0.0106 → 0.9778 | 0.9887 → 0.9399 | 1.9875 → 13.4368 | 6.4616 → 5.7302 |
| 29 | hold | -0.0091 → 0.9846 | 1.0038 → 1.1262 | 1.9666 → 12.4372 | 5.0211 → 2.1429 |
| 43 | position | 0.0083 → 0.9806 | 1.0062 → 1.0217 | 2.0126 → 15.7346 | 3.3671 → 16.2047 |
| 43 | hold | 0.0005 → 0.9813 | 0.9903 → 1.0317 | 1.9881 → 15.4677 | 3.1834 → 15.6737 |

Residual standard deviations are normalized by the prescribed quarter marginal
scale. The theoretical target is one; finite, outcome-truncated samples differ.

Steering-residual lag-one correlation rises from approximately zero to
**.978–.985**. Mean residual sign runs lengthen from about two ticks to 12–17 ticks.
Physical rotation responds differently by policy: seed 17/43 turn runs become
longer, while seed 29's become shorter. Correlated residuals are not equivalent to
longer physical turn runs in every closed-loop policy.

Across the sideways/backward persistent trials, the largest absolute net heading
change is **245.05°** and the largest goal-bearing improvement is **170.05°**.
These are separate maxima and need not occur in the same trial or at useful times.
Yet the greatest distance improvement in any such trial is only **0.217 units**,
versus **0.101** with independent noise. There are **zero useful-turn events** under
the frozen simultaneous bearing/distance definition. Rotation alone does not
provide the turn, translate and settle sequence that is missing.

Read-only aggregation of saved action traces also shows narrow propulsion
coverage. All sideways/backward sampled episodes run their full 900 ticks. In the
persistent condition, mean throttle is **6.26% / 3.71% / 4.78%** for seeds 17/29/43;
maximum throttle is **9.60% / 6.39% / 7.04%**. Mean brake is **61.73% / 48.84% /
63.11%**, with minimum brake **49.81% / 36.30% / 51.94%**. Independent noise reaches
at most 10.21% throttle across these trials. These are fractions of the executable
control range, not measured fractions of net acceleration.

This is consistent with exploration staying near a weak-thrust, substantial-brake
holding policy even when its steering residual persists. It does not establish
propulsion as the sole cause: all channels changed together, and their relationship
to orientation and velocity matters. The extra action-range aggregation is a
post-run diagnostic of retained traces, not a new acceptance gate or intervention.

### Realized reward

Mean paired discounted return changes by **+0.291802** on position trials and
**−0.121225** on holding trials versus independent noise. Relative to the matched
Python-mean baseline, changes are **+0.250963** and **−2.941021**, respectively.
These realized-return differences do not demonstrate policy learning or directional
competence; the frozen physical gates remain decisive. Both discounted and
undiscounted reward components, full paired differences and individual outcomes
are retained. Strict signs in the raw paired-return summary include floating-point
roundoff near zero and should not be read as meaningful wins.

### Verification and retention

All **33 Python tests** pass. Every stored latent reconstructs exactly from the
saved actor means and paired Gaussian streams. Conditional log densities and action
transforms pass their declared tolerances; logged and executed controls match
exactly. All **240 first samples** are paired exactly, and every stream regenerates
exactly. Weights, input checkpoints/exports and global RNG remain unchanged.

All **60 initial native/Python parity comparisons** pass. All sixty native baseline
traces reproduce the originals exactly. Native and Python-mean outcomes and
completion ticks agree in all sixty paired episodes; small floating-point trace
and reward differences are retained rather than described as full trace identity.
The native test evidence remains the unchanged **421-pass/two-ignored** baseline;
no new native test run is claimed.

All six workers and the verifier exit zero, without retries. Concurrent worker
elapsed times are **35.6–290.0 seconds**, not isolated throughput measurements.
The verifier process takes 7.8 seconds. The experiment uses **600 episodes**,
**265,980 action ticks** and **720 neutral setup ticks**, below all frozen limits.
Training ticks, optimizer steps and newly produced model checkpoints/exports are zero.

Retain 1,579 sources, 73 copied inputs, the unchanged native runtime, sixty native
traces, 540 compressed Python traces and policy records, 240 Gaussian streams,
all conditional densities, summaries, tests, commands and process timings. All
44 previous manifests and copied input sources remain unchanged. Archive sealing
verifies source, input and runtime hashes. See
results (`docs/data/flight-persistent-probe-v1-results.json`, in git history) and
[archive receipt](data/flight-persistent-probe-v1-archive.json). Local retention is
not off-machine backup. Confirmation partitions and model promotion remain unused.

### Next decision

Keep the production exploration and advantage-centering defaults unchanged.
Close LF-049 without a training continuation or persistence-coefficient search.
This candidate supplies rotation, but does not supply the missing nonforward
arrival experience. The finding is scoped to the frozen policies, amplitude,
persistence and case matrix; it does not rule out all persistent exploration.

Next, freeze an **actuator-specific exploration probe** before spending another
training budget. Hold steering exploration fixed and compare the current
throttle/brake residual scale with one predeclared wider propulsion setting.
The hypothesis is that applying the spin-protecting quarter scale to every channel
also prevents useful propulsion excursions around the holding policies. Retain
all three policies, paired random streams, signed position cases and original holds.
Choose and document the single candidate's exact law, scale, seeds, bounds and
joint physical gates before running it; do not select the setting after viewing
flight outcomes. Keep rewards, network weights, control transforms and 60 Hz
execution fixed. This proposed probe has not been implemented or run.

Measure joint throttle/brake coverage, its timing relative to goal bearing and
velocity, distance progress, settled arrival and holding disruption. Larger
controls or better return alone cannot justify training. This remains stochastic
exploration by the learned policy, without a goal-aware scripted maneuver,
teacher labels, native fallback or controller handoff. If the bounded probe still
fails to expose nonforward maneuvers with holding retained, revisit the skill
curriculum and initialization explicitly instead of extending this run.

