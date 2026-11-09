# Paired full/quarter-noise short-recovery training

**Status: complete; stopped at the quarter-arm pilot gate, 2026-10-24.**
Experiment `learned-flight-noise-training-v1-20261024` follows the
[frozen-policy noise probe](flight-noise-probe-v1.md). Test whether the improved sampled
experience at quarter noise translates into learned correction while retaining holding.

## Fixed comparison and exploration contract

Two arms, `full` and `quarter`, seeds 17, 29, 43. Initialize all tensors from the preserved
LF-033 early-arm update-128 policies; fresh Adam, matched sampler and CPU/Python RNG.
Copy LF-036 short-recovery configs. Only the declared training exploration contract changes:
`continuous-gaussian-scaled-ar1-v3`, rho 0, `std_multiplier` 1 or .25. Within a seed, arm
configs differ **only** in this multiplier. Both retain updates=64 as the upper bound;
the initial pilot explicitly stops after sixteen updates.

The effective latent standard deviation is `exp(clamp(log_std, -5, 2))*std_multiplier`.
The multiplier is fixed per run; learned `log_std` remains trainable, initialized exactly
from the parent. Record both base and effective deviations. The existing AR conditional
center and reset semantics are unchanged. Scale the Gaussian used by **both collection
and PPO density/entropy**, then apply the existing tanh/sigmoid action transforms. Do not
scale commands after computing probabilities. Retain old v2 behavior exactly; v3 validates
an explicit finite multiplier in [.01,1]. Multiplication by one follows the unchanged
arithmetic path for exact control replay.

Mean-action evaluation is unchanged and deterministic in all scored suites. Thus endpoint
scores measure learned policy changes, not a different evaluation noise level. Exports
retain base log_std and unchanged mean inference; the training multiplier belongs to the
checkpoint/config contract. The training runner rejects old sampled-v2 evaluation with
v3 training, because that decoder would silently omit the multiplier. No stochastic
export/evaluation compatibility is claimed or needed here.

Keep 39,447 parameters, schema-v1 inputs, 60 Hz controls, existing reward (regulation zero),
PPO four slots ×256 ticks, four epochs/minibatch 256, learning rate .0003, gamma .9995,
lambda .98, clip .2, value .5, entropy .01, gradient clip .5, target KL .03. Native source,
runtime, physics and success tolerances are unchanged. No teacher, rescue or handoff.

## Cases and gates

Use the identical LF-036 fixed task matrix: 64 training cases (seed 4510001), sixteen
development cases (4510002), equally divided among settled, position (distance 4–8),
velocity (speed 1.2–2) and spin (configured magnitude .3–.5) errors. Preserve signed
body-relative directions, case indices, all starts and geometry checks. Dwell **30 complete
intervals**, deadline 900; limits distance 3, speed 1, absolute spin native-f32 .2, free
heading. Original regression uses the same sixteen LF-033 starts at seed 4310002,
dwell 30/deadline 1,800. No replacements or added failures; confirmation 4510003 unused.

Pilot: six sixteen-update runs, six exact checkpoint 15→16 resumes, original-regression
and primary evaluation at initialization/final. Full-noise checkpoints 0/16 must reproduce
all three LF-036 parents exactly, allowing only the explicit equivalent exploration
descriptor and the four declared production source hashes to differ. All model, Adam,
collector, traces, counters, RNG and history fields must match. Full final primary and
original-regression score records must also reproduce. Every initial trace must reproduce
the copied baseline; both arms have identical initial tensors/RNG/collector.

Reuse the pilot gates separately for each arm:

- Every family/seed covers all signed classes (four for position/velocity, two for spin);
  nonsettled families have ≥256 above-limit correction rows out of 4,096 each.
- Every seed has ≥1 sampled successful short hold.
- At least two seeds have ≥30 fully eligible pre-action rows in position-error episodes.
- At least two seeds retain ≥12/16 original immediate holds, ≥2 per heading stratum,
  zero physical failures. Report every loss from initial 16/16.

**Only quarter-arm passage authorizes conditional main training.** If it fails, end this
experiment at the pilot without extension or gate changes. If it passes, run all six
matched 64-update main runs (including full controls), from the same initial tensors and
configs, requiring checkpoints 0/16 to reproduce each pilot exactly. Do not select a
checkpoint/seed based on intermediate performance.

