# Clear entry geometry for committed post-escape travel

## Frozen question and policy

The [travel commitment](escape-travel.md) prevents pursuit cancellation but
does not reach capture. The retained ship comes within arrival distance and
speed while a neighboring planet remains its native local frame. The ordinary
entry point is 85 units above the destination along the ship's current radial
direction; that point can lie inside another body's approach area. Local climb
then takes precedence. Test an endpoint choice that keeps the ordinary entry
height and requires clearance from neighboring bodies.

`clear_transfer_approach_v1` is opt-in through
`--transfer-approach-seats none|0|1|both`, default `none`, and requires escape
travel. It runs only during the existing committed transfer, aboard the same
full ship with armed controls, ready queries, flight enabled and no supported
feet. Destination/selection identity, the original 60-second transfer deadline
and 20-second progress limit remain unchanged. It creates no capture task and
does not supply synthetic local observations.

Screen the ordinary entry against every other planet's radius plus **105**, the
sun's radius plus **110**, and the arena radius minus **20**. These are geometric
endpoint margins, not predictions of physical flight or native frame ownership.
If the ordinary entry fails, sample **32 equally spaced bearings**, including
the original radial direction, on the same radius-plus-85 circle. Choose the
clear candidate nearest the current ship. Retain its world bearing relative to
the destination center while it remains clear; recheck moving obstacles every
eligible tick. A previous bearing is usable only for the same commitment,
selection, vehicle and destination. At most **33 candidate screens** occur per
tick, including revalidation. If no entry passes, retain ordinary guidance.

Preserve local climb priority, obstacle routing, flight controls, defensive
weapon gates, solar/recovery priority, all native arrival and landing checks,
and physical capture-route validation. A selected entry is used only when the
ordinary coordinator chooses transfer rather than launch. This does not extend
the escape's boundary controller, add a forecast or consume planner graph or
physics-query credits. Reaching a screened point does not authorize landing.

## Comparison plan fixed before candidate outcomes

Baseline: `target/escape-travel/v1/summary.json`, SHA-256
`b0dfa9ca2351728d916c6a3877bcbfb894ff437cbe12c08396a1170009022873`.
Retain all **17 cases**, original seeds, complete lengths, opponents, budgets,
physics and prior options. Replay all 17 with this option disabled and require
exact reports, seven streams, sensors and allocation ledgers. Then run all 17
comparisons, enabling only the evaluated seat in the same 15 enabled cases and
keeping the two cover-off controls disabled. Keep the three health regressions
separate from the eight primary armed cases. Limit concurrent simulations to
two. Freeze implementation, tests and this plan before outcomes; copy and hash
the profiled release binary.

`tools/validate-transfer-approach.py` retains all prior native route, physical
transfer, continuation, cover, receipt, abort, escape and travel audits. Save
the exact consumed observation and actions for every eligible geometry decision.
Independently reconstruct candidate screens, selection and retained bearing,
travel identity/deadlines, counters and actual use by transfer. Any first changed
control must follow an alternate entry actually used for transfer. Runs with no
such use must remain exact after removing only this option's telemetry.

Measure native local-frame changes, complete arrival-gate satisfaction, actual
capture task/landing/exit/claim/boarding/departure, damage, vehicle and pilot loss,
and complete outcomes. Retain the earlier no-escape reference alongside the
immediate baseline. Report every primary and health case, including failures.
Do not retune the frozen policy after observing these cases. These correlated
development runs do not establish independent playing strength or device
performance. Defaults, remote deployment and PR publication remain unchanged;
work stays local.

## Results

The geometric entry screen works as specified, but **does not resolve the
physical approach failure**. Both affected world 1 P1 powered cases still fail
to reach a capture task and lose their ships earlier. Keep the option disabled
by default. No policy constants or controls were retuned after these outcomes.

Both primary and health-enabled runs retain the escape/travel handoff at
**8376**, destination **2**, and absolute deadline **11976**. The ordinary entry
first violates the neighboring planet's 105-unit clearance margin at **8801**,
which is also the first changed control. Each candidate records **809 geometry
decisions**: 425 ordinary entries and 384 alternate entries. Of those alternates,
**182** supply transfer guidance; the other **202** occur while the existing
local climb controller has priority. At most **33 endpoint screens** occur on
one tick. The selected bearing is revalidated against each current observation.

| Affected case | Ship loss before → after | Match end before → after | Claims / departures before → after |
| --- | --- | --- | --- |
| Primary world 1 P1 powered | 9274 → 9185 | 10181 → 9407 | 1 / 1 → 1 / 1 |
| Health world 1 P1 powered | 9274 → 9185 | 10181 → 9407 | 1 / 1 → 1 / 1 |

Each ship starts the trip with **31.174 hull**, then loses its remaining hull to
a laser hit at **9185**, ending the commitment with `recovery required`. Each
pilot subsequently dies in a planet impact at **9407**. No new pursuit, landing,
capture task, hatch exit, claim, boarding or completed departure occurs on this
trip. Neither defensive weapon fires during the commitment. There are **505**
deferred pursuit classifier ticks: 493 ownership-based opportunities and 12
incoming-fire responses. As in the prior experiment, these are classifier
decisions before the optional health gate, not distinct attacks.

### Why the clear endpoint is insufficient

The unchanged local climb condition is:

```text
local clearance < 70 + falling_speed² / 50
```

