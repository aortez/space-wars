# Navigation v2: the match envelope

LF-088, 2026-10-28. The navigation step of the
[match envelope v2 plan](design/learned-flight-match-envelope-v2.md): a v2
command contract for the planet-frame motor of
[LF-087](flight-motor-teacher-v5.md), goals that rest in a planet's spinning
frame, a match-transfer task family on generated arenas, a scripted law for
all of it, and a supervised fit of that law into the C3 navigation network.
The law arrives on every practice task of two seeds and on 45 of 48
transfers; the fit's numbers are in the last section. Implementation:
`learned_navigation.rs` (contract, loader, deadband, the goal-relative
conversion), `flight_stack.rs` and `flight_navigation_bridge.rs` (the
conversion at the two hosts), `flight_environment.rs` (frame goals),
`generation/transfer.rs` (the family), `tools/flight_navigation/reference_v2.py`
(the law) and `fit_v2.py` (the fit), with every result under
`data/training-experiments/learned-flight-navigation-fit-v2-20261028`.

## The contract

A v1 command was a heading and a speed to 20 in the pilot's planet frame. A
v2 command (`navigation-command-v2`, export format 5) is the ship's desired
velocity relative to the goal: a heading relative to the ship's heading at
the decision tick and a speed to 60. The host adds the goal's velocity
relative to the pilot's planet frame exactly, from the goal it issued and the
frame velocity it observes, before the motor sees the command. A speed under
half a unit is a stop. Two fits taught the two rules. Under the first v2
contract the network had to encode a moving goal's velocity in its own output
to park at it, and the arrival tolerance of one unit per second is a sixtieth
of the output range: eleven of twelve held-out moving-goal tasks ended in a
wobble at the goal. The deadband turns a policy's near-zero output into a
stop the motor brakes to, while the law's crawl into the arrival radius, at
under a unit per second, still passes; a deadband of one or two units, tried
with an approach floor above it, either crept at the velocity tolerance's
edge or circled a stop disk it never entered. The direction pair is floored
at an eighth so a collapsed pair is a forward command rather than a refusal.

## Goals rest in the spinning frame

The motor commands velocities relative to the pilot's planet frame, which
spins with the planet, and the physical brake decelerates toward that frame.
A goal fixed in world axes above a spinning planet therefore moves at up to
seven units per second in the frame the ship parks against, and the first
transfer runs ended in a parking wobble: the ship could match the goal's
speed but never settle within a unit of it. A transfer goal is now the task's
offset from its planet's center, turned with the planet's spin since the
start and carried by its orbit; a practice goal above a spinning planet is
placed from the ship and then rides the surface likewise, under
`FLIGHT_SPINNING_ENVIRONMENT_VERSION`, so traces of the earlier world-fixed
goals do not replay. A goal above a still planet keeps the original
arithmetic, and the v1 parity replay is unchanged. The practice admission
checks the arc such a goal sweeps over the deadline, so a spinning planet no
longer carries a goal through a structure.

## The match-transfer family

A transfer starts in open-wing flight 60 to 100 above one planet of a
generated standard arena, climbing at 5 to 18 with some drift and spin, and
its goal is the mission's entry point above another planet: 85 above the
base radius, or 25 above the highest terrain when that reaches higher, never
above the handoff altitude, on the side facing the start planet. Admission
sweeps every planet along its orbit for the whole deadline and rejects a goal
that comes within 30 of another planet's terrain or of the sun's heat, since
a planet orbits into a hovering ship as surely as the ship flies into it,
and rejects starts inside the heat and distances outside 100 to 600. Seeds
7700101 and 7700102 give two sets of 24; the sun, the boundary and the other
seat's ship are present.

## The law

The v2 law keeps the v1 structure, a ring of candidate headings scored by
their free reach toward a routed aim, and adds what the probe of LF-086 and
the transfer runs demanded. Every quantity is measured against the right
body. The pilot's planet frame is a planet the pilot belongs to, not always
the one the ship is near, and its velocity at the ship, orbit plus spin, can
reach thirty units per second; the law reads the surfaces' clearance along
the ship's motion relative to the nearest surface, the sun's along the world
motion, and expresses its escapes as world velocities. One transfer hovered
at the heat's edge for seven hundred ticks because its outward escape,
commanded in the frame, cancelled exactly against the frame's own motion
toward the sun.

- The approach is planned in the goal's frame and crawls into the stop
  radius under the velocity tolerance; the command is the pursuit relative to
  the goal, which the host completes.
- Clearances are read along the world motion a command produces, bent by
  gravity for half a second, with margins that widen with speed.
- The momentum stop: when the motion the ship has cannot stop inside its
  clearance at the planning deceleration, the command is a stop against the
  surface, with the nose turned toward the free way on.
