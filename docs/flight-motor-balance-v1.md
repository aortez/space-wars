# Saved motor force-balance and update audit v1

LF-066, 2026-10-25. C4 audit 21; learning-attempt count remains 32.
Frozen before audit execution. The user authorized inspecting LF-065's saved
updates after lambda 1 improved corrective feedback without qualifying tracking.

## Question and fixed inputs

Separate incorrect command-specific force balance from weak restoring response,
then relate both to the saved advantages and actual policy changes near target
speed. Compare **lambda095/lambda1**, seeds **17/29/43**, all **129 checkpoints**
and **128 rollouts** from each LF-065 main run. Both initial/final native exports
are retained. No new training, optimizer step, physics tick, checkpoint selection,
confirmation use or model promotion. Original stopping and strict physical
qualification remain unchanged, including original seed-17 failures.

Reuse the **54 common physical observation anchors** from LF-065/LF-062. Preserve
their ordering, lateral velocity, heading, angular velocity, COM/origin offset,
native capability fields and previous actions. Translate COM and origin velocity
together when varying along-command speed; derive residual velocity errors from
the modified original inputs. All policies see exactly the same panel.

## Fixed-input response measurements

- At checkpoints **0/1/4/16/32/64/96/128**, sweep commands **4/6/10/12/16/18**
  and actual forward speeds **0–30 in .5 increments**, at every anchor: **19,764
  rows/checkpoint**, **948,672** total dense rows. Retain every actuator logit,
  action and algebraic along-command control acceleration.
- For each curve, report every adjacent sign-changing acceleration bracket,
  linear-interpolated zero, secant slope and positive-to-negative versus reverse
  crossing. Keep exact-zero samples separate with unknown stability; keep absent
  or multiple crossings explicit. Do not choose an out-of-range root or relabel
  a frozen-input crossing as a closed-loop equilibrium.
- At **all 129 checkpoints**, use commands **0/4/6/10/12/16/18** and signed
  actual-minus-command errors **−1/−.25/0/+.25/+1**: **1,890 rows/checkpoint**,
  **1,462,860** total history rows. Measure target acceleration, throttle/brake
  response and acceleration slope over ±.25. Repeat the force calculation with
  actions held at the center to separate passive braking from policy response.
- At final checkpoint only, repeat the dense sweep with previous-action inputs
  zeroed: **118,584** total rows. This measures dependence on those inputs, not a
  replacement runtime observation or history-free controller.
- Verify initial/final local panels against the unchanged native loader:
  **22,680 native/Python comparisons**, tolerance 1e−5 absolute/relative on raw
  actor and value outputs. Check exact paired initial local/dense responses and
  zero-command logits/actions across every checkpoint.

The control-force formula is the existing native open-wing force law, including
origin-velocity braking, its speed-10 cap and thrust governor. This is instantaneous
algebra, not a physical simulation. Full physical qualifications remain LF-065's
unmodified results. Unseen commands 6/12/18 are descriptive; no checkpoint is
selected from them. Anchor 51 remains the previously declared illustrative state;
retain all anchors rather than selecting only a favorable one.

## Saved training signals and actual updates

Read all **786,432 saved rollout rows**. Recompute each saved log likelihood from
its pre-update model (1e−4 absolute/1e−5 relative tolerance) and reproduce the
original separate movement/stopping advantage normalization. At identical saved
inputs, compare the before/after deterministic actions and control acceleration.
Verify **393,216 stopping rows** retain exactly the same actor logits and sampling
deviations; check frozen parent weights at all **774 checkpoints**.

Record raw/normalized advantage, mean/sample acceleration, throttle/brake,
before/after changes, and Gaussian mean-logit scores
`normalized_advantage * (sample_logit - mean_logit) / sigma²`. A positive score
favors increasing that logit in the local unclipped policy objective. It is not
the full parameter gradient or the eventual PPO update: parameter sharing,
minibatching, clipping, entropy, normalization and optimizer state still matter.

Use the established readiness screen: heading <=5 degrees, spin <=.2 and lateral
speed <=1. Retain under (<−1), near ([-1,+1]) and over (>+1) signed speed-error
groups, split by training commands **4/10/16**. Also split the ready near-target
region into **[−3,0)** and **(0,3]** for each command, excluding exact-zero ties.
Keep movement/stopping totals and all nonready rows in the broad accounting.

