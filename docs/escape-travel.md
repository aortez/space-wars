# Bounded travel commitment after capture escape

## Frozen question and policy

The [capture-escape experiment](capture-escape.md) prolonged initial ship
survival, but the next transfer was canceled by a new pursuit. Both affected
matches then lost with fewer completed objectives. Test one bounded transfer
commitment that keeps defensive weapons available without changing the existing
transfer flight motor or capture permissions.

`escape_travel_commitment_v1` is opt-in through
`--escape-travel-seats none|0|1|both`, default `none`, and requires capture escape.
Arm once on the same tick an actual-hatch escape finishes by separation or its
original deadline. Require an available full ship, aboard and flying without
supported feet, armed/ready controls, enabled flight, no capture/recovery/pursuit
or current destination, and the original vehicle. Other escape endings or a
delayed handoff cannot arm this policy. It can defer a new pursuit on this first
tick so ordinary destination selection has a chance to run.

Bind the commitment to that first ordinary selection and its original tick.
Reuse the existing **60-second transfer limit** and **20-second progress limit**.
The absolute deadline is fixed at handoff plus 3600 ticks; incoming hits, progress,
safety overrides and target sightings cannot renew it. The completed escape's
original receipt and 12-second clock remain historical and unchanged. This is
a separate mission commitment, not a refreshed escape or flight certificate.

Defer new pursuits while this one transfer remains active, including vulnerable
targets, incoming-fire responses and ownership-based opportunities. Record the
current classifier reason before the optional health gate, so deferral counts
describe eligible classifier decisions rather than proving that every one would
otherwise pass the health gate. Preserve ordinary solar/terrain flight safety,
recovery priority and destination evaluation. A destination change, invalid or
already secured target, stalled progress, deadline, recovery or capture-task
handoff ends the commitment without rearming from the same escape. A capture
handoff still needs ordinary native arrival and all later physical route gates.

During eligible transfer/launch ticks, use only the combat controller's current
weapon actions. Keep its visibility, ground occlusion, readiness, range, lead
alignment, ground clearance and combat-break checks. The transfer motor remains
unchanged; the experiment changes pursuit admission and eligible defensive fire.
It does not add a new transfer forecast or boundary controller. The prior
escape's boundary intervention is not extended by this option.

## Comparison plan, fixed before candidate outcomes

Baseline: `target/capture-escape/v1/summary.json`, SHA-256
`b9caf00030385f1cce0602bdaff4f7412d9699ada60f5dda7862d661398e700f`.
Retain the same **17 cases**, complete match lengths, seeds, opponents, budgets,
physics and all earlier options. Replay all 17 with the new option disabled,
requiring exact reports, seven streams, sensors and allocation ledgers. Run
17 complete comparisons with only the evaluated seat enabled in the 15 existing
enabled cases; retain the two cover-off controls. Keep the three health-enabled
regressions separate from the eight primary armed cases. Use at most two
simulations concurrently.

Freeze source, tests and this plan before candidate outcomes, and copy/hash the
profiled release binary. `tools/validate-escape-travel.py` keeps every previous
native route, publication, physical transfer, crossing, continuation, receipt,
abort and escape audit. Retain the exact consumed observation and actions for
every travel handoff, active tick, deferral and completion. Bind its clocks and
vehicle to the completed escape; audit immutable selection/deadline, current
pursuit classification, weapon eligibility, counter changes and capture handoff.
Any first changed action must follow or coincide with the recorded handoff.
Cases without a handoff must remain exact after removing only this new option's
telemetry, including all earlier escape and failure evidence.

Measure actual travel, destination changes, physical capture/departure, damage,
ship/pilot loss and final results. Report all eight primary armed cases and
the health regressions, including failures. Compare against the escape baseline
and retain the earlier no-escape regression context. Suppressed pursuit alone
does not establish useful travel or an objective completion. Do not tune this
policy after seeing the retained outcomes. These correlated development cases
do not establish independent playing strength or Raspberry Pi performance.
Defaults and remote deployment remain unchanged; work stays local.

## Results

The commitment prevents pursuit from canceling the selected trip, but produces
**no new capture handoff, claim or completed departure**. Only the primary and
health-enabled world 1 P1 powered cases change. Both still lose, with one claim
and one completed departure each. Keep this policy opt-in; these outcomes do
not support promotion or further tuning against these same cases.

In both cases, the completed escape hands off at **8376** and ordinary selection
chooses planet **2** on that tick. The commitment retains its original deadline
of **11976**. It runs for **898 travel ticks**, deferring **574** eligible pursuit
classifier decisions: 470 ownership-based opportunities and 104 incoming-fire
responses. These are classifier ticks before the optional health gate, not 574
distinct attacks or necessarily 574 otherwise admitted pursuits. The ship loses
its hull to a cannon hit at **9274**; recovery ends the commitment immediately.
Neither the deadline nor a capture handoff ends it, and it never rearms from
that escape.

| Affected case | First changed control | Ship loss before → after | Match end before → after |
| --- | --- | --- | --- |
| Primary world 1 P1 powered | 8671 | 9254 → 9274 | 10031 → 10181 |
| Health world 1 P1 powered | 9152 | 9270 → 9274 | 10349 → 10181 |

The first changed controls coincide with the earlier pursuit admissions. After
the shared handoff, the new runs contain no pursuit, landing, capture task,
hatch exit, claim, boarding or completed departure before ship loss. Both pilots
later die in a planet impact at 10181. The small shifts in ship survival do not
recover the earlier objective regression: before the escape experiment, these
same two cases each completed **three claims and three departures**, ending at
22016 and 21354 respectively.

