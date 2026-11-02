# The navigation law's detour around the sun (LF-095)

**Status: done, 2026-11-01.** LF-094 left the learned stack with a tail of
long legs beside the sun, where the navigation law crawls at the edge of
its heat disk. The crawl is the chase of the sun's detour waypoint: half a
radian ahead on a circle 294 units from the sun's center, it leads the ship
on a pursuit curve that settles at 258 units, inside the disk the speed
limit keeps clear, where every heading toward it is speed-limited and the
momentum brake stops every motion that drifts inward. Judged by the first
tick within 50 units of the entry on 236 transfers with the sun in the way,
since the pass count counts legs reached but unsettled as failures, the
sun's waypoint at the circle's radius over the cosine of its lead, with a
momentum brake that reads the surfaces alone outside the disk, is never
later and at least 120 ticks sooner on 21 of 193 tasks, ends 206 within 50
units against 194, and keeps the standard transfers. The network refit to
that law, its second seed selected on seeds 1 to 24 and validated on 49 to
60, is the retained navigation network: over 48 seeds under the default
settings its seats hand off 7.25 times per ten minutes against the LF-093
network's 6.61 and the native-only seats' 6.72, 9 of its legs run past
2,000 ticks against 22 and 8, and it makes 6 budget replans against 12 and
3, with identical first legs 32 faster, 40 within five percent and 23
slower of 95.

## The crawl

[LF-094](flight-withheld-legs-v1.md) gave the learned stack every transfer
leg and found its long legs beside the sun, where the navigation law crawls
at the edge of its heat disk: of 236 transfers with the sun in the way
(`tools/sun_side.py`), the law passes 115, and the successes that take the
whole deadline lose 300 to 400 ticks there. Traced in distances from the
sun's center (`tools/sun_trace.py`), the mechanism is the chase of the
detour waypoint. The law's global route rounds the sun on a circle 294
units from its center, 70 beyond the heat radius, by a waypoint half a
radian ahead of the ship on that circle; a ship chasing a point that keeps
half a radian ahead on a circle follows a pursuit curve that settles at the
circle's radius times the cosine of the lead, 258 units, and the local
planner's waypoint, a short lead along the line to the global aim, follows
the ship in. The heat disk the law's speed limit keeps clear reaches 263 to
273 units, so from about 285 inward every heading toward the waypoint
enters the disk, the clearance along it falls from 90 to 18 units and the
commanded speed from 60 to 20; at the disk's edge the ship's own motion,
bent inward, cannot stop inside its clearance and the momentum brake stops
it; it re-accelerates along a clear heading, drifts in, and brakes again.
On one traced task the ship spent ticks 540 to 900 between 260 and 272
units from the center at 15 to 32 units per second; on another, ticks 450
to 720. The far failures are a different loss: a planet beside the sun
pushes the detour waypoint into its own disk, the route changes sides and
the ship flies back the way it came.

A variant of the law was tried for each mechanism (`tools/law_variant.py`,
`tools/law_on_tasks.py`), on the same 236 tasks: a wider circle around the
sun, 105 or 135 units beyond the heat; a waypoint at the circle's radius
over the cosine of its lead, so the chase settles on the circle, for every
body or for the sun alone; a tangent waypoint; a momentum brake that reads
the surfaces alone outside the heat disk; and a waypoint moved outward
before the route changes sides. The pass count is a poor judge of them:
the lead for every body passes 102 against 115, but 15 of the 17 tasks it
loses end within 4 units of the entry, reached and not settled before the
deadline, which the mission's handoff takes at 105 units. So the law runs
now record the first tick within 50 units of the entry
(`ticks_within_50`), and the variants are judged by it on the tasks both
reach.

| The law on the 236 sun-side transfers | Passed | Within 50 units | Beyond 300 units | Reached 50 by tick 1,200 | On the tasks both reach: at least 120 ticks sooner, later |
|---|---|---|---|---|---|
| Current law | 115 | 194 | 25 | 122 | |
| The sun's circle 105 units beyond the heat | 113 | 201 | 23 | 124 | 14, 1 |
| The lead on the sun's circle | 114 | 202 | 21 | 125 | 17, 1 |
| The lead on the sun's circle with the heat-free brake | 116 | 206 | 20 | 125 | 21, 0 |
| The heat-free brake with the 105-unit circle | 114 | 206 | 20 | 125 | 21, 0 |
| The lead on the sun's circle with the 105-unit circle | 113 | 196 | 25 | 121 | 9, 3 |
| The lead for every body | 102 | 203 | 20 | | |
| The sun's circle 135 units beyond the heat | 110 | 201 | 20 | | |
| The tangent waypoint with the 135-unit circle | 109 | 203 | 18 | | |
| The heat-free brake alone | 117 | 197 | 22 | | |
| A waypoint moved outward before the route changes sides | 114 | 192 | 27 | | |

