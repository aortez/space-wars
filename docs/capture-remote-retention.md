# Retaining source survey geometry

## Plan

The active survey screen left four numeric remote local references without raw
site geometry after their planner request retired. This slice retains a bounded
amount of original geometry and asks whether it can support the same conditional
arrival screen. It does not change live controls, destination ranking, acquisition
time, or future-threat modeling. Deployment remains paused.

`--retain-remote-surveys true` requires the existing fixed-source remote-arrival
comparison. One diagnostic cache per configured pilot observes the actual world
and the planner's existing raw snapshot before controls. It keeps at most two
neutral-planet groups, each with up to four slots from one original generation.
No work is dispatched and no physics is queried. The cache is absent by default.

Admission requires a sample measured at this actual observation or the preceding
contiguous observation. Its planet pose, revision and neutral identity must match
the actual measurement frame. First-seen older data is refused, including after
a gap or reset. The original generation, measurement tick, geometry, status,
historical opponent and cover are preserved. Snapshot time does not refresh age.
All samples retain the existing 1800-tick source and projected-arrival age limits.

Pending or missing samples leave prior evidence intact. A fresh negative or
incomplete sample replaces its old positive slot. A newer generation replaces a
whole group only after an admissible actual measurement, without inheriting old
slots. Duplicate IDs and regressed replacement generations are refused; a resumed
retained generation may refresh its own slots with newly witnessed measurements.
Fresh conflicting evidence from a refused generation clears the affected group.
Same-tick calls do not rewrite history. Groups are evicted by oldest latest measurement, with lower
planet index evicted first on ties. Positive and negative groups consume the
same capacity. Material, radius, ownership/flag, claim rules and capture counters
invalidate the affected group. Actor, episode, ship or spaceling changes, schema
failure, death/finished matches, clock regression or observation gaps clear memory.

Frozen snapshots are private typed records bound to the source observation and
pilot identity. They feed only the diagnostic screen, separately from ordinary
and hypothetical controller inputs and the evaluator's existing cost cache.
Retained groups can add at most sixteen charged direction operations across a
comparison: two planets, four sites, two directions. The shared residual allowance
remains 64 after unchanged playing work, with zero diagnostic physics queries.
Cache copying, validation, projection and serialization consume CPU outside this
operation count. Retention also happens outside the existing observation/dispatch
timing clocks; no Pi performance or timing-overhead claim is made.

Before measuring outcomes, freeze runtime, runner, tests and this plan in a commit.
Repeat all 13 prior fixed commands in `capture-remote-arrival-v1.json`, pairing
active-only versus retained geometry (26 runs, 21 source groups, 62 candidates).
Keep all seeds, source ticks, durations, seats and quiet/asteroid conditions.
Use a `sensor-profile` release binary. Neither new source selection nor a positive
screen is a success precondition: distant arrivals may exceed the age limit or
lack a modeled handoff. Retain refusals, partial results and publication delays.

`tools/retain-remote-surveys.py` verifies that active-only reports and allocation
ledgers equal the existing raw archive. It checks full control/observation traces,
physical outcomes, sensors and upstream work across both arms; the old motor
forecasts and costs must remain exact or retain an exact censored prefix.
Reconstruct each retained measurement's dispatch and actual planet frame and
verify uninterrupted identity through source time. Reuse the existing independent
projection/solar reconstruction with its 0.01-unit sign ambiguity and 0.02-unit
departure-order ambiguity thresholds. Audit capacity, queue lifetime, cumulative
work and the residual budget. Preserve commands, hashes, raw records, unknown
reasons and all candidate denominators, then request an independent review/audit.

Tests cover previous-tick dispatch, retirement, negative replacement, generation
changes, conflicting updates, deterministic eviction, frame mismatches, resets,
expiry, foreign snapshots, zero budget and unchanged old comparison payloads.

## Results

Runtime, runner and plan froze at `1c4a9acae2cd006ffff90ed3009cf1b3db868e15`.
The profiled binary SHA-256 is
`406f5f682e97f52dd2ba7b834e1812059107042dd4b560ac548c5415835dbfb0`.
All **26 runs** completed. Every active-only full trace, comparison report and
allocation ledger matches the previous raw archive, excluding wall-clock timing.
Every retained arm preserves its paired controls, observations, physics, sensor
counts, upstream work and old forecasts/costs. All **21 requests publish at the
same ticks** in both arms. Cancellation remains 18 source expirations, two ends
of unassisted flight and one material/ownership change in each arm.

