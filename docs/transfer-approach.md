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
