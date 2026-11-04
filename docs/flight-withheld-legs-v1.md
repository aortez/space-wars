# Flying the withheld legs (LF-094)

**Status: done, 2026-11-01.** LF-093 left two of every five transfer ticks
of a learned seat to the native transfer law, because the mission published
a leg only while its entry lay clear of the other planets' margins, the
sun's and the boundary's. The mission now publishes every leg: the pilot's
own entry while it is clear, otherwise the clear entry nearest the ship
among 32 bearings around the destination, which the stack can fly to and
the handoff can fire from. The stack flies every leg tick, 53 percent of
them to such an alternate, and parks at no planet. Over 48 seeds under the
default settings its seats hand off 6.61 times per ten minutes against the
native-only seats' 6.72, its median leg is 642 ticks against their 734, and
from identical starts it flies the 20 first legs beside the sun at a
typical ratio of 0.99 to the native law; 22 of its legs run past 2,000
ticks against 8, where the law crawls at the edge of its heat disk. The law
passes 115 of 236 transfers with the sun in the way, and none of ten
variants of its sun routing and braking passes more, so the networks are
unchanged and the crawl is the next deficiency, taken up by
[LF-095](flight-sun-detour-v1.md).

## The withheld legs

[LF-093](flight-long-transfers-v1.md) found that two of every five transfer
ticks of a learned seat were never the stack's to fly. The mission published
a transfer leg only while its entry, 85 units above the destination on the
line toward the ship, lay clear of the other planets' margins (their radius
and 105 units), the sun's (its radius and 110) and the boundary's (20 units
inside it); otherwise its own transfer controls flew the tick. The rule came
from [LF-090](flight-mission-polish-v1.md), where the stack, given an entry
that another planet's orbit had carried into that planet's margin, flew to
it and parked on that planet's surface, where the handoff cannot fire. On
seeds 1 to 24 the rule withheld 39 percent of the learned seats' transfer
ticks, 20 for the sun and 18 for another planet, and 163 of their 355
transfer attempts were never published at all. The direction is a stack
that flies every leg, with native control a diagnostic and not a fallback,
so those legs are the next to fly.

## A clear entry for every leg

The pilot's own flight does not need its entry clear: its detours round the
bodies in the way, and the mission's arrival rule fires within 105 units of
the destination's radius at a relative speed under 18, from any bearing. The
stack flies to the point it is given, so the leg now takes a clear one. The
pilot keeps its own entry while that is clear; otherwise the leg takes the
clear entry nearest the ship among the 32 bearings around the destination,
the search the mission's transfer approach already makes for escape travel
(`mission_transfer_approach.rs`), and keeps that bearing while it stays
clear; when no bearing is clear, the leg takes the pilot's entry anyway and
counts the tick as blocked. The pilot's telemetry counts the published ticks
by entry (`learned_entries`), the match record carries them for the learned
seats, and the per-tick trace names each leg tick's entry (`entry`). The
native-only matches are unchanged; every match with a learned seat differs.

On seeds 1 to 24 every transfer tick of the learned seats is now published:
47 percent to the pilot's entry, 53 to a clear alternate, 0.1 blocked. The
alternate's share exceeds the withheld share because a leg keeps its
alternate bearing after the pilot's entry clears. No published tick parks the
ship at another planet.

## Whole matches

The same seeds and settings as LF-093's selection, validation and
confirmation, the LF-093 stack with its legs withheld against the same stack
given a clear entry for every leg (`tools/lf094_matches.sh`). The
native-only matches of each block are identical to LF-093's.

| Whole ten-minute matches | Withheld, seeds 1 to 24 | Clear entry, seeds 1 to 24 | Withheld, seeds 49 to 60 | Clear entry, seeds 49 to 60 |
|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 13, 23, 11 of 47 | 17, 18, 13 of 48 | 7, 14, 3 of 24 | 8, 10, 5 of 23 |
| Typical first-leg ratio | 0.97 | 0.97 | 0.96 | 0.95 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 707, 651, 709 | 668.5, 707, 709 | 524, 631, 727 | 534, 698, 727 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 3, 2, 2 | 10, 0, 2 | 1, 0, 2 | 3, 2, 2 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 6.4, 7.0, 6.7 | 5.5, 6.7, 6.7 | 6.5, 6.2, 6.5 | 7.3, 5.9, 6.5 |
| Budget replans: learned seats, their native opponents, native-only seats | 1, 0, 1 | 5, 1, 1 | 2, 0, 1 | 3, 1, 1 |
| Recoveries: learned seats against their native opponents | 23 against 26 | 24 against 30 | 11 against 8 | 10 against 6 |
| Decided matches: won by the learned seat, by the native seat | 24, 24 | 21, 27 | 12, 11 | 13, 11 |
| Leg ticks the stack flew | 105,564 | 174,200 | 45,289 | 86,663 |
| Published ticks by entry: ordinary, clear alternate, blocked | not recorded | 82516, 91481, 203 | not recorded | 41093, 45332, 238 |
| Leg ticks the native controls flew after a refusal | 11 of 105,575 | 0 of 174,200 | 4 of 45,293 | 0 of 86,663 |
| Flown ticks spinning over 1.5 radians per second | 31% | 33% | 31% | 33% |

