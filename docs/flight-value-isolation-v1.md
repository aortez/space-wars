# Value-gradient isolation pilot

**Status: complete; bounded sixteen-update pilot, 2026-10-24.**
Experiment `learned-flight-value-isolation-v1-20261024` follows the failed
[update-32 continuation](flight-rate-continuation-v1.md). Test one specific interference
path suggested by LF-039: value-loss updates to the shared controller representation.
This is a bounded hypothesis test, not an assumption that the critic is the sole cause.

## Intervention and unchanged behavior

Add explicit training config `value_gradient` with two versioned modes:
`shared-encoder-v1` (legacy/default) and `head-only-v1`. The head-only mode feeds
`hidden.detach()` to the value head during PPO updates; the policy head still receives
live hidden features. The value head remains trainable with coefficient .5. Collection,
GAE/returns, value predictions, policy/entropy losses, clipping and Adam remain unchanged.
The direct encoder gradient from value loss is zero; this does not remove the critic's
influence through later advantages or the shared global gradient-clipping norm.

At fixed weights, actor/value forward outputs are exactly identical in both modes.
Parameter count (39,447), inference architecture, export format, native runtime and
control decoding do not change. The representation still learns from policy and entropy.
The value head must fit on those changing features without shaping them directly; worse
value fitting is possible and is part of the tradeoff being tested. This is not a separate
critic network or a zero-value-loss experiment.

Production changes are limited to `model.py` (validated mode and detach boundary),
`ppo.py` (routing during current-state training forward passes) and `run.py` (validate
before creating output). Omitted mode preserves the legacy path. Checkpoint identity
includes the explicit config and source hashes; real resume rejects differences.

## Frozen paired pilot

Arms `shared` and `head_only`, seeds 17/29/43, sixteen updates each. Both initialize from
the original LF-033 learned weights with fresh Adam, not the regressed update-32 states.
Use learning rate .000075, quarter-noise v3/rho zero, the same 64 signed-family training
cases at seed 4510001, sixteen development cases at 4510002 and sixteen original-hold
cases at 4310002. Keep thirty-tick dwell, primary deadline 900/original deadline 1800,
position/speed/spin limits 3/1/native-f32 .2, free heading, 60 Hz, schema-v1, reward and
all PPO settings. Configs differ only in the explicit gradient mode.

Require the shared arm to reproduce LF-040 smaller-rate checkpoints 0/1/16 exactly
except for the explicit equivalent shared-mode field and the three declared production
source hashes in an **audit copy**. No model, Adam, collector, RNG, history or other
contract difference is allowed. Final score records must reproduce too. Real checkpoint
loading remains strict; require all six exact 15→16 resumes. Initial state/optimizer/RNG/
collector equality, identical first-rollout successor collectors, initial trace replay,
192 native/Python parity comparisons and independent reward/dwell reconstruction remain
mandatory. Reuse the retained first rollout for whole-batch post-update likelihood/KL
measurements; no extra rollout collection or optimization is added for that measurement.

## Gates and scope

Keep LF-040's frozen pilot criteria: every signed class visited; nonsettled families
have ≥256 above-limit rows or at least one sampled recovery; every seed has ≥1 sampled
success; position episodes have ≥30 eligible rows in at least two seeds; at least two
seeds retain ≥12/16 original immediate holds, ≥2 in every heading stratum, without
physical failure. Report the historical exposure gate as well. Retention is never waived.

Stronger progression still requires ≥12/16 primary recoveries, ≥3/4 in every family,
zero physical failures and holding retention jointly in at least two seeds, with gains
≥2 over original initialization in at least two seeds. A comparative learning benefit
requires head-only pilot/progression passage and ≥4 more primary successes than shared
in at least two seeds, with no fewer original immediate holds in those pairs. Report
retention-only benefit or mixed effects separately. Position directions remain explicit.
No automatic update-32/64 run, checkpoint selection, confirmation or promotion.

## Budget and validation

