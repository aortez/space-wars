# Flight envelope probe v1

LF-086, 2026-10-28. An exploratory probe of the integrated
[learned flight stack](flight-stack-integration-v1.md) and the game's native
flight laws outside the stack's qualified envelope, run to decide how to
extend that envelope toward
[general matches](design/learned-flight-match-envelope-v2.md). No training,
no frozen protocol: the identical-start harness with its `generated`
envelope keeps every admitted family of the C3 generator with the
generator's own seeded gravity and planet spin, including the moving-goal
family the v1 stack never saw. Two generation seeds, 96 tasks each.

| Group | Tasks | Learned stack | Arrival reference | Mission transfer |
|---|---|---|---|---|
| static families, gravity off | 74 | 73 | 18 | 28 |
| static families, gravity on | 70 | 14 | 1 | 4 |
| moving goals, both | 48 | 0 | 21 | 31 |
| planet spin on, all | 111 | 50 | 21 | 49 |
| all, seed 7700002 | 96 | 44 | 18 | 33 |
| all, seed 7700003 | 96 | 43 | 16 | 30 |

Planet spin at the generator's ±0.015 radians per second changes nothing
for the stack: with gravity off it passes 73 of 74 static tasks whether the
planet spins or not. Gravity breaks it: 14 of 70, almost all deadlines, the
ship unable to hold a point when the brake alone leaves a sink rate above
the velocity tolerance. Moving goals break it completely: 0 of 48, since the
law has no pursuit term and the network never saw a goal with velocity.
The native transfer law, which carries gravity and target-velocity
feedforward, passes 35 of 90 tasks with gravity on and 31 of 48 moving
goals, and the native arrival law passes 21 moving goals; both still
collide in the obstacle families.

Practice gravity on the generator's planet runs from 1.4 units per second
squared at altitude 150 to about 10 at the carved-passage altitudes of 16 to
21. A probe of five generated match worlds (`match_envelope_probe`) found
planet radii from 18 to 144 units, orbital speeds from 12 to 21 units per
second, planet spin up to ±0.037 radians per second, gravity at the surface
of 12 to 18 and at the mission's transfer altitude of 85 between 1.2 and 7.6,
a sun of radius 200 with a heat hazard beyond it, and a circular boundary of
radius 730 to 1,070. The practice planet's gravity range therefore covers
the match range at transfer altitudes.

Results and the failing traces are under
`data/training-experiments/learned-flight-envelope-probe-v1-20261028`, with
both summaries copied to
[docs data](data/flight-envelope-probe-v1-summary.json).

## Addendum, LF-088

Two later changes alter what this probe would read today. The comparison
harness issued the learned arm's goal once, at its starting position with the
task's velocity, so a moving goal went stale for the driver while the
environment's target moved on; it now issues the environment's current target
every tick, as a host does. And goals above a spinning planet now rest in the
planet's spinning frame, under a new trace version, instead of staying fixed
in world axes. Rerun with both changes on the same generated set, seed
7700002, the v1 stack arrives on 34 of 96: 9 of 24 clear, 11 obstructed, 14
altered and none of the moving family, 23 of 44 with gravity off against 11
of 52 with it on, and 27 of 57 with the planet still against 7 of 39 with it
spinning. Gravity and moving goals remain the envelope's breakers; spin,
harmless to a world-fixed goal, now matters because the goal rides the
surface the v1 stack does not command against. The rerun is archived under
`data/training-experiments/learned-flight-navigation-fit-v2-20261028/comparison/v1-stack-generated-7700002`.
