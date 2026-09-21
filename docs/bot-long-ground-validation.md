# Independent validation of ground-trip costs

The frozen timing model is **not yet validated for substantial walking trips**.
A fixed batch of 48 new generated matches produced only three long pure-walking
choices. All exceeded the calibration domain and all were abandoned. Two reached
the ground task's 90-second limit; the third never landed. Small errors on the
many short completed trips cannot establish accuracy on these longer routes.

This checkpoint adds `tools/validate-ground-costs.py`, a read-only postprocessor
of the [frozen trip estimator](bot-trip-estimate.md). It separates ground phases,
groups attempts using their original prelanding references, and attaches actual
movement diagnostics. It does not fit a model or change bot controls, physics,
budgets, policies or defaults. The [phase-aware landing model](bot-phase-landing-estimate.md)
and the ground calibration both remain frozen.

## Experiment and measurement contract

Before starting the runs, `design.json` declares twelve new world seeds, both
v11 seats against v10, and quiet/three-second mixed asteroid conditions: 48
normal matches, each capped at 600 simulated seconds. All runs are retained;
there is no success-dependent stopping or refitting. Dense traces cover every
tick. These twelve seeds are disjoint from all eleven previously used
calibration/diagnostic worlds checked by `analyze.py`.

The comparison executable is the unchanged runtime at
`43764580869c1ad56e6b7e9c4b7a21f485496220`, with live objective planning,
ground reuse, route dependencies and early candidates enabled. Its SHA-256 is
`21d634428b86d3a4e5702af779b7b21fe8c4c842c4fe13f0ef12ee5e3e23a9de`.
The ground profile SHA-256 remains
`23c9a12bec8eec532f750a1b9b67295cd4dd96380435155b4439503bbc881f06`.

A trip is classified as long if either its original outbound or return/boarding
reference is at least four seconds. For pure walking this means at least 20
route units under the existing length/5 reference. Walking, jumping and powered
crossing categories stay separate. A later shorter route cannot move an attempt
into the short cohort. Missing choices and missing route evidence remain visible.
These are reference times, not measured travel times or physical speed limits.

The postprocessor retains the original prediction and compares:

| Phase | Actual interval |
| --- | --- |
| Exit | Landed → exited |
| Outbound | Exited → first observed claim progress |
| Claim | First observed claim progress → ownership acquired |
| Return/board | Ownership acquired → boarded |
| Departure | Boarded → departed |
| Ground total | Exited → boarded |
| Landed-to-departed total | Landed → departed |

The totals sum the corresponding original phase predictions. An unknown leg
keeps its total unknown. Unfinished phases retain elapsed bounds as censored
observations; they never enter completed timing-error statistics. No-choice
attempts still retain their actual phases. Attempts without observations are
preserved separately rather than assigned invented choices; this batch has none.

Observed dependency changes are cut at each phase's endpoint. Results with no
recorded changes are useful to distinguish from changed plans, but they do not
prove physical permission or an unobstructed path. Global revision changes also
do not prove that the selected route was damaged.

Optional execution diagnostics read the original manifest and hash-verified
traces. They record support, balance, actual movement input, native goals and
counter ranges inside each phase's certain observed interval. A `jump` goal alone
does not imply a jump command. The first actual ground-task route is retained
as post-hoc evidence, never substituted for the original forecast. Missing ticks
stay missing; no interpolation is used.

## Results

The 48 matches finished after **16,574 simulated seconds** in total, about 4.6
hours. All physics/material audits are clean, and all **273,004 live-planning
rows** stay within their shared graph and query quotas. Those checks establish
simulation and budget integrity, not ground-trip success or improved strategy.

There are 212 v11 attempts and 146 observed prelanding choices:

| Original choice category | Attempts | Completed | Abandoned | Match ended |
| --- | ---: | ---: | ---: | ---: |
| No existing flag | 86 | 66 | 16 | 4 |
| Short walking | 54 | 29 | 24 | 1 |
| Long walking | 3 | 0 | 3 | 0 |
| Long powered crossing | 3 | 1 | 2 | 0 |
| No prelanding choice observed | 66 | 1 | 62 | 3 |
| **All** | **212** | **97** | **107** | **8** |

