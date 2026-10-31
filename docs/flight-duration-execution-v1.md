# Training-duration by execution-mode comparison v1

**Status: completed, 2026-10-24 — one model improved sampled execution; two declined in both modes.** Experiment
`learned-flight-duration-execution-v1-20261024`, following the
[duration regression](flight-mixed-duration-v1.md). This is an evaluation-only audit
of saved policies, not another training run or a new qualification criterion.
The original pre-run specification is retained separately in the archive.

## Question and fixed comparison

Additional training reduced mixed-execution arrivals from 137/192 at update 128 to
84/192 at update 256, even as changing-weight training recorded additional arrivals.
Did frozen performance improve under the fully sampled execution used during
collection, while degrading under mixed execution, or did both modes deteriorate?

Use learner seeds **17/29/43**, the same sixteen aligned development starts from
partition **4210002**, and four seeded repeats. Complete the missing cell in this
two-by-two comparison:

| Frozen checkpoint | Fully sampled controls | Mean steering, sampled buttons |
|---|---|---|
| Update 128 | Reuse archived 11/64, 3/64, 6/64 | Reuse 56/64, 43/64, 38/64 |
| Update 256 | **Run 64 attempts per model** | Reuse 38/64, 40/64, 6/64 |

The prior duration experiment verified complete training-state equality at update
128 before continuing the same learning trajectories to update 256. No checkpoint
search, new tasks, optimizer steps, weight edits, curriculum changes or reserved
confirmation use. The old checkpoints' exact-resume rules remain strict; archived
exports are read only for inference and checked tensor-for-tensor against their
saved models. Current update-256 source/runtime contracts must match before inference.

Fully sampled execution is the unchanged independent Gaussian auxiliary latent law
used by collection (`gaussian-latent-ar1-v1`, rho=0). Steering executes tanh of its
sampled latent using that checkpoint's own saved learned deviation. Button means
are inverse-normal-CDF of clamped sigmoid logits, with unit deviation and zero
threshold. Draw the full steering/thrust/brake vector every tick. Mixed execution
consumes the same draws but executes the network's steering mean; both modes retain
learned control of all three channels. No native decisions, labels, fallback or rescue.

Use episode seed `5310000 + learner_seed*10000 + repeat*1000 + candidate_index`;
reset once per episode, with repeat 0–3 and retained candidate order. Pair standardized
innovations using each checkpoint's own actor means and steering deviation. Compare
new sampled-256 streams against sampled-128 and mixed-256 on their shared tick prefixes.
Identical random innovations do not imply identical state-dependent button decisions.

Keep 60 Hz, the existing model/observations, open-wing physics and stationary aligned
4–12-unit task distribution. Arrival remains distance ≤3, relative speed ≤1 and
absolute spin ≤.2 rad/s for thirty consecutive complete ticks. Deadline: 1,800 ticks.
Partitions 4210003 and 4110003 remain untouched. Native gameplay decoding is separate.

## Budget, findings and interpretation

- **192 new episodes**, capped at **345,600 physics ticks** and **1,200 seconds per
  model**, including trace writes and pairing analysis. Three concurrent workers.
- Reuse all 576 archived trials in the other three cells. No replacement trials,
  additional models or budget extension. An incomplete/infrastructure-failed worker
  prevents a completed finding; retain the failed work.
- Preflight cap: 1,000 ticks; the four declared tests use **160 ticks**. They check
  exact archived sampled-128 and mixed-256 prefixes, seeded repeatability, matching
  streams across changed steering deviations, unchanged tensors, export identity,
  malformed/incomplete pairing and finding-rule boundaries.

Report per-model, per-repeat and distance-group success, paired gains/losses,
physical failures, goal entries, speed/spin compliance, dwell and action probabilities
for all four cells. Recompute archived diagnostics with the same functions as the new
cell. Check every success independently against its last thirty actual physics frames.
No reward totals or training-episode counts substitute for frozen-policy success.