| Seeds 37 to 48, whole matches | Withheld, default settings | Clear entry, default settings | Withheld, asteroids 8 s | Clear entry, asteroids 8 s |
|---|---|---|---|---|
| Identical first legs: faster, within 5%, slower | 7, 12, 4 of 23 | 11, 9, 3 of 23 | 6, 13, 3 of 22 | 11, 9, 2 of 22 |
| Typical first-leg ratio | 1.04 | 0.94 | 0.97 | 0.88 |
| All legs, median ticks: learned seats, their native opponents, native-only seats | 709, 810, 778 | 696, 929, 778 | 638, 550, 732 | 724.5, 595, 732 |
| Legs over 2,000 ticks: learned seats, their native opponents, native-only seats | 3, 3, 4 | 9, 2, 4 | 5, 4, 3 | 5, 3, 3 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 6.8, 6.1, 6.9 | 8.2, 6.0, 6.9 | 7.9, 7.4, 7.0 | 6.5, 7.5, 7.0 |
| Budget replans: learned seats, their native opponents, native-only seats | 5, 3, 1 | 4, 7, 1 | 7, 2, 3 | 9, 4, 3 |
| Recoveries: learned seats against their native opponents | 10 against 8 | 12 against 13 | 8 against 4 | 5 against 5 |
| Decided matches: won by the learned seat, by the native seat | 10, 14 | 15, 8 | 11, 12 | 12, 10 |
| Leg ticks the stack flew | 52,617 | 127,865 | 56,505 | 132,123 |
| Published ticks by entry: ordinary, clear alternate, blocked | not recorded | 67491, 59974, 422 | not recorded | 58362, 73140, 642 |
| Leg ticks the native controls flew after a refusal | 30 of 52,647 | 22 of 127,887 | 15 of 56,520 | 21 of 132,144 |
| Flown ticks spinning over 1.5 radians per second | 31% | 32% | 32% | 31% |

The stack now flies two thirds more leg ticks, and over the 48 seeds under
the default settings its seats hand off 6.61 times per ten minutes against
6.53 with the legs withheld and the native-only seats' 6.72; its median leg
is 642 ticks against 672 and the native-only seats' 734.5; its identical
first legs are 36 faster, 37 within five percent and 21 slower of 94, a
typical ratio of 0.96 against 0.99; and the learned seats win 49 of the
decided matches to 46, where they won 46 to 49 before. What it does not do
is finish the hardest legs: 22 of its legs run past 2,000 ticks against 7
with the legs withheld and the native-only seats' 8, and it makes 12 budget
replans against 8 and 3.

The traces of seeds 1 to 24 (`tools/entries.py`, `tools/sun_pool.py`) show
where. Of the one-second windows a leg flies more than 200 units from its
entry, 33 percent close less than 10 units when the leg flies an alternate
entry, against 18 percent on the pilot's own; 83 of the 136 attempts that
fly an alternate end at the handoff, 37 in a hunt and 4 in a replan, and
30 of them run past 1,200 ticks against 11 of the 117 on the pilot's entry.
The slow windows lie beside the sun: 46 percent of the windows flown 50 to
110 units outside the sun's radius make no progress, 15 to 22 percent
elsewhere. That band is the edge of the navigation law's heat disk, where
LF-093 traced the law's own crawl: its detour circle around the sun lies 70
units beyond the heat radius, a ship chasing a waypoint half a radian ahead
on that circle settles inside the disk its speed limit keeps clear, and the
limit holds it to a few units per second. The withheld legs were the legs
to planets orbiting near the sun, and giving them to the stack brings the
crawl into a quarter of its flying.

## The crawl at the heat

