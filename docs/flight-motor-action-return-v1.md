# Matched-state motor action/return probe v1

LF-064, 2026-10-25. C4 audit 20; learning-attempt count remains 31.
This specification is frozen before the probe. The user authorized comparing
alternative throttle/brake actions from identical states after LF-063 failed.

## Question and fixed policies

Do small corrective actuator changes improve physical tracking and observed
discounted rewards, and do the saved critic's TD/GAE estimates rank them similarly?
Use **both LF-063 arms, seeds 17/29/43, final update 128 only**. No optimizer,
new model, teacher labels or native guidance. Preserve every policy, source
checkpoint and previous archive. Zero-command behavior is unchanged by inspection:
no weights or runtime code are modified. This probe is not a skill qualification.

The original LF-063 training used independent Gaussian throttle/brake latents
and deterministic learned steering. Persistent exploration was an earlier stage;
it is not the sampling contract for these saved models. Use each frozen model's
actual command-gated sampling deviations, with no amplitude multiplier or AR state.

## Common real-state panel

Copy all **108 final development traces** for rest/slow/fast starts, three command
speeds (6/12/18), two bearings, both arms and three seeds. Also retain six final
checkpoints, six exports, the original task matrix, results and parent manifest.
No confirmation set or additional policy checkpoint is consulted.

Select **18 shared anchors**: three speeds ×two bearings ×three speed bands.
Underspeed uses the slow-start initial state (half the requested speed), overspeed
the fast-start initial state (1.5×). Their initial items must match across all six
runs. Near-target states are the earliest saved states in ticks 1–600 with
along-command error <=.25 in magnitude, lateral speed <=1, heading <=5 degrees,
spin <=.2 and no terminal outcome. Search the declared rest/slow/fast trace pool;
break ties by residual before movement, ascending seed, then case index. Fail
preparation if any cell is missing. No selection by return or candidate action.

The resulting six near-target prefixes are residual seed 29, fast-start cases
8/10 at tick 14, 24/26 at tick 21 and 40/42 at tick 34. Their signed speed errors
are approximately +.182/+.229/+.207. The twelve other prefixes are initial states.
Apply every saved policy to every anchor, preserving the whole native physical
state, command, elapsed time, previous action, dwell and retention history through
exact trace replay. No manual velocity injection or artificial state reset.

## Intervention and continuation

Each anchor/policy has **nine variants**: unchanged baseline; throttle latent
+/-.5 or brake latent +/-.5, each for either **one tick or 30 ticks**. The offset
is applied to the current learned actuator mean on each intervention tick. It
then ends, leaving the same saved policy in control. Steering is always the
learned deterministic output. Sigmoid transforms bound throttle/brake naturally;
there is no additional clamp or brake rule. A thirty-tick perturbation evaluates
an action sequence, not a single-action Q value.

Each variant runs for up to **120 ticks/two seconds** with the original physics,
reward and terminal rules. Compare two continuation modes:

- Deterministic means, including during the intervention.
- The original independent Gaussian sampling distribution, with **eight fixed
  noise replicates**. A dedicated generator seeded 640001 creates a frozen
  [18,8,120,2] standard-normal innovation tensor. Share the same draws across
  variants and all six policies at each anchor. During the intervention, add the
  declared offset to the sampled latent; afterward use the unmodified distribution.

Thus every alternative is paired with the same-state, same-noise baseline.
Deterministic baselines use native actions exactly. Physical failure terminates
immediately, receives its existing -5 penalty and remains in the evidence. Remove
terminated batch members by replaying the remaining live traces, with those replay
ticks counted separately. No rollout extends the original task deadline or treats
failure as success. Baselines receive no special corrective action.

## Returns and credit

Report horizons **1/30/120** from the intervention start: physical velocity error,
signed forward error, actual discounted reward, terminal status, critic bootstrap,
bootstrapped return, initial TD residual and first-step GAE. Use the unchanged
reward, gamma **.99**, lambda **.95** and production GAE function. Terminal states
bootstrap zero. If termination precedes a horizon, report the performed ticks and
the same stopped return; do not repeat its terminal penalty or invent later physics.