For underspeed, positive acceleration change is corrective; for overspeed,
negative is corrective. Record corrective/opposing sampled-action counts and
their normalized advantage sums, actuator-score signs and actual same-state
acceleration changes. This classifies instantaneous force corrections, not
counterfactual long-term action values. Retain every update and row-weighted
summaries for **1–32, 33–96, 97–128 and 1–128**. No posterior grouping replaces
the frozen groups; any further descriptive analysis is identified as post-run.

## Validation, budgets and retention

**14 Python tests pass**: seven new balance/bracketing, grid, readiness, passive
force, update-sign/stopping-invariance and weighted-aggregation checks, plus seven
existing feedback tests. Synthetic checks add no physics or optimizer steps.
The unchanged runtime's five model/48 environment tests remain inherited evidence.

Run **six audits and one verifier**, at most three concurrent. Each audit has
**600 internal /620 external seconds**, verifier **300/320**. Abort dependent
work on failure; retain original logs/partial outputs and separately record any
necessary correction. No automatic retry or extension. Require at least 2 GiB
free on the artifact disk before freeze; retain a 512 MiB reserve.

Freeze all source/config/specification/input hashes before execution. Retain
**1,631 sources**, **1,560 copied inputs** (1,554 checkpoint/rollout/export files,
anchors and five parent result/diagnostic files), runtime, response tensors,
per-update statistics, all test/process logs, budgets and completed documents.
Verify the full LF-065 manifest and copied input origins, and preserve all **61
previous manifests**. Neither old policies nor source experiments are rewritten.

Archive `data/training-experiments/learned-flight-motor-balance-v1-20261025`, linked
to `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-balance-v1-20261025`.
Use the existing standalone checkout. Local retention is not off-machine backup.


## Results

**The remaining failures include both an incorrect speed balance and weak learned
restoring force. Updates for different requested speeds also pull strongly on the
same outputs.** All six audits finish within 11.60–13.60 internal seconds
(12.58–14.63 externally), with no new physics, optimization or policy change.
LF-065's strict tracking results remain **0/0/8 versus 0/8/8** for lambda .95/1;
the audit does not create a newly qualified controller.

### Where the saved policies balance

The table gives the median nearest positive-to-negative force crossing across
54 anchors at the final checkpoint, by requested speed. Missing/multiple roots
remain in the full per-curve data. These are **static fixed-input force crossings**,
with previous actions and lateral/rotational state held fixed, not predicted
closed-loop equilibrium speeds.

| Arm /seed | 4 | 6 | 10 | 12 | 16 | 18 |
|---|---|---|---|---|---|---|
| λ=.95 /17 | 1.17 | 1.25 | 1.34 | 1.37 | 1.44 | 1.47 |
| λ=.95 /29 | 2.18 | 2.45 | 2.75 | 2.85 | 3.01 | 3.09 |
| λ=.95 /43 | 5.21 | 6.85 | 9.13 | 9.75 | none ≤30 | none ≤30 |
| λ=1 /17 | 6.09 | 7.44 | 9.08 | 9.66 | 22.19 | 29.27 |
| λ=1 /29 | 4.77 | 5.93 | 7.62 | 8.32 | 9.59 | 11.27 |
| λ=1 /43 | 4.83 | 6.23 | 8.21 | 9.01 | 13.79 | 18.64 |

Lambda-1 seed 43 has a restoring crossing in all 54 anchors at every command.
All 54 are within one unit of requests 4/6; none are within one at requests
10/12/16; **28/54** are within one at request 18. At the shared anchor 51,
request 12 balances at **9.012**, close to LF-065's actual rest-start endpoint
**9.049**. Its training request 10 balances at **8.213**, so this limitation is
already present at a trained command and is not only unseen-command interpolation.

For lambda-1 seed 29, request-12 balance is **8.321** at the same anchor versus
the physical rest endpoint **8.187**. Request-18 balance is **11.268**, versus
the physical endpoint **10.060** while that trial is still changing slowly.
Seed 17's force curves must not be read as successful flight: its frozen steering
still misses the heading criterion and its near-target training coverage is poor.

For the lambda-.95 seed-43 control, no restoring crossing appears below speed 30
at requests 16/18. At request 12, only 25/54 curves have one; the table's median
uses those 25 and does not imply the remaining states balance. All other table
cells except the explicit missing roots have 54 crossings.

