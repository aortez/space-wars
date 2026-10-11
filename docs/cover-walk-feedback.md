# Advance cover search after a finished walking attempt

Cover search now advances through completed unsuccessful walking attempts.
The physical trials consume 36 completion notices, but produce no additional
captures or armed wins. Keep the option disabled by default.

The [extended-corridor study](extended-landing-routes.md) finishes 28 longer
walking hypotheses without finding a usable route. Cover search keeps waiting
for its first candidate despite that completed effort. This experiment lets it
request another candidate while keeping the original site's other route types
unknown and eligible for later positive evidence. It remains opt-in.

## Scheduling and evidence

`--cover-walk-feedback true` requires extended walking corridors and their
existing prerequisites. Reports use `live_joint_objective_v10` or
`live_jetpack_objective_v10`. Disabled runs retain prior behavior and omit the
new optional fields.

An unsuccessful focused patch or requested corridor records its candidate ID
only after the child job finishes. Successful walks, skipped bounds, unfinished
jobs and actual-hatch checks do not produce this signal. The live planner may
attach `exhausted_walk` to the existing request evidence after validating the
request. Its actor, generation, objective, request tick and original measurement
tick identify historical work; it is never a negative route certificate or
permission to land, exit, fly, claim or board.

Cover search consumes the notice only for its current selected, currently
observed unknown candidate. Check actor, site, objective, material revision,
source age, observation tick and request identity. Both source and request
must be from this search. Missing, stale, foreign or invalidated evidence keeps
the original wait. A positive usable route takes precedence through the native
selector, even if that site previously supplied an unsuccessful walking attempt.

Move an attempted walking site from `pending` to `walk_deferred`, without adding
it to rejected sites or counting a measured route. Request the next pending
site within the existing eight-probe limit. If all eight walking hypotheses
finish, retain their unknown status and the last requested site's full fallback
until the original 600-tick deadline; do not create more probes or reset time.
Material/flag context changes clear the deferred list with the original limits
intact. Current cover, solar, required-site and actual boarding checks remain.

Once an unsuccessful walking probe has finished, a different selected site with
a current landing scan may replace its unfinished full survey. Count the old
job's charged work, then submit a new snapshot with its own real source tick.
Do not retime retained measurements, cancel unfinished walking hypotheses,
restart for deferred/absent scans, or replace an actual touchdown request this
way. Without a changed selected request, the original full fallback continues.

The shared 4 graph / 384 query allowance and 120-tick lifetime stay unchanged.
Feedback uses existing finished work and bounded metadata; it performs no extra
physics queries. Snapshot construction and immediate/publication validation
remain outside operation quotas and inside sensor timing. No frame-time or Pi
performance claim follows from the operation limit.

## Frozen physical plan

Freeze code, tests, runner and this plan before collecting outcomes. Compare to
every shared case in `target/extended-routes/v1/summary.json`:

1. Replay all six directed missions with feedback disabled. Require identical
   physical/mission fields, seven controller/evidence streams per game, ordinary
   sensor rows, allocation ledgers and non-timing planner counters.
2. Enable feedback for all six 180-second directed missions, retaining both
   route models, blocked bearing -0.8 with cover on/off, +0.8 control with cover
   on, seed 42 and seat 0.
3. Enable feedback for all eight 600-second armed matches, regardless of
   directed results. Retain both worlds, both v13 seats versus v10, both route
   models, weapons, two active planners and no asteroids.

Run at most two games concurrently. Change only feedback, binary and output
path; retain every loss, unknown route and unfinished visit. Preserve the
original physics/evidence auditor, first action differences, physical claims,
original-ship boarding and departure witnesses. Additionally audit every
consumed completion notice against its source and search context, with the
unchanged probe/deadline limits. Record receipt delivery, candidate advancement,
route delivery and physical success separately. Verify original hashes before
and after the study, and preserve all new raw streams.

```sh
python3 tools/validate-walk-feedback.py \
  --prior target/extended-routes/v1/summary.json \
  --binary target/walk-feedback/surface_mission_soak-COMMIT \
  --out target/walk-feedback/v1
```

## Observed feedback and physical outcomes

The frozen `06749cc` implementation completes all 20 planned runs. All six
disabled replays retain their physical/mission fields, 42 exact evidence streams,
64,800 ordinary sensor rows, allocation ledgers and non-timing planner counters.
All prior input hashes remain unchanged.

Across the 14 enabled games, 40 requests deliver a completed-walk notice, first
received at source ages of 4–38 ticks. Cover search consumes 36 notices and
advances its pending list. The planner replaces 34 finished probes when their
next selected candidates receive current scans. Four later notices repeat the
last candidate after all eight have been tried; they do not spend more probes.

