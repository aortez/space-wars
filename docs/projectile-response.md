# Brief physical responses to a projectile warning

## Frozen laboratory comparison

The [projectile diagnostic](projectile-diagnostics.md) flags the old missile
55 ticks before impact in the two speed-limited transfers. Test one fixed
half-second response, then let the original policies finish the match. This is
a controlled laboratory intervention, not a promoted mission policy.

`--probe-projectile-response none|observe|brake|left|right` defaults to `none`.
The probe runs only for the explicitly evaluated seat in a duel with capture
and projectile tracing enabled. It cannot be combined with the impact or
successor-control probes. It changes no physical state directly, and performs
no future simulation to choose a response.

Use the existing 600-unit / 64-projectile diagnostic and unchanged two-second
circle-entry screen. At the first warning during an eligible committed escape
transfer, choose the earliest projected entry, breaking exact ties by stable
projectile ID. Include all owners. No tick, missile identity, launch time or
outcome is hard-coded into the trigger. The older baseline has a *different*
missile warning later in the transfer, so preserve and test those cases too.

Eligibility requires the current native `Transfer` goal, active original
transfer identity/deadline, an aboard full ship, ready controls/queries/flight,
flying with no supported feet, and no capture or recovery task. Match completion,
any native priority change or transfer identity change permanently ends the
attempt. These checks run every tick, including during the pulse. The original
transfer deadline and progress controller remain authoritative.

Freeze **30 wall ticks**, with at most one attempt per match:

| Mode | Physical input during the pulse |
| --- | --- |
| Observe | Keep all original actions; record the warning and window |
| Brake | Retain native turn; release thrust, request braking and open wings |
| Left | Request turn −1 and open wings; retain native braking and thrust only when not braking |
| Right | Request turn +1 and open wings; retain native braking and thrust only when not braking |

Weapons and interaction remain unchanged. Lateral steering uses the real ship's
turn/thrust dynamics, with no instantaneous sideways force. The warning need not
remain present throughout the pulse; the fixed deadline cannot renew or restart.
Normal controls resume at the first tick outside the window, or earlier when a
native priority/identity check fails. The probe does not reset bot memory.

## Retained trials and checks

Use the four enabled cases from
`target/projectile-diagnostics/v1/summary.json`, SHA-256
`70ba8a21f883aaeedb935c9e0e06b18a7e46227ca92175f237a2dd2fd76a50dc`:
clear-entry and speed-limited world 1 P1 powered, each primary and health-enabled.
Run all four modes above on every case: **16 full-match trials**. Add **one
disabled control** for the primary speed-limited case. Preserve seeds, opponents,
physics, options, full match lengths and planner budgets. At most two concurrent
simulations. Freeze implementation, tests and this plan before outcomes; retain
the frozen binary and its hash. Do not select a duration or retune from results.

Require exact original observations/actions for both seats through the trigger
(substituting the recorded native action on the trigger tick). Observe and
disabled cases must retain full-match parity, including projectile traces.
Independently reconstruct every trigger, source identity, deadline, priority
stop and action byte. After the pulse, require the applied action to equal the
recorded native action. Save dense damage/contact receipts, physical losses,
source observations, all pulse actions and checks. Existing native physical
capture and planner-budget audits remain active.

Report the initial missile's eventual contacts, damage from other sources,
ship/pilot survival, native arrival/capture handoff, physical claims/departures
and complete match outcomes. Compare all three interventions to their immediate
observe control and keep the health cases separate. Preserving the stronger
earlier no-escape objective reference remains necessary. Avoiding one contact
alone does not establish a useful or generally safe policy.

Defaults, deployment and ordinary bot behavior remain unchanged. All work and
commits remain local.
