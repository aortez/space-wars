# Experimental capture destination policy v12

`material_mission_v12` adds the first behavior driven by the
[capture evaluator](capture-mission-evaluation.md): replacing the current
destination with a sufficiently shorter supported capture trip. It reuses
v10's local flight, landing, joint walking/return, combat and recovery tasks.
It does not include the v11 jetpack-planning experiment or promote a new default.

Both player selectors offer **mission v12** (the destination planner), saved as
`destination-bot`.
Automatic matches retain it, and the HUD names each seat's actual policy.
Legacy v9 and Planner v10 remain selectable and do not consume proposals.

## Decision boundary

The existing host still emits controls, attaches remote observations, then
spends the shared planning allowance. On a later control tick v12 may consume
a completed comparison, after checking its original dependencies against the
new observation. It cannot construct a proposal from a partial search.

A switch requires all of the following:

- The entire shortlist has supported costs and is not truncated. Unknown,
  expired, changed or unsupported options retain the v10 destination.
- The suggested destination beats the current remaining reference by at least
  **five seconds and twenty percent**, and fits the current match clock.
- The report matches the pilot, policy, selected visit/site, ship form/location,
  material revisions, ownership, flag position and claim duration. Report source
  age is at most two seconds; stored samples keep their original age.
- Recovery, solar avoidance and opportunistic combat keep their existing
  priority. Destruction, a dead pilot or a finished match invalidates a proposal.
- The ship has no supported feet and is still flying. Survey, circling and a
  high approach may switch; Surface, departure and return commitments cannot.
  Radial clearance must exceed `35 + falling_speed² / 50` world units.
- There has been no earlier destination switch in this trip, and the proposed
  planet is not deferred after a failed attempt. A natural new selection starts
  a new trip; reset clears the experiment's state.

The switch records the abandoned attempt and a new selection. The ordinary
transfer controller must fly there and obtain current landing/exit/boarding
evidence. Remote climb samples and predicted durations never authorize a
physical action. `destination_planning` telemetry records the switch count,
last source/decision tick, destination pair and both estimates.

These are conditional time estimates, not utility or survival estimates.
Opposing fire, detours, likely stalls, the strategic value of removing an enemy
flag, and the cost of the following mission are not scored. Consequently an
earlier foothold can be a worse longer-term itinerary.

## Making native evidence usable

Two issues appeared when a physical flagged trip finally offered an attractive
alternative:

1. Native objective routes arrive every thirty ticks. Between those surveys,
   absence was overwriting numeric costs with “unmeasured” before the bounded
   evaluation could be used. A valid sample now survives an ordinary cadence
   gap for less than thirty ticks, without renewing its source time. Explicit
   stale live work, a new negative route, edits or the next missing survey revoke
   it. Current landing/hatch/cover observations remain consumption gates.
2. Ground-cost validity used acceleration at the airborne ship, so descending
   invalidated an unchanged ground route. Tactical observations now publish the
   same acceleration at the flag that the existing ground-route planner uses.
   Dependency checks use that value with the existing 0.01 tolerance. Ship
   altitude cannot renew/invalidate a flag's route; a changed ground field can.

Published reports retain their own evidence and dependencies, separate from a
pending refresh. A newer request cannot refresh an older result's lifetime.
Timing medians and their supported walking domain are unchanged.

## Comparison and reproduction

The native client retains synchronous local sensors and the shared allowance of
**four evaluator units and 384 remote physical queries per tick** for both seats.
Each evaluator actor remains capped at two units. This is not a budget for all
bot sensors or wall time. Proposal validation is a bounded read of at most eight
planet/evidence records and performs no physics queries.

The runner can now use `--live-objective-seats none` to match native local sensing
while retaining the remote dispatcher. The new `--world destination` fixture
uses two rounded physical planets and an initial opposing flag on measured
retained ground. Only initialization installs that flag; the subsequent flights,
claims, losses and transfers all use ordinary actions and one shared step.

```sh
cargo build --release -p spacewars-ai --example surface_mission_soak
python3 tools/compare-capture-destinations.py
cargo test -p spacewars-ai --test capture_destination
```

The comparison includes four controlled v10/v12 pairs, both seats and mirrored
travel, plus eight generated-match pairs with fresh SHA-256-derived seeds,
v12 in each seat, and asteroids off/every three seconds. Each run lasts three
simulated minutes; none is discarded for a stall or lack of switching. These
are short physical comparisons, not finished-match win-rate evidence.

The controlled cases exercise both a supported change of destination and
unsupported mirrored approaches that retain v10. CI also checks exact control
fallback with zero evaluator fuel through a full flagged capture/return, unsafe
descent and touchdown gates, expiry and dependency changes, reset/replay,
controller persistence and the native host's unchanged v9/v10 round.

## Results at `5cbd780`

The [compact comparison record](data/capture-destination-planner-v1.json) retains
all 24 runs, commands, binary/report hashes, visits, failed attempts and work
counts. They cover 72 simulated minutes; all physics audits passed. Measured
remote work peaked at 126 queries in a tick, below the shared 384 allowance.

| Supported physical case | First claim, v10 → v12 | First departure after boarding, v10 → v12 | All planets owned, v10 → v12 |
| --- | ---: | ---: | ---: |
| P1, ordinary layout | 52.55 → 38.98 s | 72.50 → 42.75 s | 96.33 → 101.85 s |
| P2, mirrored layout | 55.77 → 47.07 s | 76.05 → 50.78 s | 115.77 → 112.55 s |