At update 64, progression requires at least two **same** seeds with primary ≥12/16,
≥3/4 in every error family, zero physical failures, and original immediate holds ≥12/16,
≥2 per heading stratum, zero physical failures. Also require primary gain ≥2 over
initialization in at least two seeds. Quarter-vs-full learning benefit requires ≥4 extra
primary successes in at least two seeds at the matched 64-update endpoint. A pilot pass
is an experience gate, not capability qualification. Report forward/left/backward/right
position results explicitly; forward-only behavior cannot pass the family threshold.
Do not extend dwell, use confirmation, promote models or claim general navigation.

## Budgets, validation and retention

Pilot: 6×16 original +6×1 resumed updates = **104,448 training action ticks**, at most
**1,632 optimizer steps**. Six training, six resume, six regression/parity workers and
one verification process: **19 processes**. Conditional main adds 6×64 = **393,216 ticks**,
at most 6,144 optimizer steps, and thirteen processes. Combined maximum **497,664 ticks**,
**7,776 optimizer steps**, **32 processes**. All CPU workers use one Torch thread.

Training limits: 1,800 seconds each (external 1,820); extra evaluation 600 seconds
(external 620); each verification 600 seconds (external 620). No failure-driven extensions.
Each stage has **384 evaluation episodes**, maximum **518,400 action ticks**:
192 primary ×900 and 192 original regression ×1,800. Combined maximum **768 episodes /
1,036,800 ticks**. Native/Python parity: sixteen primary fixtures at every endpoint,
**192 comparisons per stage**, maximum 384. Neutral setup ticks are separate.

Retain **108 pilot/resume checkpoints and 24 exports**; conditional main adds 390
checkpoints and twelve exports, maximum **498 checkpoints /36 exports**. Preserve all
completed/active traces, admissions, reward/dwell reconstruction, process logs and gate
checks. Freeze 1,558 sources, unchanged runtime, six configs, specification, **122 copied
inputs** and **33 earlier manifests** before learning. Inputs: three initial checkpoints,
three exports, three configs, six parent update-0/16 checkpoints, 96 baseline traces,
six parent final score records, three admissions and two parent reports.

Test versioned validation, exact unit-amplitude sampling/RNG, scaled conditional density
and gradients, entropy, collector/PPO likelihood consistency and the sampled-evaluation
guard. Reuse native baseline 421 passed/two ignored. All source/runtime/input hashes,
parent reproduction, exact resumes, independent physical outcomes/rewards, inference parity
and budgets must pass. Local retention is not off-machine backup; no commit is implied.

## Results

Quarter-noise training produces more successful samples but worsens deterministic recovery and retention; the pilot stops without a longer run.

### Pilot

| Arm | Seed | Sampled successes | Primary initial → final /16 | Final settled / position / velocity / spin (each /4) | Original immediate holds initial → final /16 |
|---|---:|---:|---|---|---|
| full | 17 | 0 | 10 → 6 | 3 / 0 / 0 / 3 | 16 → 14 |
| quarter | 17 | 7 | 10 → 4 | 2 / 0 / 1 / 1 | 16 → 3 |
| full | 29 | 3 | 9 → 9 | 2 / 1 / 3 / 3 | 16 → 13 |
| quarter | 29 | 23 | 9 → 4 | 0 / 0 / 1 / 3 | 16 → 6 |
| full | 43 | 1 | 13 → 9 | 3 / 0 / 3 / 3 | 16 → 14 |
| quarter | 43 | 33 | 13 → 10 | 4 / 0 / 2 / 4 | 16 → 14 |

At sixteen updates, full-noise sampled successes are 0/3/1 and quarter-noise successes are 7/23/33 (4 versus 63 total). This confirms that the lower amplitude changes the training experience materially. Deterministic primary recovery nevertheless falls from 32/48 initially to 24/48 full and 18/48 quarter. Original immediate holds fall from 48/48 to 41/48 full and 23/48 quarter; quarter per-seed retention is 3/16, 6/16 and 14/16, so only one seed passes the retention requirement. Quarter passes sampled-success and position-eligibility checks, but fails exposure and retention; the conditional 64-update comparison is not run. No learning benefit or progression is qualified. All 384 evaluation episodes are free of physical failures.

Pilot gates: `{"full": {"checks": {"signed_exposure": true, "sampled_success_each_seed": false, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": true}, "passed": false}, "quarter": {"checks": {"signed_exposure": false, "sampled_success_each_seed": true, "position_eligible_30_rows_two_seeds": true, "short_hold_retention_two_seeds": false}, "passed": false}}`. Conditional main executed: **False**.

### Training evidence and directional scope