### Arrival and weapon evidence

The ship approaches its destination, but never satisfies the complete native
arrival gate. The ordinary gate requires the destination to be the pilot's
current local planet, center distance below destination radius plus 105,
relative speed below 18, and ready queries. Across each 898-tick commitment:

| Arrival condition | Ticks |
| --- | --- |
| Within destination arrival distance | 233 |
| Within distance and speed limits, with ready queries | 87 |
| Within distance and using the destination's local frame | 0 |
| Complete native arrival gate | 0 |

All 87 otherwise eligible ticks use **planet 1's local frame** while traveling
to planet 2. They occur between 8918 and 9005. At the closest approach, tick
**8978**, distance to planet 2's center is **114.138**, its radius is **43.381**,
and radial clearance is **70.757**. At 8979 the relative speed falls to **0.152**,
but the local planet remains 1. The corresponding clearance above planet 1 is
about 44.075. These measurements use world positions and radii, not the capped
landing altitude sensor. The run switches from transfer to launch at 8841 and
back to transfer at 9100 without a capture task.

This identifies an approach/local-frame mismatch, not permission to relax the
arrival guard. The ship needs to approach the destination in a way that enters
its native local frame before handing control to capture. A substituted planet
observation or remote capture permission would bypass the physical conditions
that the existing landing and route checks rely on.

Defensive fire is available through the existing combat controller, but neither
run fires a laser or cannon during committed travel. Its combat goal is route
around planet for 332 ticks, climb clear of ground for 448, and engage ship for
118. In 100 engagement ticks the opponent is visible, unoccluded and in range,
with weapons ready. None passes the native **0.08-radian** lead-alignment gate;
the smallest error among those eligible ticks is **2.665 radians** at 9255.
The fixed forward weapons cannot fire at that target while the unchanged travel
motor holds its current heading. Suppressing pursuit does not itself establish
effective defense.

### Complete retained outcomes

The eight primary armed cases retain **2 wins, 26 claims and 25 completed
departures**, exactly the escape baseline totals. The earlier no-escape totals
were 2 wins, 28 claims and 27 departures. In the tables below, “before” is the
escape baseline, not the earlier no-escape reference.

| Primary case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 4 → 4 | 4 → 4 | 21907 → 21907 |
| World 0 P1 powered | win → win | 5 → 5 | 5 → 5 | 36000 → 36000 |
| World 0 P2 walking | loss → loss | 4 → 4 | 4 → 4 | 26591 → 26591 |
| World 0 P2 powered | loss → loss | 4 → 4 | 3 → 3 | 30536 → 30536 |
| World 1 P1 walking | loss → loss | 1 → 1 | 1 → 1 | 36000 → 36000 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 10031 → 10181 |
| World 1 P2 walking | win → win | 4 → 4 | 4 → 4 | 36000 → 36000 |
| World 1 P2 powered | loss → loss | 3 → 3 | 3 → 3 | 36000 → 36000 |

The three health regressions remain separate diagnostics:

| Health case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 3 → 3 | 3 → 3 | 16190 → 16190 |
| World 0 P1 powered | loss → loss | 3 → 3 | 3 → 3 | 15014 → 15014 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 10349 → 10181 |

### Verification and evidence

Implementation, tests and this policy plan were frozen in **`48cc417`** before
candidate outcomes. All **1,061 Rust tests** and **717 Python tests** pass,
including fresh handoff binding, immutable clocks and selection, deferral,
capture handoff, recovery/safety priority, reset and native weapon eligibility.
Formatting, strict AI Clippy with `--no-deps`, and profiled/ordinary release
builds pass. Scenario Clippy retains its same seven pre-existing findings.

All **34 saved-run audits** pass. The 17 disabled replays retain exact reports,
seven streams, sensors and allocation ledgers. Among the 17 candidate runs,
15 preserve the prior state after removing only the new travel telemetry; two
change controls. Native dispatch remains capped at **4 graph operations / 384
queries**, with maximum publication age **120 ticks**. The earlier route,
physical crossing, continuation, receipt, abort and escape audits remain in
force. No checker correction or simulation rerun was needed for this evidence.

The frozen profiled binary is
`target/escape-travel/surface_mission_soak-48cc417`, SHA-256
`b78c35e2c3ed10cc82a8ce468db5816276716e046487572a618bae449bdcdc2d`.
The complete summary at `target/escape-travel/v1/summary.json` has SHA-256
`b0dfa9ca2351728d916c6a3877bcbfb894ff437cbe12c08396a1170009022873`.
[The manifest](data/escape-travel-v1.json) records all outcomes and verification
checks. [The archive](data/escape-travel-v1.json.gz)
retains the frozen plan, summaries, every consumed travel observation, prior
native receipt/abort and route/physical witnesses, first changed controls,
mission/vehicle/damage history, arrival and weapon geometry, reproduction
scripts, validation logs and raw-file hashes. Full streams remain at the hashed
local paths. All **166 embedded documents**, **653 raw files**, retained source
inputs and both frozen binaries pass hash verification. Work remains local,
with defaults unchanged.

### Next investigation

Investigate transfer approach geometry between neighboring planets and the
handoff into the destination's native local frame. Preserve the actual landing,
capture-route and arrival guards. Use the retained mismatch as a diagnostic,
then freeze any proposed approach change before measuring complete outcomes
and regressions. The current commitment addresses pursuit cancellation, but
does not establish a successful physical trip or stronger play.
