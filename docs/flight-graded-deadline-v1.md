# Graded deadline comparison v1

Completed comparison, 2026-10-23: **neither arm passed the learning or progression
gate; the graded reward did not improve arrival.** Experiment
`learned-flight-graded-deadline-v1-20261023`. The original pre-run specification
is retained separately in its archive. Parent:
[learning-signal audit](flight-learning-signal-audit-v1.md). The user authorized
a bounded partial-progress score after the audit verified identical complete
returns among surviving timeouts from the same state. Hypothesis: a graded
deadline outcome will give proximity lasting training value and improve travel
and arrival within the fixed budget.

## Frozen objective change

Two fresh arms with learner seeds **17, 29, 43**:

- `control`: `deadline_distance_weight=0`, exactly the original -2 timeout reward.
- `graded_deadline`: `deadline_distance_weight=4`.

At a real deadline only:

```
excess = max(final_COM_distance - 3, 0)
terminal_reward = -2 - deadline_distance_weight * excess / (30 + excess)
```

The fixed **3-unit** radius is the accepted arrival position tolerance. The
**30-unit** scale gives half the extra penalty at 30 units outside that radius,
within the current 4–80-unit starting-distance envelope. The weight **4** bounds
deadline rewards between -6 and -2, preserving separation from physical failure
(-10) and success (+10). These are declared experimental settings, not optimized
coefficients. No tuning or alternate scales are selected after seeing results.

| Final distance | Control deadline reward | Graded deadline reward |
| --- | --- | --- |
| ≤3 | -2 | -2 |
| 10 | -2 | -2.757 |
| 20 | -2 | -3.447 |
| 33 | -2 | -4 |
| 80 | -2 | -4.879 |

The grade uses final distance, not the best distance visited, so a fast fly-through
followed by departure is not credited for its earlier closest point. It is absolute
proximity, not progress relative to a particular task's starting distance. No bonus
is granted for deliberate failure, and time cannot be extended by controller action.
The additional term is part of the **base terminal reward**, so potential telescoping
does not cancel it. For equal-length episodes from the same state, the complete
return difference is gamma^(N-1) times the difference in terminal grade.

This first test grades distance only. Timeouts within the position radius still
tie even if speed/spin/dwell differ; successful low-speed, low-spin arrival remains
the stronger separate outcome. Intermediate potential shaping, velocity coefficient
.25, spin coefficient .25, distance coefficient .5, their scales, terminal potential
zero, time cost -.001 and all other terminal rewards are unchanged. No graded
reward is applied at a rollout boundary or budget-censored endpoint.

Both arms use the original **lambda=.98**, gamma=.9995, 256-tick rollouts in four
persistent slots, independent Gaussian latent exploration (rho=0), the same
39,445-parameter model/critic, observations, optimizer and **60 Hz** physical
decisions. Keep learning rate .0003, four epochs, minibatch 256, clip .2, value
weight .5, entropy weight .01, gradient clip .5 and target KL .03. Only the deadline
distance weight differs between arms. No native labels, decisions, takeover or rescue.

## Tasks, budgets and gates

Keep `clear-control-v1`: stationary goals in strata 4–12, 12–24, 24–48 and 48–80;
initial speed maxima 1/2/4/6 and spin maxima .2/.4/.6/.8; full headings/bearings,
altitude 140–150, zero gravity/planet spin and no added obstacles/edits. Training
partition 4110001 has 16 accepted tasks per stratum; development 4110002 has four.
Keep native geometric admission/rejection audits private to generation.

Physical success remains distance ≤3, relative speed ≤1 unit/s and absolute spin
≤.2 rad/s for **30 complete consecutive ticks**. Final heading is free. Deadline
remains 1,800 ticks. Initial/final deterministic evaluation uses all 16 development
tasks and physical outcomes, never graded reward as a substitute for success.

