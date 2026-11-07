# The motor in ground contact and at the half turn (LF-099)

**Status: done, 2026-11-06; the LF-099 motor and navigation networks replace
the LF-095 pair in the v2 fixtures, and the climb legs fly with them and are
retained.** The
motor now trains on climbs that start on the ground, come down onto it or
begin with the goal behind the nose, under a teacher whose thrust waits for
the nose and whose turn near the half turn keeps its direction. The ground
stalls that kept [LF-098](flight-climb-band-v1.md)'s climb legs out of the
retained flight are gone from the traces: on seeds 61 to 72 no climb runs
past 600 ticks, against 16 attempts and 25,865 ticks before, and under loose
dirt two attempts on one seat do, against 24 on four seats. Over 252 whole
matches on seeds 1 to 84 the learned seats hand off 6.79 times per ten
minutes against 6.90 for the climb legs over the LF-095 stack (paired by
seed and seat, -0.11 with a 95 percent interval of -0.80 to +0.60) and the
native-only seats' 6.83, with 13 legs over 2,000 ticks against 15, 34
attempts at their deadline against 52, 19 budget replans against 28, 16
percent of their flown ticks spinning against 26, and 95 decided matches won
to 72. One mission rule changes with them: while the stack flies a pursuit
climb, the opponent's range no longer ends it until the ship is 20 units
above the launch band, since a stack that turns its nose before it thrusts
otherwise flies legs of a few ticks at the band's edge, one after another.
The 8-second asteroid stream is the exception: 5.95 handoffs per ten minutes
against 8.01 for the LF-095 stack with the same hold (-2.05, -3.64 to -0.40)
and 7.38 without it, in longer matches that the learned seats win 16 to 7
and in which they spend twice as long hunting, with one seat grounded
through ten launch attempts.

## What LF-098 left

LF-098 bound the launch band's climb and the hunt's pursuit climb to the
stack, and the climbs took it into contact with the ground, where the motor
had never trained: every motor task ended on contact, and its 18 inputs
carry no contact, altitude or clearance. Two motor deficiencies showed in
the traces. Lying on its side, the ship thrust into the ground as soon as
the turn brought the nose within about 80 degrees of the command, the thrust
pinned the hull and the turn stopped. With the goal behind the ship, near a
half turn, the motor braked and its steering fell to nearly zero: the fit
passes through zero where the teacher's steering jumps from full one way to
full the other. On seeds 61 to 72, 16 of 184 climb attempts in the traces
ran past 600 ticks, 25,865 ticks in all, 12 of them on the ground.

## Contact in the motor's tasks

A flight task now has a `contact_allowed` flag (serde default false, omitted
when false): surface contact is a state to fly out of instead of a
collision. Damage, loss and the boundary still end the task. Only the traces
of such tasks carry the new versions, flight environment 6 and motor 5, so
every other trace keeps its version and replays as before. A v2 motor task
may set the flag, and may then start at rest; the v1 contract rejects it
(`a_contact_task_flies_on_after_touching_down_and_replays`).

Five task sets use it (`tools/flight_motor/teacher_v2.py`):

- **Grounded climbs** (96): the ship drops from 15 units with its nose at
  one of eight attitudes from upright to inverted and rests on the ground
  under neutral controls, then is commanded within 0.4 radians of straight
  up at 20, 40 or 57 units per second, on the practice planet and on the
  planets of a generated arena.
- **Low falling climbs** (80): 18 or 30 units up, falling at 5 or 12, with
  the nose 1.6, 2.4 or π radians either side of the local up and the same
  climb command at 20 or 57.
- **Half turns** (80): 80 units up over the practice planet, 40 over an
  arena planet, with the command 2.6, 2.9 or π radians either side of the
  nose, up or along the tangent, at rest or falling at 10.
- **Drifting half turns** (64, practice planet only): the match's dead spot
  held a heading error of 2.7 with the ship drifting sideways, so the
  command up at 57 lies 2.4 to 3.0 radians either side of the nose while the
  ship drifts along the tangent at 20 or 40, level or falling at 5.
- **Tilted climbs** (48): at rest 70 units up, the launch band's edge, with
  the nose 0.5, 0.7 or 0.9 radians either side of up and the command within
  0.2 radians of up at 40 or 57, which measures how soon a climb starts
  while the nose is still turning.

The first three train the motor; the last two judge the law and the fits and
stay out of training. A task passes on the motor's arrival rule, the
velocity within one unit of the command for 30 ticks, which a fast climb
above an arena planet can miss after it has left the ground, as the gravity
and the frame change under it, so the sets also count a task as recovered at
the first tick after 30 ticks out of contact with at least half the command
speed along the command, and report that tick.

## The law

