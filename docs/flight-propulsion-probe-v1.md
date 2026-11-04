# Frozen propulsion-exploration probe

**Status: complete; joint gate failed, 2026-10-25.** Experiment
`learned-flight-propulsion-probe-v1-20261025` follows LF-049. Correlated quarter-scale
noise produced rotation but no sideways/backward arrival. In those attempts,
throttle remained below 9.6% with substantial braking. Test whether wider
propulsion exploration supplies maneuver experience while holding remains usable.
This is a no-training diagnostic, not policy qualification.

## Frozen policies, cases and conditions

Use the original LF-047 residual-architecture update-0 policies, seeds 17/29/43,
with their zero-output goal branches. Preserve every weight and learned deviation.
Production Python/Rust sources and the native runtime remain unchanged. Native
execution means running the same learned model in Rust; it supplies no teacher or
fallback. No optimizer, new model checkpoints/exports, handoff or promotion.

Reuse LF-049's admitted development cases without replacement:

- Position: recovery seed 4510002, indices 1/5/9/13, forward/left/backward/right,
  deadline 900 action ticks.
- Hold: seed 4310002, all sixteen cases and four heading strata, deadline 1,800.

Keep COM distance ≤3, speed ≤1, absolute spin ≤native-f32 .2, thirty complete
consecutive intervals of dwell, and **60 Hz decisions/physics**. Admit all sixty
policy/suite cases. Run native deterministic and Python mean once per case, and
both sampled conditions four times per case:

| Condition | Steering marginal scale | Throttle/brake marginal scale | Residual rho |
|---|---:|---:|---:|
| quarter | learned std × .25 | learned std × .25 | .98 |
| propulsion | learned std × .25 | learned std × 2 | .98 |

The candidate widens throttle/brake residuals by **eight times** relative to the
quarter baseline. It changes neither the steering law nor temporal persistence.
Closed-loop states and actor outputs may nevertheless diverge in every channel.
The scale 2 candidate is chosen before flight outcomes: learned propulsion stds
are about .55–.64, so its marginal latent std is about 1.1–1.3 instead of .14–.16.
This provides substantially broader sigmoid-control coverage around the observed
low-throttle means. There is one candidate, no coefficient grid or favorable-repeat
selection. The coefficient is not selected to satisfy the eventual physical gates.

## Conditional law and paired evidence

The local audit law uses the production AR(1) center and quarter-scale distribution,
then multiplies its conditional scales by `[1,1,1]` or `[1,8,8]`:

`z[t] = mu[t] + rho*(z[t-1]-mu[t-1]) + sigma*.25*sqrt(1-rho²)*f*epsilon[t]`.

Use effective rho zero at each episode's first action, giving full marginal scale.
Previous actor/latent state resets every episode. Sampling remains tanh steering
and sigmoid throttle/brake, with no post-sampling clamp or action override. The
wider vector scaling is **audit-only**: production v3's scalar [.01,1] contract and
all training defaults stay unchanged. Conditional transformed-action densities
must use the widened scale and the production Jacobians.

Reuse all 240 original private-generator float32 Gaussian streams exactly:
`4910000 + model_seed*100000 + suite*10000 + repeat*100 + case_index`, position=0,
hold=1, with shape `[deadline,3]`. Both conditions consume common prefixes until
terminal. Require exact first actor vectors and steering samples/actions; wider
propulsion samples and joint log densities intentionally differ. Preserve RNG.

Require exact reproduction of all sixty native traces, all sixty Python-mean
traces/policy records, and all 240 LF-049 persistent traces/policy records by the
quarter baseline. Keep their immutable copies. Independently regenerate streams,
reconstruct every sampled latent exactly, verify conditional log densities within
1e-5 absolute/relative and decoded actions within 1e-7 for vectorized rounding.
Logged and executed actions must match exactly. Native/Python initial-observation
parity covers sixty cases (raw and controls 1e-5). Keep unchanged checkpoint/export
hashes and report any native/Python deterministic outcome/timing differences.

## Measurements and frozen opportunity gates

Reconstruct the existing physical outcomes, dwell, first tolerance exits, eligible
rows, radius entries, distance/bearing improvements and wrapped heading movement.
The inherited descriptive useful-turn event requires ≥20° bearing improvement and
≥1 unit distance improvement at the same frame, not rotation alone. Preserve
parent rewards/gamma, discounted and undiscounted components, and paired returns
against quarter and Python mean. None of these diagnostic events replaces arrival.

For each executed control retain range/mean, throttle ≥.25 ticks, brake ≤.25 ticks,
their joint count and longest contiguous run. For position cases also record joint
ticks with **pre-action** absolute goal bearing ≤30°, and those also having positive
velocity toward the current goal (>1e-6), relative to target velocity. Zero-distance
goals have no bearing. Thresholds are descriptive fractions of control range,
not proof of net acceleration or competent action. Report nonforward cases separately.

Measure residual mean/std/lag-one correlation independently for all three channels,
normalized by their prescribed marginal scales. Pool only within-episode pairs;
finite outcome-truncated observations need not have exactly unit empirical variance.

For **at least two seeds jointly**, require:

