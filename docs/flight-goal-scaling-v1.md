# Function-preserving goal scaling pilot

**Status: complete; stopped at update 16, 2026-10-24.** Experiment
`learned-flight-goal-scaling-v1-20261024` follows LF-044's weak goal response in
the retained holding policies. Test whether changing the optimization scale of
goal features improves physical position recovery while preserving holding.

## Sole comparison and initial-function check

Both arms warm-start the original recovery policies (LF-042 update 0), seeds
17/29/43, with matched empty Adam states and identical collectors/RNG. Keep head-only
value gradients, learning rate .000075, quarter noise/rho zero, the signed-family
cases, reward, 60 Hz control, thirty-tick dwell and all physical criteria unchanged.
Stop both arms at update **16**. No longer continuation is authorized by this pilot.

`schema` retains schema-v1. `goal` uses explicit `goal-only32-v1`: multiply only
the two ship-local goal-displacement inputs by 32, leaving velocity and every other
feature unchanged. On weights-only initialization, divide the corresponding columns
of query and fusion1 weights by 32. Export folds the factor back into those weights.
This preserves the starting function and the 39,447-parameter inference architecture.
The changed parameterization affects gradients and their interaction with Adam and
global gradient clipping; it is not a claim that those optimizer effects are absent.

Before training, require all six initial exports to match their original exports
byte-for-byte. Compare training-model, folded-export and native outputs on LF-044's
fixed eight anchor instances per seed, with 26 goal observations each. Training and
folded Python outputs must match exactly; native raw tolerance is 1e-5 absolute and
relative, native control tolerance 1e-5. Stop if this preflight fails.

Production changes are limited to the explicit scaling mode in model.py, its
function-preserving weights-only initialization in initialization.py, and early mode
validation in run.py. Schema and legacy task scaling remain unchanged. Warm-start
hash checks and strict checkpoint/config/source identity remain enforced. No source
normalization is allowed by actual resume. The audit alone may normalize these three
declared source hashes to compare the schema arm against LF-042's head-only control.

## Physical endpoints and frozen gates

Evaluate all sixteen primary recovery cases and sixteen original holding cases at
updates 0/16. Require initial exports and physical traces to match the originals.
Require the schema control to reproduce full LF-042 states at updates 0/1/16 and
final physical score records after only the audit source normalization. Both arms'
first collected rollout must match the parent; all six 15→16 restarts must reproduce
the entire final model, optimizer, collector, RNG and history exactly.

The paired usefulness gate requires **at least two seeds jointly** to:

- Gain ≥1 position success and ≥1 overall recovery success over both their original
  policy and their matched schema-control endpoint.
- Complete at least one nonforward position case (left, backward or right).
- Retain ≥12/16 original immediate holds, with ≥2 in every heading stratum and no
  physical failures; primary recovery must also have zero physical failures.

Report existing entry-stage and stronger progression gates separately. Larger goal
response alone cannot pass a physical gate. Inspect successful position directions,
sampled position successes/eligible rows, KL stops and optimizer steps for all seeds.
Evaluate final goal response on the same retained LF-044 inputs; reuse the verified
preflight as the initial comparison. Do not select favorable checkpoints or seeds.

## Budgets and retention

Twenty processes: one preflight, six 16-update pilots, six 15→16 restart checks,
six endpoint/response checks and one verifier. Training processes ≤1800 seconds
(1820 watchdog); preflight/extra/verification ≤600 (620 watchdog). No retry.

Use 104,448 newly collected training ticks, ≤1,632 optimizer steps and ≤21,600
active-trace restoration ticks. Physical evaluation totals 384 episodes and
≤518,400 ticks. There are 192 native/Python physical-observation parity comparisons
(192 reset/setup ticks), plus 1,248 preflight and 1,248 final goal-response comparisons:
**2,688 comparisons total**. Goal-response inputs are copied, so no new anchor replay
or counterfactual flight is needed. Config updates=64 remains capped by stop-after=16.

Freeze 1,571 sources, six configs, inputs, runtime/dependencies and all 40 previous
manifest hashes before inference or training. Thirty focused tests passed, covering
function preservation, isolated goal gradient scaling, legacy compatibility, hash
validation, strict resume and joint physical gates. Native runtime is unchanged from
the retained 421-pass/two-ignored baseline. Retain 108 new checkpoints, 24 pilot/resume
exports, six preflight exports, every physical trace and response output. No native
teacher/handoff, confirmation, promotion or general-navigation claim follows here.

## Results

Goal-only scaling preserves all 48 holds but adds no position recovery; the paired usefulness gate fails.

| Arm | Seed | Recovery: initial → final /16 | Original holds: initial → final /16 | Final settled / position / velocity / spin, each /4 | Sampled successes | KL stops /16 |
|---|---:|---|---|---|---:|---:|
| schema | 17 | 10 → 10 | 16 → 16 | 4 / 0 / 2 / 4 | 7 | 0 |
| goal | 17 | 10 → 10 | 16 → 16 | 4 / 0 / 2 / 4 | 7 | 0 |
| schema | 29 | 9 → 11 | 16 → 16 | 4 / 1 / 3 / 3 | 67 | 0 |
| goal | 29 | 9 → 9 | 16 → 16 | 4 / 1 / 3 / 1 | 71 | 0 |
| schema | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 100 | 0 |
| goal | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 100 | 0 |