The v6 teacher recovers 45 of the 96 grounded climbs, and the retained motor
network 43. On the ground the teacher's thrust is the trouble, as LF-098
found: probes with fixed controls show that a ship on its side turns upright
in 0.5 to 1.5 seconds with the thrust at 0.1 or less and not at all with 0.2
or more, that a ship on its back cannot make the short turn and at best
makes the long one through the half turn, and that the brake, which slows
the hull's straight-line motion, resists on the ground the pivot that turns
the hull upright, though not in the air. Four terms were added to the
teacher, each inert at its default (`teacher_v2.py`, with tests in
`teacher_v2_tests.py`):

- **A thrust gate**: the moving thrust fades from full to none as the angle
  between the nose and the attitude vector grows from `open` to `closed`
  radians, so the thrust waits for the nose.
- **A half-turn band**: within `half_turn` radians of a half turn, of the
  heading error or of the attitude vector's angle from the nose, a saturated
  turn keeps the direction its previous steering gave it, with a bias toward
  the shorter turn when there is none. The previous steering is among the
  motor's inputs, so the network can carry the commitment that the sine and
  cosine of the error cannot. The band is wider than the error's half turn
  because gravity and the brake's share put the attitude vector behind the
  nose while the heading error is still short of it (2.7 on LF-098's seed
  66).
- **A brake floor**: the moving brake fades in over relative speeds from
  `low` to `high`, so a ship at rest does not brake on its jitter.
- **A lift**: the gate reopens under gravity for a slow ship whose nose lies
  within a band of radians from straight up, where the thrust lifts the ship
  instead of pinning it, for a climb at the band's edge or a ship on its
  feet.

Each variant flew the motor's sets with the teacher itself as the motor, and
the navigation law over it on the first transfer and practice seeds, as in
[LF-091](flight-motor-steering-v1.md) (`tools/lf099_sweep.sh`):

| Teacher | Matrix (320) | Grounded climbs (96): passed, recovered (mean tick) | Low falling climbs (80): recovered (tick) | Half turns (80): recovered (tick) | Drifting half turns (64): recovered (tick) | Tilted climbs (48): passed, recovered (tick) | Transfers (24), spinning share | Practice (96), spinning share |
|---|---|---|---|---|---|---|---|---|
| v6 | 316 | 38, 45 (98) | 78 (127) | 80 (130) | 64 (115) | 46, 48 (41) | 22, 0.24 | 96, 0.17 |
| gate 0.25 to 0.5 | 315 | 65, 84 (206) | 78 (165) | 80 (168) | - | - | 21, 0.11 | 96, 0.08 |
| gate 0.35 to 0.7 | 312 | 70, 82 (172) | 78 (147) | 80 (153) | - | - | 22, 0.11 | 96, 0.09 |
| gate 0.45 to 0.9 | 314 | 60, 73 (148) | 79 (142) | 80 (144) | - | - | 22, 0.14 | 96, 0.11 |
| gate 0.35 to 0.7, brake floor 0.5 to 2 | 312 | 71, 82 (169) | 79 (152) | 80 (152) | - | - | 22, 0.12 | 86, 0.09 |
| gate 0.35 to 0.7, half turn 0.7 (v7) | 313 | 70, 81 (171) | 79 (139) | 80 (142) | 64 (138) | 47, 48 (56) | 22, 0.12 | 96, 0.10 |
| gate 0.35 to 0.7, half turn 1.0 | 313 | 71, 82 (172) | 79 (139) | 80 (138) | - | - | 22, 0.11 | 96, 0.10 |
| v7, lift 0.6 to 1.0 rad under 5 to 10 | 313 | 61, 74 (156) | 79 (136) | 80 (140) | 64 (134) | 46, 48 (53) | 22, 0.12 | 96, 0.11 |
| v7, lift 0.8 to 1.2 rad under 5 to 10 | 313 | 51, 64 (144) | 79 (135) | 80 (140) | 64 (133) | 46, 48 (53) | 22, 0.12 | 96, 0.11 |
| v7, lift 0.6 to 1.0 rad under 10 to 20 | 313 | 61, 74 (153) | 79 (127) | 80 (135) | 64 (130) | 46, 48 (51) | 21, 0.13 | 96, 0.11 |

Every variant passes all 312 v1 tasks. The gate nearly doubles the grounded
recoveries and halves the spinning share in the navigation loop, at the cost
of slower recoveries where the nose has far to turn; the narrowest gate
recovers most but slowest, the widest least. The brake floor adds nothing on
the ground and costs ten practice tasks, so it is left out. The half-turn
band takes 8 to 15 ticks off the falling and half-turn recoveries. The v7
law is the 0.35 to 0.7 gate with the 0.7 band, and it is now the teacher's
default, with `GAINS_V6` and `GAINS_V5` keeping the earlier laws. On the
tilted climbs the gate costs 15 ticks before a climb starts, 56 against 41;
the lift wins back 3 to 5 of them and costs 7 to 17 grounded recoveries,
mostly of ships that start on their sides or backs and turn up through
those angles while still on the ground, so it is left out too. Both teachers
recover every drifting half turn, at mean ticks of 115 and 138.