The long walking references are 40.01, 63.83 and 101.17 seconds outbound.
The existing calibration only covers outbound references through 10.60 seconds
(about 53 route units), and return references through 10.12 seconds. Most of
those calibration trips are near the hatch; only one supplies a substantial
successful walk. All three fresh long walks correctly remain unknown rather
than extrapolating that narrow evidence. There are **zero completed numeric
long-walk comparisons**.

For the short walking cohort, 29 completed ground totals have a median absolute
error of 0.067 seconds but a mean of 1.039 seconds. The largest underestimate is
12.90 seconds. The 15 completed comparisons with unchanged recorded evidence
have the same median, a mean of 0.218 seconds and a largest underestimate of
1.24 seconds. This checks the common short-trip accounting; it does not close
the longer-distance gap. The JSON retains all phase errors and unfinished trips,
including two short-walk ground attempts whose elapsed time already exceeds
their historical maximum. Historical envelopes are not confidence intervals.

## Retained long-walk failures

All three are quiet P1 runs. The identifiers below bind to exact commands,
traces and original choices in the archive. The surrounding match still uses
ordinary competition; quiet means no random asteroid arrivals.

| Run / selection tick | Original outbound / return reference | What actually happened |
| --- | ---: | --- |
| `long0-asteroids0-seat0` / 1991 | 63.83 / 63.47 s | Exited at 5957; blocked ground task at 11359; abandoned at 11360. |
| `long6-asteroids0-seat0` / 8019 | 101.17 / 0.00 s | Exited at 10192; blocked ground task at 15594; abandoned at 15595. |
| `long11-asteroids0-seat0` / 7457 | 40.01 / 39.30 s | Never landed; capture approach exhausted its time or retry budget at 10412. |

In `long0`, the first actual route is still a 319.17-unit continuous walk.
The pilot reaches waypoint index 66 on a 177-node path, then remains there. It
is knocked down at tick 8120, enters recovery at 8135 and returns to balanced
at 11217. The controller spends **3,097 ticks (51.62 seconds)** in `get_up`.
Every one reports a blocked get-up result and no measured standing clearance or
crawl direction. The posture observation reports stable support throughout;
3,082 of those ticks are in the recovering state. The counter reaches 58 get-up
attempts. There are no ground-route invalidations, crossings or terrain revision
changes on that leg. The native failure is `ground traversal exceeded ninety
seconds`, not a successful slow arrival.

This identifies a posture/clearance failure worth reducing. It does not yet
establish whether body placement, the physical clearance test or the available
crawl corridors cause the blockage. Increasing a travel-time residual would
hide that distinction. Start a reduction around ticks 7757–8135, retain the
moving ground and contact geometry, then inspect both the standing sweep and
crawl measurements. Relevant code is `ground_task/posture.rs` and the engine's
`spaceling.rs`. The archive includes raw observations, controls and posture
transitions, so the case need not be rediscovered.

In `long6`, the first actual route is **507.88 units**, with a nominal reference
of 101.58 seconds. It uses a 497-node path around almost the whole planet. The
pilot stays balanced, advances to waypoint index 386 and keeps making progress
until the unchanged 90-second task limit expires. There are no emitted primary
presses, recorded jumps or route invalidations. The frequent `jump` goal labels
occur without a jump command and must not be counted as commanded jumping.
Here the planner's own reference already exceeds the execution allowance;
the reference is not a proven lower bound on physical completion time.

In `long11`, eight cover replans contribute to nine capture replans before the
approach fails. There is no actual walking interval to evaluate. Keep it in the
original long-choice denominator without assigning it a walk-time error.

## Powered routes and changed plans