| Arm | Seed | Position eligible rows | Sampled position successes | Effective final std: steering / thrust / brake | Final position successes: forward / left / backward / right |
|---|---:|---:|---:|---|---|
| full | 17 | 0 | 0 | 0.4624 / 0.5805 / 0.6351 | 0 / 0 / 0 / 0 |
| quarter | 17 | 0 | 0 | 0.1147 / 0.1441 / 0.1597 | 0 / 0 / 0 / 0 |
| full | 29 | 194 | 0 | 0.4876 / 0.5638 / 0.5969 | 1 / 0 / 0 / 0 |
| quarter | 29 | 42 | 1 | 0.1233 / 0.1420 / 0.1500 | 0 / 0 / 0 / 0 |
| full | 43 | 262 | 0 | 0.4905 / 0.5481 / 0.6068 | 0 / 0 / 0 / 0 |
| quarter | 43 | 163 | 1 | 0.1235 / 0.1364 / 0.1516 | 0 / 0 / 0 / 0 |

Every signed direction is visited in both arms. The quarter exposure failure is specifically seed-29 spin: 190 above-limit spin rows versus the frozen 256 minimum, despite six successful sampled spin recoveries. Fewer out-of-tolerance rows can accompany better recovery, so this threshold should not be interpreted as missing directions or simple failure to sample the task. The separate retention failure independently requires stopping, and no gate is changed retrospectively. Quarter has two sampled position successes (seed-29 and seed-43 training case index 1, both forward); full has none. Final deterministic quarter position recovery is 0/12 in every direction; full retains one forward success. Effective quarter deviations remain close to their intended reduced values, rather than expanding back to full amplitude. Existing endpoint traces show concrete mean-policy changes at identical original holding starts: seed-17 quarter first-decision throttle averages 6.12% initially and 7.86% finally; thirteen original-hold cases first exceed the speed limit. Seed-29 quarter loses ten original holds through spin exits. These observations narrow the next audit toward update behavior, but do not establish a unique cause such as critic interference, entropy or insufficient network capacity. The recorded endpoint-action inspection uses saved evaluations only, without new simulation.

### Verification and retention

Sixteen focused Python tests passed, including scaled density/entropy gradients, collector/PPO consistency, legacy sampling and RNG identity, fresh-process restarts and the incompatible sampled-evaluation guard. Native runtime/source remains unchanged from the retained 421-pass/two-ignored baseline.

All six pilot resumes reproduce exact complete states. Full-noise states 0/16 reproduce all three LF-036 parents after only the explicit equivalent descriptor and four declared source-hash differences are normalized in an audit copy. No model/optimizer/collector/history/RNG difference is allowed, and actual checkpoint continuation still enforces strict identity. All full-arm final score records reproduce; both arms start from identical tensors, optimizer, RNG and collector states. Initial parent traces, parity and independent reward/physical-dwell reconstruction pass.

Used **104,448 training action ticks**, **1,146 optimizer steps**, **144,375 evaluation ticks** in **384 episodes**, and **192 parity comparisons**. All 19 subprocesses exited zero within their fixed limits. Retain 108 new checkpoints and 24 exports. No confirmation use, model promotion, longer dwell or general-navigation claim.

Archive `data/training-experiments/learned-flight-noise-training-v1-20261024` retains 1,558 sources, unchanged runtime, six configs, 122 copied inputs, all traces, admissions, checkpoints, logs and gate evidence. All 33 prior manifests and copied inputs remain unchanged. See results (`docs/data/flight-noise-training-v1-results.json`, in git history) and [receipt](data/flight-noise-training-v1-archive.json).

## Next decision

Preserve the original LF-033 policies and do not extend training or dwell. Freeze an update-attribution audit using the saved quarter-noise pilot checkpoints and rollouts before changing another training setting. Reproduce selected updates exactly, then measure how policy, value and entropy contributions and optimizer steps change mean steering/throttle/brake on fixed already-declared holding and correction states. Track likelihood ratios/KL, shared-encoder versus actor-head changes and sampled advantage/return signs, while distinguishing gradient evidence from counterfactual update effects. Compare with the matched full-noise controls and account for earlier attribution audits rather than repeating their conclusions by assumption. This should test why successful experience is not improving the mean policy; do not assume lower learning rate, a new reward or a larger network is the answer. Separately review future exposure gates so successful quick corrections are not penalized for spending less time outside tolerance, while keeping the retention gate mandatory. No new attribution audit is run here; confirmation, model promotion, native teachers/handoffs and later curriculum stages remain unused.