Observed returns exclude a learned tail. Bootstrapped returns append gamma^H V(sH)
only for nonterminal cutoffs; GAE uses all intervening saved value predictions.
Retain those values so apparent action ranking can be separated into observed
rewards and learned estimates. Report paired differences, eight replicate values,
means, variation and sign agreement, rather than selecting a favorable noise draw.
Retain actual first actions/latents/deviations to expose intervention magnitude.

These are new counterfactual trajectories, not the original training advantages
or actual PPO updates. Group advantage normalization uses a common affine
transformation within movement samples, so relative ranks alone do not establish
the direction of a full network/Adam update. The critic was trained on noisy
behavior; deterministic-policy returns have a different continuation distribution.
Eight noisy replicates and a two-second cutoff are local evidence, not unbiased
infinite-horizon values or proof of a sole optimization defect. Training uses
commands 4/10/16; these development probes use 6/12/18, so any diagnosis remains
subject to this command generalization difference.

## Verification, budgets and retention

Nine focused tests cover anchor selection, coordinate projection, prefix integrity,
paired noise/durations, exact production GAE, terminal accounting, reward/critic
ranking disagreement, batch termination and native replay. Two initial synthetic
test-fixture issues (aliasing in the fake bridge and exact float comparison) were
corrected before freeze; retain both test logs. The two native test executions
perform **8 new +12 replay physics ticks and six neutral setups** in total.
Neither required a production implementation correction. The existing runtime is
byte-identical to LF-063; its 5 native model and 48 inherited environment tests
remain inherited evidence, not new test executions.

Six probe processes, at most three concurrent, each capped at **900 internal /
920 external seconds**. One verification process capped at **600/620 seconds**.
Seven bounded processes; no automatic retry or training extension. The full matrix
has **8,748 branches in 972 saved batch files**, at most **1,049,760 new probe
physics ticks**. Initial-prefix restoration would take **67,068 ticks** without
early failures; allow up to **400,000 restore ticks per policy** for terminal
batch compaction, counted as overhead. Repeat all **108 deterministic baseline
traces** through native replay: at most **13,788 additional replay ticks**.
Count neutral setups independently (8,856 absent early batch compaction).

Require **78 exact original-source prefix/suffix matches**: all twelve initial
states in all six policies, plus the six near-target states in their source policy.
Native/Python comparisons at steps 0/29/119 and endpoints yield up to **34,992
comparisons**. Independently verify every performed action, value, reward and
credit summary from retained traces, inputs and frozen innovations. No criterion
is relaxed if an implementation or replay check fails.

Freeze sources, source archive, specification, input inventory, anchors,
innovations, runtime and runner hashes before execution. Preserve all **59 prior
manifests**. Retain all copied inputs, prefixes, full branch traces, value sequences,
metrics, stochastic draws, failures, test evidence and process logs. Verify original
input hashes again at seal. Require >=2 GiB free before freeze and retain a 512 MiB
per-process reserve on the artifact disk.

Archive `data/training-experiments/learned-flight-motor-action-return-v1-20261025`,
linked to `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-action-return-v1-20261025`.
Use the existing standalone checkout. Local retention is not off-machine backup.


## Results

All six policy probes and the aggregate verifier finish within budget, with
**no failed run, retry, physical failure, source correction or policy update**.
Every branch performs its full 120 ticks. All prescribed anchors, variants and
noise draws are retained; none is selected away after observing its outcome.

Two distinct findings emerge:

1. **Corrective braking already gets favorable local credit in the overshooting
   seed.** Across both seed-43 actors, all three commands, both bearings, both
   intervention durations and all eight noise draws, the **384** corrective
   comparisons (less throttle or more brake) improve observed two-second return.
   All **384 also improve TD and GAE relative to the same-state/noise baseline**.
   This probe therefore does not support suppressive braking credit as the local
   explanation for seed 43's overspeed failure at these states.
