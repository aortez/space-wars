# Learned goal-residual branch pilot

**Status: complete; stopped at update 16, 2026-10-24.** Experiment
`learned-flight-goal-residual-v1-20261024` follows LF-046's goal-path attenuation
audit. Test whether a direct learned goal path improves physical position recovery
while retaining the existing holding skill.

## Architecture and comparison

Compare the unchanged `schema` architecture with `residual`, using seeds 17/29/43
and the original holding policies retained by LF-045. Both arms use schema scaling,
head-only value gradients, learning rate .000075, quarter noise/rho zero, the same
signed-family tasks, reward, globally normalized advantages, optimizer settings,
60 Hz control and thirty-tick arrival dwell. Stop every pilot at update **16**.

The branch is `2 → 16 tanh → 3`, added to the actor's raw steering/thrust/brake
outputs before the existing action transforms. Its inputs are the two ship-local
goal coordinates divided by 12 physical units (the observation's goal/300 times
25). This fixed task scale is independent of the main encoder's input scaling.
Initialize the hidden weights orthogonally with gain sqrt(2), hidden bias zero,
and final weight/bias zero. The branch adds **99 parameters**, for **39,546 total**.
It bypasses both existing fusion tanh layers. The value head remains attached to
the original hidden features; the entire existing policy remains trainable.

Construct the branch after the base model inside an isolated RNG scope, preserving
base initialization and subsequent sampling RNG. Hash-checked weights-only loading
may extend a legacy model with an untouched zero-output branch. It cannot silently
discard a trained branch. The zero final layer preserves the initial function;
the first gradient can train that final layer, then train the hidden layer. Added
parameters participate in the existing global clipping/Adam update, so this is an
architecture comparison, not an assertion that optimizer interactions are absent.

Use architecture `polar-conv8x8-coordinates-set-attention32-mlp64x64-goal16-v2`,
export/checkpoint format 3, with unchanged observation/action schema v2. Legacy
architecture files remain format 2. Python and Rust reject incompatible version,
architecture and tensor combinations. Actual checkpoint continuation retains
strict source/config/runtime identity and never normalizes hashes.

Production edits are limited to model, initialization, execution export loading,
checkpoint identity/loading and early run validation, plus Rust inference and the
bridge's supported-architecture list. For audit comparison only, normalize those
five Python source hashes and the explicitly retained old/new bridge hashes when
comparing the schema control against LF-045. All other state must match exactly.

## Preflight and physical gates

Before training, require exact initial base weights, RNG and Python/native outputs
between arms on all eight retained anchor instances per seed, 26 goal observations
each. Legacy exports must match the originals byte-for-byte. Residual exports have
their distinct architecture and extra weights. Folded/training Python outputs must
agree exactly; native tolerance is 1e-5 absolute/relative on raw outputs and 1e-5
on controls. Unit checks also exercise nonzero branch outputs, saturated base
layers, malformed exports and fresh-process checkpoint continuation.

Evaluate all sixteen primary recovery cases and sixteen original holding cases
at updates 0/16. Initial physical traces and first collected rollouts must match
the originals. The schema control must reproduce LF-045 at checkpoints 0/1/16
and final physical results after the declared audit identity normalization. Check
all six exact 15→16 restarts, including weights, Adam, collector, RNG and history.
Initial Adam moments are empty with identical hyperparameters; the residual arm
has four additional parameter tensors in its parameter group.

Retain LF-045's paired usefulness gate: **at least two seeds jointly** must gain
≥1 position success and ≥1 overall recovery success over both the original policy
and matched control; complete ≥1 nonforward position case; retain ≥12/16 original
immediate holds with ≥2 per heading stratum; and have zero physical failures in
both suites. Report existing entry and stronger progression gates separately.
Larger goal response alone cannot pass a physical gate. Report every seed, sampled
position successes and eligible rows, goal-response contrasts, branch parameter
changes, KL stops and optimizer counts. No favorable checkpoint/seed selection.

## Budgets and retention

Twenty processes: one preflight, six pilots, six restart checks, six endpoint/response
checks and one verifier. Training processes ≤1800 seconds (1820 watchdog), other
processes ≤600 seconds (620 watchdog). Stop on any failure; no scored retry or
automatic extension. Config updates=64 remains capped by stop-after=16.