Classify each model descriptively: sampled success increased while mixed decreased;
both decreased; or sampled was unchanged while mixed decreased. Declare **material
opposing changes** only if at least two models gain ≥8/64 fully sampled arrivals,
have a positive sampled-success change in at least three repeats, and lose mixed
arrivals. This is a diagnostic finding, not qualification or a significance test.
Report smaller or heterogeneous changes explicitly rather than forcing a binary cause.

Opposing changes would support an execution-dependent effect of additional learning,
but not prove the mismatch caused the optimizer to change in that way. Declines in
both modes would show that mixed decoding alone cannot explain those regressions.
Even a positive finding does not establish a competent policy. Repeats share sixteen
starts and three models; they are not additional independent tasks or trained models.

## Verification and retention

Production training/evaluation source and the native bridge are unchanged. Reuse the
previously verified runtime; do not rerun old diagnostic loaders that require obsolete
source identities. Check new inference tensors before and after evaluation, native
initial physics against both references, first actor output against mixed-256, exact
executed actions against physics, and paired innovations within 2e-5.

Freeze this specification, selected dirty/untracked source, six model checkpoints and
exports, task admissions, all three reference cells and their summaries before the
retained full run. Preserve every new physics/probability pair, per-episode seeds,
logs, commands, analysis and source/runtime identities. Seal a new SHA-256 inventory,
verify all nineteen earlier manifests and referenced parent inputs unchanged, and
update the handoff/journal. Local retention is not off-machine backup. No commit or
clean checkout is implied.

## Results

All **192 new attempts** completed in **320,906 physics ticks**, plus 160 preflight
ticks. Workers took 239.4–267.4 concurrent seconds. Reused cells account for 324,138
sampled-128, 177,917 mixed-128 and 241,531 mixed-256 ticks. No model changed and no
optimizer steps, additional tasks, budget extensions or confirmation evaluations occurred.

| Model | Sampled: 128 → 256 | Change | Mixed: 128 → 256 | Change |
|---|---:|---:|---:|---:|
| 17 | 11/64 → 6/64 | −5 | 56/64 → 38/64 | −18 |
| 29 | 3/64 → 12/64 | +9 | 43/64 → 40/64 | −3 |
| 43 | 6/64 → 3/64 | −3 | 38/64 → 6/64 | −32 |
| Pooled, descriptive | 20/192 → 21/192 | +1 | 137/192 → 84/192 | −53 |

Only seed 29 met the per-model opposing-change rule: sampled success improved by
nine, with positive changes in three repeats, while mixed success fell. Seeds 17
and 43 declined under both modes. The predeclared **material-opposing-changes finding
failed**, because it required at least two models. This is heterogeneous evidence,
not a uniform explanation of the duration regression. Pooled sampled success was
10.4% then 10.9%; those nearby totals hide the different model-level changes.

All new unsuccessful trials reached the deadline: **21 successes, 171 deadlines,
zero collisions/damage**. All four cells had zero physical failures. New sampled
successes by repeat and increasing distance group (each entry is out of sixteen):

| Model | Repeats 0–3 | Distance groups 4–6 / 6–8 / 8–10 / 10–12 |
|---|---|---|
| 17 | 0, 3, 1, 2 | 3, 2, 1, 0 |
| 29 | 4, 2, 6, 0 | 7, 2, 2, 1 |
| 43 | 2, 0, 1, 0 | 3, 0, 0, 0 |

Under fully sampled execution, the two checkpoints shared only one successful paired
trial; nineteen old successes were lost and twenty new successes appeared. This is
not stable mastery of the same starts. At update 256, mixed execution retained twenty
of the sampled successes, gained 64 and lost one, producing 84 arrivals. Mixed
execution still substantially improves settling for these models; it does not erase
the learning regression.

### Approach deteriorated under both execution modes

