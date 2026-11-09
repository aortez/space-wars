# Credit assignment comparison v1

**Completed 2026-10-23: lambda=1 increased sampled thrust and goal-radius entries,
but both arms had 0/48 final arrivals and failed learning/progression gates.**

Specification and results for `learned-flight-credit-v1-20261023`. The exact
pre-run specification is retained separately in the archive. Parent:
[velocity shaping comparison](flight-velocity-shaping-v1.md). The user authorized
testing longer credit assignment after removing velocity shaping failed to improve
arrival. Hypothesis: less attenuation of delayed evidence within each rollout
will help learn goal-directed travel and arrival with the existing reward.

## Frozen comparison

Two fresh arms, each with learner seeds **17, 29, 43**:

- `control`: GAE **lambda=.98**, the existing setting.
- `long_credit`: GAE **lambda=1.0**.

Only `lambda` differs between paired configurations. Both use the **original
reward**, including velocity shaping coefficient .25, independent Gaussian latent
exploration (`gaussian-latent-ar1-v1`, rho=0), the same 39,445-parameter model,
critic, observation schema, tasks and 60 Hz observation/action/physics cadence.
No architecture, reward, rollout-length, exploration, spin-shaping or task changes.
No native decisions, labels, fallback, rescue or action holds.

Our estimator is:

```
delta[t] = reward[t] + gamma * live[t] * value[t+1] - value[t]
advantage[t] = delta[t] + gamma * lambda * live[t] * advantage[t+1]
critic_target[t] = advantage[t] + value[t]
```

Gamma remains .9995. The coefficient on a residual 180 ticks (three seconds)
later is about **.0241** in the control and **.9139** at lambda=1; at 240 ticks
it is **.0070** versus **.8869**. These are within-rollout residual weights, not
hard memory limits or the total influence of future outcomes. Rollouts remain
256 ticks per slot (4.27 seconds), and the tail bootstraps from the critic.
Episodes persist across rollouts, but sampled residuals do not propagate directly
across those boundaries in this implementation. Physical terminals zero bootstrap
and stop credit from leaking into a reset episode.

