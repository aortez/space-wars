# Quarter-noise learning-rate continuation to update 32

**Status: complete; stopped at update 32, 2026-10-24.**
Experiment `learned-flight-rate-continuation-v1-20261024` continues the
[learning-rate pilot](flight-rate-training-v1.md). Its smaller-rate arm preserved 44/48
holds and passed the entry gate, but recovery fell from 32/48 to 26/48 and no position
case succeeded. Test whether another bounded block improves recovery rather than merely
delaying regression. No new reward, tasks, network or optimizer setting.

## Fixed continuation and baselines

Resume both rates (.0003 control, .000075 smaller), all seeds 17/29/43, from their exact
LF-040 update-16 states. Retain Adam history, RNG, active episodes, sampler, exploration
state, configs and source identity. Stop at update **32**. Config updates=64 remains for
strict identity, but every command explicitly stops at 32; no run may advance to 64.
The sixteen newly collected updates keep quarter-noise v3, 60 Hz, the original four
signed-error families, thirty-tick dwell, existing success limits and deadline 900.

Three baselines remain distinct: original update-0 physical scores, reproduced update-16
boundary evaluations, and new update-32 endpoints. Evaluate the same sixteen primary
recovery and sixteen original-hold cases at the boundary and final endpoint. Require
all boundary traces and exported boundary policies to reproduce LF-040 exactly. Retain
192 native/Python inference comparisons, exact source/contracts and all six 31→32
restart comparisons. The inherited first sixteen history rows and completed episodes
must be unchanged. All extra evaluations are deterministic.

The production resume runner saves checkpoints 17–32, not a new checkpoint 16. Preserve
the six input checkpoint-16 files as parents. Copy previously completed training traces
into the new run's evidence directory after training, refusing any overwrite; these are
inherited evidence, not newly collected samples. Independently reconstruct full 0→32
training rewards/physical dwell, report cumulative exposure separately from new 16→32
completed outcomes, and count only new optimizer steps in this experiment's budget.

## Frozen decision checks

For each arm, the continuation review gate requires **at least two seeds jointly** to:

- Improve primary recoveries by at least one case over that seed's update-16 endpoint.
- Retain ≥12/16 original immediate holds, ≥2 per heading stratum and no physical failures.
- Have no physical failures in final primary recovery evaluation.

This gate only establishes eligibility to discuss a separately frozen update-64 test.
No automatic extension occurs even if it passes. Keep the stronger original-baseline
progression criterion: ≥12/16 recoveries, ≥3/4 in each family and holding retention jointly
in at least two seeds, plus recovery gains ≥2 over update 0 in at least two seeds. Report
all position directions separately. A gain over a regressed boundary alone does not
qualify new competence. Report the LF-040 revised exposure/experience gate cumulatively;
newly completed successes are a separate count. Do not change thresholds after results.

## Budget and evidence

Six 16→32 continuations plus six single-update 31→32 resumes: 104,448 newly collected
training ticks, ≤1,632 optimizer steps, ≤43,200 active-trace restoration ticks. Four
endpoint suites per run total 384 episodes, ≤518,400 evaluation ticks; 192 parity
comparisons. Nineteen processes in four bounded stages, ≤1800 seconds per training
process (1820 watchdog), ≤600 seconds per extra evaluation/verification (620 watchdog).
Stop and retain failures; no silent retry, checkpoint selection or budget extension.

Before execution retain all parent checkpoints, configs, boundary policies/traces,
completed training traces, admissions, parent results, source archive and unchanged
native runtime/dependencies. Hash all 36 earlier manifests. Ten focused tests cover
joint gates, baseline separation, prefix preservation and non-overwriting trace copies.
Native baseline is unchanged at 421 passed/two ignored. Retain 102 newly generated
checkpoints and 24 exports, alongside clearly identified copied parent artifacts.
No confirmation, gameplay promotion, native teacher/handoff, longer dwell or broader
navigation claim. All source, input, config and previous-manifest hashes must verify
before sealing the completed archive.

## Results

The update-32 continuation loses holding retention and further reduces recovery; the extension gate fails at both learning rates.

| Rate | Seed | Recovery: update 0 →16 →32 /16 | Original holds: update 0 →16 →32 /16 | New sampled successes | New KL stops /16 | New optimizer steps | Final settled / position / velocity / spin, each /4 |
|---|---:|---|---|---:|---:|---:|---|
| control | 17 | 10 → 4 → 0 | 16 → 3 → 0 | 0 | 6 | 183 | 0 / 0 / 0 / 0 |
| control | 29 | 9 → 4 → 4 | 16 → 6 → 8 | 3 | 5 | 222 | 1 / 0 / 1 / 2 |
| control | 43 | 13 → 10 → 8 | 16 → 14 → 13 | 3 | 11 | 154 | 4 / 0 / 1 / 3 |
| smaller | 17 | 10 → 8 → 8 | 16 → 14 → 11 | 1 | 2 | 238 | 4 / 0 / 1 / 3 |
| smaller | 29 | 9 → 8 → 4 | 16 → 16 → 10 | 12 | 5 | 208 | 1 / 0 / 1 / 2 |
| smaller | 43 | 13 → 10 → 4 | 16 → 14 → 10 | 5 | 4 | 219 | 3 / 0 / 0 / 1 |