The lead on the sun's circle with the heat-free brake is the revision:
it is never later on the tasks both reach and at least 120 ticks sooner on
21 of 193, 206 tasks end within 50 units against 194 and 20 beyond 300
against 25, and on the 96 standard transfers at seed 7700301 it passes 88
against 87, with the same 96 within 50 units and 2 at least 120 ticks
sooner. Traced on the two crawl tasks above, it keeps the ship 286 to 311
units from the sun's center at 45 to 59 units per second and reaches the
entry in 1,600 and 1,392 ticks against 1,800. The wider circle with the
heat-free brake does as much on the set, but the wider circle lengthens
every detour around the sun where the lead changes only where the chase
would settle inside the disk.

## The law revision

The law gains two parameters (`reference_v2.py`). With `sun_lead` the sun's
detour waypoint lies at its circle's radius over the cosine of its lead,
335 units from the center instead of 294 for the half-radian lead, so the
chase settles on the circle; the planets' waypoints are unchanged, since
their chase settles outside their disks, and the lead for every body lost
13 passes on the sun-side set. `bodies` now returns the disks with the
sun's index, which the routing could not tell from a disk alone. With
`heat_brake` off, the momentum brake reads the surfaces alone while the
ship is outside the heat disk; inside it the brake reads the heat as before
and the sun escape takes over. Both are on in the law, and the law run from
the module reproduces the variant run on all 236 sun-side tasks. The law
runs also record the first tick within 50 units of the goal
(`ticks_within_50`).

## The refit

The navigation network is refitted to the revised law with the LF-093
recipe (`tools/nav_fit_long.sh`): the LF-091 recipe with 96 long transfers
in the first round, 72 in each refinement round and 12 in each evaluation
set, seed 1, over the retained motor network. The law's targets differ from
the retained fit's only where a leg rounds the sun or the ship's motion
runs toward the heat from outside the disk.

| Navigation fit | Development: standard sets, long transfers | Held out | The law on the held-out set |
|---|---|---|---|
| Retained, LF-093 | 68 of 72, 3 of 12 | 69 of 72, 2 of 12 | 69 of 72, 5 of 12 |
| Revised law, seed 1 | 68 of 72, 1 of 12 | 69 of 72, 4 of 12 | 69 of 72, 5 of 12 |
| Revised law, seed 2 | 66 of 72, 3 of 12 | 68 of 72, 5 of 12 | 69 of 72, 5 of 12 |

Both refits are level with the retained fit on the standard sets and within
the long transfers' noise, with parities of 3.1e-6 and 2.4e-6 against the
Python fits; the matches choose between them.

## Whole matches

The refit is judged as LF-094 judged the publication: whole ten-minute
matches under the default settings on seeds 1 to 24, 49 to 60 and 37 to 48,
and under the 8-second asteroid stream on 37 to 48, with every leg
published (`tools/lf094_matches.sh`, the candidate export over the retained
motor through `SPACEWARS_LEARNED_NAVIGATION`). The LF-094 runs of the
retained network are the baseline; the native-only matches are the same
matches in every column.