## The fits

The fit is the [LF-087](flight-motor-teacher-v5.md) recipe as LF-091 ran it
on the v7 law: the 64-wide actor with the steering weighted three to one,
400,000 synthetic rows, 300 teacher episodes and three rounds of 200
episodes flown by the network and labeled by the teacher, with three in ten
of the episodes drawn from the grounded, falling and half-turn families with
random parameters (`--contact-share 0.3`). Four seeds were fitted, and each
was scored on the motor's sets and in the stack harness under the LF-095
navigation network (`tools/lf099_fit_seeds.sh`):

| Motor | Matrix: development, held out (320 each) | V1 sets (312) | Grounded climbs: passed, recovered (tick) | Low falling climbs: recovered (tick) | Half turns: recovered (tick) | Drifting half turns: recovered (tick) | Tilted climbs: passed, recovered (tick) | Stack under the LF-095 navigation: transfers; practice | Spinning share |
|---|---|---|---|---|---|---|---|---|---|
| LF-091 fit (retained) | 263, 260 | 312 | 34, 43 (96) | 72 (110) | 79 (128) | 64 (153) | 45, 48 (56) | 21, 21; 90, 91 | 0.14 to 0.19 |
| v7, seed 1 | 197, 214 | 312 | 65, 79 (174) | 79 (143) | 80 (147) | - | - | 15, 19; 83, 87 | 0.09 to 0.11 |
| v7, seed 2 | 269, 268 | 312 | 62, 79 (178) | 79 (140) | 80 (149) | 64 (140) | 45, 48 (55) | 18, 19; 86, 89 | 0.09 to 0.12 |
| v7, seed 3 | 229, 242 | 312 | 65, 78 (175) | 79 (132) | 80 (143) | - | - | 16, 15; 81, 84 | 0.08 to 0.09 |
| v7, seed 4 | 229, 231 | 312 | 59, 75 (170) | 79 (137) | 80 (145) | - | - | 20, 20; 84, 86 | 0.10 to 0.12 |

Every fit recovers 75 to 79 of the grounded climbs against the LF-091 fit's
43, and the second seed is the one above the LF-091 fit on the matrix, 269
and 268 against 263 and 260. Under the LF-095 navigation network it passes
fewer stack tasks than the LF-091 fit, but that network was fitted with the
LF-091 motor in its loop. Flown by the navigation law itself, the second
seed passes 21 and 24 transfers and all 192 practice tasks at a spinning
share of 0.07 to 0.09, against 22, 21, 96 and 96 at 0.12 to 0.18 for the
LF-091 fit and 20, 22, 96 and 95 for the fourth seed, and on the transfers
it and the LF-091 fit both pass, 21 in each set, it arrives 16 and 10
percent sooner on average, in 942 ticks against 1,124 and 1,074 against
1,192. The second seed is the LF-099 motor; it matches its Python fit within
9.5e-6 in every action.

## The navigation refit

The LF-095 navigation recipe was refitted with the LF-099 motor in its loop,
on two seeds (`tools/lf099_nav_fit.sh`), and each pair was scored in the
stack harness (`tools/lf099_stack.sh`):

| Navigation network | Development (84) | Held out (84) | Law, held out (84) | Stack over the LF-099 motor: transfers; practice | Spinning share |
|---|---|---|---|---|---|
| LF-095 network | 69 | 73 | 74 | 18, 19; 86, 89 | 0.09 to 0.12 |
| Refit, seed 1 | 70 | 66 | 79 | 20, 19; 91, 90 | 0.09 to 0.12 |
| Refit, seed 2 | 66 | 68 | 79 | 20, 20; 88, 87 | 0.09 to 0.16 |

The LF-095 network's own scores come from its fit over the LF-091 motor; its
law passed 74 held-out tasks flying that motor and passes 79 flying the
LF-099 motor. Both refits pass more of the stack harness's transfers than
the LF-095 network over the same motor, 39 and 40 against 37, and the first
more of its practice tasks, 181 against 175. The first imitates its law less
well on the held-out tasks, 66 against the law's 79, with 8 of 12 obstructed
tasks and 4 of 12 long transfers, and the matches chose it over the second
anyway. In whole matches on seeds 1 to 24, all but the last flight without
the band's edge hold (below):