- A moving ship pays for turning away from the approach it already flies, a
  fraction of the aim's distance at a half turn, so two detours of equal
  reach do not take turns from one decision to the next and spin the nose.
- The fall guard climbs away from the nearest surface, riding that surface,
  as the time to reach it under the gravity there drops through two to four
  seconds, and fades out within sixty of the goal.
- Inside the sun's inflated heat disk the law leaves outward in the world;
  outward headings from inside the disk are free, inward ones blocked.
- Far goals are routed around the planet and sun disks of the object record
  with the mission's margins; behind a body the side to round follows the
  way the ship already moves, and a waypoint that lands inside another
  body's disk rounds the other way.

| Set | Passed | Mean ticks of arrivals |
|---|---|---|
| practice, seed 7700002: clear, obstructed, altered, moving | 24, 24, 24, 24 of 24 | 668, 925, 793, 678 |
| practice, seed 7700003 | 24, 24, 24, 24 of 24 | 728, 919, 771, 575 |
| transfers, seed 7700101 | 22 of 24 | 992 |
| transfers, seed 7700102 | 23 of 24 | 1109 |

The three transfer failures are one planet whose orbit crosses the heat,
where the ship leaves the heat but loses the goal (two cases of the same
arena, one of them stopped 17 from the goal), and one hover at the entry
point with a spin of 0.7, the motor fit's precision under gravity.

## The fit

The LF-083 pipeline on the v2 contract, with the law labelling every row:
384 perturbed law episodes on 96 transfers and 72 tasks of each practice
family, then four rounds of 192 episodes flown by the network itself and
labelled by the law, each round refitting from the accumulated rows. The
speed loss adds the error in units, squared over ten, to the v1
cross-entropy, with a stop needing only a speed under a quarter unit, and
rows inside the hold radius weigh three times, because arrival is decided
there by commands the tolerances leave little room for. Two rounds reached
58 and 56 of 72; the network's own rounds are what close the gap, since they
label the states the network actually reaches, and four reached 65 and 66.

| Evaluation | Transfers | Clear | Obstructed | Altered | Moving | Parity |
|---|---|---|---|---|---|---|
| development: transfers 7700101, practice 7700002 | 19/24 | 12/12 | 10/12 | 12/12 | 12/12 | 3.8e-6 |
| held out: transfers 7700102, practice 7700003 | 22/24 | 11/12 | 11/12 | 11/12 | 11/12 | |
| the law on the held-out set | 23/24 | 12/12 | 12/12 | 12/12 | 12/12 | |

Of the thirteen failures, eight are parking blur: the ship at 2.5 to 4.5
from the goal with a velocity or a spin just over tolerance, where the
network's copy of the law's step at the stop radius is a fraction of a unit
off. Three are collisions in cluttered geometry before tick 500, where the
fit's heading error is largest, and two are the transfers the law itself
loses near the planet whose orbit crosses the heat. The export is the
navigation of the v2 stack under `fixtures/learned-flight-stack-v2`.

## Identical starts through the gameplay driver

`flight_stack_comparison` flies the retained v2 stack through
`FlightStackDriver` against the two native laws from identical starts: the
guidance component's arrival law and the mission's transfer law. Its learned
arm had issued each goal once, at the starting position with the task's
velocity, so a moving goal went stale for the driver while the environment's
target moved on; it now issues the environment's current target every tick,
as a host does, and the driver's arrivals match the bridge's exactly. The
`flight_stack_parity_v2` test replays an archived evaluation case through
the driver with the goal issued the same way and requires the same actions
and frames.

| Set | Learned stack | Mission transfer law | Arrival law | Shared passes, learned minus mission ticks |
|---|---|---|---|---|
| transfers 7700101, 24 | 19 | 13 | 0 | 11 shared, 11 ticks faster, faster on 8 |
| transfers 7700102, 24 | 22 | 11 | 0 | 11 shared, 363 ticks faster, faster on all |
| practice 7700002, 96 | 91 | 18 | 18 | 18 shared, 98 ticks faster, faster on 12 |
| practice 7700003, 96 | 89 | 13 | 15 | 12 shared, 129 ticks faster, faster on 8 |

Against the arrival law on the practice sets the learned stack is 638 and
644 ticks faster on the shared passes. The comparisons, the law runs and
every fit are archived with the harness binary and its hash under the
experiment directory; the summary is copied to
[docs data](data/flight-navigation-v2-summary.json).

## What remains

The mission binding that followed, [LF-089](flight-mission-binding-v1.md),
makes the stack reachable from a match: the mission issues the entry point
as a goal each tick, switches the ship to learned flight control for the
leg, and takes control back at its handoff, selectable as the launcher's
learned flight bot. Two law gaps are known: a planet whose orbit crosses the sun's heat, where the
ship leaves the heat but loses the goal, and the motor fit's hover spin at
an entry point under gravity. Polishing stays off the path until a measured
deficiency calls for it.
