# Focused landing routes within the shared allowance

The opt-in focused pass delivers walking routes within the existing allowance
and enables additional physical captures. Sheltered directed approaches still
time out, and v13 loses all eight armed trials. Keep this mode experimental.

The [powered mission study](powered-mission-integration.md) found that a full
ground survey cannot finish within 120 ticks at four graph steps per dispatch.
This experiment measures a small walking corridor before the full survey.
It keeps the shared 4 graph / 384 physics query allowance, source lifetime,
current landing checks, physical hull and route dependency validation.

`--focused-objective-routes true` is opt-in and requires route dependencies
and early route delivery. The report uses `live_joint_objective_v7` or
`live_jetpack_objective_v7`. Native sensors and default planner configurations
keep their existing behavior.

## Measurement contract

Each request tries one candidate from the existing eight-site shortlist, or
the actual landed pose when present. A small arc joins its hatch and the flag,
with two ground-sample margins at each end. At most 17 of the ordinary 512
sample positions are measured. Longer approaches skip this extra pass. New
requests rotate through the shortlist for the same objective, so an unavailable
first candidate does not monopolize every retry. Reset/removal clears this
selection state; changing objectives starts at the first candidate.

The patch uses existing footing queries, directed adjacent walking checks,
hypothetical ship geometry, round-trip search and path dependency construction.
It uses the existing sparse index for small graphs. The hull overlay queries
each path sample directly, consuming the query allowance on every call. This
performs additional physics queries where the ordinary overlay would first
spend a graph step on a distance test; it does not batch queries or relabel
uncharged work. Tests count actual preview calls against charged queries.

Only positive patch routes enter early publication. Missing footing or a
disconnected patch leaves other walks, jumps and powered routes unknown. The
complete original survey runs afterward, against its original snapshot and
clock, and can replace the early answer. Its completed result must still match
the native full survey. Patch attempts, completions and successes are counted
separately from complete candidate/survey measurements. Source expiry and
retired-work accounting remain unchanged.

This first pass measures walking routes even when powered planning is enabled.
It does not simplify the existing flight predictor or claim that its forecast
can finish under this allowance. Synchronous observations, snapshot creation,
publication validation and bounded setup remain outside dispatch counters.
Operation quotas do not measure total frame time.

## Frozen replay plan

Freeze implementation, tests, runner and this plan before collecting mission
outcomes. Use the exact shared cases from `target/powered-mission/v3/summary.json`:

1. Replay all six directed missions with focused routes disabled. Require the
   original physical/mission/visit report fields, seven controller/evidence
   streams per run, ordinary sensor counters and shared allocations to match.
2. Enable focused routes in all six directed missions: blocked approach with
   cover on/off and the walkable control, each with walking/powered v13. Keep
   the 180-second horizon, seed 42, real mission state, cover/retry settings,
   flag-cost admission and current-neutral sensing.
3. Enable focused routes in all eight shared armed matches, regardless of the
   directed outcomes. Keep both generated worlds, both v13 seats versus v10,
   the same 600-second rules, weapons and two active planners. Compare against
   the immutable original shared runs. Preserve every loss and unfinished visit.

This is six retention replays and 14 new runs. All configuration except the
focused-route switch, executable and output location remains the same. Two
independent runs may execute concurrently; desktop timing is not a Pi benchmark.
Keep the earlier games and all raw hashes. Require per-tick conservation, the
combined allowance, original measurement age and current publication validation.
Audit positive partial routes, first delivery clocks, matching capture-site
telemetry, launches, claims, boarding and departure using the existing auditor.
Record the first action difference in each comparison. Route delivery,
controller selection, physical success and winning remain separate outcomes.

Do not increase quotas, extend lifetimes or change defaults based on these
results. If the focused pass delivers a route but the controller still cannot
capture, preserve that distinction and diagnose the observed handoff.

```sh
python3 tools/validate-focused-routes.py \
  --prior target/powered-mission/v3/summary.json \
  --binary target/focused-routes/surface_mission_soak-COMMIT \
  --out target/focused-routes/v1
```

## Delivery and physical results

The frozen `aeb1174` binary completes all 20 runs, and the original audit passes
without correction. All six disabled replays match their earlier physical and
mission fields, 42 controller/evidence streams, 64,800 ordinary sensor rows and
shared allocations. The earlier corpus remains unchanged.

Every directed run now receives positive route evidence. Walking and powered
arms have the same directed outcomes:

| Case | Age at first delivery per request | Requests delivering routes | Old completed sorties | Focused completed sorties |
| --- | ---: | ---: | ---: | ---: |
| Blocked, cover on | 39 ticks | 9 | 0 | 1 |
| Blocked, cover off | 39 ticks | 23 | 0 | 1 |
| Walkable control, cover on | 50–56 ticks | 9 | 0 | 1 |

Counts are per run. Each completed directed sortie captures the neutral
planet, boards the original ship and departs. The enemy-directed visits do not
earn a claim or departure. With cover enabled they fail at the cover-route
evidence deadline; the cover-off case exhausts its approach budget and retains
another unfinished visit at the horizon.

Across the 14 new runs, 1,210 focused attempts start, 601 finish and 593 produce
positive walking measurements. The eight completed patches without a usable
route remain unknown; no negative patch result is published. There are 35,461
deliveries, including repeated validation of the same measurement. First
delivery ages in armed runs range from 32 to 113 ticks; no published evidence
exceeds 120 ticks. The complete fallback survey still finishes zero candidates,
and no flight forecast or jetpack launch occurs.

## Armed results