Use 104,448 new training ticks, ≤1,632 optimizer steps and ≤21,600 active-trace
restoration ticks. Physical evaluation totals 384 episodes and ≤518,400 ticks.
Add 192 reset/setup ticks for endpoint parity; the 1,248 preflight and 1,248 final
goal-response comparisons reuse saved inputs without physics. Total native/Python
comparisons: **2,688**. No new anchor replay or counterfactual flight.

Freeze 1,575 sources, six configs, copied inputs, the rebuilt native runtime,
dependencies and all 42 previous manifest hashes before experiment inference or
training. Retain every checkpoint (108), pilot/resume export (24), preflight export
(6), physical trace, response output, command, test log and timing. Preserve the
initial unit-test fixture-path failure and its correction; unit tests are separate
from scored trials. Local retention is not off-machine backup. This introduces no
native teacher, target-action labels or handoff. No reserved confirmation, promotion
or general-navigation claim follows this pilot.

## Results

The learned goal branch preserves all 48 holds and changes goal response, but adds no position recovery; the paired usefulness gate fails.

| Arm | Seed | Recovery: initial → final /16 | Original holds: initial → final /16 | Final settled / position / velocity / spin, each /4 | Sampled successes | KL stops /16 |
|---|---:|---|---|---|---:|---:|
| schema | 17 | 10 → 10 | 16 → 16 | 4 / 0 / 2 / 4 | 7 | 0 |
| residual | 17 | 10 → 10 | 16 → 16 | 4 / 0 / 2 / 4 | 9 | 0 |
| schema | 29 | 9 → 11 | 16 → 16 | 4 / 1 / 3 / 3 | 67 | 0 |
| residual | 29 | 9 → 10 | 16 → 16 | 4 / 1 / 3 / 2 | 69 | 0 |
| schema | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 100 | 0 |
| residual | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 100 | 0 |

Both arms retain **48/48 original immediate holds**. The residual architecture
finishes with **33/48 recovery successes**, compared with **34/48** in the matched
schema control and **32/48** at initialization. Seed 29 has one fewer spin recovery
than the control. Both arms finish at **2/12 position successes**, the same two
forward cases already completed by their original policies. No sideways or backward
position case succeeds or enters the goal radius in final development evaluation.

The paired usefulness gate fails in all three seeds, as does stronger progression
for both arms. The weaker entry/exposure gates pass. No physical failures occur in
either evaluation suite; unsuccessful attempts reach their deadlines. Both arms
execute all **768 pilot optimizer steps** with **zero KL stops**, so early stopping
does not explain this result. This is a failed physical-improvement test, not a
qualified navigation model. There is no continuation or model promotion.

### Position experience and goal response

All four new branch tensors change in every seed. Maximum absolute output-weight
changes are .00462/.00397/.00401 for seeds 17/29/43; hidden-weight changes are
.00190/.00194/.00155. The extra path is trainable from its zero-output start.

The final full-goal steering spans are **.02640/.00753/.04132** in the residual arm,
versus **.0000169/.00627/.0000731** in the controls. However, right-minus-left
steering contrasts at eight units remain small: absolute maxima
**.000521/.001699/.002642**, all below the retained .05 diagnostic threshold.
Correct-sign counts are **0/0/8 of eight contexts** for residual seeds 17/29/43,
versus **0/6/0** in the controls. Thus stronger dependence on goal coordinates does
not establish useful directional control; two seeds have the wrong lateral sign
throughout this fixed probe. These are local response diagnostics, not flown
counterfactuals or a causal attribution of the entire failure to one component.

The residual arm has **178 sampled training successes** versus **174** in the
control. Both have only **three position successes**, all forward: seed 29 case 1,
seed 43 cases 1/17. Their completion ticks remain 232/272/281. Position-eligible
rows are **0/255/150** in the residual arm, versus **0/253/154** in the controls.
More aggregate training successes did not yield new position competence.

Each response-table entry takes the maximum over eight fixed start/later contexts.
Thrust/brake spans are percentage points; steering uses the -1 to +1 command scale.
The full signed contrasts and outputs remain in the retained results.