Each run receives **128 updates / 131,072 training ticks**, with a **1,200-second**
wall guard including evaluation. Six runs total **786,432 training ticks**, plus
at most 345,600 development ticks. No budget extensions, replacement seeds,
checkpoint search or added individual failures. Run concurrently and report actual
concurrent timing. Incomplete/nonfinite/infrastructure-failed runs cannot pass gates.

Predeclared gates:

1. **Arrival benefit:** graded final successes exceed paired control by ≥4/16 in
   at least two seeds, and the graded arm passes the learning gate.
2. **Learning:** ≥4/16 final successes and gain ≥4 versus initialization in at
   least two seeds, separately per arm.
3. **Progression:** ≥12/16 final successes, gain ≥8, ≥2 successes per stratum and
   zero physical failures in at least two seeds, separately per arm.

Only a progression-passing arm gets reserved confirmation seed 4110003, four tasks
per stratum and all three final policies. Require ≥12/16, ≥2 successes per stratum
and zero physical failures in at least two seeds. Conditional maximum 172,800 extra
ticks if both qualify, 300-second wall guard per policy. Otherwise leave confirmation
unused. No obstacle training in this comparison.

Inspect final/minimum distance and distance reduction, goal entries, speed/spin at
entry, dwell, collision/damage, thrust/brake use, thrust duration and optimizer
diagnostics. These can show partial travel without satisfying a gate. Report final
distance by seed/stratum alongside initial distances so stopping is not mistaken
for improvement against a drifting control. Do not compare cross-arm reward totals
as evidence of physical competence. Qualification uses final policies; relative
best references keep the existing physical selection rule.

## Verification and retention

Before learning, test control equivalence, monotonicity/bounds/finite inputs, equal
reward inside the fixed position tolerance, isolation to deadline terminals, lasting
complete-return differences and preservation of success/timeout/failure ordering
through the full 1,800-tick discounted budget. Exercise real deadline transitions
with identical physics/actions across reward settings and verify exact fresh-process
resume in both arms. Keep policy/exploration/velocity/credit tests passing.

Paired initial model/optimizer/sampler/RNG states, task admissions and initial
evaluations must match. Check trained exports against Rust on 96 starts total.
Read-only comparison with the prior credit experiment's control may verify exact
baseline reproduction. Old checkpoints retain their archived source/config contract;
current code does not silently accept an incompatible exact resume.

Freeze selected dirty/untracked source, this document and six configs before the
retained build/run. Keep all **774 checkpoints**, twelve exports, completed/active
training traces, evaluation traces, latest/best references, commands and validation
logs. Seal source/runtime/results with a SHA-256 inventory; twelve prior experiment
manifests remain unchanged. Local retention is not an off-machine backup; external
dependency/toolchain binaries are excluded. No commit or clean checkout is implied.

## Results

All six runs completed 128 updates each: **786,432 training ticks** and **315,912
development ticks**, with concurrent elapsed times of 662.6–700.9 seconds per run.
The frozen formula, budgets, tasks and gates were not revised during training.
See compact results (`docs/data/flight-graded-deadline-v1-results.json`, in git history) and the
[archive receipt](data/flight-graded-deadline-v1-archive.json).

| Measure | Control | Graded deadline |
| --- | ---: | ---: |
| Final physical successes | 0/48 | 0/48 |
| Final deadlines | 48 | 48 |
| Final goal-radius entries | 0 | 0 |
| Final collisions / damage | 0 / 0 | 0 / 0 |
| Training successes | 1 | 0 |
| Training collisions | 15 | 17 |
| Training deadlines | 203 | 202 |
| Budget-censored attempts | 12 | 12 |
| Training goal-radius entries, including censored attempts | 13/231 | 12/231 |
| Maximum training dwell | 30 ticks | 18 ticks |
| Training thrust fraction | 13.5% | 20.3% |

Every seed had zero initial and final successes. Both learning and progression
gates failed, as did the paired arrival-benefit gate (gains 0/0/0). Confirmation
partition 4110003 remains unused. No obstacle training or model promotion followed.

