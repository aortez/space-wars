# Learning gain under mixed execution v1

Completed diagnostic, 2026-10-23: **0/192 initial arrivals versus 137/192 trained
arrivals under the same mixed execution rule. All three models passed the learning-gain
finding rule.** Experiment `learned-flight-learning-gain-v1-20261023`; the original
pre-run specification is retained separately. Parent:
[steering-noise ablation](flight-steering-ablation-v1.md). The user approved making
the mixed execution mode explicit and comparing original untrained versus trained
checkpoints under that same mode before expanding task difficulty.

## Question and execution contract

Did the retained training improve physical arrival under the candidate
[`mean-steering-sampled-buttons-v1`](design/learned-flight-execution-v1.md) execution
contract? Both checkpoints execute the network steering mean and independently sample
thrust/brake. Validate and retain the machine-readable descriptor before each worker.
This is an explicit candidate diagnostic contract, not a production default or
qualification change. No training, optimizer step, altered reward, new model or task.

Use original saved **update-0 checkpoints** from the aligned-approach experiment for
learner seeds **17/29/43**. Reuse its trained update-128 weights and the previous
ablation's **192 retained mixed-execution trials** as the trained baseline. Do not
reinitialize models approximately from learner seeds or select different checkpoints.

Use the same sixteen development tasks, partition **4210002**, with four repetitions
per model/task and the same formula
`5310000 + learner_seed*10000 + repeat*1000 + candidate_index`, repeat 0–3. The initial
and trained evaluations have identical task starts and common underlying random draws.
Their probabilities and actions may differ because their weights and visited states
differ. Continue consuming the unused steering draw to preserve button streams.

Initial and trained checkpoints have different saved steering deviations. Compare
standardized draws using **each row's own saved deviation**, rather than applying the
trained deviation to both. Verify pairing on every shared tick before either episode
ends, within 2e-5 absolute tolerance. Check exact initial physics equality; actor outputs
are expected to differ across checkpoints. Source/runtime/checkpoint identities must
match, and all initial tensors must remain unchanged after inference.

## Scope, budgets and finding rule

Only the aligned stationary-start task is tested: four 4–6/6–8/8–10/10–12-unit strata,
zero gravity/planet spin, no added obstacles or terrain edits. Preserve all three
learned controls, 60 Hz, 1,800-tick deadlines and arrival within 3 units at speed ≤1
and absolute spin ≤.2 for **thirty consecutive ticks**. No native guidance or rescue.

Run **192 new initial-model episodes**, 64 per seed, with at most **345,600 physics
ticks**, and **1,200 seconds per worker** including trace writes. Reuse the trained
baseline's **177,917 ticks** without new simulation. Preflight is separately capped
at 1,000 ticks; the declared tests consume **128**. No extensions, replacement trials,
new task generation or reserved confirmation use.

Predeclare a *material learning gain* finding only if at least two trained models:

- Achieve ≥25% success and gain ≥25 percentage points (at least sixteen additional
  successes out of 64) over their own initialization under the same execution rule.
- Achieve ≥4/16 successes in at least three of the four trained repeats.

This is the same diagnostic rate/gain/repeat threshold used in the preceding work,
now applied in the initial-to-trained direction. It is not a rewritten qualification
gate. Report paired gains and losses, outcomes per model/repeat/stratum, collision and
damage, goal entry, dwell, action probabilities and physical speed/spin diagnostics.
Pooled rates are descriptive; four repetitions share sixteen tasks, not sixty-four
independent starts or extra trained models. Incomplete or infrastructure-failed
workers cannot support a positive finding.

A positive result would establish a learning effect for these checkpoints under this
candidate execution rule on this development set. It would not establish which
training component caused improvement, or qualify broader flight, obstacles, gameplay
integration or fresh-start generalization. The candidate was selected using development
evidence; later independent acceptance must remain separately specified. Both reserved
partitions (4110003 and 4210003) remain unused here.

## Verification and retention

Preflight tests reject incompatible execution descriptors, normalize paired draws
with different checkpoint scales, verify repeatable initial rollouts and an exact
trained archived prefix, check unchanged weights and validate the direction and
boundaries of paired learning-gain accounting. Keep production learner and Rust code
unchanged; new diagnostic code stays outside the checkpoint's source-hashed package.

Freeze this document, the execution descriptor/design and exact source before retained
runs. Copy and hash three initial and final checkpoints/exports, contracts, admissions,
three trained result files and all 192 trained physics/probability trace pairs. Retain
all 192 new initial trace pairs, commands and runtime identity. Seal with SHA-256 checks
on every file; sixteen prior experiment manifests and all referenced inputs remain
unchanged. Local retention is not off-machine backup; external dependency/toolchain
binaries are excluded. No commit or clean checkout is implied.

## Results

All **192 new initial-checkpoint episodes** completed in **184,513 physics ticks**,
plus **128 preflight ticks**. The trained results reused **177,917** archived ticks.
Workers took 133.7–138.7 concurrent seconds. There were no optimizer steps, new training,
checkpoint substitutions or budget extensions. See compact results (`docs/data/flight-learning-gain-v1-results.json`, in git history)
and [archive receipt](data/flight-learning-gain-v1-archive.json).