| Default, seeds 1 to 24, whole ten-minute matches | Transfer legs only | Climb legs, LF-095 stack | LF-099 motor, LF-095 navigation | LF-099 motor, second navigation refit, no hold | LF-099 stack, no hold | LF-099 stack |
|---|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 14, 19, 15 of 48 | 33, 7, 8 of 48 | 25, 9, 14 of 48 | 29, 7, 12 of 48 | 31, 12, 5 of 48 | 31, 12, 5 of 48 |
| Typical first-leg ratio | 1.01 | 0.94 | 0.97 | 0.96 | 0.89 | 0.89 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 590.5, 772, 709 | 767.5, 689, 709 | 728, 735, 709 | 706.5, 746, 709 | 733, 775, 709 | 733, 794.5, 709 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 5, 3, 2 | 2, 3, 2 | 7, 5, 2 | 3, 4, 2 | 0, 2, 2 | 1, 2, 2 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 7.5, 6.8, 6.7 | 7.1, 7.4, 6.7 | 6.3, 6.7, 6.7 | 7.0, 6.9, 6.7 | 6.8, 6.8, 6.7 | 6.4, 6.6, 6.7 |
| Budget replans: learned seats, their native opponents, native-only seats | 3, 0, 1 | 9, 0, 1 | 5, 2, 1 | 3, 1, 1 | 2, 1, 1 | 3, 1, 1 |
| Recoveries: learned seats against their native opponents | 24 against 26 | 19 against 20 | 27 against 21 | 21 against 14 | 19 against 18 | 22 against 20 |
| Decided matches: won by the learned seat, by the native seat | 25, 23 | 27, 21 | 26, 22 | 26, 21 | 27, 21 | 26, 22 |
| Leg ticks the stack flew | 190,493 | 272,228 | 257,834 | 236,126 | 214,752 | 219,512 |
| Published ticks by entry: ordinary, clear alternate, blocked | 108358, 81854, 285 | 109665, 92348, 2 | 108887, 96317, 251 | 92204, 86201, 31 | 92431, 73860, 0 | 95354, 70657, 0 |
| Leg ticks the native controls flew after a refusal | 4 of 190,497 | 106 of 272,334 | 61 of 257,895 | 152 of 236,278 | 65 of 214,817 | 65 of 219,577 |
| Flown ticks spinning over 1.5 radians per second | 30% | 26% | 18% | 19% | 15% | 15% |
| Climb legs flown, and their ticks | none | 421, 70,288 | 473, 52,427 | 341, 57,757 | 515, 48,472 | 308, 53,512 |
| Leg ticks the native controls flew in surface contact | none | none | none | none | none | none |
| Attempts ended at their 30-second deadline | 10 | 10 | 12 | 9 | 7 | 7 |
| Sorties and recoveries of the learned seats | 145, 24 | 127, 19 | 119, 27 | 117, 21 | 117, 19 | 123, 22 |

The LF-099 motor under the LF-095 navigation network hands off 6.3 times per
ten minutes with 7 legs over 2,000 ticks; the first refit flies the
identical first legs at the lowest typical ratio of any flight, 0.89, 31
faster and 5 slower of 48, with no leg over 2,000 ticks, 7 deadlines and 65
refused ticks at 6.8 handoffs, against the second refit's 29 and 12, 3 legs,
9 deadlines and 152 refused ticks at 7.0. The first refit is the LF-099
navigation network; it matches its Python fit within 2.9e-6 in every head.

## The band's edge

The first refit's confirmation over seeds 1 to 84 flew 4,271 climb legs
against the LF-095 stack's 1,364, and five seats flew more than 100 each
against none, seed 41 seat 0 1,906 of them. On seeds 61 to 72 its traces
show 659 pursuit attempts with a median of 3 ticks. A pursuit climb ends
when the ship clears 140 above every planet or the opponent is beyond 300
units, and the band starts it again whenever the ship is below 70 plus a
falling term. With the opponent far away, a ship crossing the band's edge is
released the tick it rises above it and climbs again the tick it sinks back.
The pilot's own climb carries the ship over the edge in those ticks, but the
stack's motor turns the nose before it thrusts, so each leg ends at the edge
with a few of the pilot's ticks between, and the ship hovers there.

While the stack flies the leg, the opponent's range now releases a pursuit
climb only once the ship is 20 units above the band
(`PURSUIT_CLIMB_EDGE_HOLD` in `mission_pilot.rs`, with
`a_learned_pursuit_climb_holds_through_the_band_edge_against_the_range_release`);
the clearance still ends it, and the pilot's own climbs are unchanged, so
the native-only matches of seeds 1 to 12 are identical byte for byte. With
the hold, no seat flies more than 26 climb legs, 1,034 in all; the five
seats fly 6 to 14. On seeds 61 to 72 the stack's pursuit attempts number 92
with a median of 199 ticks. Over seeds 1 to 84 the hold leaves the LF-099
stack's rate level, -0.13 (-0.38 to +0.09), though on seeds 1 to 24 alone it
is -0.31 (-0.78 to -0.02), and the LF-095 stack's too, -0.04 (-0.27 to
+0.19).

## Whole matches

Each flight played the client's match harness in release, as in LF-098: for
every seed, the recovery bot in both seats, then the learned flight bot in
one seat and the recovery bot in the other, in both seat orders, for ten
minutes under the match rules. The flights are LF-098's, the committed stack
flying the transfer legs only and the climb legs; the climb legs over the
LF-095 stack with the hold, which separates the hold from the networks; and
the LF-099 stack without the hold and with it.

