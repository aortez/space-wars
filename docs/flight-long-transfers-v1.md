# Long transfers in the navigation fit (LF-093)

**Status: done, 2026-11-01.** LF-092 left the learned seats with more long
legs and fewer handoffs than the native seats under the default settings,
the longest stalling beside the sun. Scored against its law on every
decision of those matches, the navigation network was furthest off beyond
the 600 units its transfers had covered and when routing a far goal around
a planet or the sun, where its squared-error fit averages the law's choice
of side. The transfer generator now also draws long transfers, from 600 to
3,000 units, and the LF-091 recipe with them as a sixth family is the
retained navigation network. In whole matches it halves the legs over 2,000
ticks and cuts the budget replans from 6 to 1 on seeds 1 to 24, validates
on seeds 49 to 60, and on LF-092's seeds 37 to 48 closes both gaps under
the default settings: 6.8 handoffs per ten minutes against the native-only
seats' 6.9, and 3 legs over 2,000 ticks against 3 and 4. Under the 8-second
asteroid stream its seats hand off 7.9 times against 7.0, though 5 legs run
past 2,000 ticks against 3. Its mean error against the law in those matches
falls from 22 to 16 units per second, and from 38 to 25 when routing.
Weighting the routed rows, an encoder that reads the goal beside each
object and six variants of the law's routing did not do enough to adopt.
The traces now say why a transfer tick has no learned leg: two of every
five transfer ticks of the learned seats are withheld because the entry
lies beside the sun or another planet, and the native law flies them.
Flying those is the next step.

## The stalls

[LF-092](flight-transfer-confirmation-v1.md) left the learned seats with
more legs over 2,000 ticks than the native seats (7 against 3 on seeds 37
to 48 under the default settings), and the two longest stalled beside the
sun. Every navigation decision of the learned seats in those twelve matches
was dumped (`SPACEWARS_TRACE_DUMP=0:36000:1`, whose new third field keeps
every n-th navigation decision of the tick window) and the navigation law
was run on each decision's own observation (`tools/imitation_gap.py`); the
same comparison was made on every row the retained network was fitted to
(`tools/fit_coverage.py`). The error is the length of the difference
between the two commands, velocities relative to the goal, in units per
second.

| The law's situation | Fit rows | Mean error | Match decisions | Mean error |
|---|---|---|---|---|
| Aimed at the goal | 108,004 | 0.5 | 97 | 28.9 |
| A local waypoint | 78,044 | 8.3 | 5,782 | 11.3 |
| Routed around a planet or the sun | 7,917 | 31.8 | 3,140 | 37.6 |
| Climbing from a surface or leaving the heat | 15,030 | 16.5 | 1,798 | 23.9 |
| The momentum brake | 1,876 | 23.5 | 187 | 69.8 |
| All | 210,871 | 5.9 | 11,004 | 22.0 |

The aimed rows of the fit are mostly the arrival dwell, a few units from
the goal. Two things stand out. The fit's transfers start at most 600 units
from their entry, so it had 1,644 rows beyond 600 units where the matches
had 3,385 decisions, and the error there is 34 to 47 units per second in
both. And the routed decisions, where the law sends a far goal around a
planet or the sun, are 4 percent of the fit's rows but 29 percent of the
match decisions, with the largest error of any common situation.

Most of the routed error is the side. Behind a body the law rounds it the
way the ship already moves around it, or the nearer way when the ship does
not, and a squared-error fit of a choice between two sides averages them
into a command at the body, which keeps the ship on the line behind it. The
fit's error grows with the goal's angle behind the body, from 23 units per
second at 60 to 90 degrees to 45 at 170 to 180, and is 50 when the ship is
not moving around the body against 29 when its motion settles the side.
Rounding the sun with the goal 150 to 170 degrees behind it, the network's
heading was more than 45 degrees off the law's on 65 percent of the match
decisions.

## Long transfers

The transfer generator draws one candidate task per index on its own
generated arena and admits it when its start lies within the family's
range. A `TransferRange` now chooses the range: the standard 100 to 600
units, whose records are unchanged, or the long range beyond 600 units to
3,000, where each index draws the same task as in the standard range and
its record says `"range": "long"`. The deadline stays 1,800 ticks, the
length of a mission's learned transfer attempt, so the observation's
remaining time reads as it does in a match; the longest legs cannot finish
within it, so a long task is judged by its final distance as much as by its
outcome. The bridge's `generate_transfers` takes the range, the reference's
`--set long_transfer` flies the law on them, and `fit_v2.py` adds them as a
sixth family (`--long-transfers`, `--round-long-transfers`,
`--evaluation-long-transfers`), drawn from each set's transfer seed.

