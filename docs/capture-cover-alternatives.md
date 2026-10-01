# Reachable alternatives after cover rejection

The [cooldown experiment](capture-cover-retries.md) changes selected sites but
does not reliably improve capture progress. The next question is whether a
measured alternative supplies both usable cover and a route to the objective.
This is a diagnostic study; no controller, scoring coefficient or default changes.

## Observed questions

In the recorded P1 -0.8 failure at world tick 3,930, 37 of 59 observed sites have
ground and approach cover. The native eight-site objective shortlist contains
three usable exposed round trips, one footing failure and four sheltered sites
with disconnected outbound routes. The other 51 sites have no native route
measurement. This does not establish that every sheltered landing is unreachable.

In the successful +0.8 control, the same retry has four sheltered round trips and
the native controller selects one. In the recorded asteroid case at tick 2,475,
one sheltered round trip is already measured, but its long walk contributes a
large score. These suggest different boundaries: shortlist coverage, actual route
failure and ranking a covered but longer approach.

## Diagnostic contract

`--probe-cover-ticks` takes at most 16 unique **world ticks**;
`--probe-cover-seat` identifies the observed player without changing the trial's
existing observer seat. Absent options preserve the harness output contract.
Each requested tick records the exact immutable observation and post-intent
mission telemetry. Fresh choices use the existing native read-only comparison,
which must reproduce the selected site, direction, solar plan and rejection
counters. Retained sites and unavailable choices remain explicitly unknown.

The route probe measures every site in that observation on a cloned physical
world, in batches of at most eight using the unchanged JointRoundTrip sensor.
It rejects stale/wrong-actor inputs, duplicate sites and partial landing surveys.
The combined diagnostic output never becomes a controller survey. Native route
rows must match their independently batched counterparts exactly. It does not
discover sites absent from the observed survey or supply a future cover guarantee.

Diagnostic physical queries and wall times are outside live planning quotas and
are profiled separately. No diagnostic result enters controls or live demand.
The ordinary 4 graph / 384 query accounting must remain unchanged on replay;
the extra probe work is not claimed to fit that allowance or a Pi frame budget.

The analysis distinguishes ground-and-approach cover from the ranker's stronger
ground/approach/departure preference. Ground-only cover is retained in the raw
record; the native below-40-height exception also requires reaching the approach
alignment and speed gate. A distant site's projected height alone is not proof
that the ship can descend there. A diagnostic round trip remains a route
hypothesis, not a physically completed approach, claim or departure.

## Frozen replay plan

Freeze the probe, tests, analysis and this plan before measuring additional routes.
Use the cooldown study's existing commands and both arms, keeping its seeds,
deadlines, policies, observer seats and native query cadence. Add only the probe
options and ordinary traces where the preceding study did not retain them.

| Recorded case | Tested seat | World ticks, both arms |
| --- | ---: | --- |
| P1 value-destination -0.8 | 0 | 3,270; 3,930; 4,080 |
| P1 value-destination +0.8 control | 0 | 3,510 |
| Recorded world-3 asteroid/P2 | 1 | 2,475; 2,895; 9,383; 10,875 |
| Fresh world-3 asteroid/P1 from cooldown study | 0 | 9,750 |
| Fresh world-3 asteroid/P2 from cooldown study | 1 | 13,365 |

These are ten diagnostic replays and twenty snapshots of known trajectories,
including both favorable and unfavorable cooldown outcomes. They are not fresh
strength trials. Unavailable snapshots and changed capture tasks remain in the
denominator, including the candidate that never reaches the original failed trip.

Require exact physical reports, mission/visit records and all previously retained
controller/evaluator/planner streams. Require sensor stage calls/counters and
combined planner allocations to match, apart from wall time. Bind each successful
native diagnostic to the replay's exact trace observation and mission record.
Audit batch completeness, native route parity, round-trip validity, cover
penalties and native score ordering. Preserve unknowns and all source/raw hashes.

```sh
python3 tools/probe-cover-alternatives.py \
  --study target/cover-retry-cooldown/v2 \
  --binary target/release/examples/surface_mission_soak \
  --out target/cover-alternatives/v1
```

Use the results to choose a separately declared next experiment. Do not retune
the rejected cooldown or claim a landing-policy improvement from diagnostic
route availability alone.

## Results

All ten replays and twenty requested snapshots completed in
`target/cover-alternatives/v1`, using frozen source `98f9831` and preserved binary
`target/cover-alternatives/surface_mission_soak-98f9831`, SHA-256
`dde8c8233b0eb12704dcf2deac70424a33a44a84809524f439d47411a00cfe89`.
The [results archive](data/capture-cover-alternatives-v1.json) contains commands,
hashes, parity checks and per-site findings. Its linked compressed input archive
retains every complete probe observation, native assessment and route batch for
reanalysis without repeating physics. All raw hashes were rechecked at export.

