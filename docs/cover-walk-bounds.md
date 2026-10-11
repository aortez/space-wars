# Unsupported walking corridors during cover search

## Implementation and frozen plan

Add the opt-in `--cover-walk-bounds true` mode on top of completed-walk feedback.
Profiles are `live_joint_objective_v11` and `live_jetpack_objective_v11`; all bot
defaults remain unchanged. The disabled mode retains the v10 behavior and format.

The corridor constructor now returns its required angular steps and configured
limit when a requested corridor is too long. This uses the same rounded node
separation and four endpoint margins as admission. It performs no physical
measurement and increments no attempt/completion counter. Report it separately
as `unsupported_walk`, never as `exhausted_walk`, a negative route certificate,
or a landing permission. Actual hatches and absent selected sites get no notice.

Both notices use the original actor, site, request generation, measurement tick,
and objective. Cover search accepts only a current matching scan, objective,
revision and source clock, with no invalidation or deferred submission. It moves
the site to the existing `walk_deferred` list, leaving it eligible for any later
positive route. This list contains unknown hypotheses; the notice type records
whether a walk was attempted. The next scanned candidate may replace the old
full fallback, with all old charges counted and a fresh snapshot for the new
request. Without a changed request, the original fallback continues.

Keep the eight-probe cap, original 600-tick deadline, 120-tick source lifetime,
shared 4 graph / 384 query allowance, and all route, equipment, cover and boarding
checks. Once all candidates have been deferred, keep the final site requested
for its full fallback until the original deadline or a normal search completion.
Snapshot construction and immediate/publication validation remain outside the
operation quota; this change makes no frame-time or Raspberry Pi speedup claim.

Freeze implementation, tests, runner and this plan before viewing outcomes.
Compare against `target/walk-feedback/v1/summary.json`:

1. Six directed disabled replays must retain all seven evidence/controller
   streams, ordinary sensor rows, allocation ledgers, physical mission fields
   and non-timing planner telemetry exactly.
2. Run all six enabled directed 180-second missions: both route models, blocked
   bearing -0.8 with cover on/off, +0.8 control with cover on, seed 42 and seat 0.
3. Run all eight enabled armed 600-second matches: both generated worlds, both
   v13 seats against v10, both route models, weapons, two planners and no asteroids.

Run at most two games concurrently. Change only the new flag, binary and output
directory. Keep every loss and unfinished visit. Reuse the existing physical
and publication auditor unchanged; separately audit every consumed method
notice against its source/search context and preserve its type. Report attempted
walks, unsupported hypotheses, new requests, route publications and physical
captures separately. Check prior raw hashes and binary hashes before/after.
Preserve full raw streams, first action differences, and original-ship departure
witnesses. Investigate the remaining powered path from code and measured work;
do not infer powered infeasibility from a missing walking route.

```sh
python3 tools/validate-walk-bounds.py \
  --prior target/walk-feedback/v1/summary.json \
  --binary target/walk-bounds/surface_mission_soak-COMMIT \
  --out target/walk-bounds/v1
```

## Results

The frozen `4ff3abe` implementation completes all 20 runs. All six disabled
replays retain 42 exact streams, 64,800 ordinary sensor rows, physical mission
fields, allocation ledgers and non-timing planner telemetry. Prior hashes remain
unchanged.

Across the 14 enabled games, 44 requests report an unsupported corridor, always
first observed at source age one. Cover search consumes 28 of these notices.
The remaining notices repeat a last requested site without spending more probes.
The existing completed-walk feedback retains 40 receipts and 36 consumed
advances at source ages 4–38. The planner records 24 replacements of unsupported
requests and 34 replacements of completed unsuccessful walking probes. Request
replacements can also follow a changed selected site after search completion.

Completed captures and departures remain unchanged in every game. Both directed
controls complete the enemy and neutral captures; the four blocked games still
complete only the neutral capture. All eight armed games remain losses with
identical controls, physical visits, round records and combat metrics. Twelve
of the 14 enabled games retain their complete previous action sequence.

In each blocked cover-on game, the first search still starts at 3,911 and has a
4,511 deadline. Its first five completed walking attempts hand off exactly as
before. The previously stalled candidates now advance:

| Candidate | Required steps | Source tick | Notice consumed |
| --- | ---: | ---: | ---: |
| Site 5 | 226 | 4,171 | 4,172 |
| Site 6 | 234 | 4,173 | 4,174 |
| Site 7 | 242 | 4,175 | 4,176 |

Each exceeds the unchanged 224-step bound. At 4,176, all eight hypotheses are in
`walk_deferred`, the pending list is empty, and site 7 remains requested for the
full fallback. The search does not spend a ninth probe or infer that the flag is
unreachable. It reaches its original deadline and abandons the visit at 4,512.

These two games first change controls at tick 8,859, during a later enemy
approach. That later search starts at 9,112 and retains its 600-tick deadline of
9,712. Its eight candidates finish deferral by 9,363; the visit ends at 9,713.
Previously that search started at 9,512 and the visit ended at 10,113. Neither
version lands or captures during this visit. The archive retains both versions
of the first changed observation and the deadline evidence.

