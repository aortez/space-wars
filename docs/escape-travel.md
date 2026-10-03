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
