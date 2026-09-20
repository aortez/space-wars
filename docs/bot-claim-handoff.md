# Finish a live flag raise before resuming ground travel

This fixes the v11 execution bug identified by the
[bounded capture-trip experiment](bot-successor-sortie.md), using `dd53831` as
the baseline. Once the physical world reports that the bot is raising its own
flag, the powered ground planner holds its current footing instead of selecting
another route endpoint around that flag. The flag finishes in the recorded case.
The return leg still exceeds the existing task clock, so the complete nominated
trip remains unsuccessful.

The change applies to the experimental `material_mission_v11` powered flag
planner. v9/v10 retain their controls; the launcher's Planner selection remains
v10. The prior v11 is constructible at the baseline commit and its exact runner
is retained for the comparisons below.

## The execution change

`LandingObjective::read` deliberately describes a flag on a planet not yet owned
by the actor. That includes the actor's partially raised flag. Previously,
`planned_flag_target` treated the transition from the old enemy flag to the new
own flag as another trip to plan. Moving toward that new endpoint repeatedly
cancelled the live raise.

For the powered planner only, the ground task now yields to ordinary claim
handling when the current observation reports all of these facts: the planet
is neutral; this actor is the claimant; its own flag is in the raising phase
with raising status; and the actor is balanced and supported by that planet.
It clears the obsolete flag approach and walk path. This is recomputed from
each observation, not a latched permission or a new simulated claim.

The existing observation/map validation, posture and active-crossing handling
run first. A missing grounded hatch still fails the joint trip. Losing support,
destroying the flag, changing ownership, or losing active raising status returns
control to the existing ground logic. The original ground, capture and overall
trial deadlines remain. Actual ownership, rather than partial progress, triggers
the normal new hatch task; that task checks current return routes and transfer
eligibility. No world rule, sensor model or work allowance changes.

## Exact failure replay

The reproduction is still P2 v11 versus P1 v10 on seed
`7725194555774358125`, without random asteroids, explicitly trying site `1:45`
after escape handoff tick 11153. The original boundary/cover/successor probes and
live shared objective planner remain enabled. Both runtimes record a dense
ground window from tick 17700 through 18739.

The source observation, committed controls and all first-six-second continuation
samples match the parent. The dense observations and controls also match through
neutralization at 18048. The first changed command is at 18076: the old bot starts
moving toward another endpoint, while the fixed bot remains still.

| Milestone | Parent | Fixed v11 |
| --- | ---: | ---: |
| Land / exit | 13318 / 13319 | 13318 / 13319 |
| Enemy flag neutralized | 18048 | 18048 |
| Own raising resets in the dense window | 19 needing settlement, 1 losing support | 0 |
| Planet secured | Never during trial | 18228 |
| Boarding / departure during trial | Neither | Neither |
| Trial stops | Ground deadline, 18721 | Capture-task deadline, 21168 |
| Eventual whole-match result | P2 loses at 579.30s | P2 wins at 558.77s |

The fixed raise takes exactly 180 physical ticks (three seconds). Each tick
retains balanced support, raising status and neutral movement controls. Ownership
then changes through the ordinary world update. A fresh return task starts at
18228. The eventual win includes later ordinary recovery and combat; it does
not count as completing the nominated trip.

### What still prevents the return

The capture task starts at 12167 and keeps its original 150-second limit. After
claiming, it has 49 seconds left before that limit fires. The return task's own
90-second clock has not expired; the host capture task ends first.

The measured return route is 115.3 units long, with zero planned jumps and no
reported route failure. It is planned once. In 48.7 seconds of sampled return
control, the nearer hatch's straight-line distance falls from 111.7 to 65.6 units.
There are no recorded displacements, but the controller issues 19 jumps. These
distances are not remaining surface-path lengths.

The first three jumps are directly inspectable in the dense window. At ticks
18357, 18589 and 18689, the currently retained edges are walking edges, about
1.295 units long. Each jump follows exactly 46 ticks without the controller's
required distance improvement. Thus those jumps come from the existing stuck
fallback, not a planned obstacle jump.