| Default settings, seeds 1 to 84, whole ten-minute matches | Transfer legs only | Climb legs, LF-095 stack | LF-095 stack, edge hold | LF-099 stack, no hold | LF-099 stack |
|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 61, 69, 36 of 166 | 106, 28, 32 of 166 | 106, 28, 32 of 166 | 104, 36, 26 of 166 | 104, 36, 26 of 166 |
| Typical first-leg ratio | 0.97 | 0.91 | 0.91 | 0.92 | 0.92 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 681, 736, 728 | 717, 687, 728 | 722, 706, 728 | 715.5, 702.5, 728 | 719, 725, 728 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 23, 21, 18 | 15, 20, 18 | 15, 19, 18 | 9, 14, 18 | 13, 14, 18 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 7.1, 7.1, 6.8 | 6.9, 7.0, 6.8 | 6.9, 6.9, 6.8 | 6.9, 6.7, 6.8 | 6.8, 6.5, 6.8 |
| Budget replans: learned seats, their native opponents, native-only seats | 16, 6, 6 | 28, 6, 6 | 29, 5, 6 | 17, 7, 6 | 19, 7, 6 |
| Recoveries: learned seats against their native opponents | 85 against 92 | 73 against 81 | 76 against 82 | 65 against 77 | 70 against 72 |
| Decided matches: won by the learned seat, by the native seat | 81, 87 | 94, 73 | 92, 76 | 88, 80 | 95, 72 |
| Leg ticks the stack flew | 714,436 | 958,665 | 954,611 | 847,036 | 838,517 |
| Published ticks by entry: ordinary, clear alternate, blocked | 400922, 311768, 1806 | 369588, 343237, 993 | 371966, 336956, 993 | 349463, 306211, 1088 | 347421, 312092, 1088 |
| Leg ticks the native controls flew after a refusal | 60 of 714,496 | 298 of 958,963 | 315 of 954,926 | 285 of 847,321 | 315 of 838,832 |
| Flown ticks spinning over 1.5 radians per second | 29% | 26% | 26% | 16% | 16% |
| Climb legs flown, and their ticks | none | 1364, 245,015 | 1176, 244,870 | 4271, 190,388 | 1034, 178,066 |
| Leg ticks the native controls flew in surface contact | none | none | none | none | none |
| Attempts ended at their 30-second deadline | 30 | 52 | 50 | 31 | 34 |
| Sorties and recoveries of the learned seats | 514, 85 | 470, 73 | 470, 76 | 449, 65 | 454, 70 |

Over seeds 1 to 84 the LF-099 stack is level with the climb legs over the
LF-095 stack on the rate, 6.79 against 6.90 and 6.87 with the hold, and on
the median leg, 719 ticks against 717 (+2, -82 to +87). It runs 13 legs past
2,000 ticks against 15, 34 attempts to their deadline against 52, 19 budget
replans against 28 and 16 percent of its flown ticks spinning against 26,
and it flies 178,066 climb ticks against 245,015. Against the transfer legs
alone, the flight on main, it hands off 0.32 times per ten minutes less
(-1.08 to +0.46), flies its first legs faster on 104 of 166 against 61, and
wins 95 decided matches to 72 against 81 to 87. The native controls fly 315
of its 838,832 leg ticks after the stack refuses the ship's state, and none
in surface contact.

