# Polishing the learned transfer leg against its measured deficiencies (LF-090)

**Status: delivered, 2026-10-29.** The three slow legs that the
[mission binding](flight-mission-binding-v1.md) measured were traced to
their causes in the match, two of them fixed in the navigation law, the
network refitted and the refit selected in matches, and a fourth cause
found in the abandoned legs closed in the pilot. The retained stack, in
24 seeded matches in both seat orders on two seed ranges, flies shorter
legs than the native transfer law overall (median 793 against 833 ticks on
seeds 1 to 12 and 779 against 911 on seeds 13 to 24), makes more handoffs
(55 against 53 and 61 against 55), and on identical first legs is within
five percent of the native law on 22 of 47, faster on 10 and slower on 15.
The arena wall no longer appears, no refusal occurred on the first range
and four on the second, legs resumed after a withdrawal 11 times in all
against 200 before, and two legs in 47 seats were abandoned by the
mission's progress budget against 18 before. What remains is the motor's
steering, which holds the nose at its turn-rate limit for almost half of a
leg's ticks. The learned seat's record in the matches that ended by a
pilot's death, 2 won and 7 lost, traces to combat under the pilot's own
controls long after the legs ended and to the seat, not the controller.

## Reproducing the slow legs

Each slow leg was traced per tick in the match with the policy observation
the navigation network saw, the law's decision on that same observation,
the network's command, the motor's action and the ship's physical state
(`trace_learned_transfer_seat` with `SPACEWARS_TRACE_DUMP`). Running the
Python law on the dumped observations separates the law's own behavior from
the network's imitation of it without a headless reproduction of the match
geometry, which the planets' orbital phase would have made approximate.

- **The flip near the sun was the law's.** With the goal behind the sun,
  the law picks the detour side by the sign of the goal's angle around the
  body, unless the ship's tangential speed around the body exceeds the
  commitment threshold of 5 units per second, when it keeps the side it is
  moving toward. A ship that is nearly still drifts at 4 to 6 units per
  second under gravity and its own reversals, so the drift crossed the
  threshold every few seconds and the motion rule handed the route to the
  other side each time. Over the 1,700-tick hover the law's world heading
  alternated between two directions 170 degrees apart; the network's
  commands were a blurred copy of that.
- **The standstill was the arena wall.** The object record places the ship
  at 99.8 percent of the arena radius with the nearest surface 2.6 units
  away and motionless in the world: the ship was pinned against the
  boundary with the motor at full thrust into it. The law's routing ignores
  the enclosing edge, so a detour on the far side of the sun could run
  outside the arena, and the network had never seen a wall. The law itself,
  on those observations, commands a heading away from the wall; the
  network's command pointed into it, an imitation failure in a state the
  training families never contained.
- **The long way round was not the geometry.** The law rounds a planet the
  way the native pilot does, by a waypoint half a radian ahead on a circle
  105 units above the planet's bound, and on that leg it chose the shorter
  arc. The law's world heading was steady for the whole 900 ticks while the
  network's differed from it by 50 to 150 degrees on most decisions,
  flipping between two modes as the ship spun. The network imitates the
  law poorly in the state just after launch with the goal behind the
  departure planet, and the motor keeps the nose at its turn-rate limit of
  1.8 radians per second for 46 percent of the leg's ticks, so the thrust
  direction wobbles and the ship skims the planet at altitudes of 13 to 25.

## The law changes

Two changes to `tools/flight_navigation/reference_v2.py`, both in the
routing on the object record:

- `detour_commit_speed` rises from 5 to 12 units per second, so only a ship
  that is rounding a body keeps its side by motion; a drifting ship chooses
  by the goal's angle, which changes once as the goal passes directly
  behind the body and otherwise holds. On the dumped hover the routed side
  now flips exactly once in 800 ticks instead of every 90.
- `enclosing_edge` reads the arena's edge from the object record, and a
  detour waypoint that would lie outside the edge by the body margin rounds
  the other way, as a waypoint inside another body's disk already did.

