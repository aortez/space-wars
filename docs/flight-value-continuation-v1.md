# Value-gradient continuation through update 32

**Status: complete; stopped at update 32, 2026-10-24.**
Experiment `learned-flight-value-continuation-v1-20261024` continues the
[value-isolation pilot](flight-value-isolation-v1.md). Head-only value gradients retained
48/48 original holds and achieved 34/48 recoveries at update 16, versus shared's 44/48
and 26/48. Test whether that benefit persists through another block, and whether recovery
improves beyond preserving the original policies. Original recovery was 32/48.

## Fixed comparison and reconstruction

Resume both explicit modes (`shared-encoder-v1`, `head-only-v1`), seeds 17/29/43, from
exact LF-042 update-16 checkpoints. Preserve model, Adam, collector, active episodes,
exploration, RNG, config and history. Stop at update **32**. Learning rate remains .000075,
quarter noise/rho zero, 60 Hz, identical signed-family cases, reward, thirty-tick dwell,
physical limits, deadlines, network and inference. No production source changes.

Keep the original update-0 scores, reproduced update-16 boundary and update-32 endpoint
separate. Evaluate the same sixteen recovery cases and sixteen original holds at boundary
and final. Require boundary policy exports and every boundary trace to match LF-042.
Require all six exact 31→32 restart states, unchanged history/completed-episode prefixes,
192 native/Python comparisons and independent reward/dwell reconstruction. The shared
update-32 states and final score records must reproduce LF-041's smaller-rate controls,
allowing only the declared equivalent shared-mode field and model/ppo/run source hashes
in audit comparison. Actual checkpoint loading remains strict and unchanged.

New checkpoints are 17–32; copied input checkpoint 16 is the parent, not newly trained.
Copy completed pre-boundary training traces only for evidence reconstruction, refusing
any overwrite. Report cumulative exposure separately from new 16→32 outcomes, eligibility
and optimizer steps. A restarted active episode retains its existing prefix.

## Frozen gates

Reuse the exact LF-041 continuation review gate: at least two seeds **jointly** gain
≥1 primary success over their own update-16 score, retain ≥12/16 original immediate
holds with ≥2 in every heading stratum and no physical failures, and have no primary
physical failures. Passing permits discussion of a separately frozen update-64 test;
it never automatically launches one. Both arms are reported, with head-only deciding
review eligibility. Original-baseline progression still requires ≥12/16 recovery,
≥3/4 in every family and holding retention jointly in two seeds, plus gains ≥2 over
update 0 in two seeds. Position directions remain explicit. No thresholds change after
results. Retention alone or beating a regressing control does not qualify broad learning.

## Budgets and retention

Six 16→32 runs plus six 31→32 resumes: 104,448 newly collected ticks, ≤1,632 optimizer
steps, ≤43,200 restoration ticks. Boundary/final evaluations total 384 episodes,
≤518,400 ticks, with 192 parity comparisons. Nineteen processes in four bounded stages.
Training ≤1800 seconds (1820 watchdog), extra evaluation/verification ≤600 (620 watchdog).
The unchanged config updates=64 is capped by explicit stop-after=32 on every command.
No automatic extension, silent retry, checkpoint selection, confirmation or promotion.

Freeze 1,567 source files, six configs, input checkpoint/policy/boundary/historical traces,
LF-041 shared-control references, admissions, parent results, runtime/dependencies and
all 38 earlier manifest hashes before execution. Fourteen existing focused tests cover
gradient routing, strict identity, shared equivalence, joint continuation gates, baseline
separation, prefix preservation and non-overwriting evidence copies. Native baseline
remains 421 passed/two ignored; production source/runtime are unchanged from LF-042.
Retain 102 new checkpoints, 24 exports, every trace, gate and validation result. No native
teacher/handoff, task changes, longer dwell or general navigation claim.

## Results

Head-only value training retains 48/48 original holds through update 32, but recovery stays at 34/48 and the extension gate fails.

| Value gradients | Seed | Recovery: update 0 →16 →32 /16 | Original holds: update 0 →16 →32 /16 | New sampled successes | New KL stops /16 | New optimizer steps | Final settled / position / velocity / spin, each /4 |
|---|---:|---|---|---:|---:|---:|---|
| shared | 17 | 10 → 8 → 8 | 16 → 14 → 11 | 1 | 2 | 238 | 4 / 0 / 1 / 3 |
| shared | 29 | 9 → 8 → 4 | 16 → 16 → 10 | 12 | 5 | 208 | 1 / 0 / 1 / 2 |
| shared | 43 | 13 → 10 → 4 | 16 → 14 → 10 | 5 | 4 | 219 | 3 / 0 / 0 / 1 |
| head_only | 17 | 10 → 10 → 10 | 16 → 16 → 16 | 7 | 0 | 256 | 4 / 0 / 2 / 4 |
| head_only | 29 | 9 → 11 → 11 | 16 → 16 → 16 | 56 | 1 | 248 | 4 / 1 / 2 / 4 |
| head_only | 43 | 13 → 13 → 13 | 16 → 16 → 16 | 118 | 0 | 256 | 4 / 1 / 4 / 4 |