| Mode | Goal entries at 128 | Goal entries at 256 | Full arrivals at 256 |
|---|---:|---:|---:|
| Fully sampled | 139/192 | 85/192 | 21/192 |
| Mixed | 143/192 | 86/192 | 84/192 |

Additional training reduced approach in both modes by similar amounts. With fully
sampled steering, 64 attempts entered without completing dwell and 107 never entered.
With mixed execution, only two entered without completing dwell and 106 never entered.
These are aggregate descriptions of paired trials, not claims of identical trajectories.

Fully sampled near-goal spin compliance improved from 58.7% to 63.3%, but remained
well below mixed execution's approximately 98.5%. Sampled speed compliance improved
from 78.5% to 82.5%. Better sampled settling coexisted with fewer approaches; the
overall sampled arrival count therefore conceals both effects. These compliance
figures use pre-decision states inside the radius, consistently across all four cells;
LF-023's separate post-step summaries use a slightly different alignment.

The per-model control changes remain different. Seed 29's sampled brake use rose
from 75.65% to 94.20%, and its near-goal speed compliance rose from 70.0% to 87.0%.
Seed 17 braked less (91.95% to 65.62%) and lost approach and arrivals. Seed 43's
sampled thrust frequency fell from 2.34% to 0.27%; goal entries fell from forty to
seven. These trajectory averages alone do not isolate an action channel's causal effect.

An additional descriptive check compares predictions at the **same sixteen starting
states**, avoiding the confound of different visited trajectories:

| Model | Mean initial thrust probability, 128 → 256 | Mean initial brake probability, 128 → 256 |
|---|---:|---:|
| 17 | 3.83% → 4.73% | 92.00% → 63.95% |
| 29 | 5.17% → 4.20% | 75.14% → 93.89% |
| 43 | 2.52% → 0.29% | 38.14% → 59.54% |

Seed 43's starting thrust probabilities fell from a 2.16–2.95% range to 0.25–0.35%.
This establishes a learned reduction at matched inputs, not just an averaging effect.
It does not establish which reward term or gradient contribution caused that change.

### Verification and retention

All four preflight tests passed. New trajectories matched both reference initial
physics, and the first update-256 actor outputs matched the mixed reference exactly.
Standardized innovations agreed within **7.16×10⁻⁷** across 300,536 shared ticks
against sampled-128 and 240,133 against mixed-256. Each comparison uses its own saved
steering deviation. Every model tensor was unchanged after evaluation.

Every success in all four cells passed the strict independent thirty-frame physical
check. Policy action records exactly matched executed physics actions. Recomputed
reference outcomes matched their archived counts. No incomplete trial or infrastructure
failure occurred. Python compilation and whitespace checks passed; production source
and runtime were unchanged.

The new archive retains all 192 new and 576 reused physics/probability trace pairs,
six model checkpoints/exports, 1,530 selected source files, 1,179 copied parent inputs,
frozen specification, commands, logs and analysis. All nineteen earlier manifests
and referenced parent inputs remain unchanged. See [compact results](data/flight-duration-execution-v1-results.json)
and [archive receipt](data/flight-duration-execution-v1-archive.json). Local retention
is not off-machine backup.

### Interpretation and next decision

Execution-dependent learning changes exist in seed 29, but **mixed decoding alone
cannot explain the regressions of seeds 17 and 43**. Approach deteriorated under both
modes. The audit neither identifies a unique optimizer/reward fault nor establishes
an architecture-capacity limit. The increased count of changing-weight training
successes was not a reliable measure of improvement in the final policies.

The next useful step is a separately frozen **retrospective PPO-update audit**, starting
with seed 43's matched-input thrust reduction between updates 128 and 256. Use the
saved checkpoints and reproducible rollouts to examine the advantage signal and
policy/value gradient contributions around that change. Distinguish pressure from
the learning objective from interference in the shared network before changing
training settings. No additional training budget, new task selection, qualification
change, native fallback or reserved confirmation use followed this diagnostic.