| Condition | Flight | Handoffs per ten minutes: learned seats, native opponents, native-only seats | Identical first legs: faster, within 5%, slower | Legs over 2,000 ticks: learned, native-only | Deadlines | Climb ticks flown | Native ticks in contact | Decided matches: won, lost |
|---|---|---|---|---|---|---|---|---|
| Asteroids 8 s, seeds 37 to 48 | Transfer legs only | 7.38, 7.15, 7.00 | 7, 11, 4 | 6, 3 | 9 | none | none | 14, 9 |
| Asteroids 8 s, seeds 37 to 48 | Climb legs, LF-095 stack | 7.38, 9.46, 7.00 | 15, 4, 3 | 1, 3 | 4 | 18,587 | none | 10, 13 |
| Asteroids 8 s, seeds 37 to 48 | LF-095 stack, edge hold | 8.01, 9.08, 7.00 | 15, 4, 3 | 1, 3 | 4 | 17,794 | none | 11, 12 |
| Asteroids 8 s, seeds 37 to 48 | LF-099 motor, LF-095 navigation, edge hold | 6.86, 7.46, 7.00 | 12, 5, 5 | 4, 3 | 6 | 26,817 | none | 9, 15 |
| Asteroids 8 s, seeds 37 to 48 | LF-099 stack, no hold | 6.15, 6.22, 7.00 | 16, 4, 2 | 4, 3 | 10 | 51,614 | none | 16, 7 |
| Asteroids 8 s, seeds 37 to 48 | LF-099 stack | 5.95, 6.68, 7.00 | 16, 4, 2 | 4, 3 | 11 | 59,117 | none | 16, 7 |
| Asteroids 3 s, seeds 37 to 48 | Transfer legs only | 6.11, 7.09, 7.01 | 10, 7, 6 | 5, 1 | 6 | none | none | 12, 11 |
| Asteroids 3 s, seeds 37 to 48 | Climb legs, LF-095 stack | 8.54, 7.53, 7.01 | 16, 2, 5 | 3, 1 | 6 | 23,502 | none | 12, 11 |
| Asteroids 3 s, seeds 37 to 48 | LF-095 stack, edge hold | 8.29, 7.22, 7.01 | 16, 2, 5 | 3, 1 | 6 | 26,519 | none | 10, 13 |
| Asteroids 3 s, seeds 37 to 48 | LF-099 motor, LF-095 navigation, edge hold | 7.26, 5.23, 7.01 | 13, 6, 5 | 5, 1 | 8 | 25,782 | none | 13, 9 |
| Asteroids 3 s, seeds 37 to 48 | LF-099 stack, no hold | 8.20, 6.18, 7.01 | 14, 7, 3 | 2, 1 | 4 | 28,067 | none | 15, 9 |
| Asteroids 3 s, seeds 37 to 48 | LF-099 stack | 8.26, 6.33, 7.01 | 14, 7, 3 | 2, 1 | 4 | 27,134 | none | 17, 6 |
| Loose dirt, seeds 37 to 48 | Transfer legs only | 8.04, 5.31, 7.07 | 10, 10, 3 | 1, 2 | 2 | none | none | 16, 8 |
| Loose dirt, seeds 37 to 48 | Climb legs, LF-095 stack | 7.08, 6.78, 7.07 | 16, 3, 4 | 3, 2 | 21 | 66,268 | none | 12, 11 |
| Loose dirt, seeds 37 to 48 | LF-095 stack, edge hold | 6.80, 6.57, 7.07 | 16, 3, 4 | 2, 2 | 22 | 72,701 | none | 13, 10 |
| Loose dirt, seeds 37 to 48 | LF-099 stack, no hold | 7.33, 5.73, 7.07 | 19, 1, 3 | 2, 2 | 2 | 22,666 | none | 12, 10 |
| Loose dirt, seeds 37 to 48 | LF-099 stack | 7.30, 5.75, 7.07 | 19, 1, 3 | 3, 2 | 5 | 26,236 | none | 13, 9 |

Paired by seed and seat order against the LF-095 stack with the hold, with
seeded bootstrap intervals over the seats (`tools/lf098_paired.py` for the
rate, `tools/lf099_summary.py` for the median leg):

| Condition | Flight | Handoffs per ten minutes: against, flight | Difference (95% interval) | Seats higher, lower | Outcomes better, worse | Median leg, ticks: against, flight | Difference (95% interval) |
|---|---|---|---|---|---|---|---|
| Default, seeds 1 to 84 | LF-099 stack | 6.87, 6.79 | -0.07 (-0.74 to +0.62) | 92, 76 | 34, 31 | 722, 719 | -3 (-86 to +78) |
| Asteroids 8 s, seeds 37 to 48 | LF-099 stack | 8.01, 5.95 | -2.05 (-3.64 to -0.40) | 10, 14 | 10, 5 | 674, 780 | +106 (-134 to +204) |
| Asteroids 8 s, seeds 37 to 48 | LF-099 motor, LF-095 navigation | 8.01, 6.86 | -1.14 (-3.01 to +0.92) | 14, 10 | 5, 8 | 674, 800 | +126 (-136 to +309) |
| Asteroids 3 s, seeds 37 to 48 | LF-099 stack | 8.29, 8.26 | -0.03 (-2.24 to +2.19) | 13, 11 | 10, 4 | 815, 775 | -40 (-270 to +234) |
| Loose dirt, seeds 37 to 48 | LF-099 stack | 6.80, 7.30 | +0.50 (-1.06 to +2.30) | 12, 12 | 7, 6 | 572, 697 | +125 (-107 to +323) |

Under the 3-second stream and loose dirt the LF-099 stack is level with the
LF-095 stack, and under loose dirt its deadlines fall from 22 to 5. Under
the 3-second stream, LF-096's weak spot, it keeps the climb legs' gain over
the transfer legs alone, 8.26 handoffs against 6.11 (+2.15, +0.15 to +4.18).
Under the 8-second stream its rate is lower by 2.05 handoffs per ten
minutes, the one loss against the LF-095 stack whose interval excludes zero.
Its median leg is longer, 780 ticks against 674, but within the variation of
the seats (+106, -134 to +204), and its matches are longer: the learned
seats win 16 decided matches to 7 against 11 to 12, their matches last
544,158 ticks against 436,098 for 90 handoffs against 97, and their native
opponents' rate falls as well, from 9.08 to 6.68. In the traces of those 24
seats the LF-099 seats spend 157,070 ticks hunting against 75,773, and the
hunt flies no transfer; it flies 186 pursuit climbs against 66, about a
third more per tick of hunting. The LF-099 motor under the LF-095 navigation
network falls between, at 6.86. Five of the stack's eleven deadlines are one
seat's, seed 48 seat 0, grounded through ten launch attempts (below).