Three original long choices are powered crossings. Two never land. The third,
`long0-asteroids3-seat0` at selection tick 14786, completes: outbound 8.82 seconds,
claim 5.98, return/board 0.033 and departure 3.85. Its original outbound reference
was 12.48 seconds. The first actual ground-task route is 15.77 units with one
flight, and execution includes lift, crossing and landing phases. It is not a
walking calibration sample. The frozen profile has no crossing calibration, so
the forecast stays unknown despite successful execution. Any future use of this
case for fitting makes it training/diagnostic data, not a fresh validation case.

## Next bounded step

Use controlled physical trips to fill the missing distance range before these
costs influence mission selection. Another unconstrained random batch would
mostly reproduce hatch-adjacent claims and extreme failures.

1. Extend the existing `surface_flag_soak` fixture with dense trace output and
   an explicit controlled-ground evaluation scope. It already supports physical
   defender claiming, an attacker approach and landing-bearing restriction.
   Record the first prelanding choice, actual route, claim, boarding and departure;
   do not label this fixture as a normal mission or reconstruct the first choice
   from the exit-time route.
2. Predeclare moderate route-distance bands, for example 20–40 and 40–60 units,
   both walking directions and both seats on independently generated geometry.
   Use a separate setup pass to measure candidate distances before observing
   outcomes. Keep unreachable setups and achieved distances in the denominator;
   do not silently search until a trip succeeds. Verify the intended route is
   still present at exit, and separate later replans from timing errors.
3. Evaluate the frozen model where its domain allows it. References beyond the
   existing range remain unknown. Only after this evaluation should a separate
   training set expand the profile, followed by another independent validation.
   Preserve failed and interrupted legs alongside completed timing comparisons.
4. Treat the retained posture blockage and the route/deadline mismatch as
   distinct behavior investigations. Neither a larger timeout nor a fitted slow
   average is justified by this batch alone. Carry powered routes, remote flight,
   completion probability and pilot exposure forward as explicit unknowns.

## Tests, evidence and reproduction

**119 Python tests pass**, including 20 new contracts for distance cohorts,
phase origins, unknown totals, censored outcomes, dependency-change cutoffs,
immutable first choices, actual controls, trace identity, missing ticks and CLI
provenance. Existing tools-test CI discovers these tests. Runtime code is
unchanged; the physical experiment reuses the verified executable.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-trip-costs.py evaluate \
  --manifest target/bot-long-ground-validation/inputs.json \
  --profile target/bot-trip-estimate/calibration/profile.json \
  --out target/bot-long-ground-validation/reproduced-frozen
python3 tools/validate-ground-costs.py \
  --evaluation target/bot-long-ground-validation/reproduced-frozen/evaluation.json \
  --manifest target/bot-long-ground-validation/inputs.json \
  --out target/bot-long-ground-validation/reproduced-ground
```

Evidence lives under `target/bot-long-ground-validation/`, archived in
`/home/oldman/.codex/visualizations/2026/09/21/bot-long-ground-validation/`.
The archive contains commands, all 48 reports and dense traces, frozen-model
hashes, original predictions, diagnostics, tests and the source patch. Adjacent
metadata records the archive hash and binds the patch to its final commit.
Restore the `bot-trip-estimate` and `bot-outbound-walking` archives for the frozen
ground profile and executable. Restore `bot-phase-landing-estimate` for the
unchanged phase profile, and `bot-rolling-trip-estimate` for the previous-world
independence check. Dependency paths and hashes are in the archive manifest.

`run-matrix.py` declares the experiment and runs all configurations. It refuses
to overwrite an existing source freeze. Seeds are the first eight SHA-256 bytes,
big endian, of `long-ground-estimate-holdout-v1:0` through `:11`. `analyze.py`
checks independence, simulation audits, quotas and preservation of the frozen
forecasts; `checks.json` and `summary.json` retain its results. `long-cases.json`
contains all six long choices, and `walk-failures.json` provides fixed-time
checkpoints, native failure messages and posture observations for the three
walking cases. All twelve worlds are now known diagnostic evidence.