Each changes destination once, while the old target remains nearer. The new
neutral trip physically lands, captures, boards and departs. In the first case
the source comparison predicts 46.90 seconds for continuing the flagged trip
versus 28.65 for the neutral alternative. Actual remaining time until departure
is 59.97 seconds for v10 and 30.22 for v12. These outcomes do not calibrate the
guard or timing constants; no values were fitted to these trials.

The first foothold/return improves, but the itinerary tradeoff is visible: the
first case secures all planets 5.52 seconds later. Finishing both full round
trips is also later in both supported cases (121.78 versus 100.12 seconds, and
132.83 versus 119.70). Choosing an easier first mission is not equivalent to
optimizing the full match.

The other two controlled mirror cases retain unsupported/stalled flagged
approaches and have no switches. All eight generated-match pairs also have no
switches. Their recorded physical milestones, final pilots/planets, combat,
asteroid events and audit outcomes match v10. Across the eight experimental
generated runs, 158 completed preferences favor the current target and 2,004
reports have no complete preference. These repeated reports are correlated;
they establish fallback coverage, not a strength improvement.

Artifacts are in `target/capture-destination-planner/matrix`; the earlier
bearing exploration is under `explore` and is excluded from the comparison
matrix. In particular, unsupported powered/jumping routes stay unknown.

## Finished-match comparison protocol

Before examining new outcomes, the next comparison declares four worlds from
the first eight SHA-256 bytes, little-endian, of
`native-destination-finished-v1:0` through `:3`:
`1342523865096599469`, `3849100356505807845`, `186767996776005237`, and
`4311410596101621856`. Each world runs quiet and with mixed asteroids every
three seconds. Each condition gets one v10/v10 control and two experimental
matches with v12 in opposite seats: **24 runs and 16 matched comparisons**.
The same control is reused for its two seat comparisons, not counted as two
independent games. Every run must reach pilot death or the existing ten-minute
match timer; unfinished runs cannot count as draws. The Pi's saved fifteen-minute
autoplay timer remains separate from this headless comparison.

```sh
cargo build --release -p spacewars-ai --example surface_mission_soak
python3 tools/compare-capture-destinations.py --finished-matches
```

Native synchronous local sensing and the four-unit/384-query shared remote
allowance stay fixed. The runner records all planned conditions before execution,
rotates execution order, retains both pilots' deaths/health, ownership, complete
and abandoned visits, recovery, long phases, every recorded destination switch,
and incomplete evaluation counts. It compares the initial world and recorded
physical outcomes against the same-seat control; a no-switch run must match.
Parameters are not fitted to these worlds. Wins, survival and the later itinerary
will be read together with how often v12 actually changes a decision. Four worlds
are diagnostic coverage, not a precise general win-rate estimate.

The [completed results](capture-destination-finished-matches.md) retain fifteen
unchanged comparisons and one slower trip that changes a win into a loss. They
identify transfer timing and ownership value as the next planning work; v12
remains experimental.

## Validation and Picade deployment

The final runtime at `3a49860` differs from the comparison build only in the
shortened picker label and its UI expectations/documentation. Validation covers:

- All 90 workspace test targets: **1,927 passed, 47 existing ignored, no
  failures**. The initial workspace command was terminated during the older
  mission endurance tests; those three remaining physical targets, the AI
  examples, and the CLI/control targets were completed in separate commands.
- The new physical destination tests, the native v9/v10 control parity test,
  and controller persistence checks passed. The ignored display-dependent
  functional tests were compiled, but were not run locally without a display.
- Formatting and strict AI Clippy passed with the CI Rust 1.89.0 toolchain;
  the Python suite passed all 329 tests.
- All 24 three-minute comparison runs passed their physics audits and query
  limits, with exact recorded physical outcome comparisons for no-switch pairs.

`./update.sh --fast --target sw-picade.local` installed the final runtime without
an OS reboot. The installed client/CLI hashes matched the deployment manifest;
the kiosk remained active with no service restarts after installation. A real
1024×768 screenshot confirmed that **mission v12** fits the selector and that
the match HUD reads **Planner bot v10** for P1 and **Destination bot v12** for P2.
After returning to the launcher, its saved thirty-second idle countdown
automatically launched that pairing. Autoplay remains enabled on the device.

Local logs and screenshots are under `target/capture-destination-planner`,
including `workspace-tests.log`, `remaining-physical-tests.log`,
`examples-tests.log`, `control-cli-tests.log`, `deploy-final.log`,
`pi-settings-final.png`, `pi-live-final.png` and `pi-autoplay-final.txt`.

## Follow-up

Keep the experiment selectable until broader evidence warrants promotion.
The finished-match comparison makes transfer cost and the value of first
foothold versus enemy-flag removal the next useful planning slice. Missing
destination costs remain a coverage limitation. Preserve the slower natural
match and unsupported mirrored cases as regressions;
do not assign invented costs just to produce a choice. The frozen v10 baseline,
query caps and retained failed attempts make that comparison repeatable.

Independent review succeeded on retry after the earlier authentication failures.
It found that the headless successor-continuation option bypassed v12's
evaluator consumption while still reporting v12. The runner now explicitly
rejects that unsupported combination in either seat. Direct consumption tests
also cover changed local routes, ground gravity, hatches, cover and cadence
expiry, beyond the physical successful-path tests.

Review of the finished-match tool caught policy labels embedded inside damage
events. Physical comparison now excludes only those events' diagnostic mission
telemetry, retaining their tick, seat and complete vitals. A regression verifies
that different labels compare equal while different physical damage does not.
