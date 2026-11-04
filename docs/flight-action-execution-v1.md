# Frozen action execution comparison v1

Completed diagnostic, 2026-10-23: **sampled execution achieved 20/192 arrivals,
deterministic execution 0/48. The predeclared material-improvement threshold was
not met.** Experiment `learned-flight-action-execution-v1-20261023`; its original
pre-run specification remains separate in the archive.
Parent: [aligned-approach diagnostic](flight-aligned-approach-v1.md). The user
approved comparing sampled versus deterministic actions from the same frozen
models after sampled training often approached the goal while final deterministic
policies stayed still.

## Question and scope

Does executing the trained action distribution improve physical arrival compared
with the existing deterministic rule? Use only the three final update-128 models
from aligned-approach seeds **17/29/43** and their unchanged 16 development tasks
(partition **4210002**). No optimizer, training continuation, checkpoint selection,
new tasks, curriculum change or reserved confirmation use.

Each model executes two modes:

- **Deterministic:** existing Rust runtime rule: tanh of the steering mean, and
  button on when its logit is nonnegative (probability at least .5). Replay each
  development task once; require its complete trace to equal the parent archive.
- **Sampled:** the actual independent Gaussian latent action law used during
  training (rho=0), including steering noise and both button latents. Button means
  are inverse-normal-CDF of sigmoid(logit), with the existing 1e-6 tail clamp;
  unit button latent standard deviation and the saved learned steering deviation.
  Threshold sampled button latents at zero and tanh the sampled steering latent.
  Execute each task **four times**, with independently seeded randomness.

Per-episode RNG seed is `5310000 + learner_seed*10000 + repeat*1000 + candidate_index`,
with repeat 0–3; reset RNG and physics for every episode. The accepted candidate
indices are below 512, so streams have distinct seeds. Sampling uses the same
production distribution function. The order is deterministic mode first, then
sampled repeats in order, with candidates in the retained admission order. Per-task
seeding isolates samples from earlier outcomes and ordering. No AR history persists
across episodes; rho=0 is the frozen parent configuration.

This is a comparison of the **joint action execution rule**. Sampling changes steering
as well as buttons; an improvement cannot be attributed solely to thrust thresholding.
It also differs from the earlier training observations because weights are now fixed
and tasks are the same across modes. No native controller decisions, labels or rescue.

## Physical contract, budgets and finding rule

Keep 60 Hz, the existing world/ship/observation schema, 4–12-unit stationary aligned
starts, 1,800-tick deadline and arrival within 3 units at relative speed ≤1 and
absolute spin ≤.2 for 30 consecutive ticks. Physical outcomes determine success.
No reward-based score or altered arrival criterion is used.

Per model: 16 deterministic + 64 sampled episodes = **80**, capped at **144,000
physics ticks** and **1,200 seconds** including analysis and trace writes. Across
three models: **240 episodes / at most 432,000 ticks**. Preflight is separately
limited to 1,000 ticks; the declared tests consume **128**. No extensions, repeat
replacement or extra checkpoints. Retain partial work if a worker fails; incomplete
or infrastructure-failed diagnostics cannot support the finding rule.

Predeclare a *material arrival improvement* finding only if at least two models:

1. Achieve ≥16/64 sampled successes (25%) and ≥25 percentage-point gain over their
   paired deterministic success rate.
2. Achieve ≥4/16 successes in at least three of the four sampled repeats.

This is a diagnostic finding threshold, **not a replacement qualification gate**.
Report collisions/damage alongside any gain. Even a passing diagnostic does not
qualify general flight, change the existing deterministic gate or trigger reserved
confirmation. A separately accepted execution contract would be needed for deployment.
Failure to meet the threshold does not prove action selection irrelevant; approach
and settling diagnostics may still identify a useful difference.

Report per model, repeat and distance stratum. Pooled counts are descriptive only:
four repeats are repeated trials on the same tasks, not four additional trained
models. Record goal entries, instantaneous tolerance satisfaction, maximum dwell,
final distance reduction, thrust/brake frequency and outcomes. Retain every tick's
actor outputs, button probabilities, sampled latents and executed action, as well as
the physics trace. Summarize probabilities and speed/spin compliance both overall
and while inside the goal radius. Compare probabilities on visited states descriptively;
different execution rules visit different states.

## Verification and retention

Preflight verifies exact sampling-law equivalence, deterministic threshold semantics,
seeded repeatability, distinct per-episode seeds, an exact native trajectory prefix,
unchanged model tensors and finding-rule boundaries. The production training package
and Rust physics/runtime are unchanged; keep diagnostic code outside its source-hashed
package so archived checkpoint contracts remain valid. Check the full source/runtime
contract before inference and recheck every model tensor afterward.

Copy and hash the three final checkpoints, exports, development admissions, contracts
and all 48 parent evaluation traces. Reuse the parent's executed Rust binary and
record runtime versions. Freeze this specification, exact source and input inventory
before the retained runs. Verify all 48 complete deterministic replays, retain all
240 trajectories and probability logs, and seal the new archive with a SHA-256
inventory. Fourteen earlier manifests and all referenced parent files must remain
unchanged. Local retention is not off-machine backup; dependency/toolchain binaries
are excluded. No commit or clean checkout is implied.

## Results

All **240 episodes** completed, using **410,538 physics ticks**, plus the separately
recorded **128 preflight ticks**. Workers took 333.3–347.4 concurrent seconds each.
No model weights changed and no optimizer steps occurred. See [compact results](data/flight-action-execution-v1-results.json)
and [archive receipt](data/flight-action-execution-v1-archive.json).