The law was measured on the legs the stack now flies. Of 576 standard and
long transfers drawn at the LF-093 seeds, 445 have their entry within 600
units of the sun's center, and in 236 the straight line from the start
passes within 330 of it, where the sun is in the way (`tools/sun_side.py`).
The law, flying the retained motor network, passes 115 of those 236, 66 of
the 77 standard ones and 49 of the 159 long ones, at a mean of 1,315 ticks
for a median start of 599 units; 194 end within 50 units of the entry and
25 more than 300 away. Traced decision by decision (`tools/law_trace.py`),
the successes that take the whole deadline lose 300 to 400 ticks at the
edge of the heat disk: the ship chasing the detour waypoint settles inside
the disk, as LF-093 found, and from 6 to 18 units outside it every heading
toward the waypoint enters the disk, so the speed limit holds the ship to
8 to 22 units per second, with the momentum brake's stops among the
decisions, for hundreds of ticks. The far failures
reverse: a planet beside the sun pushes the detour waypoint into its own
disk, the route changes sides, and the ship flies back the way it came.

| The law on the 236 sun-side transfers | Passed | Within 50 units | Beyond 300 units |
|---|---|---|---|
| Current law | 115 | 194 | 25 |
| The sun's circle 105 units beyond the heat | 113 | 201 | 23 |
| The sun's circle 135 units beyond the heat | 110 | 201 | 20 |
| The waypoint at the circle's radius over the cosine of its lead, so the chase settles on the circle | 102 | 203 | 20 |
| The lead with the 105-unit circle | 101 | 195 | 28 |
| The tangent waypoint with the 135-unit circle | 109 | 203 | 18 |
| The momentum brake reading the surfaces alone outside the heat disk | 117 | 197 | 22 |
| A waypoint moved outward before the route changes sides | 114 | 192 | 27 |
| Both | 116 | 196 | 24 |
| The heat-free brake with the 105-unit circle | 114 | 206 | 20 |

None changes the law enough to refit for. The brake was not the mechanism:
reading the surfaces alone outside the disk passes two more tasks and
leaves the successes' time unchanged, so the ship crawls on the candidate
speed limit alone, which holds every heading that enters the disk. The
detour variants move the failures between tasks, as they did on LF-093's
long transfers; the 105-unit circle with the heat-free brake ends 206 tasks
within 50 units of the entry against 194 and 20 beyond 300 against 25, at
least 50 units closer on 25 tasks and farther on 3, and is the candidate
when the law is next revised. The network keeps the current law.

The matches say the stack is about as slow as the native law on these legs.
Of the 48 identical first legs on seeds 1 to 24, the stack's leg took an
alternate entry on 20; its typical ratio to the native seat on them is
0.99, 8 faster, 5 within five percent and 7 slower, with a median of 1,338
ticks against the native seats' 1,231, and 0.96 on the 28 that kept the
pilot's entry. The legs beside the sun are long for both laws; the stack's
tail is longer, 2,798 ticks against 1,840 at seed 4, seat 1, from the
crawl.

## The retained stack

The networks are unchanged: the LF-093 navigation network over the LF-091
motor network, in the v2 fixtures. What changes is the mission's
publication, so every transfer tick of a learned seat is the stack's, with
a clear entry where the pilot's is blocked, and the trace's `withheld`
column names only the control-state conditions that remain. The native
controls still fly a refused tick, 0 to 22 of 86,663 to 174,200 leg ticks
in these runs, every one for a control state outside the stack's envelope.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the preflight of the committed LF-093 stack (`lf094-preflight-1..12`, byte
for byte LF-093's run), the match runs (`lf094-entry-<seeds>`,
`lf094-entry-<settings>-37..48`), the traces (`lf094-traces-entry-1..24`),
the sun-side task sets and the law's runs on them (`law-sun-side`), the
test binaries with their source states (`bin/engine_client-lf093-head`,
`bin/engine_client-lf094-entry`) and the scripts in `tools`:
`lf094_matches.sh` plays the matches, `native_identical.py` checks the
native-only matches against LF-093's, `trace_seats.sh` traces the learned
seats, `entries.py` and `sun_pool.py` read the traces, `sun_side.py` draws
the sun-side set, `law_on_tasks.py`, `law_sun_side.sh` and `law_variant.py`
(with the lead, heat-free brake and outward variants) run the law on it,
`law_trace.py` traces it, and `nav_fit_long.sh` holds the LF-093 fit recipe
for the refit a better law would get. `tools/lf094_summary.py` writes the
[summary](data/flight-withheld-legs-v1-summary.json) from those runs, and
`tools/lf094_tables.py` prints the match tables above from it.
