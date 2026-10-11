# Longer requested walking corridors

The extension fits the original allowance and lifetime in positive unit cases,
but supplies no new usable routes in this physical corpus. All 14 games retain
their controls and outcomes. Keep it disabled by default; the remaining cover
wait needs another route hypothesis or another candidate.

The [requested-route study](requested-landing-routes.md) resolves the directed
control's cover wait, but the blocked case requests a site about 182 samples
from its flag, beyond the 96-edge limit. This experiment extends the measured
walking corridor while preserving the shared 4 graph / 384 query allowance
and the 120-tick source lifetime. It remains opt-in.

`--extended-objective-routes true` requires requested corridors and their
existing prerequisites. Reports use `live_joint_objective_v9` or
`live_jetpack_objective_v9`. Default planning and prior opt-in modes retain
their original behavior.

## Measurement and accounting

Keep the original dispatch sequence for all corridors inside the 96-edge
bound. For a longer selected-site or actual-hatch corridor, permit up to 224
edges including the existing endpoint margins. The rounded endpoint separation
must therefore be at most 220 samples. Still test only the shorter surface arc;
missing support, obstructed capsules/hulls and failed boarding stay unknown.
The original full survey remains the fallback, with the same snapshot and age.

The longer pass commits an edge's result when its final return support query
succeeds. This removes the separate graph operation that previously only
copied the completed edge's length, query footprint and endpoint. It retains
the node inspection, both directions' nine terrain capsule checks, nine hull
checks and three support rays. Every advertised physics operation still makes
exactly one actual query. The result update appends at most three fixed query
areas; it performs no variable-length scan, extra query, graph search or child
job. This follows the existing ground survey's pattern
of committing an edge after its final physical query.

Two maximum corridors require at most 462 graph operations including their
start windows and parent handoffs, within 120 dispatches at four operations.
This is an operation bound, not a frame-time guarantee. Unit checks compare the
old and new edge handoffs on the same long hypotheses: identical paths, results,
query footprints and query counts, with one fewer graph operation per completed
edge. They also check near-limit two-actor delivery, both directions, blocked
hulls, unchanged short-route timing, actual return, full-survey parity, warm
snapshot age, zero allowance, expiry and cancellation.

Query geometry, walking rise, dependency validation, landing/boarding permissions
and the source clock are unchanged. The new pass adds no jumping or flight
forecast. Count extended starts/completions/successes as a subset of corridor
work, including retired jobs. Snapshot preparation, fixed setup, publication
validation and ordinary immediate/on-foot sensors remain outside dispatch.

## Frozen physical trial plan

Freeze implementation, tests, runner and this plan before collecting outcomes.
Use every shared case from `target/requested-routes/v1/summary.json`:

1. Replay the six directed missions with extension disabled. Require the prior
   physical/mission fields, seven controller/evidence streams per game, ordinary
   sensor rows, allocation ledgers and all non-timing planner counters to match.
2. Enable the extension in all six 180-second directed missions, keeping both
   route models, blocked bearing -0.8 with cover on/off, +0.8 control with cover
   on, seed 42 and seat 0.
3. Enable it in all eight 600-second armed matches, regardless of directed
   results. Retain both generated worlds, both v13 seats versus v10, both route
   models, weapons, two active planners and no asteroids.

This is six retention replays and 14 new runs. Change only the extension option,
binary and output path. Preserve every loss, unknown route and unfinished visit.
Do not change the allowance, lifetime, cover deadline, probe count, equipment
checks or defaults in response to outcomes. Use two concurrent runs at most;
desktop timings do not establish Pi performance.

Use the existing auditor for physics, source clocks, current validation,
per-tick combined quotas, launches, claims, original-ship boarding and departure.
Keep first deliveries, selected-site observations and cover-state transitions,
and record the first action difference from the prior corpus. Route delivery
and physical success remain separate. Verify all original hashes before and
after the experiment; keep the frozen inputs and all new raw streams.

```sh
python3 tools/validate-extended-routes.py \
  --prior target/requested-routes/v1/summary.json \
  --binary target/extended-routes/surface_mission_soak-COMMIT \
  --out target/extended-routes/v1
```

## Physical results