| Whole ten-minute matches | Clear entry, seeds 1 to 24 | Clear entry, sun refit, seeds 1 to 24 | Clear entry, sun refit seed 2, seeds 1 to 24 | Clear entry, seeds 49 to 60 | Clear entry, sun refit, seeds 49 to 60 | Clear entry, sun refit seed 2, seeds 49 to 60 |
|---|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 17, 18, 13 of 48 | 17, 20, 11 of 48 | 14, 19, 15 of 48 | 8, 10, 5 of 23 | 13, 9, 2 of 24 | 10, 11, 3 of 24 |
| Typical first-leg ratio | 0.97 | 0.98 | 1.01 | 0.95 | 0.88 | 0.96 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 668.5, 707, 709 | 691, 716.5, 709 | 590.5, 772, 709 | 534, 698, 727 | 621.5, 701, 727 | 738, 830, 727 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 10, 0, 2 | 8, 2, 2 | 5, 3, 2 | 3, 2, 2 | 3, 1, 2 | 1, 2, 2 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 5.5, 6.7, 6.7 | 6.7, 6.0, 6.7 | 7.5, 6.8, 6.7 | 7.3, 5.9, 6.5 | 7.2, 6.5, 6.5 | 7.5, 5.8, 6.5 |
| Budget replans: learned seats, their native opponents, native-only seats | 5, 1, 1 | 4, 1, 1 | 3, 0, 1 | 3, 1, 1 | 5, 2, 1 | 1, 1, 1 |
| Recoveries: learned seats against their native opponents | 24 against 30 | 17 against 17 | 24 against 26 | 10 against 6 | 10 against 10 | 11 against 9 |
| Decided matches: won by the learned seat, by the native seat | 21, 27 | 25, 23 | 25, 23 | 13, 11 | 12, 11 | 15, 9 |
| Leg ticks the stack flew | 174,200 | 185,965 | 190,493 | 86,663 | 91,821 | 89,859 |
| Published ticks by entry: ordinary, clear alternate, blocked | 82516, 91481, 203 | 93501, 92312, 155 | 108358, 81854, 285 | 41093, 45332, 238 | 43632, 47883, 312 | 47622, 42019, 230 |
| Leg ticks the native controls flew after a refusal | 0 of 174,200 | 3 of 185,968 | 4 of 190,497 | 0 of 86,663 | 6 of 91,827 | 12 of 89,871 |
| Flown ticks spinning over 1.5 radians per second | 33% | 30% | 30% | 33% | 31% | 29% |

| Seeds 37 to 48, whole matches | Clear entry, default settings | Clear entry, sun refit, default settings | Clear entry, sun refit seed 2, default settings | Clear entry, asteroids 8 s | Clear entry, sun refit, asteroids 8 s | Clear entry, sun refit seed 2, asteroids 8 s |
|---|---|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 11, 9, 3 of 23 | 10, 11, 2 of 23 | 8, 10, 5 of 23 | 11, 9, 2 of 22 | 10, 11, 1 of 22 | 7, 11, 4 of 22 |
| Typical first-leg ratio | 0.94 | 0.91 | 1.04 | 0.88 | 0.85 | 0.92 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 696, 929, 778 | 586.5, 714, 778 | 729, 654, 778 | 724.5, 595, 732 | 781, 720, 732 | 751, 794, 732 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 9, 2, 4 | 1, 4, 4 | 3, 5, 4 | 5, 3, 3 | 1, 3, 3 | 6, 3, 3 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 8.2, 6.0, 6.9 | 7.7, 7.6, 6.9 | 6.6, 7.8, 6.9 | 6.5, 7.5, 7.0 | 7.0, 6.7, 7.0 | 7.4, 7.1, 7.0 |
| Budget replans: learned seats, their native opponents, native-only seats | 4, 7, 1 | 4, 8, 1 | 2, 4, 1 | 9, 4, 3 | 9, 4, 3 | 2, 2, 3 |
| Recoveries: learned seats against their native opponents | 12 against 13 | 11 against 7 | 11 against 11 | 5 against 5 | 7 against 9 | 9 against 3 |
| Decided matches: won by the learned seat, by the native seat | 15, 8 | 11, 12 | 9, 15 | 12, 10 | 11, 12 | 14, 9 |
| Leg ticks the stack flew | 127,865 | 99,848 | 107,038 | 132,123 | 82,787 | 124,517 |
| Published ticks by entry: ordinary, clear alternate, blocked | 67491, 59974, 422 | 46508, 52581, 759 | 55264, 50689, 1085 | 58362, 73140, 642 | 45374, 36762, 653 | 69553, 54727, 262 |
| Leg ticks the native controls flew after a refusal | 22 of 127,887 | 0 of 99,848 | 0 of 107,038 | 21 of 132,144 | 2 of 82,789 | 25 of 124,542 |
| Flown ticks spinning over 1.5 radians per second | 32% | 30% | 29% | 31% | 33% | 30% |

