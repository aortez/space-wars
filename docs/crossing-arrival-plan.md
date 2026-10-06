# Terrain-gap destination-footing candidate

The [sequence diagnosis](live-claim-sequences-results.md) found a gap flight
reported complete at its takeoff ledge. Test one change: when a terrain-gap
crossing is descending, retain all existing balance, support, speed and
sideways-arrival checks, and additionally require the pilot's feet to be
within the existing 1-unit arrival range of the destination in planet-local
coordinates. Vehicle crossings, steering, phase transitions before arrival,
deadlines, route selection, physics and host planning remain unchanged.

This is a local candidate on `bot-crossing-arrival`; it does not select a new
default bot or authorize promotion/deployment. Do not change the candidate
after observing its replay outcome in this experiment.

## Regression and qualification checks

Preserve the four native observations at completion ticks 18,562, 19,180,
19,762 and 19,963 from World 1/P2 no-stop, bound to the preceding review
bundle's hash. A focused controller test must accept the first three and
reject the last, including rigid world transforms and common velocity shifts.
The rejected landing must not extend the existing ninety-second deadline.
Run the bot library/integration suite, profiled harness tests, formatting,
bot-scoped Clippy and Python auditors. Keep original logs and build receipts.

Build the candidate with Rust 1.89.0 and the same locked release/profiled
harness configuration as the baseline. Commit the candidate, regression
fixture/tests, plan and runner before freezing commands and all input hashes.
Exactly one existing runtime source may change:
`crates/spacewars-ai/src/jetpack_crossing.rs`. New tests/fixtures and this
step's runner/test/plan are additional inputs. Previous evidence stays intact.

## Four complete selected games

Use the previous four observational replays as baselines. Reuse their full
commands, including both actors, seeds, selected policies, shared planning,
asteroids, observer settings and complete 600-second round limit. Only the
executable and output paths change. Execute once each, in this order:

1. World 1/P2 integrated: require exact retention.
2. World 3/P1 integrated: require exact retention.
3. World 3/P1 no-stop: require exact retention.
4. World 1/P2 no-stop: the affected recovery.

There are zero fresh games. Run sequentially, losslessly archive each before
the next, and preserve all raw streams and original archives. The three
controls must match all deterministic streams and non-timing report/sensor/
planning data. For the affected case require identical dense trace rows
until P2's false completion at tick 19,963, then identical consumed
observations at that first decision change. Preserve both actors and the
earliest changed action even if it differs from the first telemetry change.

Continue all existing configuration, physical visit, stopping, capture-route,
fuel, planning, native recovery and observer audits. Additionally inspect
crossings in both `mission.capture.ground` and `mission.recovery.ground`.
Bind every reported terrain-gap completion to native feet within the
destination radius. Preserve attempts, phase transitions, charge, completed
crossings, claims, rebuilds, boarding, ship losses, pilot survival and full
round outcomes. Include final-step lifecycle records.

An invalid audit stops the sequence and retains its raw recording. Explicit
`--reaudit` may repair this step's Python runner/tests and reuse the same
recordings. It may not change the candidate, fixture, commands, plan or other
frozen inputs, retry games, or replace unfavorable outcomes.

## Decision

Correcting the false completion is necessary but not a recovery improvement
by itself. Advance only to broader validation if the affected pilot completes
more recoveries, keeps its winning outcome and survival, and incurs no extra
ship loss, with all audits and controls passing. Otherwise record the result
as correctness-only, with recovery improvement unproven. A native rebuild or
claim short of boarding is reported as partial progress, not completion.
Do not promote based on these selected games. Preserve the complete evidence,
failure/resume history if any, and one supported next hypothesis.
