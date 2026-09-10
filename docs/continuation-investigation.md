# Continuing toward a complete material Spacewars match

Historical investigation on 2026-09-10 against implementation `5a090b1`,
documented at `25fbe2b`. The findings below describe that checkpoint. The first
implementation chunk is now covered by [usable landings](usable-landings.md);
[objective landing selection](objective-landings.md) now has a bounded first implementation.
Prospective jetpack access, damaged approaches and the match lifecycle remain work.
The source review and archived desktop/Pi traces add detail to the
[mission reliability results](mission-reliability.md).

## What the current results establish

Both platforms complete capture and departure in 12/13 capture-then-hunt cases.
Every prepared isolated pursuit reaches weapon contact, although four cases on
each platform miss contact within the original three-minute whole-mission
window. The additional quiet routes complete 4/4 on desktop and 2/4 on Pi.
The final 76 trials pass physical/material audits. Existing test results and
their limits are recorded in the reliability report; no new simulations or
test suite were run for this investigation.

These results support continuing to match integration. The remaining delays
have different causes and should be measured separately.

## Findings from the slow traces

| Recorded case | Evidence | Implication |
| --- | --- | --- |
| Seed 0, P2, reflected quiet route, Pi, planet 2 | Arrival at 59.70s; two attempts stop with one accepted rear foot. Retries begin at 84.43s and 110.27s. Actual landing at 126.82s, claim at 129.92s, boarding at 129.95s. | Most of this visit's delay is touchdown and repeated approaches. Claiming works promptly after a usable landing. |
| Same route, planet 1 | Both feet settle at 166.35s, but transfer reports `exit_blocked`. The ship is 3.12 units sideways from the surveyed vehicle position. At 168.35s the bot lifts off to retry. | A valid planned hatch does not guarantee a usable hatch at the actual touchdown pose. The landing milestone was real; the subsequent retry was caused by blocked access. |
| Seed 2, P2, desktop hunt, planet 0 | At 145s the ship is nearly stationary, only 0.64 units sideways from its site, with zero accepted feet. It retries at 151.73s and lands at 170.83s. | Position error alone cannot diagnose all stalled touchdowns. Actual hull and foot contact details are needed. |
| Seed 7, P2, unreflected hunt, both platforms | First two visits depart at about 102s and 134s. The last trip remains unfinished at 180s. Desktop distance to its last destination falls from about 1,090 units at 134s to 264 at 179s, with intervening clearance climbs and detours. | This case combines an expensive first landing with a long final transfer. It is not evidence of a permanently motionless bot or a failed weapon controller. |
| Seed 42, P2, reflected Pi asteroid duel | At 110s the on-foot bot is about 111 degrees around the planet from the enemy flag. At 179s it is about 50 degrees away, after walking, jumping and a jetpack crossing. | This is continuing ground traversal rather than a stationary claim timer. The chosen landing leaves a long journey to the objective. |

The last row's surface-distance estimates are about 283 and 127 units, computed
as angular separation times the planet's conservative radius. They are an
explanation of scale, not measured walkable path lengths. The bot does not
reach or claim that flag before the trial ends. At the late samples its parked
ship also reports only one accepted foot, so return eligibility remains a
separate concern.

At the two one-foot stops in the first row, lateral errors are approximately
2.17 and 2.12 units. Traces report accepted foot counts but not the complete
contact manifolds. They cannot distinguish a missing physical contact, a hull
resting on a step, or a contact rejected by the support-normal filter. That
distinction must precede any change to landing eligibility.

## Relevant implementation boundaries

- [Landing surveys](../scenarios/spacewars/src/surface_sortie/pilot.rs)
  sample 64 bearings, foot rays, a belly ray and hatch capsule clearance.
  Full-ship surveys do not test the entire hull over a range of touchdown
  offsets. The pod survey already has additional corner checks.
- [Final approach](../crates/spacewars-ai/src/pilot.rs) commits to settling,
  then commands heading while the shared assist descends. It no longer
  corrects lateral site error in that phase. A two-second blocked-hatch wait
  or a touchdown progress timeout causes a fresh approach.
- [Physical landing](../scenarios/spacewars/src/surface_sortie/landing.rs)
  requires two supported feet, open wings, low relative motion and 0.25s of
  settling. [Contact queries](../scenarios/spacewars/src/physics.rs) require
  retained planet material and suitable contact normals; earned landings
  have a bounded corner-contact continuation rule.
- [Transfer readiness](../scenarios/spacewars/src/surface_sortie.rs) uses the
  actual hatch and capsule clearance. `ExitBlocked` combines several causes,
  including missing access, another actor, or obstructed capsule space.
- [Tactical landing selection](../crates/spacewars-ai/src/tactical_sortie.rs)
  scores approach arc and cover after solar checks. It does not score distance
  or traversability from the hatch to an enemy flag. The
  [capture wrapper](../crates/spacewars-ai/src/tactical_capture.rs) starts its
  [ground task](../crates/spacewars-ai/src/ground_task.rs) after exiting; that
  task has a ninety-second traversal budget.