| Frozen model | Deterministic successes | Sampled successes | Sampled rate | Successes by sampled repeat |
| --- | ---: | ---: | ---: | --- |
| 17 | 0/16 | 11/64 | 17.2% | 4, 4, 1, 2 |
| 29 | 0/16 | 3/64 | 4.7% | 2, 1, 0, 0 |
| 43 | 0/16 | 6/64 | 9.4% | 0, 1, 3, 2 |
| Pooled, descriptive only | 0/48 | 20/192 | 10.4% | Repeated tasks across three models |

Every remaining episode was a deadline. Neither mode had a collision or damage.
All **48 full deterministic trajectories exactly matched the parent archive**,
including the unchanged no-thrust behavior. There is a measured execution difference,
but none of the models reached the required 25% sampled success rate, and none met
the repeat-consistency requirement. The predeclared material-improvement finding is
therefore false. Existing qualification gates remain failed and unchanged; no
confirmation partition was used and no model was promoted.

Sampled successes by distance stratum (16 attempts per stratum per model):

| Model | 4–6 units | 6–8 units | 8–10 units | 10–12 units |
| --- | ---: | ---: | ---: | ---: |
| 17 | 6/16 | 2/16 | 3/16 | 0/16 |
| 29 | 2/16 | 0/16 | 1/16 | 0/16 |
| 43 | 2/16 | 2/16 | 0/16 | 2/16 |

### What action execution changes

Across **every visited decision state in both modes**, predicted thrust probability
was below .5. Deterministic execution therefore always disabled thrust. Sampled
execution still applied occasional pulses from the same model distribution:

| Model | Mean sampled thrust probability | Executed thrust fraction | Executed brake fraction | Steering latent standard deviation |
| --- | ---: | ---: | ---: | ---: |
| 17 | 3.78% | 3.83% | 91.95% | .593 |
| 29 | 5.02% | 5.01% | 75.65% | .624 |
| 43 | 2.38% | 2.34% | 38.56% | .571 |

These are tick-weighted summaries on sampled trajectories, not paired-state
sensitivity measurements. Pooled executed thrust was 3.73% versus zero in
deterministic mode. The distribution assigned low probabilities to thrust rather
than demanding no thrust at all; thresholding erased those occasional actions.
The joint sampled rule also changed steering and brake choices. This experiment
does not establish which individual change accounts for the arrival gains.

Unlike the preceding changing-policy training successes, these **20 successes use
fixed final weights on the same development tasks as the deterministic evaluations**.
They demonstrate occasional successful execution of those distributions. Without a
sampled initialization control, they do not measure how much training improved the
sampled policy, and the low rates do not establish flight competence.

### Approach is common; settling remains unreliable

| Model | Sampled goal entries | Briefly met all tolerances | Full arrivals | Mean final distance reduction |
| --- | ---: | ---: | ---: | ---: |
| 17 | 53/64 | 53/64 | 11/64 | +2.15 units |
| 29 | 46/64 | 46/64 | 3/64 | −5.41 units |
| 43 | 40/64 | 40/64 | 6/64 | −4.82 units |

Overall, **139/192** sampled episodes entered the radius and eventually met all
instantaneous tolerances, but only **20** held them for 30 consecutive ticks. A
negative final distance reduction means the ship finished farther from the goal
than it started, despite any earlier approach. Deterministic runs never entered.

Across the 59,552 sampled decision states inside the radius, speed met its tolerance
on **78.5%** of ticks and spin on **58.7%**. By model, speed compliance was
86.6%/70.0%/71.4%, and spin compliance 57.9%/59.1%/59.9%. Near-goal mean thrust
probabilities remained 3.77%/5.04%/2.51%; these descriptive averages do not prove
that the network ignores distance. The trace evidence identifies unreliable dwell,
with frequent spin-limit violations, rather than a lack of all approach behavior.
It does not by itself isolate the cause of the spin or speed violations.

### Validation and retention

All **four diagnostic tests** passed: training-law equivalence, native prefix replay,
seeded repeatability/seed uniqueness and finding-rule boundaries. Python compilation
and whitespace checks passed. Every model tensor was unchanged after execution;
full production source/runtime contracts matched before inference. All deterministic
traces reproduced exactly, and Python deterministic commands agreed with native
executed controls at every tick. Sampled traces retain their actual commands,
probabilities and latents. Every successful trace's final 30 frames satisfied the
physical tolerances. No infrastructure failure or incomplete episode occurred.

The sealed archive retains all **240 physics/probability trace pairs**, the three
input checkpoints and exports, exact tasks, frozen specification, source/runtime
and commands. All fourteen previous manifests and sixty referenced parent files
are unchanged. Local retention is not off-machine backup.

### Interpretation and next decision

Action execution is a verified part of the discrepancy: the frozen sampled policies
sometimes reach and settle, while deterministic conversion suppresses all thrust and
never approaches. This is not a Rust/Python inference mismatch. It is also not the
whole answer: roughly 90% of sampled attempts still fail, with limited distance
coverage and unreliable dwell. No competent controller or unique training failure
cause has been established.

Propose a separately bounded **channel ablation** using these same fixed models and
development tasks: keep steering at the network's deterministic mean while sampling
both buttons. That would isolate the effect of steering noise relative to the existing
fully sampled baseline. Inspect spin/dwell, success and distance coverage under the
unchanged physical contract. Keep the steering output state-dependent; do not replace
it with a hand-written controller. No such ablation, further training, new confirmation
use or execution-contract promotion occurred in this diagnostic.

