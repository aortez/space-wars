# Task-family advantage centering pilot

**Status: complete; stopped at update 16, 2026-10-24.** Experiment
`learned-flight-family-centering-v1-20261024` follows LF-047's unsuccessful direct
goal-branch pilot and LF-046's mixed-family gradient findings. Test whether removing
between-family advantage offsets improves physical position recovery with holding
retained. Negative globally centered advantages do not themselves prove an error.

## Sole comparison and mapping

Use the same 39,546-parameter residual architecture and original zero-output warm
starts in both arms, seeds 17/29/43. `global` retains the default `global-mean-v1`;
`family` explicitly uses `family-mean-v1`. For raw rollout advantages A, both use
divisor `max(population_std(A), 1e-8)` across the complete 1,024-row rollout.
The control subtracts the whole-rollout mean. The candidate subtracts each row's
task-family mean, calculated over the entire rollout before minibatch permutation.
There is no per-family variance normalization and no minibatch-local centering.

Family IDs 0/1/2/3 mean settled/position/velocity/spin. The collector records the
current candidate's index modulo four before every action, validates its declared
`settle-recover-v1/stratum-N` recipe against the slot, and checks all pool members.
Labels stay aligned with their observations through episode reset and time-major
flattening. Only this declared recipe supports family centering. Invalid modes
and unsupported recipes fail before output creation/physics. Missing, malformed or
incomplete labels fail before a family-centered optimizer step.

Keep head-only value gradients, learning rate .000075, quarter noise/rho zero,
schema input scaling, tasks/partitions, rewards, GAE, action likelihoods, clipping,
epochs/minibatches, 60 Hz control and thirty-tick arrival dwell unchanged. Stop
both arms at update **16**. Raw returns and critic targets are not centered. The
standard deviation is common within each rollout; after policy divergence, the
arms can of course collect different raw advantages and thus different divisors.

Production changes are confined to ppo.py, collector.py and run.py. Default update
arithmetic is retained. The collector adds labels for this recipe and run.py saves
per-update advantage evidence separately from checkpoint history. No architecture,
native runtime, observation, action, reward or inference-export change occurs.
Actual resume remains strict about config, source and runtime identities. Only
the audit may normalize the three declared Python source hashes when comparing
the global control against LF-047's residual arm; all other checkpoint fields must
match exactly. No teacher, target-action labels, native fallback or handoff.

## Verification, diagnostics and physical gates

Before training, require both arms' initial models, exports and RNG to equal the
retained LF-047 residual starts. On eight fixed contexts per seed, 26 observations
each, require exact initial outputs between arms and folded/training Python parity;
native raw tolerance is 1e-5 absolute/relative, control tolerance 1e-5. Initial
physical traces and first collectors must match the originals. Require the global
control to reproduce LF-047 at checkpoints 0/1/16 and final physical scores. All
six 15→16 restarts must reproduce model, Adam, collector, RNG and history exactly.

For every pilot update, retain all 1,024 raw advantages, family IDs and normalized
advantages, along with the shared mean/std/divisor and family means/positive-row
counts. These read-only diagnostics must not consume RNG or change training data.
Independently reconstruct the normalization and statistics in the verifier. Compare
first-rollout raw advantages exactly across arms, and update-16 diagnostics against
each restart. Measure actual parameter changes between every pair of adjacent
checkpoints for the goal branch, actor, critic and whole network. These are Adam
parameter changes over complete updates, not raw gradient attribution.

Evaluate sixteen primary recovery cases and sixteen original holding cases at
updates 0/16. Retain the same paired usefulness gate: **at least two seeds jointly**
must gain ≥1 position and ≥1 overall recovery success over both the original policy
and matched control; complete ≥1 nonforward position case; retain ≥12/16 original
immediate holds with ≥2 per heading stratum; and have zero physical failures in
both suites. Report entry/exposure and stronger progression gates separately.
Advantage signs, training rewards or larger goal response cannot pass a physical
gate. Include every seed, position directions, sampled position successes/eligible
rows, KL stops, optimizer counts and the retained fixed-context response probes.

## Budgets and retention

Twenty processes: one preflight, six pilots, six exact restart checks, six endpoint
and response checks, one verifier. Training processes ≤1800 seconds (1820 watchdog),
others ≤600 seconds (620 watchdog). Stop on failure; no scored retry or automatic
extension. Config updates=64 stays capped by stop-after=16.

Use 104,448 new training ticks, ≤1,632 optimizer steps, ≤21,600 active-prefix
restoration ticks, 384 physical evaluation episodes and ≤518,400 evaluation ticks.
Endpoint parity uses 192 reset/setup ticks. The 1,248 initial and 1,248 final goal
comparisons reuse saved inputs; total native/Python comparisons are **2,688**.
Diagnostics add no rollouts or optimizer branches. Retain 102 advantage records
(96 pilot, six restart), 96 measured update deltas, 108 checkpoints, 24 pilot/resume
exports and six preflight exports, plus all physical traces and response outputs.

