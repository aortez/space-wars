# Quarter-noise learning-rate pilot

**Status: complete; bounded pilot only, 2026-10-24.**
Experiment `learned-flight-rate-training-v1-20261024` follows the
[exact update audit](flight-noise-update-audit-v1.md). Quarter-noise training stopped on
KL in 35/48 updates versus 5/48 at full noise, including every first update after one
optimizer step. Test whether a smaller learning rate improves physical recovery and
retains holding; a lower KL or slower regression alone is not learning qualification.

## Fixed comparison

Six sixteen-update pilots: `control` learning rate .0003 and `smaller` .000075, seeds
17/29/43. Both use quarter-amplitude v3 exploration, rho zero, and the same preserved
LF-033 weights with fresh Adam. Configs differ only in learning rate. Keep the LF-038
config field updates=64 for strict control identity, but every command explicitly stops
at sixteen; this experiment contains **no conditional longer run**.

Reuse exactly the 64 training cases at seed 4510001 and sixteen development cases at
4510002, with four balanced signed-error families, thirty-tick dwell and deadline 900.
Original holding uses sixteen starts at 4310002, dwell thirty/deadline 1800. Position,
speed and spin limits remain 3, 1 and native-f32 .2, with free heading. Keep 60 Hz physics
and control, schema-v1 inputs, 39,447 parameters, Gaussian deviations/trainable log_std,
reward, PPO epochs/minibatches, clipping, KL guard and all loss coefficients unchanged.
No teacher, handoff, architecture change, confirmation use or model promotion.

Evaluate primary recovery and original holding at initialization/final in deterministic
mean mode. Record all directional position outcomes, sampled successes, physical
eligibility, effective deviations, optimizer-step counts and KL stops. Compare the
first update's final whole-batch likelihood ratios/KL on the frozen original first
rollout: both arms' first collectors must reproduce the same parent successor exactly.
This measures a completed update, not its first individual optimizer step. Subsequent
rollouts may diverge; equal physics budgets do not imply equal optimizer-step counts.

Require exact control checkpoint 0/1/16 model/Adam/collector/RNG/contracts/history and
final score-record reproduction against LF-038 quarter parents. Require all six exact
15→16 resumes. Between arms, initial models, collector and RNG must match; fresh Adam
states differ only in their declared learning-rate field. All task admissions and
initial traces must reproduce. Retain native/Python action parity and independent
physical dwell/reward reconstruction. Production source/runtime stay byte-identical.

## Frozen gates

Report the unchanged historical pilot gate alongside this revised gate:

- Every family/seed visits all signed classes (four for position/velocity, two for spin).
  Each nonsettled family has ≥256 above-limit correction rows **or ≥1 successful sampled
  recovery**. This replaces only the old requirement for ≥256 above-limit rows even
  when a correction succeeds quickly. It is an exposure check, not broad competence.
- Every seed has at least one sampled success.
- At least two seeds have ≥30 fully eligible pre-action rows in position-error episodes.
- At least two seeds retain ≥12/16 original immediate holds, ≥2 in every heading stratum,
  and zero physical failures. Success or coverage never waives this requirement.

Retain the existing stronger progression check: at least two seeds jointly reach ≥12/16
primary recoveries, ≥3/4 in each family, zero physical failures and the original holding
requirement; at least two seeds gain ≥2 primary successes over their own initialization.
A comparative learning benefit additionally requires the smaller arm to pass its pilot
and progression gates and beat the control by ≥4 primary successes in at least two seeds,
with no fewer original immediate holds in those pairs. Report improved retention or
reduced KL separately when these learning criteria fail. No gate triggers further runs.

## Budget and retention

Six pilots plus six single-update resumes: 104,448 newly collected training ticks,
≤1,632 optimizer steps, ≤21,600 additional ticks replaying active resume traces. Primary
and holding endpoints total 384 evaluation episodes, ≤518,400 evaluation ticks, and
192 native/Python parity comparisons. No additional flight evaluations or checkpoints
selected from intermediate scores. Each training process ≤1800 seconds with an external
1820-second watchdog; each extra-evaluation or verification process ≤600 seconds with
620-second watchdog. Nineteen processes in four bounded stages. Stop and retain failures;
no silent retries or budget extension.

Freeze specification, six configs, source archive, unchanged runtime/dependencies,
128 copied inputs, the first-rollout evidence, ten focused tests and all 35 previous
manifest hashes before execution. Retain 108 new checkpoints, 24 exports, all training
and evaluation traces, parity, admissions, process statuses and complete gate results.
The runner's endpoint best.json is advisory only; qualification uses the final endpoint
and frozen gates. Native baseline remains 421 passed/two ignored. No gameplay promotion.

## Results

Smaller learning steps preserve substantially more holding and pass the pilot gate, but recovery still regresses and position correction remains unlearned.