Physical reports, mission/visit records and every previously retained trace and
planner stream match their source runs exactly. Sensor parity covers 473,846
player observations. The unchanged main planner executes 258,523 dispatch ticks,
with a maximum of four graph operations and 161 physics queries in one tick.
No visit audit has an unverified or inconsistent ending.

There are 17 valid fresh-choice comparisons and 17 full route probes. The latter
perform 938 site-route measurements in 125 batches, counting repeated sites and
snapshots. Three route probes have no hostile flag objective; three native
comparisons have no selected site. These are retained as unavailable, not zero
available routes. Every native route represented in a full probe matches exactly.

### Directed failure: more sites do not provide shelter

At all three frozen clocks in both arms, all 37 ground-and-approach-covered sites
have disconnected outbound routes in the existing JointRoundTrip model. Only
sites 41–45 provide modeled round trips, and none has ground, approach or
departure cover. The native shortlist already measures 41–43; expanding it adds
44 and 45 without solving exposure. This result applies to the observed sites
and current model, not every possible physical path or future opponent position.

The +0.8 successful control is different: 53 of 59 observed sites have modeled
round trips, including all 37 approach-covered sites. Four covered routes are
already native-eligible, and the controller chooses site 0 in both arms. A wider
survey is not needed to explain or preserve this successful choice.

### Recorded asteroid case: ranking and incomplete evidence

At world tick 2,475, 22 of 29 approach-covered sites have modeled round trips.
One is already native-eligible; the other 21 are absent from the eight-site
route shortlist. The native ranker nevertheless chooses exposed site 49:

| Best direction at tick 2,475 | Approach score | Cover penalty | Round-trip score | Total |
| --- | ---: | ---: | ---: | ---: |
| Selected exposed site 49 | 240.29 | 4,000.00 | 0.00 | 4,240.29 |
| Sheltered site 4 | 148.93 | 0.00 | 4,158.98 | 4,307.91 |

These are native controller score units, not seconds. The covered site's long
walk outweighs the finite exposure penalty. Yet the execution gate cannot start
the exposed descent while the threat persists. Merely remembering a rejected
site does not reconcile ranking with that execution constraint.

At tick 2,895, the opponent's changed position leaves 24 approach-covered sites.
Seventeen have diagnostic round trips, but **none is in the native shortlist**:
the four shortlisted sheltered sites all report disconnected routes. Therefore,
zero native-eligible covered routes does not generally mean all covered routes
are unavailable. Both arms show this distinction at these clocks. The expanded
diagnostic is not admitted to play and supplies no completed-capture evidence.

The original later failed trip introduces another constraint. At tick 9,383,
the destination is neutral, so no hostile-flag route probe applies. All 11
approach-covered sites are rejected for solar safety in both directions. At
tick 10,875, the destination has a hostile flag and all 18 approach-covered
sites have disconnected modeled outbound routes. The cooldown arm has no native
selected site at these two clocks; it did not replay that original attempt.

### Changed fresh matches: no covered round trip at first exclusion

For P1 at tick 9,750, seven of 44 observed sites have approach cover; all seven
have disconnected outbound routes. Only exposed site 50 has a modeled round
trip. The predecessor selects it; the cooldown arm has no selected site at that
tick, while its independent route diagnostic still completes.

For P2 at tick 13,365, all 23 approach-covered sites among 47 observed candidates
have disconnected outbound routes. Seven exposed sites have modeled round trips.
Both arms therefore face an exposure problem that rotating among usable ground
routes does not solve. These snapshots explain constraints at the first exclusion;
they do not assign all later win/loss or recovery differences to one decision.

## Validation and next boundary

Four scenario objective tests (including two new tests), 288 AI unit tests,
four physical destination tests, 39 harness tests and 592 Python tests pass.
Formatting, strict AI Clippy with `--no-deps`, and a normal harness build without
sensor profiling pass. Strict scenario Clippy reports seven existing findings
in unchanged `pilot.rs`, `render.rs`, `surface_sortie.rs` and `lib.rs`; the log
hash and affected paths are retained in the archive. No unrelated lint changes
were made.

The native comparison takes at most 0.157 ms in this run. Full route probing,
including world cloning, reaches 45.070 ms on this host and performs extra
physical work outside live quotas. It must remain a diagnostic, not a replacement
for the bounded playing survey or a claim about Pi performance.

**Decision:** retain defaults. The next policy experiment should act on a
witnessed cover failure while still exposed: distinguish a known eligible
covered route, missing route evidence, and a currently blocked approach.
Test requiring usable cover at that point, with bounded evidence acquisition
for unknown alternatives and an explicit exit when the approach cannot progress.
Preserve unexposed first selections, successful covered retries, native solar and
route gates, original deadlines and shared work allowances. The evidence does
not justify a blanket early abort from an empty shortlist, a larger exposure
weight, or deploying the diagnostic's exhaustive work as a live sensor.