Freeze 1,577 sources, six configs, 175 copied inputs, runtime/dependencies and all
43 previous manifest hashes before experiment inference/training. All 60 Python
tests pass, including family mappings, scaling, gradient offsets, RNG neutrality,
default behavior, native parity and continuation. Native runtime is unchanged from
LF-047's 421-pass/two-ignored baseline. Preserve commands, timings, validation and
interpretation. Local retention is not off-machine backup. No reserved confirmation,
model promotion or general-navigation qualification follows this pilot.

## Results

Family centering changes the advantage signal and preserves all 48 holds, but adds no position recovery; the paired usefulness gate fails.

| Arm | Seed | Recovery: initial → final /16 | Original holds: initial → final /16 | Final settled / position / velocity / spin, each /4 | Sampled successes | KL stops /16 |
|---|---:|---|---|---|---:|---:|
| global | 17 | 10 → 10 | 16 → 16 | 4 / 0 / 2 / 4 | 9 | 0 |
| family | 17 | 10 → 9 | 16 → 16 | 4 / 0 / 1 / 4 | 9 | 0 |
| global | 29 | 9 → 10 | 16 → 16 | 4 / 1 / 3 / 2 | 69 | 0 |
| family | 29 | 9 → 10 | 16 → 16 | 4 / 1 / 3 / 2 | 43 | 0 |
| global | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 100 | 0 |
| family | 43 | 13 → 13 | 16 → 16 | 4 / 1 / 4 / 4 | 99 | 0 |

Both arms retain **48/48 original immediate holds**. The family-centered arm
finishes at **32/48 recovery successes**, compared with **33/48** in the matched
global control and **32/48** at initialization. Seed 17 loses one velocity recovery
relative to the control. Both arms retain the same **2/12 position successes**,
all forward and already present at initialization. None of the nine sideways or
backward final development cases enters the goal radius in either arm.

The paired usefulness gate fails in every seed; stronger progression fails in both
arms. The weaker entry/exposure gates pass. No physical failure occurs in either
evaluation suite; unsuccessful cases reach their deadlines. Each arm executes all
**768 pilot optimizer steps** with **zero KL stops**. No longer continuation,
confirmation use or model promotion follows this result.

### Learning signals and position experience

The manipulation works. Across the 12,288 position rows per arm, positive
normalized advantages increase from **2,582 (21.0%)** under global centering to
**6,140 (50.0%)** under family centering. In the identical first rollouts, positive
position rows change from **0/66/0** to **171/84/146** out of 256 per seed. Raw
advantages and the common divisors match exactly in those first rollouts. Every
family's normalized rollout mean is within **4.85e-7** of zero in the family arm.

These signs describe the samples relative to a baseline. They are not labels of
competent actions. After the first update, policies and trajectories diverge, so
later positive-row counts are not comparisons of identical samples. Both branch
layers, the actor and the critic change. The independent verifier
reconstructs every saved normalized advantage and measures actual Adam parameter
changes over all 96 pilot updates. Centering neither prevents updates nor supplies
successful steering experience by itself.

Family centering yields **151 sampled training successes**, versus **178** in the
control. Both have only **three position successes**, all forward: seed 29 case 1
and seed 43 cases 1/17. Their first attempts finish at ticks 232 and 272 in both
arms; seed 43 case 17 finishes at 359 ticks with family centering, versus 281 in the
control. Position-eligible rows are **0/240/209** versus **0/255/150**. No sampled
sideways/backward position success appears.

Final absolute right-minus-left steering contrasts at eight units are
**.001860/.005089/.010649** in the family arm, versus
**.000521/.001699/.002642** in the control. All remain below the retained .05
response threshold. Correct-sign counts remain **0/0/8 of eight contexts** for
both arms' seeds 17/29/43. Greater lateral response in this probe does not establish
successful turning or arrival. The experiment provides no physical support for
changing the default centering mode under this specification and budget.

| Arm | Seed | Positive position advantages /4096 | Position normalized-mean range over 16 rollouts | Sum of goal-output update norms | Sum of all-parameter update norms |
|---|---:|---:|---|---:|---:|
| global | 17 | 1971 | -0.424218 to 0.0731238 | 0.032851 | 0.323371 |
| global | 29 | 407 | -0.943088 to 0.889384 | 0.031389 | 0.601346 |
| global | 43 | 204 | -0.952001 to -0.0951049 | 0.043293 | 0.362462 |
| family | 17 | 2204 | -4.62867e-07 to 1.75089e-07 | 0.028743 | 0.291888 |
| family | 29 | 1914 | -3.91155e-08 to 3.35276e-08 | 0.034217 | 0.608758 |
| family | 43 | 2022 | -7.45058e-09 to 1.9907e-08 | 0.039542 | 0.327116 |

