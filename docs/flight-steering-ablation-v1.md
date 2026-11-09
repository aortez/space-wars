# Frozen steering-noise ablation v1

Completed diagnostic, 2026-10-23: **deterministic learned steering with sampled
buttons achieved 137/192 arrivals (71.4%), versus 20/192 (10.4%) with fully sampled
actions. All three models passed the predeclared improvement rule.** Experiment
`learned-flight-steering-ablation-v1-20261023`; its original specification is
retained separately in the archive.
Parent: [frozen action-execution comparison](flight-action-execution-v1.md). The
user approved testing deterministic steering with sampled thrust/brake after the
fully sampled policies produced occasional arrivals but frequent spin-limit violations.

## Intervention and paired baseline

Use the same final update-128 models **17/29/43**, the same sixteen aligned development
tasks (partition **4210002**) and the same four per-task repeat seeds as the parent.
Keep every model weight, task, action rate and physical requirement fixed. No training,
optimizer, new task generation, checkpoint search, native control or rescue.

The only intervention is executed steering:

- Parent: sample all three Gaussian latents, tanh the steering sample, threshold
  both sampled button latents at zero.
- New `mean_steering` mode: draw exactly the same full three-latent sample, discard
  the sampled steering value, and execute **tanh of the network's steering mean**.
  Execute the sampled thrust and brake values as before.

Steering remains learned and state-dependent, not hard-coded zero or a hand-written
controller. The unused steering draw is still consumed to preserve the button random
streams. Retain it in the probability log; its recorded standard deviation describes
the saved sampling distribution, not noise actually applied to steering in this arm.

Reuse all **192 archived fully sampled trials** as the paired baseline; do not rerun
or replace them. Each new episode uses the original seed formula:
`5310000 + learner_seed*10000 + repeat*1000 + candidate_index`, repeat 0–3.
Reset RNG and physics for each episode. For every shared tick before either paired
episode terminates, compare the standardized latent draws to the archived baseline
within 2e-5 absolute floating-point tolerance. Check identical initial physics and
actor outputs. The underlying randomness is paired; button choices need not remain
identical after trajectories diverge, because their learned probabilities depend on
state. Such downstream changes are part of the intervention's effect.

## Budget and physical criteria

New work: **192 episodes**, 64 per model, at most **345,600 physics ticks** and
**1,200 seconds per model** including trace writes. The archived baseline used
324,138 ticks, which are reused evidence rather than new simulation. Preflight has
its own 1,000-tick cap; the declared tests consume **128 ticks**. No extensions,
replacement repeats or added task cases.

Keep 60 Hz, stationary aligned starts 4–12 units from the goal, zero gravity/planet
spin, full ship physics, 1,800-tick deadlines and successful arrival within 3 units
at relative speed ≤1 and absolute spin ≤.2 for **30 consecutive ticks**. Final
heading is free. Reward is not a diagnostic score and is not changed.

Predeclare a *material arrival improvement* only if at least two models:

1. Reach ≥25% new success and gain ≥25 percentage points over their own fully
   sampled baseline (at least sixteen additional successes out of 64).
2. Reach ≥4/16 arrivals in at least three of four new repeats.

This retains the previous diagnostic finding rule, with the fully sampled baseline
in place of the deterministic baseline. Report gained/lost/both/neither-success
pairs, per-repeat and per-stratum outcomes, collisions/damage, goal entries,
instantaneous tolerances, dwell, final distance reduction, button probabilities and
speed/spin compliance inside the radius. Use pooled rates descriptively; repeated
trials on the same sixteen tasks are not additional trained models.

Incomplete or infrastructure-failed runs cannot support a positive finding. A
passing diagnostic establishes an execution-mode effect within these aligned
practice tasks, not reliable general flight or obstacle navigation. Existing
qualification gates are unchanged. No reserved confirmation, model promotion or
execution-contract change is automatic; those require separate scoped acceptance.

## Verification and retention

Test that only executed steering changes for identical actor inputs, RNG consumption
is unchanged, standardized draws stay paired when actor means differ, seeded native
rollouts repeat exactly, and the fully sampled preflight prefix equals its archived
trace/probability log. Check unchanged model tensors and paired-outcome/finding-rule
boundaries. Keep diagnostic code outside the unchanged source-hashed production
learner package; verify full checkpoint source/runtime contracts before inference.

Before retained runs, freeze this document, exact source and copied parent inputs:
three checkpoints/exports/contracts/admissions, three parent result files and all
192 baseline physics/probability log pairs. Reuse and hash the parent Rust runtime.
Afterward verify all new model tensors unchanged, physical traces complete and
latent pairing on every shared tick. Retain all 192 new trace/log pairs and all
paired baseline inputs, with commands, runtime versions and a SHA-256 manifest.
Fifteen prior manifests and every referenced input remain unchanged. Local retention
is not off-machine backup; dependency/toolchain binaries are excluded. No commit or
clean checkout is implied.

## Results

All **192 new episodes** completed in **177,917 physics ticks**, plus **128 preflight
ticks**. The **324,138** fully sampled baseline ticks were reused from the sealed
parent archive, not rerun. Workers took 128.6–153.8 concurrent seconds. There were
no optimizer steps or model changes. See compact results (`docs/data/flight-steering-ablation-v1-results.json`, in git history)
and [archive receipt](data/flight-steering-ablation-v1-archive.json).