Zeroing previous-action inputs does not remove the lambda-1 seed-43 pattern:
final median crossings remain **4.833/6.231/8.213/9.012/13.790/18.610**. This is
a sensitivity check, not a modified runtime observation contract.

### Feedback strength improves late, while a low balance persists

At anchor 51 for lambda-1 seed 43:

| Update | Request-12 balance | Request-18 balance | Request-18 acceleration slope |
|---|---|---|---|
| 32 | 9.16 | none ≤30 | +0.045 |
| 64 | 8.51 | none ≤30 | +0.011 |
| 96 | 9.07 | none ≤30 | -0.020 |
| 128 | 9.01 | 18.64 | -0.103 |

Acceleration slopes have units (units/s²)/(unit/s). At request 18, passive braking
contributes **zero** local slope because native braking is capped there; the
final **−.103** slope comes from the policy's changing actions. By comparison,
at request 6 the total slope is **−.895**, of which **−.831** is passive braking
and **−.064** is policy response. Low-speed stability benefits greatly from native
force physics; high-speed correction must come from learned control.

As a local scalar approximation only, a slope of −.103 corresponds to a response
time of roughly **9.7 seconds**, before accounting for history/heading/coupled
dynamics. That is consistent with LF-065's slow request-18 rest trajectory and
six-second acquisition miss; it is not an additional physical test. The improving
late slope means all learning has not stalled. The request-12 crossing, however,
does not move toward 12 at these selected later checkpoints. More time alone is
therefore not established as the solution.

### Near-target updates are asymmetric

These are the frozen **last-32-update** groups for ready states within three
speed units below/above the command. Scores are local normalized-advantage
Gaussian mean-logit scores; positive favors increasing that actuator logit.
Acceleration changes compare the actual before/after policies at the **same
saved input**, not different states along a trajectory.

| Seed | Request | Side | Rows | Throttle score | Brake score | Mean Δacceleration | Rows corrected |
|---|---|---|---|---|---|---|---|
| 29 | 4 | below | 225 | +0.0432 | +0.1313 | -0.0879 | 52.0% |
| 29 | 4 | above | 1914 | +0.0039 | +0.0943 | -0.1965 | 92.6% |
| 29 | 10 | below | 3842 | +0.0330 | -0.0290 | +0.1790 | 75.8% |
| 29 | 10 | above | 147 | +0.0624 | +0.2643 | -0.2851 | 53.7% |
| 29 | 16 | below | 1085 | -0.0686 | +0.0074 | +0.1266 | 60.2% |
| 29 | 16 | above | 1777 | -0.0223 | +0.0125 | -0.0942 | 63.5% |
| 43 | 4 | below | 283 | +0.1375 | -0.1071 | +0.0818 | 61.5% |
| 43 | 4 | above | 1897 | -0.0647 | +0.0635 | -0.1431 | 75.2% |
| 43 | 10 | below | 3914 | +0.0391 | +0.0294 | +0.1443 | 71.2% |
| 43 | 10 | above | 2118 | +0.0102 | +0.0246 | -0.0142 | 46.6% |
| 43 | 16 | below | 508 | +0.1129 | +0.0068 | +0.0141 | 38.2% |
| 43 | 16 | above | 689 | -0.0219 | +0.0622 | -0.5105 | 99.9% |

For seed 43 at command 16, the **689** near-overspeed rows receive a mean
acceleration change of **−.5105**, with **99.9%** corrected. The **508**
near-underspeed rows receive only **+.0141**, with **38.2%** corrected. Their
throttle score is positive (+.1129), but brake score is also slightly positive
(+.0068). Actual throttle rises .000656 and brake rises .000385 on average.
Good local acceleration credit is not sufficient to produce a strong total update.
These groups occur in only four/five updates respectively; the row counts are
not independent trials or a statistical significance claim.

The pattern is not uniform across commands or seeds. Seed-43 command-10
underspeed rows receive +.1443 on average, with 71.2% corrected. Seed 29's
command-16 underspeed rows receive +.1266 even though their average throttle
score is negative. Local score signs are not the full shared-parameter PPO
update; aggregation, clipping, optimizer history and other samples matter.
Error-signed actuator-score fields are interpreted only in forward near-target
groups; use actual force changes and raw scores for reverse motion.

Seed 17 has **no ready near-target rows at commands 10/16** over the full run,
and none at command 4 during the late window. Its frozen steering failure remains
a separate limitation. Full update/group tables for both arms/all seeds and all
four frozen windows are retained, including empty groups and nonready exposure.