Both active planners use focused routes in these eight matches. Counts and
outcomes below are for the v13 seat, so this compares games where both bots
gain route delivery. All physical claims in this table also complete boarding
and departure.

| World | v13 seat | Capture model | Old sorties | Focused sorties | Old result | Focused result |
| --- | --- | --- | ---: | ---: | --- | --- |
| 0 | P1 | Walking | 1 | 4 | Loss | Loss |
| 0 | P1 | Powered | 1 | 4 | Loss | Loss |
| 0 | P2 | Walking | 2 | 4 | Win | Loss |
| 0 | P2 | Powered | 2 | 5 | Win | Loss |
| 1 | P1 | Walking | 1 | 1 | Loss | Loss |
| 1 | P1 | Powered | 1 | 1 | Loss | Loss |
| 1 | P2 | Walking | 1 | 3 | Win | Loss |
| 1 | P2 | Powered | 1 | 3 | Win | Loss |

Six of eight runs earn more completed sorties, including enemy captures. For
example, world-0 P1 captures enemy-held planets 0 and 1 and departs at ticks
10,224 and 18,421. Nevertheless, the original four v13 wins become losses.
The two generated worlds and seat swaps are a small correlated sample, and the
powered controller also retains its existing walking behavior differences.
These results support timely route delivery, not a default change or a claim
of stronger combat performance.

## Remaining cover-search limit

The walkable directed control provides a concrete next case. At tick 3,230,
the planner delivers and the capture controller selects site 58, using a
50-tick-old positive route. Its current cover list is empty and the controller
enters `seek_cover`. At tick 3,565 it starts requiring cover-route evidence.
By tick 4,165, cover search has one probe, zero measured sites and sites 0–7
still pending; it reaches its evidence deadline. The mission abandons the visit
at 4,166. The archive retains these four observation/controller rows.

Thus a timely nearby walking route does not supply the different routes needed
by cover search. This pass is limited to a short arc near the flag; longer
approaches still enter the full survey. The next experiment should measure the
requested cover-site corridors within the same allowance and preserve their
original age, then check the resulting landing/capture handoff. Powered
crossings still require a separate delivery solution. No timing or geometric
acceptance threshold changed in this study.

## Evidence and checks

The [result manifest](data/focused-landing-routes-v1.json) records each original
comparison, physical visit outcome, combat counter, delivery age and charged
work. Its [compressed evidence](data/focused-landing-routes-v1.json.gz) includes
the exact current/prior summaries, raw file manifest, physical milestones,
route-delivery witnesses and the failed cover-search handoff. Full per-tick
streams remain at the hashed local paths.

All 450,376 new pilot observation rows and 257,588 new dispatch ticks pass the
auditor. Combined work never exceeds 4 graph / 384 queries per tick. Validation
passes 976 Rust tests, 646 Python tests, strict AI library/harness Clippy,
formatting, and profiled/ordinary release builds. Scenario Clippy retains seven
pre-existing findings in unchanged files. No Pi performance claim is made.

The preserved binary is
`target/focused-routes/surface_mission_soak-aeb1174`, SHA-256
`cd085dace65e9ab6163f42edd2e5e5c320f30179edc1e884c5aa00dc42d73298`.

## Cache-counter correction

Post-trial review found that `reused_ground` counted reuse in the full survey
but omitted reuse inside the focused pass. The new warm-cache regression fails
against `aeb1174`. Preserve the complete v1 study and archive. Correct the
counter by accumulating focused reuse while it runs and retaining that subtotal
after the full-survey handoff. This changes diagnostics only; charged graph and
query operations still come from the same queue.

Freeze the correction before rerunning the identical 20 cases into `v2`.
Require all 14 focused games to retain their physical/mission fields, seven
controller/evidence streams each, ordinary sensor counters, every allocation
ledger row and all non-timing planner counters except `reused_ground`. Require
the same 14 comparisons against the original baseline. Keep both studies.

```sh
python3 tools/validate-focused-routes.py \
  --prior target/powered-mission/v3/summary.json \
  --binary target/focused-routes/surface_mission_soak-CORRECTION \
  --replay target/focused-routes/v1/summary.json \
  --out target/focused-routes/v2
```

The `7d3faca` correction completes all 20 replays. All 98 focused
controller/evidence streams, 450,376 ordinary sensor rows, 14 complete allocation
ledgers, physical outcomes and original comparisons match v1. The six disabled
replays also retain their earlier exact matches. All 28 physical/delivery
witness files are byte-identical and reuse the original evidence archive.

Six armed runs now correctly report 571 previously omitted reused footing nodes
and 1,142 avoided ground queries in total. No walking-edge reuse count changes,
and no actual charged work changes. The warm-cache regression now passes,
including monotonic accounting through the focused/full-survey handoff.

Final validation passes 977 Rust tests, 646 Python tests, strict AI Clippy,
formatting and both release builds. The scenario Clippy check still reports only
the same seven findings in unchanged files. The [correction manifest](data/focused-landing-routes-v2.json)
and [compressed validation evidence](data/focused-landing-routes-v2.json.gz)
preserve the replay comparisons, corrected counters, hashes and test logs.
The original outcomes and experimental/default decision above are unchanged.

The current verified binary is
`target/focused-routes/surface_mission_soak-7d3faca`, SHA-256
`8be0a7949cc52a9d4bf758fecad008644d20f824a079e867a44984c975f93bf6`.
Final replay summary SHA-256:
`17372b382b09c8155ee1636482ab96f49b5e8d18359a85ecc0b91f1f78b690ec`.
