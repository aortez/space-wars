# Comparing retained arrival screens with physical replay

## Plan

The retention study recovered four distant candidate screens whose original
raw geometry had disappeared from the active survey. Join those frozen screens
to the existing physical follow-through cases at source ticks 3816, 3876, 3934
and 3997, seat 0, destination 2, seed 3491156488288037499, without asteroid pressure.
All four share one ordinary trajectory and measurements at ticks 3769/3770.
They are repeated nominations, not four independent worlds or a strength test.

The previous physical study already established that these cases arrive and
choose bearing 33, outside retained bearings 63/31. This study measures the new
join: forecast arrival timing/pose, preservation of historical geometry at the
actual arrival frame, and agreement between conditional solar screens and fresh
native assessments. It does not rediscover the old choice mismatch, force a
historical site, predict a native winner, or fit a duration constant.

The older raw follow-through archives are absent here. Regenerate exactly these
four commands from `data/capture-followthrough-v1.json`, preserving their full
transfer/acquisition/capture horizons, pursuit deferral, policy, seats and budgets.
Use the existing profiled executable/runtime. Complete trace hashes, whole-run
normalized sensor hashes/counters/stage calls, deterministic upstream ledgers,
evaluator-charge digest and capture results must match the committed historical
records. Reconcile the whole-run budget totals and maxima as well; the old
normalized full live-planning ledger is unavailable, so exact parity of that
timing-containing file cannot be established.
The complete post-choice replay also supplies planetary motion at each predicted
handoff tick. It is not a new intervention or a new controller implementation.

Before replay, commit this plan, the analysis runner and mutation tests. Pin input
projection hashes and all four commands in the output before starting physics.
The current retained ordinary traces supply an independent source prefix:
both players must match through the tick before nomination, both observations
must match at source, and the nominated action must match the frozen alternative.
Check original sample provenance and generation against their dispatch history.
No new measurements may backfill or alter the source screen.

Keep three clocks separate:

- Frozen predicted handoff.
- Actual physical coordinator handoff, with its native readiness/contact gates.
- First fresh native site choice, with the existing acquisition lifecycle guards.

Report actual-minus-predicted transfer time. Endpoint position, velocity and
heading differences compare different ticks when those clocks disagree; label
them accordingly. Separately compare planetary motion at the same predicted
tick, when observed, to isolate ephemeris drift. Do not present a ship's later
post-handoff controls as an error of its earlier transfer controller. Reconstruct
handoff and first choice from dense observations using existing probe audits;
never infer them from a successful eventual capture.

For both retained sites, preserve the original generation, measurement tick,
age, geometry and historical cover/opponent. Track planet identity and pilot/
vehicle identity continuously from source through choice, retaining the first
incompatible tick even if the endpoints match again. Record source, predicted,
actual-handoff and actual-choice ages, with the existing 1800-tick limit.

Retrospectively transport the original body geometry into actual handoff and
choice frames for analysis only. Compare it with a fresh same-ID native site at
choice, retaining missing sites, changed hatch availability and geometry residuals.
Same-frame residuals use a 0.002 reconstruction tolerance for each component
(position, velocity and unit normal have different units). This is a numerical
diagnostic, not a collision guarantee. Expired/incompatible evidence
may retain a labeled geometric comparison, but cannot become an eligible planning
screen. The age/identity validity field covers only those two conditions; solar
context, future threat and climb/boarding safety require separate checks.

For both circling directions, preserve the frozen source-only solar assessment,
the retrospective assessment in the actual frame, and the fresh native assessment.
Reconstruct native scores/solar fields with the existing independent helper:
clearance tolerance 0.01, score tolerance 0.002, and departure-order ambiguity
0.02. Clearances within 0.01 of zero stay unresolved. Record disagreements as
results; matching signs do not establish collision-free landing or combat safety.
Native approach scores remain controller units, not acquisition/landing seconds.

Retain all four cases, interruptions, censors, unknowns, native choices, source
eligibility, commands, hashes and raw witnesses. Selection is fixed by the prior
missing-geometry gap, not by new replay outcomes. No live ranking, cost admission,
duration constant, Rust runtime change or additional world query is planned.
Deployment is authorized when useful; this observational slice has no live bot
behavior to demonstrate on a device.

Use focused tests for wrong source/seat, refreshed age, transient identity changes,
missing sites, incomplete direction records and frame/time mixing; run the Python
suite and independent review/audit. Preserve exact-head CI on the existing PR.

## Results

The complete comparison froze at `0094c729e859f5256c6341f28d23ca58e1e0b83d`.
It reuses runtime `1c4a9acae2cd006ffff90ed3009cf1b3db868e15` and profiled binary
SHA-256 `406f5f682e97f52dd2ba7b834e1812059107042dd4b560ac548c5415835dbfb0`.
All **four full replays match their historical controller/observation digests**,
normalized sensor digests/counters/stage calls, deterministic upstream ledgers,
evaluator-charge digests, budget summaries and capture records. All four again
depart after completing capture. The new result is the comparison with the
retained arrival screens; these physical successes were already recorded.

| Source tick | Predicted handoff | Actual handoff | First native choice | Actual minus predicted | Sample ages at choice, 63 / 31 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 3816 | 5534 | 5508 | 5509 | -26 ticks (-0.433 s) | 1740 / 1739 |
| 3876 | 5492 | 5492 | 5493 | 0 ticks | 1724 / 1723 |
| 3934 | 5522 | 5508 | 5509 | -14 ticks (-0.233 s) | 1740 / 1739 |
| 3997 | 5547 | 5534 | 5535 | -13 ticks (-0.217 s) | 1766 / 1765 |