2. **GAE reverses some favorable underspeed return rankings.** All **144**
   sampled-mode corrective underspeed cells have positive mean observed return
   differences. GAE ranks **46** of them worse. Their reward-only GAE contribution
   is positive in all 46; the intervening critic terms reverse it. Observed return
   improves in all eight draws of every such cell; GAE is negative in all eight
   draws for 42 of the 46. This is a reproducible local discrepancy, not proof
   of a universally incorrect critic or a sole cause of failed learning.

A cell here is one policy, anchor, actuator direction and duration; its sampled
result averages eight paired draws. The 144 underspeed cells use six policies ×
six underspeed anchors ×two corrective actuator directions ×two durations.

| Arm /seed | Corrective underspeed cells | Mean observed return improves | GAE ranks the improvement worse |
|---|---:|---:|---:|
| Residual /17 | 24 | 24 | 4 |
| Residual /29 | 24 | 24 | 8 |
| Residual /43 | 24 | 24 | 12 |
| Movement /17 | 24 | 24 | 2 |
| Movement /29 | 24 | 24 | 8 |
| Movement /43 | 24 | 24 | 12 |

For seeds 17/29, extra braking from an initially overspeed state often worsens
the two-second return because their unmodified continuation already brakes too
strongly and soon becomes underspeed. An action can reduce instantaneous speed
error without improving the return of the following policy. Both directions of
each actuator change are retained; the analysis does not assume that an initially
corrective direction must win at every horizon.

### Concrete paired examples

Movement seed 43, first bearing, sampled continuation, 30-tick intervention:
the underspeed state requests 6 and starts at 3; the overspeed state requests 18
and starts at 27. Each number below is the mean difference from its matching
unperturbed baseline over eight common noise draws. Positive return/credit
differences rank the alternative higher.

| State /alternative | Observed 120-tick return difference | First GAE difference | Reward contribution to GAE difference | Critic contribution to GAE difference |
|---|---:|---:|---:|---:|
| Underspeed /more throttle | +0.05755 | -0.03962 | +0.01924 | -0.05886 |
| Underspeed /less brake | +0.02709 | -0.04164 | +0.00766 | -0.04930 |
| Overspeed /less throttle | +0.89457 | +0.12810 | +0.11212 | +0.01597 |
| Overspeed /more brake | +0.96967 | +0.14285 | +0.12273 | +0.02012 |

The reward contribution is sum((gamma*lambda)^t * reward_t); the remaining
contribution comes from the saved critic terms in GAE. This decomposition uses
the same trajectories and predictions, with no extra environment or optimizer.

In the overspeed braking example, the first applied brake increases by **6.31
percentage points** on average. Mean velocity error after two seconds falls
from **12.79 to 11.52**, and every sampled return difference is positive
(range **+0.9145 to +1.0103**). GAE likewise improves in every draw. The remaining
error is still large: this isolated intervention is not retained speed control.

In the underspeed throttle example, the first applied throttle increases by
**6.06 percentage points** on average. Observed return improves in every draw
(**+0.03910 to +0.07659**), while GAE falls in every draw
(**-0.05031 to -0.03146**). The unmodified policy eventually moves through the
target region: the extra throttle improves the accumulated return but leaves
larger endpoint error (**.612 versus .318**). That distinction and the finite
cutoff matter; neither a two-second return nor a single endpoint proves the
infinite-horizon action ordering. The same sign reversal also occurs for the
one-tick throttle and brake-release perturbations at this anchor.

### How much does the return estimator matter locally?

Across all **864** sampled-mode cells (six policies ×18 anchors ×eight
nonbaseline variants), compare the sign of each estimated difference with the
sign of its measured 120-tick return difference:

| Estimate | Matching signs /864 |
|---|---:|
| One-step TD | 306 |
| Current GAE, lambda .95 | 643 |
| 120-tick observed return plus final critic bootstrap | 855 |