The refit (`nav-fit-long-r1`) is the LF-091 recipe with 96 long transfers
in the first round, 72 in each refinement round and 12 in each evaluation
set. Of its 339,859 rows 54,555 are routed, 16 percent against the retained
fit's 4.

| Navigation fit | Development: standard sets, long transfers | Held out | The law on the held-out set |
|---|---|---|---|
| Retained, LF-091 | 65 of 72 | 68 of 72 | 69 of 72 |
| Long transfers, seed 1 | 68 of 72, 3 of 12 | 69 of 72, 2 of 12 | 69 of 72, 5 of 12 |
| Long transfers, seed 2 | 62 of 72, 2 of 12 | 64 of 72, 3 of 12 | 69 of 72, 5 of 12 |
| Long transfers, routed rows weighted four times | 54 of 72, 2 of 12 | 50 of 72, 2 of 12 | 69 of 72, 5 of 12 |

The standard sets gain most on the obstructed practice family, 8 and 9 of
12 before, 12 and 11 now.

Weighting the routed rows does not close the gap. Offline refits on the
retained fit's rows, with a ninth of the shards held out
(`tools/routed_weighting.py`), that sample routed rows eight times as often
cut their training error from 28 to 12 units per second but their held-out
error only from 32 to 26: the network memorizes the side rather than
reading it from the observation. Nor is the side lost in the pooling of the
object records: an encoder whose records also read the goal's position
(`tools/goal_objects.py`) lowers the held-out routed error only from 32 to
30. A fit with the routed rows four times as heavy (`nav-fit-long-r4`)
lowers their heading error from 26 to 22 degrees at the cost of the rest,
near the goal from 11 to 14, and loses the closed loop.

## The law on long legs

The law, flying the retained motor network, passes 87 of 96 standard
transfers at seed 7700301 but 35 of 96 long ones there, and 8 and 8 of 24
at seeds 7700101 and 7700102. Most long deadlines end a few units from the
entry, on a leg too long for 1,800 ticks: of the 144 long tasks, 118 end
within 50 units of the entry and 17 more than 300 units away. Traced
decision by decision (`tools/law_trace.py`, `tools/law_polar.py`), those
17 show three ways of losing time beside the sun:

- A crawl at the heat. The sun's detour circle lies 70 units beyond its
  heat radius, 294 units from its center, and a ship chasing a waypoint
  half a radian ahead on a circle settles at the circle's radius times
  cos 0.5, 258 units, inside the disk the law's speed limit keeps clear at
  speed (the heat radius with its 30-unit margin and the corridor's half
  width, 263 to 273 units). The clearance-limited speed falls to zero and
  the ship crawls along the heat for hundreds of ticks.
- The long way around. Once the goal is more than a quarter turn around a
  body, a ship moving around it faster than 12 units per second keeps
  rounding it the way it moves, so the drift of a launch can commit the
  ship to 219 degrees around the sun instead of 141.
- A flip at another planet. A detour waypoint inside another planet's disk
  rounds the other way. With that planet on the committed side, the
  waypoint slides in and out of its disk as the ship moves, and the route
  changes sides every few seconds, in one task for 1,000 ticks.

Each was tried as a variant of the law (`tools/law_variant.py`) on the same
144 long transfers:

| Variant | Passed | Within 50 units | Beyond 300 units |
|---|---|---|---|
| Current law | 51 | 118 | 17 |
| The sun always rounded counterclockwise | 47 | 102 | 39 |
| A waypoint at least as far around as the tangent point | 50 | 118 | 16 |
| The sun's circle 105 units beyond the heat | 51 | 124 | 14 |
| The sun's circle 135 units beyond the heat | 51 | 121 | 14 |
| The 105-unit circle, the side whose arc is clear of other bodies | 51 | 121 | 15 |
| The 105-unit circle, the motion settling the side only past 150 degrees, seed 7700301 alone | 37 of 96 | 77 of 96 | 13 of 96 |