| Model | Initial arrivals | Trained arrivals | Gain | Initial collisions | Trained collisions |
| --- | ---: | ---: | ---: | ---: | ---: |
| 17 | 0/64 | 56/64 (87.5%) | +87.5 percentage points | 63 | 0 |
| 29 | 0/64 | 43/64 (67.2%) | +67.2 percentage points | 64 | 0 |
| 43 | 0/64 | 38/64 (59.4%) | +59.4 percentage points | 64 | 0 |
| Pooled, descriptive only | 0/192 | 137/192 (71.4%) | +71.4 percentage points | 191 | 0 |

The remaining initial outcome was one deadline in model 17. All 55 trained failures
were deadlines. Every initial repeat had zero successes; trained repeat counts were
14/14/14/14, 10/11/11/11 and 9/9/10/10. **All three models pass the predeclared
learning-gain finding**, including repeat consistency. There are 137 paired gained
successes, no lost successes, and 55 pairs where neither checkpoint succeeded.
"Neither succeeded" does not mean identical failures: the trained arm eliminated
physical failures on these tasks.

The candidate decoder is the same on both sides. This comparison therefore establishes
a training-related improvement in these retained policies under that decoder, beyond
the previously measured execution-rule improvement. It does not attribute the gain
to one reward term, optimizer setting, perception branch or network weight group.

### Untrained models fly through the goal without slowing enough

All **192 initial-model attempts entered the goal radius**, but none met all
instantaneous tolerances or accumulated dwell. Across 6,960 initial decision states
inside the radius, speed compliance was **0%**, while spin compliance was **100%**.
This is a concrete distinction between passing through the destination and arriving
under control. Most initial trajectories subsequently collided far from the goal.

The trained policies entered the radius in **143/192** attempts, and **137** completed
the full thirty-tick arrival dwell. Their lower goal-entry count is not evidence of
worse arrival: safety and controlled settling improved substantially, though 49 trained
failures still never entered and six entered without completing dwell.

| Model | Initial thrust fraction | Trained thrust fraction | Initial brake fraction | Trained brake fraction |
| --- | ---: | ---: | ---: | ---: |
| 17 | 49.87% | 3.80% | 50.25% | 91.85% |
| 29 | 49.64% | 4.90% | 49.83% | 75.57% |
| 43 | 49.98% | 2.34% | 50.14% | 38.59% |

These are tick-weighted summaries on the trajectories each checkpoint visited.
Initial button probabilities were approximately .5; trained thrust probabilities
were much smaller. Training substantially changed the thrust/brake balance, but this
comparison does not isolate individual channel contributions or prove a sophisticated
route-planning strategy. In particular, model 43 reduced both button frequencies.

The trained per-stratum success counts remain 16/16/16/8 for model 17, 16/12/9/6 for
29, and 16/11/1/10 for 43 (sixteen repeated trials per stratum). Coverage is uneven;
only one model exceeded 75% overall success. The result supports a scoped learning
finding, not the broader progression or production qualification claims.

### Explicit candidate contract

[`mean-steering-sampled-buttons-v1`](design/learned-flight-execution-v1.md) is now a
versioned, machine-readable diagnostic execution contract. The workers validated
its exact descriptor and retained its hash. Both checkpoints use learned,
state-dependent steering without executed noise, plus the same independent button
sampling law at 60 Hz. The unused steering draw is consumed for reproducible pairing.
No production default, native gameplay integration or PPO exploration law changed.

The original and final checkpoints have different saved steering sampling deviations.
The pairing audit normalized each side with its own deviation and matched **124,327
shared action ticks** within **4.77×10⁻⁷**, below the frozen 2e-5 tolerance. Different
termination times limit the shared prefix; unmatched later ticks are retained but are
not claimed as paired. Initial physics states matched exactly; actor outputs correctly
differ because the weights differ.

### Validation, retention and next decision

All **four diagnostic tests** passed: incompatible-contract rejection, pairing across
different checkpoint scales, repeatable initial trajectories plus exact trained prefix
replay, and correctly directed learning-gain/paired-loss accounting. Compilation and
whitespace checks passed. Checkpoint source/runtime identities matched, all loaded
initial model tensors remained unchanged, and the retained trained models/results were
unchanged. No worker was incomplete or failed infrastructure checks.

The sealed archive retains all 192 new initial trace/probability pairs and all 192
trained pairs, original initial/final checkpoints and exports, the candidate contract,
frozen design/specification, source/runtime and commands. Sixteen prior manifests and
405 referenced parent files remain unchanged. Local retention is not off-machine backup.

**Next decision:** make the candidate decoder available in the routine evaluation
path, then separately freeze the next bounded training/evaluation recipe and its
repeat-based progression criteria. Preserve the architecture and physical requirements;
do not lower acceptance to fit the current models. Improve coverage within aligned
approaches before expanding difficulty or using reserved confirmation. This experiment
establishes a learning effect on the repeatedly used development set, not independent
confirmation. Neither reserved partition was used, and the old qualification gates
were not retroactively rewritten.