## The climbs in the traces

Per-tick traces of both learned seats on seeds 61 to 72 under the default
settings, on 37 to 48 under loose dirt and on 37 to 48 under the 8-second
stream show what the stack does with the climbs. A climb here is a run of
ticks under the mission's own climb state, the launch band or the hunt's
"climbing for a firing pass", whoever flew it; an attempt is one of the
stack's, which a 30-second deadline ends.

| Traces | Flight | Launch from the surface, median ticks (runs) | Pursuit climbs, median ticks (runs) | Climbs over 600 ticks, and their ticks | Stack attempts over 600 ticks, and their ticks | Climb ticks flown natively | Ticks aboard the stack flew | Ticks hunting |
|---|---|---|---|---|---|---|---|---|
| Default, seeds 61 to 72 | Transfer legs only | 246 (32) | 352 (88) | 7, 5,353 | - | 39,736 | 30.7% | 89,302 |
| Default, seeds 61 to 72 | Climb legs, LF-095 stack | 133 (27) | 190 (128) | 4, 25,865 | 16, 25,865 | 42 | 41.6% | 139,168 |
| Default, seeds 61 to 72 | LF-099 motor, LF-095 navigation | 133 (28) | 200 (117) | 1, 1,262 | 1, 989 | 134 | 38.7% | 109,449 |
| Default, seeds 61 to 72 | LF-099 stack, no hold | 130 (30) | 192 (100) | 0, 0 | 0, 0 | 77 | 37.4% | 85,634 |
| Default, seeds 61 to 72 | LF-099 stack | 130 (30) | 200 (91) | 0, 0 | 0, 0 | 72 | 36.9% | 84,719 |
| Loose dirt, seeds 37 to 48 | Transfer legs only | 245 (31) | 334 (120) | 10, 7,502 | - | 50,286 | 22.5% | 125,990 |
| Loose dirt, seeds 37 to 48 | Climb legs, LF-095 stack | 133 (40) | 184 (116) | 11, 38,999 | 24, 38,817 | 76 | 38.3% | 134,132 |
| Loose dirt, seeds 37 to 48 | LF-099 stack, no hold | 131 (32) | 207 (84) | 1, 1,800 | 0, 0 | 2,131 | 34.5% | 98,631 |
| Loose dirt, seeds 37 to 48 | LF-099 stack | 131 (32) | 201 (84) | 3, 6,039 | 2, 3,601 | 2,181 | 34.7% | 99,757 |
| Asteroids 8 s, seeds 37 to 48 | LF-095 stack, edge hold | 130 (30) | 179 (66) | 0, 0 | 0, 0 | 67 | 35.2% | 75,773 |
| Asteroids 8 s, seeds 37 to 48 | LF-099 stack | 132 (36) | 207 (186) | 5, 17,366 | 10, 17,221 | 220 | 35.0% | 157,070 |

The LF-099 stack climbs from the surface to the band in a median 130 ticks
and flies a pursuit climb in 200, as the LF-095 stack did, and on the
default seeds none of its climbs runs past 600 ticks. Under loose dirt two
of its attempts do, both on seed 43 seat 1, and the native controls fly
2,181 climb ticks against 76, 2,007 of them the last 2,007 ticks of seed 43
seat 0's match, a pursuit climb of 1,800 ticks and then the launch band's
climb, with the ship resting on the ground in the dirt: a leg is published
only with the feet off the ground, so the lift-off is the native controls',
and they do not get the ship up. Under the 8-second stream every attempt
past 600 ticks is seed 48 seat 0's. It flies 37 percent of the ticks aboard
on the default seeds against 41.6 for the LF-095 stack, which flew 25,865 of
them in stuck climbs.

## Retention

The climb legs join the retained flight with the LF-099 networks
(2026-11-06). LF-098 kept them out until the motor handled contact and the
half turn, and the two stalls it found are gone from the default seeds'
traces. Against the transfer legs alone, the flight on main, the LF-099
stack is level under the default settings (-0.32, -1.08 to +0.46) and loose
dirt (-0.74, -3.04 to +1.61), ahead under the 3-second stream (+2.15, +0.15
to +4.18) and behind under the 8-second stream, though within the variation
of its seats (-1.42, -3.28 to +0.56). Under the default settings it also
runs 13 legs past 2,000 ticks against 23, spins on 16 percent of its flown
ticks against 29 and wins 95 decided matches to 72 against 81 to 87. The
8-second stream takes the place of the 3-second one as the weak spot: there
the learned seats hand off 5.95 times per ten minutes against the
native-only seats' 7.00, a gap about the size of the one the transfer legs
alone leave under the 3-second stream, 6.11 against 7.01.