The relevant speed is relative to the current planet. At **8838**, the ship's
native local frame changes from destination planet 2 to planet 1. Its clearance
above planet 1 is **104.393**, but its inward speed relative to that planet is
**45.436**, requiring **111.288** clearance under the existing climb rule. The
coordinator therefore correctly switches to launch/climb on that same tick.
The selected endpoint's geometric clearance does not establish sufficient
clearance along the actual moving approach with the ship's current velocity.

Climb continues from **8838 through 9039**, after which transfer resumes at
9040. The ship remains in planet 1's frame until loss. Its closest approach to
planet 2 occurs at **8935**, while climbing: center distance **122.539**, radial
clearance **79.158**, and destination-relative speed **1.178**. Planet 1's local
clearance is only **55.063** at that point. These values come from world motion
and radii, rather than the capped landing altitude sensor.

| Arrival condition during the trip, per affected case | Travel baseline | Clear-entry candidate |
| --- | --- | --- |
| Travel ticks | 898 | 809 |
| Within destination arrival distance | 233 | 169 |
| Within distance and speed limits, with ready queries | 87 | 52 |
| Within distance and using the destination's local frame | 0 | 0 |
| Complete native arrival gate / capture handoff | 0 | 0 |

All 52 otherwise eligible candidate ticks still use the neighboring planet's
frame. The native arrival guard correctly rejects capture. The test supports
investigating the moving approach leg and relative closing speed, rather than
relaxing frame ownership or the climb/landing checks. It does not establish that
all routes to this destination are impossible.

### Complete retained outcomes

The eight primary armed cases retain **2 wins, 26 claims and 25 completed
departures**, the same totals as the travel baseline. The earlier no-escape
reference had **2 wins, 28 claims and 27 departures**; each affected case then
completed three claims and three departures. This candidate does not recover
that objective regression. “Before” below means the immediate travel baseline.

| Primary case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 4 → 4 | 4 → 4 | 21907 → 21907 |
| World 0 P1 powered | win → win | 5 → 5 | 5 → 5 | 36000 → 36000 |
| World 0 P2 walking | loss → loss | 4 → 4 | 4 → 4 | 26591 → 26591 |
| World 0 P2 powered | loss → loss | 4 → 4 | 3 → 3 | 30536 → 30536 |
| World 1 P1 walking | loss → loss | 1 → 1 | 1 → 1 | 36000 → 36000 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 10181 → 9407 |
| World 1 P2 walking | win → win | 4 → 4 | 4 → 4 | 36000 → 36000 |
| World 1 P2 powered | loss → loss | 3 → 3 | 3 → 3 | 36000 → 36000 |

The health regressions remain separate:

| Health case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 3 → 3 | 3 → 3 | 16190 → 16190 |
| World 0 P1 powered | loss → loss | 3 → 3 | 3 → 3 | 15014 → 15014 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 10181 → 9407 |

### Verification and evidence

Implementation, tests and the comparison plan were frozen in **`7e22dc6`**.
All **1,070 Rust tests** and **724 Python tests** pass, including moving-obstacle
revalidation, infeasible geometry, fixed travel identity/deadlines, native climb
priority, unchanged clear-entry controls and neutral native capture handoff.
Formatting, strict AI Clippy with `--no-deps`, and profiled/ordinary release
builds pass. Scenario Clippy retains its same seven pre-existing findings.

All **34 saved-run audits** pass: 17 disabled replays retain exact prior state,
and 15 of the 17 candidate runs remain exact after removing only the new
telemetry. Two candidate control sequences change, both first at 8801. Native
dispatch remains capped at **4 graph operations / 384 queries**, with maximum
publication age **120 ticks**. All earlier route, physical crossing,
continuation, cover, receipt, abort, escape and travel audits remain enabled.
No checker correction or simulation rerun was needed for this comparison.

The frozen profiled binary is
`target/transfer-approach/surface_mission_soak-7e22dc6`, SHA-256
`f4b173e5ed918f2a0724eba22eec81fbf32597270ddaeb0c5a341a340f287bed`.
The complete summary at `target/transfer-approach/v1/summary.json` has SHA-256
`074b66abbaa7e280cdcd9e7438cd5a5106d565f3ea6bf0bba762a0f4a114d9b3`.
[The manifest](data/transfer-approach-v1.json) records summary/archive hashes,
complete outcomes and checks. [The archive](data/transfer-approach-v1.json.gz)
preserves the frozen plan, exact consumed observations and entry decisions,
prior native witnesses, first changed controls, complete mission/vehicle/damage
history, arrival geometry, validation logs, analysis scripts and raw-file hashes.
All **184 embedded documents**, **687 raw files**, retained source inputs and
both frozen binaries pass hash verification. Full streams remain at the hashed
local paths. Work remains local and defaults are unchanged.

### Next investigation

Investigate approach velocity and the actual moving route leg before the ship
enters a neighboring planet's speed-dependent climb region. Preserve the local
climb, native frame selection and physical landing/capture permissions. A
future bounded approach policy needs to account for relative closing speed and
room to turn or slow down; a clear endpoint alone is insufficient. Freeze any
such hypothesis before new complete-match comparisons and retain this failure
as a regression case.

The [relative-speed follow-up](transfer-speed.md) now tests that intervention.
The observed trip avoids the forced-climb detour, but cannon damage destroys
both affected ships earlier, before either reaches arrival distance. No new
capture or departure results. The next investigation is the threat and remaining
hull along the post-escape trip; the measured clearance improvement alone does
not support changing defaults.