Retention fills all **four previously missing numeric remote references**.
All 20 numeric remote references now have source geometry. Across the unchanged
62-candidate denominator, 20 candidates have two sites each: **40 projected sites
and 80 direction checks**. Of these, 74 directions pass the conditional solar
screen and the same six directions fail as in the active-only study. Every
sampled site has at least one clear direction. The other 42 candidates remain
unknown: 37 lack a retained group and five are outside the neutral-ground domain.
Twenty source snapshots contain one group each; one contains no group. The
two-group capacity and cross-generation comparison are exercised by unit tests.

The newly covered distant cases are four adjacent source ticks in the same quiet
world. They share generation 3469 and the **same two measurements, ticks 3769 and
3770**; they are correlated observations, not four independent worlds.

| Source tick | Conditional arrival tick | Transfer ticks | Projected sample ages |
| ---: | ---: | ---: | ---: |
| 3816 | 5534 | 1718 | 1765, 1764 |
| 3876 | 5492 | 1616 | 1723, 1722 |
| 3934 | 5522 | 1588 | 1753, 1752 |
| 3997 | 5547 | 1550 | 1778, 1777 |

These 25.8–28.6-second handoffs extend the earlier source/nearby screen. All
sixteen new directions pass, adding exactly **16 graph operations**:
56,149 active-only versus 56,165 retained. Both arms issue zero diagnostic physics
queries and keep the shared residual allowance of 64 after unchanged playing
work. Total projected ages range from 10 to 1778 ticks. The oldest is only 22
ticks inside the 1800-tick cutoff, so retention has not made evidence indefinitely
reusable. This corpus does not exercise a projected-age rejection or fresh
negative replacement; those guards have focused mutation/unit coverage.

The run reconciles 49,440 physical ticks and 98,880 controller/sensor rows apiece.
Independent solar reconstruction again has maximum error 0.000031 units, no
ambiguous clearance signs, and 38 equal/nearly equal departure-corridor orders
retained as unresolved. The [tracked projection](data/capture-remote-retention-v1.json)
contains all commands, snapshots, screens, work, publication ticks and 312 raw
file hashes. The complete raw summary is
`target/capture-flag-survey/remote-retention-v1/summary.json`; its digest is
`3c7c6c3af8e6e5f95ec91d3966a7016b5151eb4d772dd8717cf1cd20c8ed3eb2`,
also recorded in that projection as `raw_summary_sha256`.

Review added resumed-generation failure handling, cross-generation integration
and conflicting-world tests. It also found an audit gap that could accept an old
positive despite a newer failed dispatch. Before freezing, the audit was amended
to enforce latest eligible measurements and historical status/reason, with
negative/new-generation/status mutations. Checks pass: **238 AI library tests,
18 profiled harness tests, 455 Python tests**, formatting and strict Clippy.

This closes the raw-geometry gap in the fixed comparison corpus. It does not
show that a real ship will reach the forecast pose, that the future native
survey will choose this site, or how long fresh acquisition will take. The next
bounded investigation should replay these four distant nominations physically,
record the first actual handoff and subsequent native survey/site choice, and
compare those observations with the frozen screens. Keep failed/expired cases
and the shared-world correlation explicit. Acquisition time, later terrain
changes and combat remain separate unknowns before these records can affect
live destination selection. Deployment remains paused.

The [independent audit](data/capture-remote-retention-audit-v1.json) passes all
26 runs, 312 new raw files, 156 baseline files and 26 logs. It reconstructs all
21 retained snapshots chronologically from dispatches and actual world frames,
and verifies complete trace parity, projection/solar fields, queue lifetimes,
work totals and the exact same six rejected directions. No findings remain.
It imports no new retention-runner code; the prior independent helpers are
hash-pinned. The audit record SHA-256 is
`8a888e3bc952f108f4025d44a8b01554f5dbf9238cf2db5d3ff169498732f30d`.
Its notes digest precedes this audit paragraph. The script remains at
`target/capture-flag-survey/remote-retention-post-audit.py`, SHA-256
`eedf0a1aa5aaf24671f43f47ef6937b5210174ef99ba2ba589676666e5807dcb`.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/retain-remote-surveys.py --out target/capture-flag-survey/remote-retention-repeat
```

Use a clean checkout and a new output directory. The prior raw archive is required
for exact active-only allocation/report comparison; the runner checks its hashes.