Six pilots plus six single-update resumes: 104,448 collected training ticks, ≤1,632
optimizer steps, ≤21,600 active-trace restoration ticks. Four endpoint suites per run:
384 evaluation episodes, ≤518,400 evaluation ticks, 192 parity comparisons. Each training
process ≤1800 seconds (1820 watchdog); extra evaluation/verification ≤600 (620 watchdog).
Nineteen processes in four stages. Retain failures without silent retry or extension.
The inherited config updates=64 stays for control identity; every command stops at 16.

Twenty focused tests cover zero direct value gradients in shared features, unchanged
actor gradients/forward/RNG, active value-head updates through production PPO, exact
legacy/default arithmetic, invalid-mode rejection, strict checkpoint mode mismatch,
audit comparison isolation, exploration accounting and physical gates. Native baseline
remains 421 passed/two ignored with unchanged native code/runtime.

Freeze 1,566 sources, all six configs, 128 parent input artifacts, source/runtime/dependency
identities and all 37 previous manifest hashes before execution. Retain 108 new checkpoints,
24 exports, every trace, admission, parity, process status and gate result. Preserve the
original policies and all earlier archives. No new task cases, native teacher/handoff,
longer dwell or broader navigation claim.

## Results

Head-only value training preserves all 48 original holds and improves recovery versus shared training, but broad position correction remains unqualified.

| Value gradients | Seed | Sampled successes | Recovery initial → final /16 | Final settled / position / velocity / spin, each /4 | Original holds initial → final /16 | KL stops /16 | Optimizer steps |
|---|---:|---:|---|---|---|---:|---:|
| shared | 17 | 11 | 10 → 8 | 4 / 0 / 1 / 3 | 16 → 14 | 3 | 225 |
| head_only | 17 | 7 | 10 → 10 | 4 / 0 / 2 / 4 | 16 → 16 | 0 | 256 |
| shared | 29 | 21 | 9 → 8 | 4 / 0 / 1 / 3 | 16 → 16 | 6 | 204 |
| head_only | 29 | 67 | 9 → 11 | 4 / 1 / 3 / 3 | 16 → 16 | 0 | 256 |
| shared | 43 | 47 | 13 → 10 | 3 / 0 / 3 / 4 | 16 → 14 | 9 | 170 |
| head_only | 43 | 100 | 13 → 13 | 4 / 1 / 4 / 4 | 16 → 16 | 0 | 256 |

The head-only arm retains **48/48 original immediate holds**, versus **44/48** with
shared value gradients. All three seeds retain 16/16, matching their original baseline.
Primary recovery is **34/48 head-only versus 26/48 shared**, compared with **32/48 before
training**. Head-only scores are 10/16, 11/16 and 13/16; original scores are 10/16, 9/16
and 13/16. Thus seeds 17 and 43 match their starting scores, while seed 29 gains two
velocity-recovery cases. Every seed beats its shared-training control (+2/+3/+3), but
only one improves over initialization. This is a substantial retention improvement and
a limited recovery gain, not multi-seed general recovery competence.

Head-only collects **174 sampled successes** versus shared's **79**. Per-seed counts are
7/67/100 versus 11/21/47: sample success does not improve uniformly, even though endpoint
recovery improves over shared in all three seeds. Head-only completes all **768 optimizer
steps** with **zero KL early stops** across 48 updates, versus 599 steps and 18 early
stops for shared. The same 49,152 collection ticks per arm therefore do not imply equal
optimizer exposure. Completed first-update whole-batch KL is .00530/.00784/.00321 for
head-only versus .00478/.04261/.00500 shared. Smaller KL is not universal, nor is it the
qualification criterion; the physical retention results are what support this intervention.

The paired intervention provides evidence that the **direct shared value-gradient path
contributed to regression in this setup**. It does not prove that all critic learning is
harmful or that this path explains every failure. The value head remains trainable;
its changing estimates still affect future advantages, and its gradients still enter
the global clipping norm. Data visitation and optimizer-stop behavior diverge after
training begins. The experiment isolates the routing change, not an additive numerical
share of every eventual behavioral effect. No larger network or inference change was
needed to obtain this bounded improvement.