The deterministic-mode counts are **294 /636 /846**, respectively. The long
bootstrapped target is equivalent to lambda-1 GAE over this segment, up to
floating-point evaluation order: intermediate value terms telescope, leaving
observed discounted reward plus the final nonterminal bootstrap minus V(start).
V(start) is identical across each paired comparison and cancels.

This is **sign agreement with the measured finite return, not critic accuracy
against a known infinite-horizon value**. The long target includes the same
measured rewards by construction; it is not an independent ground truth.
The result does show that reliance on intermediate value predictions can reverse
the local signal, and motivates a controlled test of less such reliance. It does
not establish that changing lambda will repair braking or yield a competent policy.
The overspeed comparisons already provide favorable current-GAE rankings.

`paired-differences.json.gz` retains all **23,328 paired horizon rows**;
`analysis.json` retains **5,184 cells** with means, ranges, standard deviations
and sign counts; `findings.json` contains the aggregate counts and examples above.
All cases, directions, horizons, deterministic continuations and noisy draws
remain available in the underlying traces and per-policy results.

### Work, validation and retention

All seven processes exit zero. Each six-policy probe takes **25.91–26.44 seconds
internally**, and verification takes **30.10 seconds**. All policies and the
global Torch RNG remain unchanged. There are **zero training ticks, optimizer
steps, new checkpoints or exports**. No model is promoted or qualified.

- **1,049,760 new probe physics ticks**, 67,068 initial-prefix restoration ticks,
  13,788 baseline replay ticks and 8,856 separately counted neutral setups.
  Preparation adds the declared 8 new /12 replay ticks and six setups from tests.
- **8,748 exact matched branch starts**, **108 complete native baseline replays**
  and **78 exact original-source continuations**. No physical failure or early
  termination occurs in the actual matrix; synthetic tests exercise that path.
- **34,992 native/Python comparisons**, plus independent reconstruction of all
  **1,049,760 performed actions and pre-action values**, all 8,748 endpoint
  values, reward-derived summaries and terminal/bootstrap accounting.
- **Nine focused tests pass** after the two documented pre-freeze fixture fixes.
  Both initial and corrected logs/native test traces are retained. The unchanged
  native model/environment tests remain inherited evidence.

Retain **1,625 frozen source files, 123 copied inputs, 18 complete anchor prefixes,
the frozen innovation tensor and all 972 branch-batch files**. Existing production
sources and native runtime are unchanged from LF-063; the two new Python sources
implement only this probe and its tests. Verify all **59 prior manifests** and
all copied input origins unchanged at seal. See the results mirror (`docs/data/flight-motor-action-return-v1-results.json`, in git history)
and [archive receipt](data/flight-motor-action-return-v1-archive.json).

## Interpretation and next decision

The reward supplies a useful local slope for seed-43 overspeed correction; the
current credit estimate also recognizes it at the tested final-policy states.
Persistent wrong-sign policy feedback therefore still needs an explanation in
state exposure, competing updates, representation/generalization or earlier
training dynamics. This audit has not isolated those mechanisms. The underspeed
counterfactuals establish a separate reason to investigate intermediate critic
dependence, rather than asserting that every braking failure is a reward defect.

The next proposed learning comparison is **GAE lambda .95 versus 1.0**, with
gamma .99, the protected residual architecture, original starting weights, all
three seeds, tasks, reward, independent exploration and 128-update budget held
fixed. Reproduce the existing residual control; preserve the original stopping
function and use the unchanged full acquisition/retention evaluation. Freeze a
separate specification before running. No architecture change, new training
cases, scripted actuator rule or simultaneous reward modification is proposed.

Lambda 1 removes intermediate critic terms from each rollout's return target;
it still uses the endpoint bootstrap and a learned value baseline. Its higher
variance and the difference between 120-tick probes and 256-tick training rollouts
are real uncertainties. The probe examined final policies on unseen commands,
not every earlier training update. This is a testable candidate, not a proven fix
or an automatically authorized extension of this audit. Original stopping remains
protected; general speed control and obstacle navigation remain unqualified.
