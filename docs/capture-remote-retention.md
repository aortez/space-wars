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

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/retain-remote-surveys.py --out target/capture-flag-survey/remote-retention-repeat
```

Use a clean checkout and a new output directory. The prior raw archive is required
for exact active-only allocation/report comparison; the runner checks its hashes.