### Commands compete through shared updates

Post-run descriptive analysis uses the already saved anchor-51 response histories,
with no new model inference. Across all 128 updates in lambda-1 seed 43, the
correlation between changes in target acceleration at commands 10 and 16 is
**.997**; correlations for 4/10 and 4/16 are **.989/.993**. Seed 29's corresponding
values are **.999/.987/.989**. Most update-to-update shifts therefore move these
fixed-command outputs together, even when their required corrections differ.

For seed 43 during updates 97–128, **15** updates contain command-10 training
rows. Their changes at the fixed command-10 target sum to **+.8861 units/s²**.
The remaining **17** updates, with no command-10 rows, sum to **−1.2458**, yielding
**−.3597** overall. Seed 29 shows a similar cancellation over the full run:
**+7.1402** from updates containing command 10 versus **−6.5274** without it.
These are sums of actual before/after fixed-state changes, not gradient components
attributed solely to one command. Updates can contain more than one command, and
the shared optimizer state also carries earlier history.

This supports command competition as a concrete hypothesis. It does not prove
that shared parameters, batch composition, normalization or representation is
individually responsible. It also does not erase the measured late feedback gain.
`transfer-analysis.json` records all six runs and both full/late windows.

### Verification and metadata correction

The seven original processes exit zero, including the original verifier. During
post-run inspection, the reused LF-062 grid helper is found to store fractional
local-error labels in an integer tensor, truncating ±.25 to zero. **The input
velocities and all policy responses used the correct offsets.** Original
verification used command labels and row order, so it did not catch this metadata
defect. No numerical policy result or physical outcome depends on the bad label.

Preserve the original source tar, results, response files and verifier log. Add
float-valued metadata in separate `local-metadata-v2.pt.gz` files, a fractional
offset regression test, and a verifier that checks metadata against generated
inputs. Record the two changed audit/test sources separately in
`validation/revision-v2`; do not regenerate policy responses or rewrite original
artifacts. The corrected eighth process passes in **3.02 external seconds**.
All original response/training/run-result hashes remain unchanged; both global
result versions are retained. The 14 original tests and 15 corrected tests pass.

Verification covers **22,680 native/Python comparisons**, **786,432 saved
likelihood rows**, **393,216 exact stopping actor/distribution rows**, **774
frozen-parent checkpoints**, **5,670 paired initial local rows** and **59,292
paired initial dense rows**. A post-run overlap check matches **5,832** local
response rows to LF-065's initial/final feedback panel within the declared
inference rounding tolerance. No RNG or model weights change.

Retain **948,672 dense responses, 1,462,860 local history responses and 118,584
neutral-history responses**, 1,631 original sources, the two declared source
corrections, 1,560 copied inputs, all 128 update summaries per run, tests/logs,
runtime, original/corrected metadata and both verifier results. All **61 prior
manifests**, copied input origins and LF-065's full sealed archive remain
unchanged. Actual new physics/training/optimizer steps are all **zero**; there
are no new checkpoints, policy exports, confirmation use or promotion. See
[results](data/flight-motor-balance-v1-results.json) and
[archive receipt](data/flight-motor-balance-v1-archive.json).

## Interpretation and next decision

The audit narrows the problem: seed 43 can now learn restoring speed feedback,
but it is weak above the native brake cap and its command-specific balance is
still wrong at several speeds. Useful local updates coexist with large changes
from training on other commands. Seed 29 shares this cancellation pattern;
seed 17 remains limited by its original steering. No single universal failure
mechanism is established across all seeds.

Design a bounded paired test of **command-balanced collection within each
optimization update**, using lambda 1 and the existing protected residual. Compare
the existing episodic command schedule with a schedule representing all training
speeds together. Keep tasks, reward, model, original starts, all three seeds,
strict physical gates and original stopping protection. Match collection shape,
GAE horizon and total sampling/optimization budgets between the two arms; freeze
the scheduler and persistent-slot details before execution rather than treating
this as an automatic extension of LF-065.

This tests the observed command competition without assuming that balancing alone
will fix it. Shared parameters, incomplete command conditioning and finite/noisy
credit may still limit learning. The read-only audit does not isolate their causal
contributions. No new training is authorized by the report itself, and no model is
selected from the diagnostic checkpoints. Confirmation remains unused; navigation
is unqualified. Preserve every original result and failure.