Seeds 1 to 24 select between the two seeds of the refit, as LF-093 selected
its fit, and seeds 49 to 60 validate the choice. On seeds 1 to 24 the
second seed's seats hand off 7.53 times per ten minutes against the first's
6.68 and the native-only seats' 6.74, with 5 legs over 2,000 ticks against
8 and 2 and 3 budget replans against 4 and 1, where the LF-093 network
under the same publication had 10 long legs and 5 replans; its identical
first legs are 14 faster, 19 within five percent and 15 slower of 48, a
typical ratio of 1.01 against the first seed's 0.98. On seeds 49 to 60 it
holds: 7.51 handoffs against 7.16 and 6.53, 1 leg over 2,000 ticks against
3 and 2, 1 budget replan against 5 and 1. On seeds 37 to 48 the first seed
is the better of the two, 7.71 handoffs against 6.58 and 1 leg over 2,000
ticks against 3, and under the 8-second stream the second has 6 legs over
2,000 ticks against 1 and the native-only seats' 3, so the twelve-seed
blocks vary between fits of one recipe by as much as the fits differ.

Over the 48 seeds under the default settings the retained refit's seats
hand off 7.25 times per ten minutes against the LF-093 network's 6.61 and
the native-only seats' 6.72, and 9 of their legs run past 2,000 ticks
against 22 and 8; the first seed's 7.05 and 12. The budget replans fall
from 12 to 6, where the native-only seats make 3 and the native opponents
in the same matches 11. The identical first legs are 32 faster, 40 within
five percent and 23 slower of 95, a typical ratio of 1.00 against 0.96, the
median leg is 654.5 ticks against 642 and the native-only seats' 734.5, and
the learned seats win 49 of the decided matches to 47.

The traces of seeds 1 to 24 (`tools/sun_pool.py`, `tools/entries.py`) show
the crawl reduced rather than gone. Of the one-second windows a leg flies
more than 200 units from its entry, those flown 50 to 110 units outside the
sun's radius close less than 10 units on 27 percent with the retained refit
and 23 with the first seed, against 46 with the LF-093 network, and the
time spent in that band falls from 513 windows to 369 and 306; on the legs
flying an alternate entry the slow windows fall from 33 to 25 and 26
percent. Over all flown ticks the retained refit's slow seconds are 17
percent against the LF-093 network's 18 and the first seed's 15, its slow
ticks beside the sun 16,740 against 20,220 and 14,760, and its longest run
of them 1,200 ticks against 1,260 and 960: the second seed stalls less
often beside the sun and cruises more slowly elsewhere, 23 percent of its
windows on the pilot's own entry against the first seed's 16. Every leg
tick is still the stack's, 43 percent to an alternate entry, and none parks
at another planet.

## The retained stack

The second seed of the refit replaces the LF-093 navigation network in
`crates/spacewars-ai/fixtures/learned-flight-stack-v2`; the motor network
is unchanged. `navigation.json` is a byte copy of
`nav-fit-sun-r2/policy-final.json` (sha256 `f532061c`), whose export agrees
with the Python fit within 2.4e-6 on every head of the development
evaluation. `provenance.json` records its source, evaluation and matches
with the LF-093 network as its predecessor, and the closed-loop parity test
(`crates/spacewars-ai/tests/flight_stack_parity_v2.rs`) replays the new
fit's archived development case 45, a clear-family task, in place of
LF-093's. The law the network imitates has the two new parameters; the
contract, the observation and the mission are unchanged.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the sun-side task sets and the law's runs on them (`law-sun-side`: the
first runs under `tasks-inway-<variant>`, the runs with the arrival time
under `tasks-inway2-<variant>`, and `tasks-inway2-revised`, the module's
law after the revision), the standard transfers at seed 7700301
(`law-sun/standard2-<variant>`), the refit (`nav-fit-sun-r1` with its
log), the match runs (`lf094-sun-<seeds>`, `lf094-sun-<settings>-37..48`),
the traces (`lf094-traces-sun-1..24`) and the scripts in `tools`:
`sun_trace.py` traces the law in distances from the sun, `law_variant.py`
(with the sun-lead variant), `law_on_tasks.py` and `law_sun_side.sh` run
the variants, `law_compare.py` compares two runs task by task,
`nav_fit_long.sh` runs the fit, `lf094_matches.sh` plays the matches and
`trace_seats.sh` traces the learned seats. `tools/lf095_summary.py` writes
the [summary](data/flight-sun-detour-v1-summary.json) from those runs,
and `tools/lf094_tables.py` prints the match tables above from it.