- Residual persistence: each channel's pooled lag-one correlation ≥.90 in both
  conditions and both suites. Exact law/scale reconstruction must also pass.
- Maneuver exposure: ≥4/12 nonforward radius entries, gain ≥2 versus quarter;
  ≥2/12 settled nonforward successes, gain ≥1; entries in at least two distinct
  nonforward directions.
- Holding retention: ≥48/64 eventual holds, ≥8/16 in each heading stratum; eventual
  and immediate hold counts each fall by no more than 8/64 compared with quarter.
- Zero physical failures in both candidate suites.

These are descriptive training-opportunity gates, not statistical significance or
navigation qualification. Larger controls, longer turns, forward-only success or
better return cannot pass the joint gate. No automatic training continuation.

## Budgets and retention

Six single-threaded Torch CPU workers (policy × suite), each ≤1800 seconds with
1820-second external watchdog; one verifier ≤600 seconds with 620-second watchdog.
Stop on failure, with no scored retry. Exactly **600 episodes**: 120 position and
480 hold. Maximum **972,000 action ticks**, plus **720 neutral setup ticks** for
episodes, admissions and parity. Zero training ticks and optimizer steps.

Freeze 1,581 source files, 918 copied inputs, specification, unchanged runtime and
dependency versions, the runner and all 45 previous manifest hashes before flight.
Inputs include three original checkpoints/exports/configs, two admissions, sixty
native baselines, the LF-049 report and six worker reports, 300 Python reference
traces plus 300 policy records, and 240 reference streams. Forty-one Python tests
cover the scalar baseline, widened law/densities, per-channel stationary scale,
reset isolation, physical/reward reconstruction, action coverage and joint gates.
Retain the unchanged 421-pass/two-ignored native baseline; no new native run claimed.

Retain sixty new native traces, 540 compressed Python traces and policy records,
240 streams, all summaries, commands, tests and process timings. Check all frozen
source/input/runtime hashes at sealing. Never overwrite previous archives. Local
retention is not off-machine backup. Confirmation partitions remain unused.

## Results

Wider propulsion noise expands actuator coverage but adds no nonforward arrivals and reduces holding; the joint gate fails in every seed.

| Condition | Position successes | Nonforward radius entries | Nonforward arrivals | Eventual holds | Immediate holds |
|---|---:|---:|---:|---:|---:|
| native | 2/12 | 0/9 | 0/9 | 48/48 | 48/48 |
| mean | 2/12 | 0/9 | 0/9 | 48/48 | 48/48 |
| quarter | 9/48 | 0/36 | 0/36 | 149/192 | 83/192 |
| propulsion | 4/48 | 0/36 | 0/36 | 101/192 | 57/192 |

Both sampled conditions retain **0/36 sideways/backward radius entries and
0/36 settled arrivals**. Forward arrivals fall from **9/12 to 4/12**, giving 9/48
versus 4/48 position successes overall. The wider candidate has **101/192 completed
holds versus 149/192** in the quarter baseline, and **57/192 immediate holds versus
83/192**. No seed passes maneuver exposure or holding retention. Seed 29 still
completes 50/64 holds, but its loss of fourteen eventual and seventeen immediate
holds exceeds the allowed eight-count drops.

All unsuccessful trials reach their deadlines; there are no physical failures in
any condition. Native and Python deterministic execution retain 2/12 position
successes, both forward, and 48/48 immediate holds. No model updates or policy
improvements occur in this frozen-policy probe.

| Seed | Forward arrivals, quarter → wider /4 | Eventual holds /64 | Immediate holds /64 | Wider hold successes by heading, each /16 | Maneuver / holding / joint gates |
|---|---:|---:|---:|---|---|
| 17 | 1 → 1 | 27 → 21 | 14 → 8 | 5 / 6 / 2 / 8 | fail / fail / fail |
| 29 | 4 → 1 | 64 → 50 | 59 → 42 | 14 / 9 / 15 / 12 | fail / fail / fail |
| 43 | 4 → 2 | 58 → 30 | 10 → 7 | 6 / 10 / 7 / 7 | fail / fail / fail |

### Actuator coverage and coordination

The law changes as intended. Normalized residual correlations across every
channel/suite/seed are **.977–.985** in the quarter baseline and **.977–.982** in the
wider candidate. Empirical normalized std ranges are .915–1.126 and .935–1.044,
respectively; the theoretical target remains one. Per-channel values are retained.

Across the 36 sideways/backward candidate episodes (32,400 action ticks), throttle
reaches **88.17%**, compared with **9.60%** in the baseline; minimum brake falls
from **36.30% to 1.62%**. These extrema are separate and need not coincide.
The candidate has **1,591 ticks at throttle ≥25%**, **4,784 ticks at brake ≤25%**,
and **210 ticks satisfying both**, versus zero for each in the quarter baseline.
The longest joint run is **25 ticks (0.417 seconds)**. Per-seed joint counts are
104/66/40, with longest runs 25/9/11 ticks.