In both world-0 P1 armed games, search starts at 20,619 with deadline 21,219.
Sites 11–18 all exceed the walker limit, requiring 236, 244, 252, 260, 252, 244,
236 and 228 steps. Their notices arrive at ticks 20,676 through 20,690, two ticks
apart. All eight remain unknown and the same search deadline ends the visit.
The world-1 P1 searches retain their existing eight completed unsuccessful walks
and finish normally when exposure clears; the new unsupported mode adds no
notice there.

## Two positive routes arrive after search has ended

The world-0 P1 games each start a fresh site-0 request at tick 21,220, after their
cover-search deadline. Each publishes a positive 143-sample walking corridor at
21,258, source age 38, with both outbound and returning paths. No capture is
active in that observation. These are valid positive route publications, but
they do not change controls or rescue the finished search.

There are 42 extended starts and completions: these two successes and 40
unsuccessful attempts. Shorter corridors retain 39 starts, 33 completions and 33
successes, with six unfinished. The full fallback still completes no candidates
and starts no powered forecast in this live corpus. A scheduling notice, a
positive publication, and an additional physical capture remain separate events.

## Powered work diagnosis

A temporary test-only patch at `4ff3abe` completes the existing incremental jobs
in the existing parked and moving crossing fixtures, for both seats. It counts
every `next_work()` operation without changing the algorithms and verifies that
world snapshot bytes stay unchanged. All four forecasts succeed when allowed to
finish outside the live deadline. The patch, exact outputs and build/test log
are archived; the temporary worktree was removed.

| Fixture | Seat | Base ground graph steps | Proposal graph steps | Forecast graph steps | Forecast queries |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parked | 0 | 3,082 | 407 | 986 | 1,964 |
| Parked | 1 | 3,082 | 407 | 982 | 1,956 |
| Moving | 0 | 1,016 | 494 | 3,307 | 5,870 |
| Moving | 1 | 1,018 | 494 | 3,289 | 5,834 |

Ground cost here is the prospective base survey before candidate hull overlay.
Proposal and forecast costs use the native parked-hull map used by the existing
forecast tests; these are stage measurements, not a claimed total live route
cost. The parked forecast measures both directions at one launch epoch. The
moving forecast measures both directions at all three launch epochs.

At four graph operations per dispatch, the inclusive 120-tick source lifetime
allows at most 484 operations across both actors. The forecast alone exceeds
that allowance in every measured case. Moving proposals also exceed it before
forecasting starts. Skipping the full ground survey or merely finding endpoints
sooner is therefore insufficient for this implementation.

The next experiment needs a local survey for one selected powered route and
bounded forecast scheduling that can deliver within the same lifetime. A
candidate optimization is to perform one integration alongside its charged
clearance query and reuse measured launch-epoch environments across directions.
That remains unimplemented and must preserve every physical query, both
directions, all launch epochs, fuel reserve, landing margins, equipment checks
and source clocks. The present measurements demonstrate excessive work, not
powered infeasibility or permission to raise the allowance.

## Evidence and validation

The [result manifest](data/cover-walk-bounds-v1.json) records every case and
comparison. Its [compressed evidence](data/cover-walk-bounds-v1.json.gz) contains
47 exact documents: frozen summaries, hashes for 261 raw files, physical and
scheduling witnesses, powered stage measurements and patch, and validation logs.

All 448,870 pilot observations and 256,835 dispatch ticks pass the unchanged
physical/publication auditor and added notice checks. Charges never exceed the
shared 4 graph / 384 query limit. None of the 37,792 publications is older than
120 ticks. Relative to the prior corpus, dispatch spends 2,884 fewer graph
operations and 9,248 fewer queries. Synchronous snapshot and validation work
remain outside those counts; no Raspberry Pi performance result is inferred.

Validation passes 1,005 Rust tests in 16 suites, 653 Python tests, one diagnostic
stage-cost test, formatting, strict AI library/harness Clippy with `--no-deps`,
and profiled/ordinary release builds. Scenario Clippy retains the same seven
findings in unchanged files. The archive also preserves the interrupted initial
test run and dependency-Clippy finding; the complete rerun passes. Unit tests
cover both directions and wraparound at the exact walker boundaries, zero-query
constructor feedback, actual-hatch exclusion, fallback parity, current-scan
replacement, source expiry, malformed notices, mixed eight-probe searches, and
later positive eligibility. The two generated worlds and seat swaps remain a
small correlated sample.

Preserved binary: `target/walk-bounds/surface_mission_soak-4ff3abe`, SHA-256
`7349860605a673526877c6c2fcdc2f7608ff95931c4da8611f4e02c519fb61c2`.
Summary: `target/walk-bounds/v1/summary.json`, SHA-256
`78191327d5a0a02e7895b6890e5e289816987c5ef452d3ad6b7ee698ff14f9b1`.