| Rate | Seed | Sampled successes | Recovery initial → final /16 | Final settled / position / velocity / spin, each /4 | Original holds initial → final /16 | KL stops /16 | Optimizer steps |
|---|---:|---:|---|---|---|---:|---:|
| control | 17 | 7 | 10 → 4 | 2 / 0 / 1 / 1 | 16 → 3 | 8 | 144 |
| smaller | 17 | 11 | 10 → 8 | 4 / 0 / 1 / 3 | 16 → 14 | 3 | 225 |
| control | 29 | 23 | 9 → 4 | 0 / 0 / 1 / 3 | 16 → 6 | 14 | 95 |
| smaller | 29 | 21 | 9 → 8 | 4 / 0 / 1 / 3 | 16 → 16 | 6 | 204 |
| control | 43 | 33 | 13 → 10 | 4 / 0 / 2 / 4 | 16 → 14 | 13 | 115 |
| smaller | 43 | 47 | 13 → 10 | 3 / 0 / 3 / 4 | 16 → 14 | 9 | 170 |

The smaller learning rate preserves **44/48 original immediate holds**, versus **23/48**
at the control rate, from an initial 48/48. Per-seed retention is 14/16, 16/16 and 14/16;
every smaller-rate seed meets the original holding requirement including heading strata.
Primary recovery improves relative to control, **26/48 versus 18/48**, but remains below
the original **32/48**. Per-seed smaller-rate recovery is 8/16, 8/16 and 10/16, compared
with initial 10/16, 9/16 and 13/16. The first two seeds improve four recoveries and eleven/
ten original holds over their controls; seed 43 ties its control. This demonstrates a
retention benefit and less recovery regression, not new recovery competence.

PPO KL early stops fall from **35/48 to 18/48** updates. The smaller-rate arm performs
599 optimizer steps versus control's 354 over the identical 49,152 collected ticks per
arm. At the first update, smaller-rate seeds 17/29/43 take 16/5/16 steps, versus one
each for control. Completed first-update whole-batch KL is .00478/.04261/.00500 versus
.03470/.02986/.03367. Seed 29 illustrates why a smaller individual step does not guarantee
lower *final-update* KL: it takes more steps, and the existing guard does not undo an
accepted step. There is no strict whole-batch post-update KL guarantee. These data support
the step-size hypothesis as a contributor to lost retention; they do not identify it
as the only obstacle to learning.

Sampled training successes increase **63 →79**, with smaller-rate seed counts 11/21/47
versus control 7/23/33. More successes do not uniformly mean better generalization.
All **twelve final position-correction evaluations fail in each arm**, including forward,
left, backward and right. Each arm has only two sampled position successes: seed 29 case
index 1 at tick 232 and seed 43 case index 1 at tick 272, both forward. No backward or
lateral position success is demonstrated. All 384 evaluation episodes have zero physical
failures; unsuccessful evaluations reach their deadlines.

The smaller-rate arm passes the **revised pilot gate**, while control fails retention.
Both fail the historical exposure gate because seed-29 spin spends fewer than 256 rows
above the spin limit: 148 smaller-rate and 190 control, with six sampled spin successes
each. Every signed class is visited. The revision was frozen before execution and accepts
those successful recoveries as exposure; it does not waive retention or demonstrate all
directions are solved. Position-eligible rows for the smaller arm are 0/89/184 versus
0/42/163 control, passing the two-seed criterion. Both arms fail the stronger progression
check; the predefined comparative learning-benefit flag is also false because it requires
progression, not only a better score than a regressing control. No longer run was triggered.

### Verification and retention

All six exact resumes reproduce complete update-16 states. Control checkpoints 0/1/16
and both final score records match LF-038 quarter parents exactly, with **no source or
contract normalization**. Initial tensors, RNG, collector and first collected rollout
successors match between arms; fresh Adam differs only in its learning-rate field.
Admissions, baseline traces, 192 native/Python parity comparisons and independent
physical-dwell/reward reconstruction pass. Ten focused Python tests pass; production
and native source/runtime are unchanged from the 421-pass/two-ignored native baseline.

Used **104,448 newly collected training ticks**, **7,416 resume-restoration ticks**,
**1,021 optimizer steps** including resumes, and **137,122 evaluation ticks** in
**384 episodes**. All nineteen subprocesses exited zero within their declared limits.
No frozen process was retried, no gate changed after execution, and no longer run occurred.

Retain 1,562 sources, six configs, 128 copied inputs, 108 new checkpoints and 24 exports,
all training/evaluation traces, admissions, likelihood diagnostics, parity and process
records. All 35 prior manifests and copied inputs remain unchanged. No confirmation or
model promotion. See [results](data/flight-rate-training-v1-results.json) and
[archive receipt](data/flight-rate-training-v1-archive.json).

### Next decision

Preserve the original LF-033 policies and the paired LF-040 endpoints. The smaller-rate arm now passes the predeclared pilot experience and retention checks, but all three seeds remain below their original recovery scores and no position case succeeds. Freeze one bounded continuation block to update 32 at both rates, using exact update-16 states and the same three seeds, tasks, reward, quarter noise and architecture. Retain the original update-0 physical baseline and update-16 endpoint as separate references. Require holding retention and a recovery improvement over update 16 in at least two seeds before proposing any update-64 extension; track every position direction and gains over the original baseline separately. This tests whether the lower rate allows learning to catch up or merely delays regression. Do not change another loss term, task distribution or network component in that duration test. No continuation is run here, and no model is promoted; confirmation, longer dwell, native teachers/handoffs and broader navigation claims remain unused.