The modified law keeps every practice task and gains a transfer:

| Set | LF-088 law | LF-090 law |
|---|---|---|
| transfers, seed 7700101 (24) | 22 | 23 |
| transfers, seed 7700102 (24) | 23 | 23 |
| practice, seed 7700002 (96) | 96 | 96 |
| practice, seed 7700003 (96) | 96 | 96 |

## The refit and its selection

Two fits of the LF-088 recipe to the modified law, four network-driven
rounds with 72 instead of 48 transfer tasks per round, differing only in
the initialization seed:

| Fit | Development (72) | Held out (72) | Transfers dev, held out |
|---|---|---|---|
| LF-088 network | 65 | 66 | 19, 20 |
| seed 1 | 62 | 63 | 20, 18 |
| seed 2 | 68 | 65 | 22, 21 |

The family scores do not predict the matches. Each network flew the
learned seats of the binding's match comparison over seeds 1 to 12, and
the first legs from identical starts against the native seats compare as:

| Network | Faster | Within 5% | Slower | Median (native 1218) | Mean (native 1202) | Handoffs (native 53) |
|---|---|---|---|---|---|---|
| LF-088 | 7 | 6 | 11 | 1166 | 1251 | 50 |
| seed 1 | 11 | 6 | 7 | 1170 | 1178 | 51 |
| seed 2 | 7 | 5 | 12 | 1258 | 1326 | 49 |

The seed 1 fit is retained: it is the only one ahead of the native law on
both the median and the mean, and it had no refusals, where the seed 2 fit
refused 18 ticks. Selecting on the same twelve seeds that are reported
favors the selection, so the retained stack was then run on seeds 13 to 24
it had not been selected on; that validation is reported below. The fit
variance between two seeds, 62 to 68 on the families and 7 to 11 faster
legs in matches, is larger than the gain a single fit shows, so any later
fit should be judged on more than one seed.

## Validation of the retained stack

With the seed 1 fit retained and the pilot's entry-clearance rule of the
next section in place, the retained stack flew both seed ranges once more.
The first-leg split on seeds 1 to 12 differs from the selection run because
the rule hands some first legs partly to the pilot's own controls.

| Seeds | Faster | Within 5% | Slower | First-leg median, learned against native | First-leg mean | Handoffs | All legs, median | Budget replans |
|---|---|---|---|---|---|---|---|---|
| 1 to 12 (24 pairs) | 5 | 12 | 7 | 1202 against 1218 | 1188 against 1202 | 55 against 53 | 793 against 833 | 1 against 0 |
| 13 to 24 (23 pairs) | 5 | 10 | 8 | 1013 against 1008 | 1125 against 1069 | 61 against 55 | 779 against 911 | 1 against 0 |

The learned seats also completed 33 and 44 sorties against the native
seats' 35 and 41. Of the 47 mixed matches, 38 were still open after 180
seconds; of the nine decided by a pilot's death, the learned seat won 2
and lost 7.

## The decided matches

Those nine were traced, and the legs are not in them. In every lost match
the last learned leg had ended 1,800 to 6,700 ticks before the end, at the
mission's handoff; the ship was then lost under the pilot's own controls
in a pursuit or a capture, the escape pod after it, and the pilot on foot
last, with the opponent 60 to 260 units away. The seat decides more than
the controller: seat 1 lost 8 of the 9 decided mixed matches whichever
controller sat in it, and 4 of the 6 decided native-only matches. The
learned seat sat in seat 1 for all six of its losses as seat 1; as seat 2
it died once in 24 matches against the native seat 2's twice, and its
native opponent in seat 1 died twice against four in native-only play.
What a learned leg changes is where the seat is when combat starts, which
these counts cannot resolve; nothing in the traces points at the binding.

## What the traces still show