At lambda=1, targets become discounted sampled returns through the remaining
rollout plus its boundary bootstrap. This also changes critic training targets;
that is part of the single estimator-setting change, not a new critic architecture.
Longer credit may increase variance and still depends on the boundary value
estimate. The [GAE paper](https://arxiv.org/html/1506.02438v6) describes this
bias/variance tradeoff. This experiment does not test full-episode Monte Carlo
returns, longer rollouts or an improved critic, and a negative result cannot rule
out every longer-horizon method.

Keep four epochs, minibatches 256, learning rate .0003, clip .2, value weight .5,
entropy weight .01, gradient clip .5 and target KL .03. Keep the original potential
coefficients and scales, time cost -.001, terminal +10 success, -2 deadline and
-10 physical/controller failure. Terminal potential is zero; environment errors
abort. Keep fresh model initializations, without selecting a prior checkpoint.

## Tasks, budgets and physical gates

The unchanged clear-control-v1 recipe has four distance strata 4–12, 12–24,
24–48 and 48–80 units, speed maxima 1/2/4/6 and spin maxima .2/.4/.6/.8. Headings
and bearings span the full circle. Goals are stationary, altitude 140–150,
gravity and planet spin zero; no added obstacles or edits. Training partition
4110001 contains 16 accepted tasks per stratum, development 4110002 four. Preserve
native geometric admission and all rejection audits as private evidence.

Success remains distance ≤3, target-relative speed ≤1 unit/s and absolute spin
≤.2 rad/s for **30 complete consecutive ticks**, with free final heading and
deadline 1,800 ticks. Evaluate initial and final deterministic policies on all
16 development tasks. Physical gates do not use reward or partial arrival.

Each run gets **128 updates / 131,072 training ticks**, with a **1,200-second**
wall guard including evaluation. Six runs total **786,432 training ticks**, plus
at most 345,600 development ticks. Run concurrently where practical and report
actual concurrent timing. No budget extension, replacement seed or checkpoint
search. Report infrastructure/nonfinite failures; incomplete runs cannot pass.

Predeclared gates:

1. **Arrival benefit:** long-credit final success exceeds paired control by ≥4/16
   in at least two seeds, and the long-credit arm passes the original learning gate.
2. **Learning:** ≥4/16 final successes and gain ≥4 versus initialization, in at
   least two seeds, separately for each arm.
3. **Progression:** ≥12/16 final successes, gain ≥8, ≥2 successes in each stratum
   and zero physical failures, in at least two seeds, separately for each arm.

Only a progression-passing arm gets reserved confirmation partition 4110003,
four tasks per stratum, all three final policies. Confirmation requires ≥12/16
successes, ≥2 per stratum and zero physical failures in at least two seeds.
Conditional maximum 172,800 additional ticks if both qualify, 300-second wall guard
per policy. Otherwise keep confirmation unused. Obstacle training is excluded.

Report action use, thrust duration, goal entries, instantaneous tolerance matches,
dwell, minimum/final distance, closing ticks, thrust while closing, first-entry
speed/spin and entry followed by departure. Inspect optimizer diagnostics already
recorded (KL stops, clip fraction, entropy, value/policy loss, gradient norms).
These explain behavior and stability; they do not replace physical gates. Relative
best references keep the original rule; qualification uses final policies only.

## Verification and retention

Test delayed-signal weights at 60 Hz, lambda=1 discounted-return equivalence,
terminal isolation and rollout bootstrap, identical first-rollout actions/rewards
with changed advantage estimates, and exact fresh-process continuation. Keep
existing policy, exploration and velocity tests passing. Config/source contracts
continue to reject incompatible resume. Verify every trained export against Rust
on 16 development starts. Paired model/optimizer/sampler/RNG initialization and
task admissions/evaluations must match. Read-only comparisons may check that the
control reproduces its parent; no additional training is authorized by that check.

Freeze selected dirty/untracked source, this specification and all six configs
before retained learning; rebuild/capture the exact Rust executable and runtime
versions. Retain all **774 checkpoints**, twelve exports, completed and active
training traces, all evaluations, latest/best references, commands and validation
logs. Seal with SHA-256 inventory and verify ten previous manifests unchanged.
External dependency/toolchain binaries are excluded. Local retention is not an
off-machine backup; no commit or clean checkout is implied.

## Results

All six runs completed **128 updates each**, totaling **786,432 training ticks**
plus **315,912 initial/final development ticks**. Concurrent elapsed times were
589.8–629.4 seconds per run. Training including checkpoints achieved 286.9–291.4
ticks/s per learner; these are concurrent measurements, not isolated throughput.
No numerical failure, wall guard or KL early stop fired. Paired initialization,
task admissions and initial evaluation matched. Budgets and criteria were unchanged.

| Arm | Seed | Final arrivals | Training success / collision / deadline | Training thrust | Final thrust / brake |
| --- | --- | --- | --- | --- | --- |
| Control (.98) | 17 | 0/16 | 0 / 4 / 68 | 15.0% | 0% / 100% |
| Control (.98) | 29 | 0/16 | 0 / 7 / 68 | 14.5% | 0% / 0% |
| Control (.98) | 43 | 0/16 | 1 / 4 / 67 | 10.9% | 0% / 3.14% |
| Long credit (1.0) | 17 | 0/16 | 0 / 8 / 67 | 35.7% | 0% / 100% |
| Long credit (1.0) | 29 | 0/16 | 0 / 11 / 64 | 34.9% | 0% / 100% |
| Long credit (1.0) | 43 | 0/16 | 0 / 12 / 64 | 36.0% | 0% / 0% |

Every run also retained four budget-censored attempts. Training statistics include
those active traces. Initial evaluations had zero successes. Every final development
episode reached its deadline, without collision or damage; none entered the goal
radius. Paired success gains were `[0, 0, 0]`. Arrival benefit, learning and
progression all failed. Confirmation partition 4110003 remains unused and no
obstacle training followed.

### Behavior and optimization

Longer credit increased mean sampled thrust use from **13.5% to 35.5%**. Across
training attempts it entered the goal radius 22/238 times, versus 13/231 in the
control. However, none of those long-credit attempts ever met distance, speed and
spin tolerances simultaneously: maximum dwell was zero. It recorded 31 collisions,
195 deadlines and no successes, compared with 15 collisions, 203 deadlines and
one success in the control, plus twelve censored attempts per arm. Thrust bursts
remained short (mean 1.60–1.62 ticks versus 1.38–1.45); none reached 30 ticks.

The increased sampled thrust did not become useful deterministic behavior. All
final policies used zero thrust. Long-credit seeds 17/29 braked continuously;
seed 43 coasted without braking. Their mean best-distance improvements were
0.61/0.39/3.14 units, but none entered the goal radius and the coasting seed
finished 24.29 units farther away on average. These incidental approaches are
not evidence of learned goal-directed translation or a braking-only arrival issue.

Larger estimator targets were accompanied by larger losses and gradients. Maximum
update-mean pre-clipping gradient norms were **6.04/9.77/7.71** for long credit,
versus **1.59/1.88/1.90** for controls. Global gradient clipping remained .5;
all optimizer steps stayed finite, with zero KL early stops in every run. Mean
value losses were .146/.189/.191 versus .021/.029/.036. Because lambda changes
the targets, these losses do not directly establish which critic is more accurate.
The report retains first/last/mean/maximum optimizer diagnostics for inspection.

### Interpretation and next action

This specific increase in credit duration changed exploration outcomes but did
not improve final flight within the budget. It does not rule out longer rollouts,
full-episode returns, a better critic or more training; those were not tested.
Direct residual credit still ends at the rollout boundary and the tail remains
estimated. No network-capacity verdict or unique cause follows from these results.

After persistence, velocity shaping and this estimator change all failed to produce
deterministic travel, the next proposed work is a focused diagnostic of the actual
learning signal before another training comparison: inspect how useful versus
unproductive turn/thrust/brake segments receive advantages, how boundary value
estimates compare with subsequent outcomes, and whether policy and critic gradients
interfere. Define the diagnostic scope and replay budget separately, using retained
trajectories/checkpoints where possible. The current results do not establish any
of those mechanisms. No additional training or criterion changes occurred here;
the default lambda remains .98 and original reward remains in place.

### Verification and archive

All **24 Python tests** passed, covering the existing policy/exploration/reward
suites plus delayed-signal decay, lambda=1 return equivalence, terminal isolation,
rollout bootstrap, identical first-rollout actions/rewards, changed advantages and
exact fresh-process continuation. Python compilation and whitespace checks passed.
No production training or Rust source changed; only the new experiment tests and
reporter were added. The captured rebuilt Rust executable is identical to the parent.

All three controls reproduced the parent controls **bitwise through update 128**:
model, optimizer, sampler/collector/trajectories, random state and training metrics.
Source contracts differ and incompatible resume remains rejected. All **96 trained
Rust/PyTorch parity fixtures** passed with matching actions and maximum absolute
output error **1.43×10⁻⁶**.

The results (`docs/data/flight-credit-assignment-v1-results.json`, in git history) retain per-seed physical
gates, trajectory diagnostics and optimizer statistics. The
[archive receipt](data/flight-credit-assignment-v1-archive.json) identifies all
**774 checkpoints**, twelve exports, traces, frozen source/runtime/configurations,
test logs and integrity evidence. Ten previous experiment manifests remain unchanged.
Retention is local, not an off-machine backup; external toolchain/dependency binaries
are excluded. Evidence is limited to three seeds and the declared stationary-goal
practice worlds. No obstacle-navigation competence is claimed.