The [return-walking investigation](bot-return-walking.md) follows this checkpoint.
Stronger steering was rejected after it exposed a deeper moving-terrain contact
failure. The subsequent [CCD velocity correction](moving-ground-ccd.md) removes
those reproduced stalls with the original controller and deadlines in place.
The controlled return now reaches its final waypoint without emergency jumps,
but still expires before boarding.

The initial investigation plan was waypoint following on this return: measure
steering, speed, progress thresholds and waypoint transitions around those three
events. Compare an execution change against the same physical route, including
real obstacles and disrupted footing. The landing scorer currently converts
ground distance using a nominal five-unit walking speed; that estimate also
needs comparison with actual completion time. Do not increase clocks before
understanding the slow traversal, and do not assume a route's existence proves
that the whole opportunity fits its time budget.

## Fresh comparisons

Two seeds, `13723705828516009897` and `15270103591317955068`, were chosen by SHA-256
of `claim-handoff-validation-v1:0` and `:1`, taking the first eight bytes as an
unsigned big-endian integer. Each runs quiet and with three-second mixed asteroid
pressure, with v11 in both seats against unchanged v10. Both old and new runtimes
run every configuration: eight pairs, sixteen complete matches. These use the
shared live planner and route-reuse/early-candidate settings, with the escape
experiments disabled. Each match can run to its existing 600-second timer.

Both runtimes produce three v11 wins and five losses, with 19 completed v11
sorties. Seven pairs have identical complete round reports and identical physical
states/actions at shared recorded trace samples. Their trace files differ in
ground telemetry and event-driven sampling. Those traces are sparse; equality
at shared samples is not a claim about every unrecorded tick.

The changed pair is the first seed under asteroid pressure with v11 in P1.
From the same landing at tick 13513, ownership completes at 14905 instead of
15333. Boarding and departure likewise move 428 ticks (7.13 seconds) earlier.
It still wins, and its completed-sortie count is unchanged. This is a second
physical capture-and-return improvement, but the small matrix does not establish
a general increase in competitive strength.

Two additional old-policy configurations, with v9/v10 swapped, run on both
binaries. Complete recorded traces, mission telemetry, metrics and round results
are identical. All twenty comparison matches plus the targeted replay finish
with clean material/physics audits; all recorded shared planning quotas hold.

## Verification and evidence

186 tests pass: 116 AI unit tests, 35 ground-navigation tests, 9 objective-landing
tests, 3 live-landing handoff tests, 2 ground/jetpack physical tests, 20 physical
mission tests and one physical combat test. Three new ground tests cover:

- Sustained own raising without walking to another endpoint, repeated calls,
  and the retained historical planner behavior on the same graph.
- Support/flag loss, enemy ownership, missing hatch and deadline interruption.
- Rejection of a newly obstructed return after actual ownership.

The positive regression fails on the parent. These focused tests use synthetic
observations; the dense failure replay and fresh paired runs supply the actual
claim, boarding and departure evidence. Formatting, client compilation and
AI/core all-target clippy with `--no-deps` and denied warnings pass.

Use Rust 1.89.0 and `--locked`. Tests use `RUST_MIN_STACK=16777216`.
Build the same `surface_mission_soak` example with `--release --features
sensor-profile`, then use the [previous reproduction command](bot-successor-sortie.md#verification-and-reproduction),
adding `--trace-start-tick 17700 --trace-end-tick 18740` for the dense ground window.

Evidence is in `target/bot-claim-handoff/`: exact commands, baseline/candidate
runners, test logs, `analyze_replay.py`, `analyze_matrix.py`, reports, traces and
planning allocations. The durable verified archive is
`/home/oldman/.codex/visualizations/2026/09/19/bot-claim-handoff/`.
Its source patch applies to `dd53831`; the final runtime is byte-identical to
the one used in the eleven candidate runs. Ten further runs use the retained
parent binary. The final return-test fixture
also explicitly records secured ownership/phase; that test-only refinement
does not alter the measured runtime. Large sensor timing logs remain in `target`.
This checkpoint does not deploy or promote v11 to a launcher default.