Head-only holding remains 48/48 at updates 0, 16 and 32. Recovery changes
32→34→34, with final per-seed scores 10/11/13. Shared holding changes 48→44→31
and recovery 32→26→16, reproducing the delayed regression seen in LF-041.
The observed retention benefit therefore persists through this second block.
This establishes neither indefinite stability nor broad recovery competence.

No head-only seed gains a net recovery success over update 16. Both arms fail the
fixed continuation review gate; neither qualifies for broader progression. The
unchanged head-only total conceals one trade: seed 29 loses velocity case 14 and
gains spin case 15. Seeds 17/43 retain the same success sets. Case-level differences
are retained in `validation/aggregate.json`. Gains over the original policies
remain 0/+2/0, so only one seed has a positive net gain over update 0.

Head-only records 181 new sampled successes versus shared's 18, but these are
on-policy training completions, not independent evidence of better recovery.
Head-only uses 760 optimizer steps and one KL stop in this block versus shared's
665 steps and eleven stops. All boundary/final physical evaluations have zero
collision, damage, escape or ship-loss failures; unsuccessful attempts time out.
The cumulative entry-stage gate still passes for head-only, while its stronger
progression and continuation gates fail. Cumulative exposure includes the first
sixteen updates and must not be read as fresh position experience.

### Position evidence from the new block

| Value gradients | Seed | New position-eligible rows | New sampled position successes | New sampled position case indices | Final position successes: forward / left / backward / right |
|---|---:|---:|---:|---|---|
| shared | 17 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| shared | 29 | 27 | 0 | none | 0 / 0 / 0 / 0 |
| shared | 43 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| head_only | 17 | 0 | 0 | none | 0 / 0 / 0 / 0 |
| head_only | 29 | 0 | 0 | none | 1 / 0 / 0 / 0 |
| head_only | 43 | 59 | 1 | 33 | 1 / 0 / 0 / 0 |

Each seed receives 4,096 new position-family ticks. Head-only has only
0/0/59 position-eligible rows in this block and 0/0/1 new sampled position successes;
shared has 0/27/0 eligible rows and no new position successes. Eligibility means
simultaneously satisfying distance, speed and spin tolerances before a decision,
not just entering the goal radius. The single new head-only position success is
seed 43, case 33, a forward case, completing at tick 405.

Final head-only position recovery remains 2/12, both forward cases (seeds 29/43),
matching its original and update-16 position totals. Neither lateral direction nor
backward recovery succeeds. Shared ends at 0/12. The overall retention improvement
has not broadened the set of position maneuvers the mean policy can complete.
These counts identify the remaining limitation without proving whether goal
conditioning, exploration, credit assignment or capacity causes it.

### Verification and retention

All boundary policy exports and primary/holding traces exactly reproduce LF-042.
All six 31→32 restarts reproduce complete model, Adam, collector, RNG and history states.
Shared update-32 states and final scores reproduce LF-041 after only the declared
equivalent mode and three source hashes are normalized in audit comparison.
Strict contracts/source identity remain unchanged; inherited history rows and completed
episodes are exact. Cumulative reward/dwell reconstruction, new-block outcome accounting,
192 parity comparisons and all nineteen process limits pass. Fourteen focused Python tests
pass; native source/runtime remains at the retained 421-pass/two-ignored baseline.

Used **104,448 newly collected training ticks**, **19,899 trace-restoration ticks**,
**1,521 optimizer steps** including restart checks and **121,414 evaluation ticks**
in **384 episodes**. No frozen process was retried. No update-64 run, confirmation or
promotion occurred. Preserve 1,567 sources, 569 input copies including 334 historical
completed traces, 102 new checkpoints and 24 exports. The historical trace copies are
explicitly identified; they do not count as new samples. All 38 previous manifests and
copied inputs remain unchanged. See [results](data/flight-value-continuation-v1-results.json)
and [archive receipt](data/flight-value-continuation-v1-archive.json).

### Next decision

Preserve the original, update-16 and update-32 policies; stop this comparison at 32 because the fixed extension gate failed. Keep head-only as an explicit experimental mode and shared as the legacy default. Next, freeze a bounded diagnostic of goal response in these preserved head-only policies: compare commands for forward, backward and lateral goals at fixed physical states, vary distance, and inspect the corresponding recorded position failures. Use all three seeds and fixed checkpoints 0/16/32. This should distinguish weak response to the goal from an attempted correction that fails to settle, before choosing another training change. The earlier LF-028 response audit used different policies and cannot answer this for the retained head-only models. The diagnostic should make no weight updates or new flight evaluations. No update-64 run, model promotion, confirmation use, longer-dwell qualification or general navigation claim follows from this result.