The wider sun circle is a little better on all three seeds and keeps the
standard transfers (88 of 96 at 7700301); the other variants move the
failures between tasks. None changes the law enough to refit for, and the
network keeps the current law; the 105-unit circle is the candidate when
the law is next revised.

## Whole matches

The selection follows LF-091 and LF-092: whole ten-minute matches under the
default settings, the learned flight bot against the recovery bot in both
seat orders, each fit flying over the retained motor
(`tools/select_long.sh`). Seeds 1 to 24 select and seeds 49 to 60, which no
candidate was selected on, validate. The first legs start from identical
states; the other rows compare the learned seats with their native
opponents in the same matches and with the native seats of the native-only
matches of the same seeds, which are the same matches in every column.

| Whole ten-minute matches | Retained, seeds 1 to 24 | Long-transfer fit, seeds 1 to 24 | Second seed, seeds 1 to 24 | Retained, seeds 49 to 60 | Long-transfer fit, seeds 49 to 60 |
|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 9, 27, 12 of 48 | 13, 23, 11 of 47 | 12, 25, 11 of 48 | 8, 13, 3 of 24 | 7, 14, 3 of 24 |
| Typical first-leg ratio | 1.00 | 0.97 | 0.98 | 0.96 | 0.96 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 640, 792, 709 | 707, 651, 709 | 757, 771, 709 | 467.5, 562, 727 | 524, 631, 727 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 6, 2, 2 | 3, 2, 2 | 4, 2, 2 | 2, 2, 2 | 1, 0, 2 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 6.8, 5.4, 6.7 | 6.4, 7.0, 6.7 | 6.5, 6.3, 6.7 | 6.6, 5.8, 6.5 | 6.5, 6.2, 6.5 |
| Budget replans: learned seats, their native opponents, native-only seats | 6, 0, 1 | 1, 0, 1 | 3, 1, 1 | 6, 1, 1 | 2, 0, 1 |
| Recoveries: learned seats against their native opponents | 20 against 14 | 23 against 26 | 23 against 20 | 6 against 13 | 11 against 8 |
| Decided matches: won by the learned seat, by the native seat | 25, 23 | 24, 24 | 25, 22 | 13, 11 | 12, 11 |
| Leg ticks the native controls flew after a refusal | 21 of 124,255 | 11 of 105,575 | 11 of 119,117 | 17 of 53,400 | 4 of 45,293 |
| Flown ticks spinning over 1.5 radians per second | 29% | 31% | 29% | 32% | 31% |

