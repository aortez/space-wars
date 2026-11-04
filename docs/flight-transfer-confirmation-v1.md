# The learned transfer leg in whole matches, and crowded object records (LF-092)

**Status: done, 2026-10-30.** The first goal is the transfer leg, flown by
the learned stack about as well as by the native transfer law. LF-091
judged it on three-minute match prefixes under the default settings; this
step plays whole matches on twelve seeds no candidate had flown, under the
default settings, with asteroid streams at mean intervals of 8 and 3
seconds, and with loose dirt. Under the default settings the stack flies
every leg tick it is handed but one, and its first legs from identical
starts and its median leg are level with the native law's, but its seats
hand off less often than the native seats (5.8 against 6.9 times per ten
minutes) and more of its legs run past 2,000 ticks (7 against 3); the two
longest stall beside the sun. With asteroids or loose dirt the
observation's 32 object records overflowed and the stack refused, so the
native controls flew 6 percent of the learned seats' leg ticks under the
8-second stream and 55 percent under the 3-second one. The observation now
keeps the planets, sun and edge and the nearest other bodies instead of
refusing, which leaves every observation within capacity, and so the
retained networks and the default-settings matches, unchanged. The network
then flies all but 15 of 186,536 leg ticks in the three crowded
conditions, its median leg is shorter than its native opponents' in each,
and under the 8-second stream its seats hand off as often as the native
seats. Handoffs are now counted against both the native seats the learned
seats replace and their native opponents in the same matches, which
corrects the "more handoffs" of the LF-091 summaries. The legs that stall
beside the sun are the next deficiency, taken up by
[LF-093](flight-long-transfers-v1.md).

## The first goal

The first goal of the learned flight work is one part of the flight: the
mission's transfer leg, from above one planet to the entry point above the
next, flown by the learned stack in place of the native transfer law and
about as well, though not necessarily the same way. LF-091 judged it on
three-minute prefixes of matches under the default settings, which leave
asteroids and loose dirt off. This step plays whole ten-minute matches on
seeds 37 to 48, which no candidate has flown, under the default settings,
with an asteroid stream at mean intervals of 8 and 3 seconds at Mixed
strength, and with round loose dirt. Asteroids are part of the goal at a
milder level than the launcher's faster streams.

The match harness takes three environment variables for this:
`SPACEWARS_MATCH_SECONDS` for the match length, `SPACEWARS_MATCH_ASTEROIDS`
for the stream's mean interval and `SPACEWARS_MATCH_DIRT` for a loose dirt
mode such as `round`. Without them it plays as before: seeds 1 to 12
reproduced the recorded run of the retained stack byte for byte.

When the stack cannot build its observation it refuses the tick, and the
pilot's own controls fly it. The fallback stays for now because it marks
the situations the stack does not cover, but the direction is a network
that flies every tick as best it can, so the share of leg ticks the native
controls fly counts against a stack.

## Whole matches

The retained stack of LF-091, as committed, against the recovery bot in
both seat orders. The first legs start from identical states, as before.
The other rows compare the learned seats with their native opponents in the
same matches, which share the world, the settings and the match length;
the handoff rates also give the native seats of the native-only matches.

| Seeds 37 to 48, whole matches | Default settings | Asteroids, 8 s | Asteroids, 3 s | Loose dirt |
|---|---|---|---|---|
| Leg ticks the native controls flew after a refusal | 1 of 65,763 | 6.3% | 55.2% | 2.4% |
| Identical first legs: faster, within 5%, slower | 6, 12, 5 of 23 | 5, 14, 3 of 22 | 4, 14, 6 of 24 | 5, 12, 6 of 23 |
| Typical first-leg ratio | 0.94 | 0.87 | 0.92 | 1.01 |
| All legs, median ticks: learned seats against their native opponents | 748 against 775 | 720 against 741 | 769 against 735 | 735 against 740.5 |
| All legs, mean ticks | 885 against 830 | 803 against 831 | 807 against 815 | 849 against 784 |
| Legs over 2,000 ticks | 7 against 3 | 6 against 4 | 3 against 1 | 8 against 3 |
| Handoffs | 102 against 121 | 135 against 131 | 95 against 102 | 100 against 104 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 5.8, 6.9, 6.9 | 7.2, 7.0, 7.0 | 5.8, 6.2, 7.0 | 6.1, 6.4, 7.1 |
| Budget replans | 6 against 3 | 12 against 5 | 3 against 3 | 3 against 3 |
| Recoveries | 17 against 13 | 11 against 15 | 13 against 16 | 12 against 11 |
| Decided matches: won by the learned seat, by the native seat | 12, 12 | 8, 14 | 11, 13 | 15, 9 |