Update norms are Euclidean norms of each complete checkpoint-to-checkpoint change,
summed over sixteen updates. They are not endpoint displacement, raw gradients,
or measures of flight competence.

| Arm | Seed | Final absolute right/left steering contrast | Final full-goal steering span | Final thrust span, pp | Final brake span, pp | Correct lateral sign /8 | Position-eligible rows |
|---|---:|---:|---:|---:|---:|---:|---:|
| global | 17 | 0.00052059 | 0.02640268 | 0.093788 | 1.163149 | 0 | 0 |
| global | 29 | 0.00169864 | 0.00752622 | 0.052640 | 0.932121 | 0 | 255 |
| global | 43 | 0.00264220 | 0.04131855 | 0.168935 | 0.799721 | 8 | 150 |
| family | 17 | 0.00186006 | 0.01816584 | 0.108784 | 1.151067 | 0 | 0 |
| family | 29 | 0.00508869 | 0.01616788 | 0.035975 | 0.470412 | 0 | 240 |
| family | 43 | 0.01064874 | 0.02624003 | 0.174649 | 0.333416 | 8 | 209 |

Response maxima span eight fixed start/later contexts; actuator spans are percentage
points. Full signed contrasts remain in the retained output.

### Verification and retention

All initial model tensors, exports, RNG states and Python/native outputs match
the retained LF-047 residual starts exactly. Initial physical traces and first
collectors match the originals. All six 15→16 restarts reproduce the full model,
Adam, collector, RNG and history; their advantage records also match exactly.
Global controls reproduce LF-047 at checkpoints 0/1/16 and final physical results
after only the three declared Python source hashes are normalized for audit.
Actual resume remains strict and the native runtime is unchanged.

All **60 Python tests**, **2,688 native/Python comparisons**, 102 advantage records,
96 update-delta measurements and physical dwell/reward reconstruction pass. Native
validation remains LF-047's retained **421-pass/two-ignored** baseline, not a new
native test run. Every one of the twenty declared processes exits zero within
its limits; none was retried. Concurrent pilot elapsed times are 73.0–89.8 seconds,
not isolated throughput measurements.

Use **104,448 newly collected training ticks**, **6,991
active-prefix restoration ticks**, **1,632 optimizer steps**
including restart checks, and **68,376 evaluation ticks** across
**384 episodes**. Endpoint parity adds 192 reset/setup ticks. Saved response inputs
and advantage diagnostics add no new flight trials or optimizer branches.

Preserve 1,577 frozen sources, 175 copied inputs, six configs, the unchanged runtime,
108 checkpoints, 24 pilot/resume exports, six preflight exports and every trace,
response and learning-signal record. All 43 previous manifests and copied input
sources remain unchanged. Source/runtime/config/archive hashes are verified. See
[results](data/flight-family-centering-v1-results.json) and
[archive receipt](data/flight-family-centering-v1-archive.json). Local retention is
not off-machine backup; no commit, confirmation use or promotion is implied.

### Next decision

Keep whole-rollout mean centering as the default. Retain family centering as an
explicit experimental option and its read-only diagnostics, without promoting it.
Stop LF-048 at update 16. Removing between-family offsets changed the intended
signal but did not produce new position competence; it is not a demonstrated cure
for the earlier gradient observations.

Next, freeze a **no-training probe of temporally persistent exploration** from the
original residual-architecture holding policies. Compare current independent
noise (rho 0) with one predeclared candidate, rho .98, while retaining quarter
marginal latent-residual scale and 60 Hz decisions/physics. Keep model weights,
reward and physical criteria fixed. Use paired repeat seeds, the declared signed
position cases and original holding cases; freeze the full matrix and tick/wall
budgets before execution. Do not search a coefficient grid or select favorable
repeats.

Measure actual turn persistence, goal-bearing/distance improvement, goal-radius
entry, settled arrival, physical failures, holding disruption and realized reward.
The question is whether this exploration law generates useful maneuver experience
without destroying holding, before committing another training budget. Successful
noise samples would establish exposure only, not a learned controller. No optimizer
updates, teacher labels, control handoff or confirmation use belong in this probe.

The earlier [persistent-exploration comparison](flight-persistent-exploration-v1.md)
failed with the old binary-action setup. Preserve that negative result. This new
proposal concerns continuous low-amplitude exploration around the learned holding
policies and must earn support from its own physical evidence. No persistence
change or new probe has been executed in LF-048.