On seeds 1 to 24 the long-transfer fit halves the legs over 2,000 ticks (3
against the retained stack's 6; the native-only seats had 2) and makes one
budget replan against six, as many as the native-only seats. Its traces
(`tools/sun_pool.py`) spend 11 percent of their flown ticks in seconds that
close less than 10 units on an entry more than 200 units away, against 19
percent, and 6,240 such ticks within 300 units of the sun against 10,800;
the longest run of them is 780 ticks against 1,140. Its first legs are
level, and its seats hand off 6.4 times per ten minutes against the
native-only seats' 6.7. The second fit of the same recipe has 4 legs over
2,000 ticks and 3 budget replans and passes fewer evaluation tasks, so the
first is the candidate. On seeds 49 to 60 it has one leg over 2,000 ticks
against the retained stack's two, two budget replans against six, and its
seats hand off as often as the native-only seats. The native opponents'
median leg moves with the matches, from 651 to 792 ticks on the same seeds,
so the native-only seats are the steadier reference: the fit's median leg
is level with theirs on seeds 1 to 24 and shorter on 49 to 60.

LF-092 measured the stalls on seeds 37 to 48. The same seeds were played
again with both stacks, under the default settings and the 8-second
asteroid stream (`tools/confirm_long.sh`); the retained stack's runs
reproduce LF-092's byte for byte.

| Seeds 37 to 48, whole matches | Retained, default settings | Long-transfer fit, default settings | Retained, asteroids 8 s | Long-transfer fit, asteroids 8 s |
|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 6, 12, 5 of 23 | 7, 12, 4 of 23 | 5, 14, 3 of 22 | 6, 13, 3 of 22 |
| Typical first-leg ratio | 0.94 | 1.04 | 0.87 | 0.97 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 748, 775, 778 | 709, 810, 778 | 710.5, 743.5, 732 | 638, 550, 732 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 7, 3, 4 | 3, 3, 4 | 6, 4, 3 | 5, 4, 3 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 5.8, 6.9, 6.9 | 6.8, 6.1, 6.9 | 7.0, 6.9, 7.0 | 7.9, 7.4, 7.0 |
| Budget replans: learned seats, their native opponents, native-only seats | 6, 3, 1 | 5, 3, 1 | 12, 5, 3 | 7, 2, 3 |
| Recoveries: learned seats against their native opponents | 17 against 13 | 10 against 8 | 10 against 14 | 8 against 4 |
| Decided matches: won by the learned seat, by the native seat | 12, 12 | 10, 14 | 7, 15 | 11, 12 |
| Leg ticks the native controls flew after a refusal | 1 of 65,763 | 30 of 52,647 | 8 of 77,786 | 15 of 56,520 |
| Flown ticks spinning over 1.5 radians per second | 30% | 31% | 30% | 32% |

Under the default settings the long-transfer fit closes both gaps LF-092
left: its seats hand off 6.8 times per ten minutes against the native-only
seats' 6.9, where the retained stack's handed off 5.8 times, and 3 of its
legs run past 2,000 ticks against the native seats' 3 and 4, where the
retained stack's 7 did. The slow seconds of its traces fall from 15 to 13
percent of the flown ticks, and the longest run of them from 1,140 to 720
ticks. Under the 8-second stream its seats hand off 7.9 times against the
native-only seats' 7.0 and make 7 budget replans against the retained
stack's 12, but five of its legs run past 2,000 ticks against the
native-only seats' three. The typical first-leg ratio, a geometric mean,
moves from 0.94 to 1.04 with one seat: at seed 41, seat 0, the retained
stack's first leg counted 162 ticks and its second 1,828, where the native
seat's first took 2,443 and the new fit's 2,619. The identical first legs
themselves are 7 faster, 12 within five percent and 4 slower against 6, 12
and 5. Every refusal was for a control state outside the stack's envelope,
as in LF-092.

The network now follows the law more closely in these matches. Every
decision of the long-transfer fit's seats was dumped and scored as in
"The stalls":

| Seeds 37 to 48, the law's situation | Retained: decisions, mean error, heading more than 45° off | Long-transfer fit |
|---|---|---|
| Aimed at the goal | 97, 28.9, 33% | 31, 9.4, 0% |
| A local waypoint | 5,782, 11.3, 4% | 4,644, 7.5, 1% |
| Routed around a planet or the sun | 3,140, 37.6, 30% | 2,512, 25.3, 14% |
| Climbing from a surface or leaving the heat | 1,798, 23.9, 25% | 1,419, 21.2, 14% |
| The momentum brake | 187, 69.8, 80% | 204, 71.9, 76% |
| All | 11,004, 22.0, 16% | 8,810, 16.3, 9% |

Beyond 600 units from the entry the error falls from 34 and 47 units per
second to 22 and 19. Rounding the sun, the heading is more than 45 degrees
off the law's on 7 percent of the decisions against 37, and on none of the
40 with the goal 150 to 170 degrees behind it. The side is still averaged
where the ship does not move around the body: with the goal behind it, the
error is 40 units per second against 22 when the ship's motion settles the
side.

The long-transfer fit's seven legs over 2,000 ticks under the default
settings, over the 48 seeds, fall into four kinds:

- Legs the native law flew. The 2,243- and 3,225-tick legs at seed 14,
  seat 1 and seed 23, seat 1 were never published to the stack: their entry
  lay beside another planet (see below), and the mission's own controls
  flew them, falling back to the launch climb 16 and 11 times.
- Legs as long for the native seats. The first legs at seed 21, seat 0 and
  seed 41, seats 0 and 1 took 2,052, 2,619 and 2,011 ticks against the
  native seats' 1,914, 2,443 and 1,949.
- The law's crawl at the heat. A 3,056-tick leg at seed 41, seat 0 spent
  1,767 ticks within 100 units of the sun's surface. For 1,440 of them the
  ship ran about 15 units outside the law's heat disk at 19 units per
  second on average, and the law's clearance limit held its own command
  under 12 units per second on 103 of 241 decisions, while the network's
  heading stayed within 45 degrees of the law's on all but 11: the first
  of the mechanisms above, the law's own.
- The averaged side. At seed 58, seat 0 the goal lay 165 degrees behind the
  planet the ship launched from. For 730 ticks the ship hovered low with
  hardly any motion around the planet while the law alternated between
  climbing and rounding it, and the network's heading was more than 45
  degrees off the law's on 72 of 122 decisions. Once the ship moved around
  the planet the network's mean error fell to 14 and then 8 units per
  second, and the ship closed 400 units in 720 ticks.

## Withheld legs

The mission publishes a transfer leg to the learned stack only while the
leg's entry, 85 units above the target on the line toward the ship, lies
clear of another planet's margin (its radius and 105 units), the sun's (its
radius and 110) and the boundary's (20 units inside it); otherwise its own
transfer controls fly the tick. In matches the entry is always this ordinary
one, since the search for a clear alternate bearing
(`mission_transfer_approach.rs`) is not enabled for them. The traces of
`trace_learned_transfer_seat` now record the first condition that held
each unpublished transfer tick back (`withheld`), and a seat is traced from
the same match its match run played: its published ticks equal the run's
leg ticks.

| Transfer ticks of the learned seats, seeds 1 to 24 | Retained | Long-transfer fit |
|---|---|---|
| Published to the learned stack | 124,255 (60%) | 105,575 (61%) |
| Withheld: the entry beside the sun | 54,268 (26%) | 34,929 (20%) |
| Withheld: the entry beside another planet | 28,867 (14%) | 31,718 (18%) |
| Withheld for another reason | 40 | 37 |
| Transfer attempts never published | 255 of 469 | 163 of 355 |

Most withheld ticks belong to attempts that are never published at all, a
target close to the sun or one whose orbit runs near another planet's; in
the 32 and 30 attempts published part of the time, the leg changes hands
once. The native law flies all of them, and the match measures count those
legs as the learned seats' own: the native fallback in the tables, the leg
ticks flown after a refusal, counts only published legs. Two of every five
transfer ticks of a learned seat are native flight, and flying them is the
next step toward a stack that needs no native controls: entries the stack
can reach beside the sun and the planets, in the fit's tasks and in the
mission's publication.

## The retained stack

The long-transfer fit replaces the LF-091 navigation network in
`crates/spacewars-ai/fixtures/learned-flight-stack-v2`; the motor network
is unchanged. `navigation.json` is a byte copy of
`nav-fit-long-r1/policy-final.json` (sha256 `57b9546e`), whose export agrees
with the Python fit within 2.9e-6 on every head of the development
evaluation. `provenance.json` records its source, evaluation and matches
with the LF-091 network as its predecessor, and the closed-loop parity test
(`crates/spacewars-ai/tests/flight_stack_parity_v2.rs`) replays the new
fit's archived development case 45, a clear-family task, in place of
LF-091's. The contract, the law, the observation and the mission are
unchanged.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the fits (`nav-fit-long-r1`, the retained one, `nav-fit-long-r1-seed2` and
`nav-fit-long-r4`, each with its log), the law's runs on the long transfer
sets and its variants (`law-sun`), the offline refits of the weighting and
the encoder variant (`sun-diagnosis/weighting`), the match runs
(`lf093-<stack>-<seeds>` for the selection and validation,
`lf093-confirm-<settings>-<stack>-37..48` for the confirmation), the traces
(`lf093-traces-w-<stack>-1..24`, `lf093-traces-validation-long`), the
dumped match decisions with their scores (`sun-diagnosis`), the test
binaries with their source states (`bin/engine_client-dump` for the matches
and dumps, `bin/engine_client-withheld` for the traces) and the scripts in
`tools`: `select_long.sh` and `confirm_long.sh` play the matches,
`trace_seats.sh` and `dump_seats.sh` trace and dump the learned seats,
`imitation_gap.py`, `fit_coverage.py`, `rounding.py`, `coverage_tables.py`
and `rounding_tables.py` compare the network with the law,
`routed_weighting.py` and `goal_objects.py` refit offline, `law_variant.py`,
`law_variants.sh`, `law_compare.py`, `law_trace.py` and `law_polar.py` run
and trace the law, and `sun_pool.py`, `sun_legs.py` and `withheld.py` read
the traces. `tools/lf093_summary.py` writes the
[summary](data/flight-long-transfers-v1-summary.json) from those runs, and
`tools/lf093_tables.py` prints the match tables above from it.