The smaller-rate arm falls from **26/48 recoveries at update 16 to 16/48 at update 32**,
versus **32/48 originally**. Seed scores move 8→8, 8→4 and 10→4. No seed improves over
its boundary score. Original immediate holds fall **44/48 →31/48**, from an original
48/48; per-seed holds are now 11/16, 10/16 and 10/16. All three smaller-rate seeds fail
the frozen retention threshold. The control arm also worsens: recovery 18/48→12/48 and
holding 23/48→21/48, with final recovery 0/4/8 and holds 0/8/13 by seed.

Both continuation review gates fail, and both stronger original-baseline progression
checks fail. The cumulative exposure/experience checks still pass, but retention fails.
Those cumulative counts must not be mistaken for continuing recovery improvement.
**No update-64 extension is justified by this experiment or executed.** The original
models and successful short-hold references remain preserved; the new endpoints are
experimental artifacts, not replacements for those references.

The smaller rate still hits the KL guard less often: **11/48** new updates versus
**22/48** control, performing 665 versus 559 optimizer steps in the main continuations.
This contrasts with the first block's 18/48 versus 35/48 stops. Fewer early stops and
more completed optimizer steps did not translate into better behavior. The initial
retention benefit from smaller steps is real within LF-040, but this continuation shows
that step-size reduction is insufficient to reverse the destructive training trend.
It does not by itself identify whether the remaining cause is policy credit, value
learning through shared features, state representation or their interaction.

Only **18 new sampled successes** occur in the smaller-rate block (1/12/5 by seed),
versus six control (0/3/3). These are new completions after update 16, not the cumulative
97 and 69 successes through update 32. Both arms used the same additional 49,152
collection ticks; episode visitation differs as policies and completion times change.
The decline from the first block's 79/63 sampled successes therefore is not a comparison
of identical episode trajectories. The unchanged deterministic evaluation suite does
establish regression in the policies themselves. All 384 evaluation episodes remain
free of physical failures; unsuccessful cases time out.

### Position evidence from the new block

| Rate | Seed | New position-eligible rows | New sampled position successes | New sampled position case indices | Final position successes: forward / left / backward / right |
|---|---:|---:|---:|---|---|
| control | 17 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| control | 29 | 97 | 0 | none | 0 / 0 / 0 / 0 |
| control | 43 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| smaller | 17 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| smaller | 29 | 27 | 0 | none | 0 / 0 / 0 / 0 |
| smaller | 43 | 0 | 0 | none | 0 / 0 / 0 / 0 |

Neither arm completes a new sampled position recovery during updates 17–32. Final
position success is **0/12 per arm**, with no forward, left, backward or right success.
Smaller-rate position episodes provide only 0/27/0 fully eligible pre-action rows by seed
in this new block (12,288 position rows total); control provides 0/97/0. The prior two
sampled forward successes per arm remain historical evidence, not new progress. The
balanced task-family quota is intact, but meaningful successful position-correction
experience remains absent in this block. No new failure cases or task changes are added.

### Verification and retention

All boundary policy exports and primary/holding traces exactly reproduce LF-040.
All six 31→32 restarts reproduce complete model, Adam, collector, RNG and history states.
Strict contracts/source identity remain unchanged; inherited history rows and completed
episodes are exact. Cumulative reward/dwell reconstruction, new-block outcome accounting,
192 parity comparisons and all nineteen process limits pass. Ten focused Python tests
pass; native source/runtime remains at the retained 421-pass/two-ignored baseline.

Used **104,448 newly collected training ticks**, **22,313 trace-restoration ticks**,
**1,297 optimizer steps** including restart checks and **245,389 evaluation ticks**
in **384 episodes**. No frozen process was retried. No update-64 run, confirmation or
promotion occurred. Preserve 1,564 sources, 455 input copies including 229 historical
completed traces, 102 new checkpoints and 24 exports. The historical trace copies are
explicitly identified; they do not count as new samples. All 36 previous manifests and
copied inputs remain unchanged. See results (`docs/data/flight-rate-continuation-v1-results.json`, in git history)
and [archive receipt](data/flight-rate-continuation-v1-archive.json).

### Next decision

Stop duration extensions for this recipe and preserve the original LF-033 weights plus all paired endpoints. Next, freeze a sixteen-update, three-seed comparison at the smaller learning rate that isolates the critic's gradient path: normal shared training versus stopping value-loss gradients at the shared encoder, while still training the value head. Start both arms from the same original learned weights and fresh Adam; keep quarter noise, signed task matrix, reward, PPO settings and physical gates fixed. This tests a specific interference path identified by LF-039 without disabling value estimation or adding network capacity. It is a hypothesis, not a diagnosed sole cause: policy-only drift was also observed. Require gradient-routing tests, exact normal-control reproduction allowing only explicitly declared source/config identity differences in audit comparison, strict real checkpoint resumes, native inference parity and actual recovery/holding measurements. Keep the original baseline separate from comparisons against a regressing control; do not label reduced damage as new competence. No gradient-routing change or follow-up pilot has been implemented or run here. Confirmation, gameplay promotion, native teachers/handoffs and broader navigation claims remain unused.