Both arms retain **48/48 original immediate holds**. The schema control ends at
**34/48 recovery successes**, exactly reproducing LF-042; goal scaling ends at
**32/48**, equal to the original total. Seed 29 accounts for the difference, with
two fewer spin-case successes than its matched control. Neither arm adds a position
success: both finish at **2/12**, the same forward cases already completed by the
original policies. No sideways or backward position case succeeds.

The fixed paired usefulness gate fails in every seed, and stronger progression
fails for both arms. The entry-stage exposure/retention gates pass; broader physical recovery remains
unqualified. There were no collision, damage,
escape or ship-loss failures in the physical evaluations; unsuccessful cases timed
out. No longer run or model promotion follows this result.

Both arms execute all 768 pilot optimizer steps across their three seeds and have
zero KL stops. Thus the observed lack of benefit here cannot be attributed to the
early KL stopping seen in earlier experiments. Goal scaling changes the learned
weights and some outcomes, but does not produce the intended recovery improvement.
This bounded test provides no evidence that goal-only scaling benefits recovery.
The cause of weak goal conditioning remains unresolved.

### Position experience and goal response

Goal scaling yields 178 sampled successes versus the control's 174, but both
have only three sampled position successes: seed 29 case 1, seed 43 cases 1/17,
all forward. The same cases complete at the same ticks (232, 272 and 281) in both
arms. Position-eligible rows are 0/236/154 under goal scaling versus 0/253/154 under
schema scaling. No sampled sideways/backward position success is present.

The amplified parameterization also fails to develop a substantial goal-direction
response in the fixed diagnostic contexts. At the final checkpoint the maximum
right-minus-left steering contrast is **.000980** under goal scaling versus
**.001007** under schema scaling. All contrasts are below .05; correct-sign counts
remain 0/6/0 out of eight anchors per seed in both arms. Full goal sweeps produce
some larger ranges, but no corresponding physical position gain.

Each row below takes the maximum over the eight fixed start/later anchor instances
for that seed. Full per-context differences, original commands and native outputs
remain in the detailed results. Throttle/brake values are percentage-point spans;
steering is on the −1 to +1 command scale.

| Arm | Seed | Right/left steering contrast: initial → final | Final full-goal steering span | Final throttle span, pp | Final brake span, pp | Position-eligible rows |
|---|---:|---|---:|---:|---:|---:|
| schema | 17 | 0.00000586 → 0.00000564 | 0.00001692 | 0.000931 | 0.000882 | 0 |
| schema | 29 | 0.00075182 → 0.00100656 | 0.00626954 | 0.016391 | 0.024012 | 253 |
| schema | 43 | 0.00001692 → 0.00001763 | 0.00007310 | 0.005362 | 0.003314 | 154 |
| goal | 17 | 0.00000586 → 0.00000588 | 0.00001764 | 0.001083 | 0.000918 | 0 |
| goal | 29 | 0.00075182 → 0.00098029 | 0.00779095 | 0.014818 | 0.030258 | 236 |
| goal | 43 | 0.00001692 → 0.00001829 | 0.00008946 | 0.011060 | 0.007421 | 154 |

### Verification and retention

All initial exports match the saved originals byte-for-byte; training-model
outputs match folded inference exactly in the preflight. All initial physical
traces and first collected rollouts match the originals. All six 15→16 restarts
reproduce complete model, optimizer, collector, RNG and history states. Schema
controls reproduce LF-042 at updates 0/1/16 and its final physical scores after
only the declared three source hashes are normalized for audit comparison. Actual
resume remains strict. Initial optimizer/collector/RNG states are matched, and
initial goal-arm weights differ only by the declared inverse goal-column scale.

All **30 focused tests**, **2,688 native/Python comparisons**, physical dwell/reward
reconstruction and twenty process limits pass. Every declared process exits zero;
none was retried. Use **104,448 newly collected training ticks**,
**6,568 trace-restoration ticks**, **1,632 optimizer steps**
including restart checks, and **67,691 evaluation ticks** in
**384 episodes**. Goal-response diagnostics reuse stored inputs and add no flight
trials. Native runtime remains at its retained 421-pass/two-ignored baseline.

Preserve 1,571 sources, 170 copied inputs, six frozen configs, 108 new checkpoints,
24 training/resume exports and six preflight exports. All 40 previous manifests and
copied input sources remain unchanged. Source/runtime/config hashes and archive
contents are verified. See [results](data/flight-goal-scaling-v1-results.json) and
[archive receipt](data/flight-goal-scaling-v1-archive.json). Local retention is not
an off-machine backup; no commit, confirmation use or promotion is implied.

### Next decision

Keep schema scaling as the legacy default and goal-only32 as an explicit
experimental mode. Preserve the original holding policies and the successful
head-only retention evidence. Stop this comparison at update 16.

Next, freeze a bounded audit of the goal information and training gradients using
the saved update-0 and update-15 states from both arms and all three seeds. Replay
their first and last pilot rollouts, verifying the recorded collector results,
without applying optimizer updates. Measure goal sensitivity through the hidden
layers, activation saturation and the policy-gradient contributions from each
task family to the goal-input connections. Include the sampled steering and
advantages in position cases. This should help distinguish a weak internal goal
path from training experience or credit that fails to reinforce useful corrections.
Choose the next architecture or exploration change from that evidence; do not
increase the scale or duration automatically. This diagnostic has not run here.

