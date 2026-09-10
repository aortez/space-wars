# Usable landings on moving material planets

This implements the first chunk from the
[continuation investigation](continuation-investigation.md): make touchdown
lead to an actual exit, claim, boarding and departure. Flag-aware landing
selection and a finished ordinary Spacewars match remain subsequent work.
The confirmed rule that a surviving pod or spaceling may reclaim and rebuild
still applies; this pass adds no elimination rule.

## Evidence and changes

The soak trace now includes bounded read-only landing diagnostics: actual hull
and foot contacts, retained-material identity, contact normal/separation/speed,
individual foot clearances, and actual hatch capsule clearance. Contact output
is capped at sixteen records per hull/foot part. These queries run outside
policy timing and never advance physics. Eight focused desktop/Pi replays
exactly matched the previous audits and mission metrics before behavior changed.

The reflected seed 0/P2 delay was a genuine one-foot stop: the other foot was
about 0.8–1.0 units above the ground, with no supporting hull contact. Holding
the surveyed heading indefinitely could not finish that landing.
`material_landing_v3` waits for half a second of slow one-foot contact, then
uses ordinary rotation to close half the measured height difference across
the six-unit foot span. Each correction is capped at 0.1 radians, remains
within eighteen degrees of outward orientation, and is stored in the moving
planet's frame. At most three corrections are allowed in that controller
attempt. Existing progress deadlines and fresh-site retries remain active.
Ray clearance guides rotation; only solver contacts can grant landing.

Desktop seed 2/P2 exposed a different failure: the hull rested on a step while
both feet were about 0.13 units above the ground. Landing surveys now query the
whole hull with 0.75 units of lateral tolerance and 0.2 units of settling depth.
Foot rays and an empty central belly ray alone could admit this obstruction.

The hatch survey now checks both the actual normal-oriented exit capsule and
room to stand radially upright, including small lateral offsets. It also
reports whether three lateral offsets combined with three headings, including
±0.1 radians of tilt, preserve clearance. The bot only rotates a one-foot stop
when this additional margin exists; otherwise it retries at a fresh site.
Checking sideways drift and tilt independently missed a Pi seed 1/P2 case:
after landing, the ship slid and tilted together, lost a usable hatch during
the claim, and required recovery. Requiring the combined tilt margin at every
site also excluded otherwise usable two-foot landings and delayed another
route. Applying it to the proposed correction keeps that distinction explicit.

The shared hatch search still looks at its central ray and one cell on either
side. It now prefers a nearby floor whose actual exit capsule is clear, while
excluding the pilot's own body from that clearance query. If none is clear,
the first nearby floor remains observable and the transfer gate rejects the
exit. Other actors, missing material and occupied exit reservations retain
their existing authority. Humans and bots use this same hatch selection.

Changed landing positions also exposed a departure regression between nearby
planets. `material_mission_v6` keeps world guidance after the nearest approach
frame switches back to the departure planet. While between two nearby bodies,
it commands velocity outward from both; for nearly opposing outward directions
it preserves tangential travel. Completion still requires the real claim and
boarding followed by physical clearance of the original planet, within the
existing thirty-second departure budget. Ship loss still enters recovery.
The composed capture identity is `tactical_sortie_v9`.

Initial two-foot settling, parked corner-contact recognition, claim rules,
boarding requirements and recovery permissions are unchanged. Experiments
with a larger single rotation and different parked-contact aggregation were
rejected after paired regressions. There are no pose snaps, automatic ownership
or extra physics/gravity steps.

## Validation

Focused tests cover bounded one-foot rotation, repeated observations, clone
and reset, unsuitable contact/motion/query states, and an obstructed central
hatch with a usable neighboring exit. Observation tests also check diagnostic
queries do not change world observations or terrain audits. The generated
mission regression now requires all three departures for seed 0/P1 and also
exercises seed 1/P2. Existing removed-ground, fragment, blocked-exit, solar,
parked-return and physical recovery cases remain required.

The final validation uses the existing twenty-five three-minute cases per
platform: thirteen capture-then-hunt routes, eight two-bot asteroid duels and
four quiet routes. Thirteen additional pursuit cases per platform separate
physical capture preparation from the chase, each with its own three-minute
cap. Outcomes come from real claim, board, departure and hit ticks; timing out
is not counted as success. Desktop and Pi results are evaluated separately.

Final source identities, paired results, workspace checks and deployment
verification will be recorded here after validation completes.

Artifacts, including diagnostic baseline replays and rejected candidates:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/usable-landings-20260910/
```