## What is left

- **A turn the ground blocks.** Seed 48 seat 0 under the 8-second stream sat
  on the ground through ten launch attempts, 17,221 ticks, under a
  navigation command of 5 to 7 units per second, with the heading error
  between 1.1 and 1.6 radians, the steering saturated one way, the thrust
  off, the brake near 0.8 and no spin; loose dirt seed 43 seat 1 sat 3,601
  ticks with the steering saturated, the thrust off and no spin at an error
  of 2.1 to 2.4 radians. The motor holds a full turn one way against a
  ground that stops it, and a ship on its back can only turn the long way.
  Its inputs carry no contact, so it cannot tell a blocked turn from a slow
  one: a contact input in a new motor contract, or a law that turns the
  other way after a held turn makes no spin, is the next motor step.
- **The lift-off in dirt.** The native controls own the lift-off by the
  leg's gate, and in loose dirt they can leave a ship on its feet for the
  rest of a match.
- **The 8-second stream.** The rate there is the stage's one significant
  loss, in matches whose flow the learned seats change by winning; a longer
  run of that condition would tell match flow from flight. Run over 48
  seeds on 2026-11-07, the condition is level: 6.73 handoffs per ten
  minutes against the native-only seats' 6.48 (+0.25, -0.84 to +1.33), the
  gap its sample; see [LF-100](flight-turn-sense-v1.md).
- **The navigation network's held-out imitation**, 66 of 84 against its
  law's 79, chosen over a closer fit by the matches.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the law sweep (`lf099-sweep2`), the motor fits and their scores
(`motor/fit-lf099-g35ht07-seed1` to `seed4`, `motor/evaluate-lf099-*`,
`motor/law-lf099` for the navigation law over the motors), the stack harness
runs (`stack-lf099-retained`, `stack-lf099-g35ht07-seed1` to `seed4`,
`stack-lf099-n1`, `stack-lf099-n2`), the navigation refits
(`nav-fit-lf099-m2-s1`, `nav-fit-lf099-m2-s2`), and the match runs of the
LF-099 motor under the LF-095 navigation network (`lf099-m2-*`, and
`lf099-m2-hold-*` with the hold), of both refits without the hold
(`lf099-n1-*`, `lf099-n2-*`), of the LF-099 stack (`lf099-e1-*`) and of the
LF-095 stack with the hold (`lf099-riding-edge-*`), each with its log; the
seat traces (`lf099-m2-traces-61..72`, `lf099-n1-traces-61..72`,
`lf099-n1-dirt-round-traces-37..48`, `lf099-e1-traces-61..72`,
`lf099-e1-dirt-round-traces-37..48`, `lf099-e1-asteroids8-traces-37..48`,
`lf099-riding-edge-asteroids8-traces-37..48`); the test binaries with their
source diffs (`bin/engine_client-lf099-head` without the hold,
`bin/engine_client-lf099-edge` with it, both flying candidates named by
`SPACEWARS_LEARNED_MOTOR` and `SPACEWARS_LEARNED_NAVIGATION`, and
`bin/engine_client-lf099-fixture` with the LF-099 networks embedded); and
the scripts. `tools/lf099_summary.py` writes the
[summary](data/flight-motor-contact-v1-summary.json) from those runs and
LF-098's, `tools/lf099_tables.py` prints the tables above from it, and
`tools/lf098_paired.py`, `tools/lf098_long_climbs.py`,
`tools/lf098_climbs.py` and `tools/lf098_shares.py` read the matches and
traces as in LF-098.

The binary without the hold, with no candidate named, reproduces the LF-098
climb legs' records of seeds 1 to 12 byte for byte, apart from the
`contact_ticks` telemetry field that the earlier of LF-098's binaries lacks
(`lf099-preflight-1..12`); the binary with the hold leaves the native-only
matches of seeds 1 to 12 byte for byte (`lf099-edge-preflight-1..12`); and
matches flown from the networks embedded in the fixtures, with no candidate
named, reproduce the candidate run's records byte for byte on seeds 1 to 12
and on seeds 37 to 48 under the 8-second stream and under loose dirt
(`lf099-fixture-same-records.txt`). The parity replay
(`flight_stack_parity_v2.rs`) now flies development case 52 of the
navigation refit, a clear-family task under gravity on a spinning practice
planet; the LF-095 case, 45, ends at its deadline with the LF-099 networks.
Under glibc 2.36 in a container, whose math functions predate the correctly
rounded ones of this host's glibc 2.43 as those of the CI runners' 2.39 do,
case 52 strays from the archive by at most 4.8e-5 in action and arrives on
the archived tick. Of the eleven clear-family development cases that passed,
five hold to the replay's tolerances off-host; case 27's initial frame
differs by one float step in its angle, case 62's archived actions do not
replay through the physics there, and in the four others the closed loop
amplifies the last-digit differences past the tolerances.