- **The motor's steering saturates, and the matrix does not show it.** The
  host now counts flown ticks with the spin above 1.5 radians per second:
  45 and 43 percent on the two seed ranges. On a traced leg the motor's
  inputs were dumped and the teacher run on them: the teacher's own
  steering is at its limit on 66 percent of the ticks, the network's on 55,
  while the navigation command is smooth (a mean heading change of 0.1
  radians and a speed change of 2.7 units per decision). The mechanism is
  the attitude loop under the continuous lateral velocity error of a cruise
  around a planet in a rotating frame: the attitude target sits 50 to 65
  degrees off the nose while the ship still spins the other way, reversing
  the spin between its limits takes 0.6 seconds at the turn-acceleration
  limit, and full thrust keeps moving the target. The motor matrix shows
  only 9 percent saturation for the same teacher because its commands are
  short acquisitions, not cruises. Two attempts were measured and rejected:
  keeping the course hold under gravity (the v4 anchor the v5 teacher fades
  out) passes 319 of 320 on the matrix and a refit scores 305 and 301
  against 287 and 299, but the stack with it drops to 21 and 22 of 24
  transfers for the law and 12 and 14 for the network, with 79 and 88 of 96
  practice tasks, and in matches the spinning share only moves to 41 and 42
  percent with no shorter legs; softer thrust, attitude or spin gains lose
  12 to 41 matrix passes each. A third attempt addressed the network's
  decision-to-decision heading noise, three times the law's on a dumped
  leg (0.104 against 0.034 radians per decision): executing the heading
  halfway between consecutive moving commands, in the driver and the bridge
  alike, halves that noise on the dump but in matches raises the spinning
  share to 48 percent on both ranges and lengthens the legs, with the
  families unchanged, so it was reverted. The swing is intrinsic to the
  attitude loop in cruise. A turning cruise family was then added to the
  motor reference sets (`--set cruise`, 64 tasks whose command heading
  turns at 0.15 or 0.3 radians per second at 40 or 55 units per second for
  25 seconds above arena and practice planets; motor tasks carry a
  `heading_rate`, trace version 4), and it does not reproduce the swing
  either: the teacher tracks the turning command with a steady lag of 5
  units per second and only 8 percent saturation, and acquires none of the
  64 under the fixed-command gate. The dumped match ticks show what does
  trigger a swing: a velocity error of 3 units per second pointing behind
  the nose, which the attitude law answers with a 150-degree turn, and
  the network's command noise keeps creating such errors. Narrow attitude
  ramps that ignore errors under the tolerance lose 33 to 92 matrix
  passes and raise cruise saturation. The swing therefore appears only
  with the network in the loop, so the next motor step measures the
  spinning share on the stack's own family traces from the identical-start
  harness, where a candidate motor is judged with the navigation network
  flying it, and designs the steering law against that measurement.
- **Legs abandoned at another planet's surface.** In the 24 learned seats
  of the twelve-seed run the mission replanned ten transfers for twenty
  seconds without closing on the destination, where the native seats
  replanned none. The traces of three show one geometry: as two planets'
  orbits pass, the destination's entry point is carried inside the other
  planet's margin, the stack flies to it and parks on that planet's
  surface with the ship's feet touching, the pilot withdraws the leg while
  the feet are supported and publishes it again when they lift, up to 24
  times, and the handoff never fires because the pilot's frame planet is
  not the destination; the entry then moves away and the budget expires.
  The mission's own transfer-approach screen calls such an entry unclear,
  so the pilot now publishes the leg only while the entry is clear of the
  other planets' margins, the sun's and the boundary, and flies its own
  detours otherwise. The native bot circles the other planet at its
  detour radius in that geometry instead of parking.
- **Parking at the edge of the stop disk.** The refit's family failures are
  mostly deadlines at a final distance of 2.8 units, the LF-088 parking
  blur, which the mission's handoff rule makes irrelevant in matches.

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the law runs, both fits, every match run with its per-tick traces and
observation dumps, and the analysis scripts.