Under the default settings the stack flies every leg tick it is handed but
one, and its first legs and median leg are level with the native law's.
Its seats hand off less often than the native seats, and seven of its legs
run past 2,000 ticks against three. With asteroids or loose dirt, the
observation's 32 object records overflow and the stack refuses; under the
3-second stream the native controls flew more than half of the learned
seats' leg ticks, so those columns measure the native law as much as the
network.

## Two ways to count handoffs

A learned seat can be compared with the native seat it replaces, from the
native-only match of the same seed, or with its native opponent in the same
match. The first asks whether the seat does as well with the network
flying; the second, whether the network keeps pace with the native law
under identical conditions. LF-091 used the first: on seeds 25 to 36 its
seats handed off 67 times against 65, over matches of nearly equal length
(9,930 and 9,760 ticks on average). Their native opponents handed off 73
times. In whole matches the swap also changes how long a match lasts:
under the default settings the matches with a learned seat ran 26,273
ticks on average against 18,550 for the native-only matches, so the raw
counts favor the learned seats (102 against 85) and the rate per ten
minutes of match time is the measure: 5.8 for the learned seats against
6.9 for both their native opponents and the native-only seats. The LF-091
report and the envelope plan described the three-minute comparison as more
handoffs; they now give both counts.

## The long legs

The two longest legs at the default settings were traced
(`trace_learned_transfer_seat`, seat 0 of seeds 41 and 43). Both lose
their time beside the sun.

The seed 43 leg took 2,707 ticks, all flown by the stack. Its attempt ran
the full 1,800-tick deadline: the ship stayed more than 900 units from the
entry point for 1,645 ticks, moving at 32 units per second on average in
the frames of planets 0 and 1, 115 to 200 units outside the mission's
routing bound around the sun, with its spin over 1.5 radians per second on
21 percent of the ticks. The next attempt closed the remaining 792 units in
906 ticks at 49 units per second.

The seed 41 leg took 3,115 ticks, of which the stack flew 1,944. For 1,171
ticks in the middle the pilot did not publish the leg while the ship flew
aboard, in ship form and climbing, most likely because LF-090's rule held
back an entry inside another planet's margin, and the native transfer law
flew. The stack then flew the last 605 units in 1,638 ticks, a closing
speed of 22 units per second at a mean ship speed of 35, 83 to 140 units
outside the sun's routing bound, with its spin over 1.5 on 41 percent of
the ticks.

The navigation law keeps clear of the heat disk and routes far goals
around the sun, so a leg whose straight line runs past the sun is where a
detour can stall; these legs are the next deficiency of the transfer.

## Crowded object records

The flight observation encodes each other moving body as one of 32 object
records and refused the tick when there were more. An asteroid stream and
its fragments exceed that, and so occasionally does loose dirt. The encoder
now keeps the planets, the sun and the arena's edge and fills the remaining
records with the other bodies whose surfaces are nearest the ship, in body
order as before. An observation within capacity is unchanged, so the
exported schema, the retained networks and the parity replays stay valid,
and the default settings, which never overflow, reproduce the run above
byte for byte. The navigation law reads only the planet, sun and edge
records and senses other bodies through its rays and polar map, but the
network pools every record it is given and has not been trained on a full
record of small bodies, so the matches judge it. The policy test that
asserted the refusal now asserts that the farthest bodies give way and the
planets stay.