None of those **210 joint ticks** occurs while pre-action absolute goal bearing is
within 30°. Consequently none also satisfies the frozen approaching-velocity
condition. This does not mean no thrust ever occurs while aligned: it is the
specific conjunction of the two control thresholds and bearing criterion. It
shows the wider samples did not supply that combination in this case matrix.

Maximum nonforward distance improvement is **0.195 units**, compared with **0.217**
for quarter noise. Mean per-episode best improvement is **0.0323 versus 0.0343**.
Both have zero descriptive useful-turn events. Larger propulsion excursions do
not produce the missing goal-directed turn, translate and settle sequence.

These observations support a coordination/experience-coverage concern, not a proof
that a single actuator or the network architecture is the root cause. The probe
changes both propulsion channels together, and its higher latent variance also
changes mean executable throttle/brake through the sigmoid transforms. Steering's
sampling law stays fixed, while feedback states and actor means can diverge.

| Seed | Nonforward joint throttle/brake ticks, quarter → wider /10800 | Longest wider joint run | Wider joint ticks aligned ≤30° | Best distance improvement, quarter → wider |
|---|---:|---:|---:|---:|
| 17 | 0 → 104 | 25 | 0 | 0.185726 → 0.101570 |
| 29 | 0 → 66 | 9 | 0 | 0.059029 → 0.123562 |
| 43 | 0 → 40 | 11 | 0 | 0.216966 → 0.194649 |

### Realized reward

Mean paired discounted return falls by **1.547275** on position trials and
**3.186332** on holding trials relative to quarter noise. Relative to the Python
mean baseline, differences are **−1.296312** and **−6.127353**. Of 48 paired position
returns, one improves and 47 decline by more than 1e-9. Holding has seven improvements,
102 declines and 83 differences within ±1e-9. All reward components and individual
paired differences are retained; none substitutes for the physical gates.

### Verification and retention

All **41 Python tests** pass. All **240 Gaussian streams** reproduce LF-049
exactly. Every conditional latent reconstructs exactly using its declared channel
scales; transformed-action log densities and vectorized decoded actions pass their
tolerances, and logged/executed actions match exactly. First actor vectors and
steering samples/actions match in all 240 pairs. Full propulsion samples and joint
densities differ as intended. Weights, checkpoint/export hashes and global RNG
remain unchanged.

All **60 native baseline traces**, **60 Python-mean traces/policy records**, and
**240 quarter-condition traces/policy records** reproduce LF-049 exactly. All sixty
initial native/Python parity comparisons pass. Native/Python mean outcomes and
completion ticks agree in all sixty episodes; this does not assert bitwise identity
between the two implementations' full trajectories. Native test evidence remains
the retained unchanged **421-pass/two-ignored** baseline, not a new native test run.

All six workers and the verifier exit zero without scored retries. Concurrent
worker times are **74.7–330.9 seconds**, not isolated throughput measurements.
The verifier process takes 8.3 seconds. The run uses **600 episodes**, **355,806
action ticks**, and **720 neutral setup ticks**, within every frozen budget.
Training ticks and optimizer steps are zero, with no new model checkpoint/export.

Preserve 1,581 frozen sources, 918 copied inputs, unchanged runtime, sixty new native
traces, 540 compressed Python traces and policy records, 240 streams, all summaries,
likelihoods, commands, tests and timings. All 45 previous manifests and copied input
sources remain unchanged. Sealing verifies frozen source/input/runtime hashes.
See [results](data/flight-propulsion-probe-v1-results.json) and
[archive receipt](data/flight-propulsion-probe-v1-archive.json). Local retention is
not off-machine backup. No confirmation use, model promotion or general-navigation
claim follows this probe.

### Next decision

Keep production exploration and training defaults unchanged. Close LF-050
without training on this wider-noise candidate or searching more amplitudes.
Both LF-049 and LF-050 failed their frozen opportunity gates: longer temporal
correlation and wider propulsion samples have not supplied nonforward arrival
experience while preserving holding. This does not rule out every exploration
law, but it does not support another noise-only continuation here.

Next, design and freeze a **goal-bearing curriculum**: begin with goals close to
the ship's forward direction, then widen the bearing range only after the same
learned policy demonstrates arrival and holding retention. Define the entire
procedural case recipe, symmetric left/right coverage, distances, stage boundaries,
case counts, evaluation sets, advancement/stop gates and total training budget
before collecting the first rollout. This replaces ad hoc additions with one
reviewable progression. Freeze the initialization and optimizer/exploration choices
as part of that recipe; compare its first bounded stage with the existing broad
bearing task distribution from matched original holding starts across all three
seeds. Keep the current physical arrival definition and a fixed original-hold
regression suite. Record wider-bearing evaluation even before a stage earns advance.

The hypothesis is that nearby directional corrections can supply learnable
successful maneuvers before requiring quarter-turn/backward travel. This has not
been demonstrated, and no curriculum experiment has been implemented or run here.
Use one policy to choose every control at 60 Hz throughout: curriculum stages
change training tasks, not runtime controllers. Add no scripted goal-aware turns,
native teacher, fallback or handoff. Preserve the original broader evaluations,
and do not qualify navigation from an easier stage's success. If the initial
bounded comparison fails, stop and revisit initialization/skill design explicitly.