| Arm | Seed | Absolute right/left contrast: initial → final | Final full-goal steering span | Final thrust span, pp | Final brake span, pp | Correct lateral sign /8 | Position-eligible rows |
|---|---:|---|---:|---:|---:|---:|---:|
| schema | 17 | 0.00000586 → 0.00000564 | 0.00001692 | 0.000931 | 0.000882 | 0 | 0 |
| schema | 29 | 0.00075182 → 0.00100656 | 0.00626954 | 0.016391 | 0.024012 | 6 | 253 |
| schema | 43 | 0.00001692 → 0.00001763 | 0.00007310 | 0.005362 | 0.003314 | 0 | 154 |
| residual | 17 | 0.00000586 → 0.00052059 | 0.02640268 | 0.093788 | 1.163149 | 0 | 0 |
| residual | 29 | 0.00075182 → 0.00169864 | 0.00752622 | 0.052640 | 0.932121 | 0 | 255 |
| residual | 43 | 0.00001692 → 0.00264220 | 0.04131855 | 0.168935 | 0.799721 | 8 | 150 |

### Verification and retention

Initial base weights, Python/native outputs and sampling RNG match exactly;
legacy exports match their saved originals byte-for-byte. Residual exports correctly
use their distinct version and include 99 extra parameters. All initial physical
traces and first collected rollouts match the originals. Every 15→16 restart
reproduces the entire model, optimizer, collector, RNG and history exactly.

Schema controls reproduce LF-045 at updates 0/1/16 and its final physical results
after only the declared five Python source hashes and rebuilt runtime identity
are normalized in audit comparison. Actual resume remains strict. Initial Adam
states are empty with identical hyperparameters; the branch adds four parameter
tensors. Both architectures retain identical observation/action contracts.

All **51 Python tests**, **421 native tests** (two ignored), **2,688 native/Python
comparisons**, physical dwell/reward reconstruction and all twenty process limits
pass. The first unit-test invocation found a bad fixture path in the new test;
that path was corrected before source freeze and the full suite passed. Both logs
are retained. No scored process failed or was retried.

Use **104,448 newly collected training ticks**, **7,322
trace-restoration ticks**, **1,632 optimizer steps** including
restart checks, and **66,818 evaluation ticks** in **384 episodes**.
Endpoint parity adds 192 reset/setup ticks. Saved goal-response inputs add no new
flight trials. Concurrent pilot elapsed times are 75.0–90.8 seconds; these are not
isolated throughput measurements.

Preserve 1,575 frozen sources, 172 copied inputs, six configs, the rebuilt runtime,
108 checkpoints, 24 pilot/resume exports, six preflight exports and every trace and
response output. All 42 previous manifests and copied input sources remain
unchanged. Source/runtime/config/archive hashes are verified. See
results (`docs/data/flight-goal-residual-v1-results.json`, in git history) and
[archive receipt](data/flight-goal-residual-v1-archive.json). Local retention is not
off-machine backup. No confirmation use, promotion or commit is implied.

### Next decision

Keep both architectures available explicitly; retain the schema architecture as
the default and preserve the original holding models. Stop LF-047 at update 16.
A direct goal path alone has not solved position learning. Do not increase model
size or extend this run on the basis of the larger response spans.

The next proposed comparison isolates **advantage centering across task families**.
LF-046 found that every inspected leftward row had negative globally centered
advantage, even when most raw advantages were positive, and position gradients
often opposed the other families. This does not prove the current update is wrong:
a negative advantage can correctly discourage an action. It supplies a concrete
hypothesis to test now that the direct goal path is available.

Use the same residual architecture and original zero-output warm starts in both
arms. Compare current whole-rollout mean subtraction with subtraction of each
family's rollout mean, retaining the same whole-rollout raw-advantage standard
deviation as the divisor. Change only centering, not per-family variance scaling,
reward, exploration, tasks, value-gradient routing or training budget. This tests
whether between-family offsets obscure useful within-family action differences;
it does not assume those sampled actions are competent.

Freeze the mapping, gradient/likelihood checks and a paired three-seed pilot before
execution. Require exact initial behavior and first rollouts, reproduction of the
LF-047 residual control, strict restart checks and the same joint physical
position/holding gate. Record family advantages and parameter updates without
substituting their signs for physical success. No centering change has been
implemented or trained here; weak exploration/experience remains unresolved.