## Recommended first implementation chunk: usable landings

Make the selected site reliably lead to two-foot support and a usable exit.
Keep the existing physical permissions and ordinary control boundary.

1. Add bounded diagnostic evidence for the failing touchdown windows: each
   foot's contact identity, normal, separation and relative motion; hull
   contact; actual versus planned pose; and the specific hatch obstruction.
   Collect this outside policy timing and avoid making production sensors
   unbounded.
2. Use those reproductions to choose the smallest correction. Likely candidates
   are stronger full-ship site clearance checks and a bounded lateral
   correction before committing to the final sink. Whether a nearby lift and
   adjustment can safely resolve a blocked exit needs a physical trial.
3. Preserve a bounded fresh-site fallback when terrain changes or another
   actor occupies the exit. A ship resting on its hull must not be declared
   landed merely because it has stopped moving.

Acceptance should report arrival-to-landing, landing-to-ready-exit, retries and
the actual support/clearance evidence. Reproduce the reflected seed 0 cases,
desktop seed 2/P2 and seed 7/P2 on both platforms. Retain the existing solar,
removed-ground, foreign-fragment, blocked-exit and parked-return regressions.
Then rerun the paired mission cases and a controller playtest. Capture and
pursuit keep their separate clocks; the existing 180-second results retain
their original meaning.

## Following chunk: land for the capture objective

When an enemy flag exists, rank otherwise safe landing sites by the combined
flight approach and bounded surface access to that flag. Prefer a feasible
short walk while retaining solar, material support, hatch and cover checks.
A distance estimate can shortlist sites; it must not be treated as proof that
craters, ships or gaps are traversable. Neutral planets still allow a flag at
the spaceling's standing point, so their landing policy need not pay for an
imaginary distant ground objective.

Use the reflected seed 42 duel and mirrored contested-flag fixtures to measure
landing-to-flag time, walking distance, crossings, return and departure. Keep
flag-footing destruction and ownership changes in the trials. Random asteroid
duels in the latest matrix did not destroy a ship, so deliberately exercised
ship-loss and rebuilding cases remain necessary evidence for the combined loop.

## Confirmed survival rule and remaining match integration

The arena currently preserves pods and spacelings and has no match outcome.
[The shared world step](../scenarios/spacewars/src/lib.rs) skips legacy game-over
evaluation when surface pilots are installed. The
[material client registrations](../crates/engine-client/src/client_scenarios/surface_sortie/mission.rs)
inherit disabled game-over support. The existing
[Spacewars client](../crates/engine-client/src/client_scenarios/spacewars.rs)
already has winner state, game-over UI integration and controller settings to
build upon.

The old rule eliminates a player who has no full ship and owns no planet.
Applying it directly would end an arena player's game even though their living
spaceling or pod could still reach a neutral planet, claim it and rebuild.
The user confirmed on 2026-09-10 that this recovery opportunity remains part
of a finished match: a surviving spaceling or escape pod keeps the player in
play even after losing their full ship and every owned planet. They can claim
or reclaim ground and rebuild through the ordinary mechanics. Asset loss
alone must not declare defeat.

The next design must specify how a surviving pilot can finally be defeated:
damage/death rules, any respawn entitlement, and simultaneous
elimination. The current combat sensor targets only occupied full ships;
mission observation can follow pods and spacelings, but the bot waits for an
aerial target. Survivor combat therefore needs explicit target eligibility,
damage, AI behavior and recovery transitions, as well as a winner screen.

Implement a material match lifecycle that preserves the confirmed survival
rule and uses actual pilot, vehicle and flag state; connect winner/draw,
restart and ordinary human/bot settings; then exercise full bot-versus-bot and human-versus-bot
rounds. Keep one shared physics step and gravity solve. A destroyed or detached
flag footing still neutralizes the planet under the agreed ownership rule.

Acceptance must include occupied ship loss with no owned planets, empty ship
loss with a living external spaceling, and destruction of the last owned flag
during recovery. Each survivor remains active and can claim/reclaim, rebuild,
board and rejoin combat using ordinary actions. Terminal-defeat tests depend
on the remaining pilot damage/death design.

The final promotion makes this material lifecycle the ordinary playable
Spacewars path, retaining the focused labs and historical evaluation baselines.
Completed rounds, recovery/defeat transitions and Pi controller playtesting
are the acceptance evidence for that milestone.

## Source evidence

Reports and traces reviewed live under:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/mission-reliability-20260910/
```

Desktop: `final-desktop-matrix/<case>/report.json` and `trace.jsonl`.
Pi: `pi-archived-missions/final-matrix/<case>/report.json` and `trace.jsonl`.
Case directories encode zero-based seats: `p0` is P1 and `p1` is P2.
In duels both pilots' traces are in the same case directory; the contested
finding above comes from seat 1 in `generated-s42-p0-mTrue-duel`.
Traces contain periodic samples and goal transitions, not every physics tick.