### Stopping is not travel

All six final policies used **zero thrust**. Every graded policy braked on every
tick. Control seed 17 also braked continuously, seed 29 never braked, and seed 43
braked on 3.14% of ticks. None entered the goal radius.

All seeds share the same development tasks. Their average initial distance is
32.40 units; the stratum means are 8.09, 19.01, 37.65 and 64.86 units. Final means:

| Arm / seed | Stratum 0 | Stratum 1 | Stratum 2 | Stratum 3 | Mean distance reduction from start |
| --- | ---: | ---: | ---: | ---: | ---: |
| Control 17 | 8.04 | 18.94 | 37.38 | 65.20 | +0.01 |
| Control 29 | 12.89 | 41.09 | 59.41 | 113.40 | −24.29 |
| Control 43 | 12.69 | 23.18 | 48.60 | 104.60 | −14.86 |
| Graded 17 | 7.39 | 19.57 | 37.34 | 65.15 | +0.04 |
| Graded 29 | 8.33 | 18.24 | 37.67 | 64.46 | +0.23 |
| Graded 43 | 8.00 | 18.66 | 37.10 | 64.99 | +0.22 |

The graded policies finish closer than the drifting controls, but remain near
their own starting distances. This is evidence of stopping behavior, not directed
travel or improved arrival. During sampled training, thrust increased in every
seed, but goal entries did not increase in aggregate and no graded attempt completed
the required dwell. Four attempts per arm met all instantaneous tolerances at least
once; meeting them briefly does not qualify as arrival.

### Accounting and validation

Every completed training episode's terminal term was checked against its actual
outcome and final distance. Control deadlines were exactly −2; graded deadlines
ranged from −5.639 to −2.424. All twelve censored attempts per arm had zero terminal
term. The old audit's previously reported example can also be rescored arithmetically:
its 29.30-unit finish now returns −2.9553, versus −3.6923 for its 86.65-unit finish,
instead of both returning −1.6751. This uses the same archived trajectories and
does not add new physics or assert that either trajectory was successful.

All **29 Python tests** passed, including reward bounds/order, legacy equivalence,
real collector deadline transitions and exact fresh-process resume for both arms.
Python compilation and whitespace checks passed. All **96 trained Rust/PyTorch
parity fixtures** passed with identical actions and maximum absolute output error
1.43×10⁻⁶. All three controls reproduced the prior credit experiment's model,
optimizer, collector, RNG and update history bitwise through update 128. Paired
initial states, admissions and initial evaluations matched. The Rust runtime was
unchanged. There were no nonfinite updates or KL early stops; maximum update-mean
pre-clipping gradient norms were 1.59–1.90 in both arms, with clipping still .5.
Value losses have different reward targets and are not direct accuracy comparisons.

The archive retains all **774 checkpoints**, twelve exports, traces, frozen source,
runtime, configs, pre-run specification and analysis commands. All twelve prior
manifests remain unchanged. Local retention is not an off-machine backup.

### Interpretation and next decision

The new base reward removes the verified timeout tie outside the arrival radius,
but this correction alone did not teach travel within the declared budget. It does
not establish that reward timing, exploration, critic learning or model capacity
is the sole remaining cause. The grade still arrives only at the 30-second deadline;
rollouts span 4.27 seconds, and earlier decisions depend on learned value propagation.
The distance-only grade also cannot distinguish speed/spin among timeouts inside
the radius. Those limits were declared before the run.

Before another broad comparison, discuss a bounded, easier approach diagnostic:
nearby destinations, initially aligned headings and low speed/spin, with the same
full action ownership and strict arrival criteria. Its purpose would be to establish
whether the learner can acquire thrust followed by braking before restoring arbitrary
headings and longer travel. Freeze its distribution, reward and gates separately;
no such training was performed here. Keep deadline weight zero as the default,
60 Hz decisions, existing physical criteria and the unused confirmation partition.