The frozen `a5bce9f` implementation completes all 20 planned runs. The six
disabled replays retain their physical/mission fields, 42 exact evidence streams,
64,800 ordinary sensor rows, allocation ledgers and non-timing planner counters.
All prior input hashes remain unchanged.

The 14 enabled games retain every original control action, physical visit,
round record and combat metric. Both directed controls still complete the
enemy and neutral captures. The four blocked directed games still complete
only the neutral capture. All eight armed matches remain losses, with identical
finish ticks and completed-sortie counts.

There are 28 extended attempts and 28 completions, with **zero usable extended
walks**. Twenty occur in the two blocked cover-on missions; eight occur in the
two world-1 P1 armed games. No extended attempt starts in the other ten games.
The original shorter corridors retain their 39 starts, 33 completions and 33
positive results. Including those unchanged routes, the new totals are 67
starts, 61 completions and 33 successes; six original shorter attempts remain
unfinished. A completed hypothesis with no positive result stays unknown.

Total live dispatch work increases by 96 graph operations and 74,452 queries
across the 14 games. Every tick still stays within 4 graph / 384 queries, but
there is no observed gameplay benefit from this extra work. Positive long-route
delivery is demonstrated by the smooth-world unit cases, not by this corpus.
The full fallback still completes no candidates and starts no flight forecasts.

## Why the selected walks fail

After freezing all outcomes, two additional diagnostic replays add logging
around the existing corridor steps in a detached copy of `a5bce9f`. Each matches
its original seven evidence streams, ordinary sensors, allocation ledger and
non-timing counters. The diagnostic binary, patch, complete logs and original
observations are archived separately; they do not replace the frozen study.

In the blocked walking mission, the first extended request uses the snapshot
from tick **3,996**. It visits 113 supported nodes, then the world capsule check
at node **388** fails. The last accepted node is 389. All ten attempts in this
replay fail the same check, including the later return to the enemy approach.
This is a clearance failure before the length bound, not an age-expired job.
Diagnostic ticks identify the measurement source, not the completion time.

The controller keeps requesting cover site 0. At tick 4,511 the search still
has the same eight pending sites and only one probe; it reaches its original
deadline, then abandons the enemy visit at 4,512. Finishing the walking attempt
does not currently tell cover search to try its next candidate.

In the world-1 P1 walking replay, the first extended request uses tick **6,337**.
It visits seven nodes before the candidate vehicle's hull blocks the capsule
at node **268**. All four attempts fail there. The selected cover candidate is
site 34 on planet 0. Neither failure proves that every walk, jump or powered
route to the flag is impossible.

The next step is to expose a completed walking-hypothesis signal so cover
search can try another candidate without labeling the current site unreachable.
Other walking arcs, jumps and powered forecasts must remain possible. This
experiment changes no search deadline, probe count, equipment check or default.

## Evidence and validation

The [result manifest](data/extended-landing-routes-v1.json) records every game,
unchanged outcome, physical visit and work total. The
[compressed evidence](data/extended-landing-routes-v1.json.gz) contains 47 exact
documents, including both study summaries, hashes for all 261 raw study files,
physical and cover witnesses, two diagnostic replays, the diagnostic patch,
the archive script and validation logs.

All 448,870 pilot observations and 256,835 dispatch ticks pass the existing
physical/evidence auditor. The 37,536 publications include repeated validation
of original shorter routes; none is a successful extended route and none is
older than 120 ticks. Validation passes 991 Rust tests in 16 suites, 649 Python
tests, strict AI library/harness Clippy, formatting and profiled/ordinary release
builds. Scenario Clippy reports only the same seven pre-existing findings in
unchanged files. Two generated worlds and seat swaps remain a small correlated
sample, and these desktop runs make no Pi performance claim.

The preserved binary is
`target/extended-routes/surface_mission_soak-a5bce9f`, SHA-256
`79b89ca942020fe206d5eef1ef0099f306bc9db075f98ba4e18c1774aae0d222`.
The complete summary at `target/extended-routes/v1/summary.json` has SHA-256
`d76241ce03b1b19b4b25dacfa2e62b4a118a545d8fe2eb4846376c82f7f5825f`.