Completed captures and departures remain unchanged in every game. Both directed
controls still complete the enemy and neutral captures. The four blocked games
still complete only the neutral capture. All eight armed matches remain losses,
with identical controls, physical visits, round records and combat metrics.
Twelve of the 14 games retain their complete original action sequence.

The two blocked cover-on games first change controls at tick 9,159, during a
later enemy approach. That visit now lasts from 8,386 to 10,113 and ends at a
cover-evidence deadline. Previously it ended at 9,470 for leaving the approach
frame, followed by another failed visit ending at 10,657. Neither version lands
or captures during those visits. The archive keeps both sides of the first
changed observation and the later deadline rows.

## What still prevents a usable route

In each blocked cover-on game, the first search starts at 3,911 and retains its
4,511 deadline. Its first five walking attempts now hand off promptly:

| Completed candidate | Measurement tick | Notice received | Next requested candidate |
| --- | ---: | ---: | --- |
| Site 0 | 3,996 | 4,026 | Site 1 |
| Site 1 | 4,027 | 4,059 | Site 2 |
| Site 2 | 4,060 | 4,094 | Site 3 |
| Site 3 | 4,095 | 4,131 | Site 4 |
| Site 4 | 4,132 | 4,170 | Site 5 |

Site 5 is approximately **222.16 angular ground samples** from the flag. Its
rounded separation plus endpoint margins requires 226 steps, above the unchanged
224-step bound. No bounded walking attempt starts, so no completion notice is
invented. Sites 5–7 remain pending, sites 0–4 remain unknown in `walk_deferred`,
and the search reaches its original deadline with six requested probes.
The later search repeats this pattern with its own original deadline of 10,112.

In both world-1 P1 armed games, the search starts at 6,304. It advances through
sites 34–41 at ticks 6,341, 6,346, 6,351, 6,356, 6,361, 6,366, 6,371 and 6,376.
All eight walking attempts finish without a usable route. The search retains
eight unknown sites, continues requesting site 41 for its full fallback and
keeps the original 6,904 deadline. Exposure clears at 6,719, ending the search
normally before that deadline. No walking failure enters the rejected-site list.

All 40 extended attempts finish without positive walking evidence. The existing
shorter corridors retain 39 starts, 33 completions and 33 successes; six shorter
attempts remain unfinished. The full fallback still completes no candidates and
starts no powered forecast. Advancing a request fixes the scheduling stall but
does not supply a path through the physical obstacles.

Remaining work includes handling candidates that the bounded walker cannot
attempt and delivering evidence for other route types. An unsupported walking
method must remain distinct from an unsuccessful measured walk, and neither
can establish that the flag is unreachable. The probe limit, deadline, source
lifetime, equipment checks and bot defaults remain unchanged.

## Evidence and validation

The [result manifest](data/cover-walk-feedback-v1.json) records every game,
physical visit, first receipt, advancement count and comparison. Its
[compressed evidence](data/cover-walk-feedback-v1.json.gz) contains 40 exact
documents: current/prior summaries, hashes for all 261 raw files, physical and
feedback witnesses, the remaining-bound calculation, archive script and logs.

All 448,870 pilot observations and 256,835 dispatch ticks pass the original
physical/evidence auditor and the added feedback checks. The combined charge
never exceeds 4 graph / 384 queries per tick. The 37,886 route publications
include repeated validation; none is older than 120 ticks and none is a positive
extended corridor. Live dispatch spends 2,542 fewer graph operations and 20,760
fewer queries across this corpus, while snapshot and other synchronous work
remain outside those quotas. This is not a measured Pi speedup.

Validation passes 999 Rust tests in 16 suites, 651 Python tests, strict AI
library/harness Clippy, formatting and profiled/ordinary release builds.
Scenario Clippy reports the same seven pre-existing findings in unchanged
files. Unit checks cover one-time advancement, later positive eligibility,
absent/stale/foreign/context-mismatched notices, both source clocks, all-eight
waiting, material changes, disabled/zero-work modes, cancellation, expiry,
two-actor request replacement and native full-fallback parity. The two generated
worlds and seat swaps remain a small correlated sample.

The preserved binary is
`target/walk-feedback/surface_mission_soak-06749cc`, SHA-256
`ad2414e6c1b7161e518316fa554578a900a451cbfa12be5f594635df4f958337`.
The complete summary at `target/walk-feedback/v1/summary.json` has SHA-256
`5c1851dcc5a764230fb78b3ae3f1e8218845ea2b3bb15cd7950440a991eafba1`.