| Frozen model | Fully sampled | Mean steering, sampled buttons | Success-rate gain | New successes by repeat |
| --- | ---: | ---: | ---: | --- |
| 17 | 11/64 (17.2%) | 56/64 (87.5%) | +70.3 percentage points | 14, 14, 14, 14 |
| 29 | 3/64 (4.7%) | 43/64 (67.2%) | +62.5 percentage points | 10, 11, 11, 11 |
| 43 | 6/64 (9.4%) | 38/64 (59.4%) | +50.0 percentage points | 9, 9, 10, 10 |
| Pooled, descriptive only | 20/192 (10.4%) | 137/192 (71.4%) | +60.9 percentage points | Three models on repeated tasks |

All three models meet both the rate/gain requirement and repeat-consistency
requirement. The predeclared **diagnostic finding passes**. This does not replace
the existing qualification gate or qualify an execution mode for deployment.
Both reserved confirmation partitions remain unused. All other new episodes were
deadlines; there was no collision or damage in either arm.

### Paired outcomes and distance coverage

| Model | Both succeeded | Newly succeeded | Lost success | Neither succeeded |
| --- | ---: | ---: | ---: | ---: |
| 17 | 11 | 45 | 0 | 8 |
| 29 | 3 | 40 | 0 | 21 |
| 43 | 6 | 32 | 0 | 26 |
| Total | 20 | 117 | 0 | 55 |

Every previously successful paired trial remained successful. New success counts by
distance stratum, with sixteen attempts per stratum/model:

| Model | 4–6 units | 6–8 units | 8–10 units | 10–12 units |
| --- | ---: | ---: | ---: | ---: |
| 17 | 16/16 | 16/16 | 16/16 | 8/16 |
| 29 | 16/16 | 12/16 | 9/16 | 6/16 |
| 43 | 16/16 | 11/16 | 1/16 | 10/16 |

Coverage remains uneven, particularly for model 43. The four repeats use the same
four tasks in each stratum; they do not provide sixteen distinct starts. This is
still the short, stationary, initially aligned task, not a test of turning toward
arbitrary goals, longer flight, gravity or obstacles.

### The principal change is successful settling

Fully sampled execution entered the goal radius in **139/192** attempts and completed
arrival in **20**. Mean steering entered in **143/192** and completed arrival in
**137**. Thus most of the gain is holding a successful arrival after entry, rather
than creating many more entries. All 143 entering attempts met the instantaneous
tolerances at least once.

Inside the goal radius, pooled spin compliance rose from **58.7% to 98.6%**, and
mean absolute spin fell from .195 to .035 rad/s. By model:

| Model | Fully sampled spin compliance | Mean-steering spin compliance | Mean-steering mean absolute spin |
| --- | ---: | ---: | ---: |
| 17 | 57.9% | 100% | .015 rad/s |
| 29 | 59.1% | 98.3% | .036 rad/s |
| 43 | 59.9% | 97.7% | .054 rad/s |

These are conditional tick summaries on different trajectories and durations.
Successful new episodes end promptly, reducing inside-radius observation time from
59,552 baseline ticks to 9,889 new ticks. Speed compliance on those states was
66.1% versus 78.5% before; that change alone does not establish worse braking because
occupancy and termination differ. The physical 30-tick success checks remain the
primary result.

Of the 55 new failures, **49 never entered the radius** (8/16/25 by model). Only six
entered and failed to finish: five for model 29, with maximum dwell 8–21 ticks, and
one for model 43, with 24 ticks. Mean final distance reduction improved from
+2.15/−5.41/−4.82 units to **+4.66/+1.95/+.91** by model. Remaining approach failures
and uneven coverage still need work.

### What stayed fixed

Model weights, observations, physics, task seeds, deadlines, arrival requirements,
60 Hz cadence and the button sampling law were unchanged. Thrust remained sparse:
3.80%/4.90%/2.34% of ticks, close to the baseline's 3.83%/5.01%/2.34%. The network's
state-dependent steering mean replaced its noisy steering sample. No hand-written
controller, target-facing correction or native takeover was introduced.

All **177,917 new action ticks** had a matching archived standard-normal innovation
for all three channels, with maximum reconstructed difference **4.77×10⁻⁷**, below
the frozen 2e-5 tolerance. Every initial physics state and actor output matched its
baseline. Button probabilities and resulting choices can diverge after the steering
intervention changes state; paired randomness does not mean forcing identical button
commands regardless of state.

The prior execution comparison showed that deterministic button thresholding disabled
all thrust and produced no approaches. This ablation now identifies a large harmful
effect of steering noise within the sampled execution mode. Together they explain
much of the observed evaluation failure on this narrow task. They do not prove the
model is adequate for general navigation or quantify learning gain under the new
mixed execution rule; initial models have not been evaluated under that rule.

### Validation, retention and next decision

All **four diagnostic tests** passed: steering-only intervention/RNG consumption,
paired innovations under changed actor means, real repeatable trajectories plus an
exact archived fully sampled prefix, and paired-outcome/finding-rule boundaries.
Compilation and whitespace checks passed. Full production source/runtime contracts
matched before inference; every model tensor remained unchanged. Executed steering
matched tanh of the actor mean on every tick. All successful traces passed the
unchanged 30-frame physical arrival check. No incomplete episode or infrastructure
failure occurred.

The archive retains all **192 new physics/probability trace pairs**, all 192 paired
baseline pairs, three input models/exports, exact source/runtime, specification,
commands and results. Fifteen previous manifests and all 399 referenced parent files
remain unchanged. Local retention is not off-machine backup.

**Next decision:** define deterministic learned steering plus sampled buttons as an
explicit candidate execution contract, then measure initial-versus-trained performance
under that same contract before extending training or task difficulty. This distinguishes
an execution improvement from demonstrated learning gain. Keep numerical acceptance
criteria and any later independent confirmation separately frozen; do not silently
replace the current deterministic qualification gate or use reserved tasks as development
data. No production default, execution contract, further training or model promotion
was changed by this diagnostic.