Each native choice occurs one observed tick after handoff. That is a fact about
these four related trajectories, not a general acquisition duration. Their
original measurement ticks remain 3769/3770 and generation 3469. Planet/pilot
identity is unchanged continuously through choice; all eight sample uses meet
the age and identity limits. The oldest actual-choice sample has **34 ticks**
left before the 1800-tick age limit. No expiry or material interruption occurs
in this quiet-world subset; mutation tests cover those bookkeeping cases.

The historical sites survive accurately as body geometry. Both IDs appear in
every fresh survey: **8/8 sample uses** pass the per-component reconstruction
tolerance. The maximum position/hatch residual is **0.000273 world units**;
hatch availability, settling margin and revision match. This does not establish
fresh climb, threat or collision safety from an old measurement alone.

All **16 direction checks** remain clear in the frozen prediction, retrospective
actual-choice calculation and native assessment. The largest native versus
retrospective clearance difference is 0.000123 units. Native clearances differ
from the frozen prediction by up to 3.779 units, with a minimum observed native
clearance of 315.276 units: these are comfortable margins, not tests near a solar
hazard boundary. The full native comparison independently checks **458 directions**,
with maximum solar reconstruction error 0.000062 units and no ambiguous clearance
signs. Its **51 equal/nearly equal departure-order cases** remain unresolved.

The arrival pose needs a more qualified reading than the timing result. The
different-time endpoint differences are 3.197–4.235 world units in planet-local
ship position and 1.195–1.353 units/s in relative velocity. Ship heading differs
by **50.47–145.99 degrees**. These compare the forecast handoff with the actual
handoff; three pairs are at different ticks. Even the same-tick pair at source
3876 differs by about 60.78 degrees, so accurate timing does not establish an
accurate landing orientation. These observations do not isolate a cause for the
ship discrepancy.

Separately, the planet at the exact predicted tick has zero position residual
at recorded precision in all four cases, velocity residual at most 0.000595
units/s, angle residual at most 0.000000008 radians and spin residual at most
0.000000447 radians/s. Orbital drift does not explain the ship heading difference
here. Post-handoff ship motion was not treated as a transfer-controller forecast.

The bot selects **bearing 33 in all four cases**, outside retained bearings 63
and 31. All fresh retained-site directions are eligible, but bearing 31—the
better retained site—scores **13.875–17.008 controller units worse** than the
winner. Its geometry is usable; it simply is not the native selector's best
approach. Those score gaps are not seconds. This confirms that a positive screen
for two samples cannot supply a prediction of the fresh choice or its full cost.

The study reconciles **27,859 physical ticks**, **55,726 controller rows and the
same number of sensor rows**, and **526,230,327 decompressed trace bytes**.
Playing maxima remain four graph operations and 126 queries within four/384.
No runtime code, playing controls, new world queries or fitted constants were
introduced. The Python suite passes **465 tests**, including ten focused replay
tests; code review found no remaining blockers after the projection correction.

The first attempt at `02fc2b8061bbb947bd4bfc85554ef8df40600548` stopped after
the first replay because the audit compared a full capture record with its
historical compact projection. Dense trajectory, sensor, upstream, budget and
capture-milestone checks had already passed. The correction reconstructs that
existing projection, including its selected site and event-derived visit tick;
it changes no physics or case selection. The initial raw attempt remains at
`target/capture-flag-survey/arrival-replay-v1`, summary SHA-256
`b657c6330aefd55f6e922a4361af76bb8ec86f3c5e26151373331c2123eec44b`.
The complete four-case run was then repeated from the corrected frozen commit.

The [tracked projection](data/capture-arrival-replay-v1.json) preserves the full
plan, comparisons, original screens, three clocks, native diagnostics and all
44 raw-file hashes. The complete raw summary is
`target/capture-flag-survey/arrival-replay-v1-complete/summary.json`, SHA-256
`7db45e0f6e25aee19754eda02cccec4f1ce433c9f7ceb81f4cb334a2713e9209`.

This closes the distant geometry/solar comparison for the fixed corpus. The next
bounded step should test a source-measured shortlist near the predicted arrival
direction against fresh native choices, without borrowing future survey geometry.
Keep missing coverage, age limits, arrival orientation and future threat explicit;
do not turn these four one-tick acquisitions into a duration constant. Test that
shortlist observationally before admitting any complete remote capture cost or
changing live selection. There is no new live behavior in this slice to deploy.

The [independent audit](data/capture-arrival-replay-audit-v1.json) passes all four
cases, 44 new raw files, 48 retained-source files and four logs. It reconstructs
the original survey snapshots, all 6427 source-to-choice observations, both
retrospective frames, 16 frozen and 32 retrospective direction checks, and all
458 fresh native directions. It verifies the tracked projection and the results
above without importing the new replay runner. No findings remain. Existing
independent and physical/native-choice helpers are hash-pinned in the record.
The audit SHA-256 is
`affbf98cb15ace3988b470db84f5b9b07d203893cacb35002778fad751c34aff`;
its notes digest precedes this paragraph. The script remains at
`target/capture-flag-survey/arrival-replay-post-audit.py`, SHA-256
`251c5074d4685bf38ebc6717909544d59205d3909c525c98aefd06d6add9e826`.

## Reproduction

```sh
python3 tools/compare-arrival-replays.py \
  --binary target/capture-flag-survey/surface-mission-soak-1c4a9ac-profiled \
  --out target/capture-flag-survey/arrival-replay-repeat
```

Use a clean checkout and a new output directory. The retained ordinary raw archive
is required; the runner verifies it against the committed projection. Earlier
physical raw files are regenerated from the committed commands and digests.