| Seeds 37 to 48, whole matches | Default settings | Asteroids, 8 s | Asteroids, 3 s | Loose dirt |
|---|---|---|---|---|
| Leg ticks the native controls flew after a refusal | 1 of 65,763 | 8 of 77,786 | 0 of 50,025 | 7 of 58,725 |
| Identical first legs: faster, within 5%, slower | 6, 12, 5 of 23 | 5, 14, 3 of 22 | 4, 14, 6 of 24 | 5, 12, 6 of 23 |
| Typical first-leg ratio | 0.94 | 0.87 | 0.89 | 1.01 |
| All legs, median ticks: learned seats against their native opponents | 748 against 775 | 710.5 against 743.5 | 672 against 776 | 682.5 against 766 |
| All legs, mean ticks | 885 against 830 | 801 against 836 | 790 against 858 | 823 against 800 |
| Legs over 2,000 ticks | 7 against 3 | 6 against 4 | 5 against 2 | 8 against 3 |
| Handoffs | 102 against 121 | 132 against 130 | 93 against 104 | 104 against 96 |
| Handoffs per ten minutes: learned seats, native opponents, native-only seats | 5.8, 6.9, 6.9 | 7.0, 6.9, 7.0 | 5.6, 6.3, 7.0 | 6.5, 6.0, 7.1 |
| Budget replans | 6 against 3 | 12 against 5 | 3 against 3 | 3 against 3 |
| Recoveries | 17 against 13 | 10 against 14 | 14 against 14 | 11 against 10 |
| Decided matches: won by the learned seat, by the native seat | 12, 12 | 7, 15 | 11, 10 | 16, 8 |

With the selection the native controls flew 8 of 77,786 leg ticks under
the 8-second stream, none of 50,025 under the 3-second one and 7 of 58,725
in loose dirt, all refused for a control state outside the stack's
envelope, such as closed or swept wings or a pilot out of the ship.
Flying the crowded ticks itself costs the stack nothing measurable: the
first legs are unchanged under the 8-second stream and slightly faster
under the 3-second one (a typical ratio of 0.89 against 0.92 when the
native law flew the crowded ticks), and the learned seats' median leg is
shorter than their native opponents' in all three conditions. Under the
8-second stream, the level the goal includes, the learned seats hand off as
often as the native seats
(7.0 times per ten minutes against 6.9 and 7.0) but make more budget
replans (12 against 5) and lost more of the decided matches (7 against
15), mostly by losing their pilot (9 times against 3). Over all four
conditions the learned seats' pilots were lost 29 times and the native
seats' 32, so that split reads as the variation of twelve seeds. The tail
of long legs stays (legs over 2,000 ticks 6 against 4, 5 against 2 and 8
against 3), and the handoff rate stays below the native seats' under the
default settings and the 3-second stream.

The native controls still fly a leg in two other ways: the rare refused
control state above, and the rule from LF-090 that holds a leg back while
its entry lies inside another planet's margin, which in seed 41 handed
1,171 ticks of one leg to the native transfer law. Both stay for now as
diagnostics; the direction is a stack that flies them.

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the preflight (`confirm-preflight-1..12`), the whole-match runs of the
committed stack (`confirm-full-37..48`, `confirm-asteroids8-refusal-37..48`,
`confirm-asteroids3-37..48`, `confirm-dirt-round-37..48`), the runs with
the object selection (`select-*-37..48`), the traces (`confirm-trace`), the
test binaries the runs used with their source states (`bin`) and the
scripts (`tools/confirm_first_goal.sh`, `tools/confirm_selection.sh`,
`tools/confirm_summary.py`, `tools/same_match.py`,
`tools/confirmation_summary.py` and `tools/confirmation_tables.py`).
`tools/confirmation_summary.py` writes the
[summary](data/flight-transfer-confirmation-v1-summary.json) from those
runs, and `tools/confirmation_tables.py` prints the tables above from it.