Both arms pass the revised pilot gate. Head-only also passes the historical exposure
gate; shared does not. Neither arm passes stronger progression: no head-only seed reaches
three position successes out of four, and only one seed gains at least two recoveries
over initialization. The predefined comparative learning-benefit flag is false because
it requires progression and ≥4 additional recoveries over shared in at least two seeds.
The observed retention benefit should not be confused with that stronger criterion.
All 384 evaluations have zero physical failures. No longer run or promotion occurred.

### Position evidence and qualification

| Value gradients | Seed | Position eligible rows | Sampled position successes | Final position successes: forward / left / backward / right |
|---|---:|---:|---:|---|
| shared | 17 | 0 | 0 | 0 / 0 / 0 / 0 |
| head_only | 17 | 0 | 0 | 0 / 0 / 0 / 0 |
| shared | 29 | 89 | 1 | 0 / 0 / 0 / 0 |
| head_only | 29 | 253 | 1 | 1 / 0 / 0 / 0 |
| shared | 43 | 184 | 1 | 0 / 0 / 0 / 0 |
| head_only | 43 | 154 | 2 | 1 / 0 / 0 / 0 |

Head-only finishes with **2/12 position successes**, both forward, matching the two
initial forward successes; shared finishes with zero. No backward, left or right position
case succeeds. Sampled position successes total three head-only versus two shared.
The extra head-only success is seed-43 training index 17 at tick 281, also forward; the
other successes are seed-29 index 1 at tick 232 and seed-43 index 1 at tick 272. Position
eligibility is 0/253/154 rows by head-only seed versus 0/89/184 shared. Directional
correction is still the missing capability. Retained forward behavior and more training
successes do not establish general navigation or obstacle avoidance.

### Verification and retention

Twenty focused tests passed, including direct gradient routing, live value-head
training through production PPO, unchanged actor gradients and fixed-weight outputs,
strict config mismatch rejection and equivalent shared/default updates. Shared-arm
checkpoints 0/1/16 reproduce all LF-040 smaller-rate model/Adam/collector/RNG/history
states after only the declared equivalent mode and three source hashes are normalized
in audit copies. Final score records reproduce. Real loading remains strict, and all
six new 15→16 resumes match exactly. Initial states, first collectors, admissions and
baseline traces match; 192 native/Python comparisons and independent physical/reward
reconstruction pass. Native source/runtime stays unchanged at 421 passed/two ignored.

Used **104,448 newly collected training ticks**, **7,700 trace-restoration ticks**,
**1,463 optimizer steps** including restart checks, and **79,591 evaluation ticks**
in **384 episodes**. All nineteen subprocesses exited zero within the frozen limits.
No frozen process was retried. No longer run, confirmation or promotion occurred.

Retain 1,566 sources, six configs, 128 copied inputs, 108 new checkpoints and 24 exports,
all traces, admissions, parity, process results and gates. The only production edits are
model.py, ppo.py and run.py; the source archive records their exact versions. All 37 prior
manifests and copied inputs remain unchanged. See [results](data/flight-value-isolation-v1-results.json)
and [archive receipt](data/flight-value-isolation-v1-archive.json).

### Next decision

Preserve the original LF-033 policies and all LF-042 endpoints; keep shared training as the legacy default and head-only as an explicit experimental setting. Freeze a bounded paired continuation to update 32 from these exact update-16 states, retaining the smaller learning rate, quarter noise, cases, reward and architecture. This tests whether removing direct value interference prevents the delayed regression observed in LF-041, rather than assuming a sixteen-update gain will persist. Require exact resumes and shared-control reproduction against LF-041 after only the declared equivalent mode/source identities are normalized in audit comparison. Keep original update-0 and boundary update-16 physical baselines separate, and require recovery improvement over update 16 with holding retention jointly in at least two seeds before considering any update-64 extension. Track position directions explicitly; no additional task cases, reward change, network capacity or native labels in this duration test. No continuation has run here, and no model is promoted. Confirmation, longer dwell and broader navigation claims remain unused.

