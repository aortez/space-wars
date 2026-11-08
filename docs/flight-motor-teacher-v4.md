# Motor teacher revision: lateral damping and crab v4

LF-078, 2026-10-25. Revision of the hand-written proportional law that
[LF-074](flight-motor-teacher-fit-v1.md) fitted into the plain motor actor,
followed by a refit. Exploratory under the two-tier process: scripted
reference sweeps, saved fits with per-case traces, no frozen protocol. The
requalification of the refitted actor under the LF-076 gates is the separate
frozen run [LF-079](flight-motor-polish-v2.md). Implementation:
`tools/flight_motor/teacher.py`, with the v1 to v4 laws retained as named
gain sets and tests that the scalar and batched forms agree for each.

## The gap

The v1 law damped lateral velocity only at command zero. With a speed
commanded, sideways motion was left to the brake's overspeed term, which
never engages for it. On the 200 held-out random tasks this showed as 92
successes and two collisions, both pure lateral starts near 28 units per
second that drifted around the planet. On the LF-055 velocity matrix, 104
cases with lateral and diagonal tracking starts and 60-degree turns at 6, 12
and 18, the v1 law passed 64 with six physical failures and only two of the
24 lateral-start tracking cases. Detours in navigation produce exactly this
situation, so the revision came before the navigation stage.

## Law changes, by scripted reference

Each step was checked by running the law itself through the native bridge on
four sets: the LF-073 speed-16 set, the aligned 6/12/18 set, the velocity
matrix and the held-out random set.

| Law | Speed-16 | Aligned | Velocity matrix | Held out | Lateral-start tracking | Mean tracking acquisition, matrix |
|---|---|---|---|---|---|---|
| v1 | 104/104 | 104/104 | 64/104, 6 failures | 92/200, 2 collisions | 2/24 | 113 |
| v2: brake 0.2 per unit of lateral speed | 104/104 | 104/104 | 88/104 | 197/200 | 16/24 | 284 |
| v3: velocity-error pursuit angle | 104/104 | 104/104 | 104/104 | 200/200 | 24/24 | 148 |
| v4: capped linear crab angle | 104/104 | 104/104 | 104/104 | 200/200 | 24/24 | 146 |

v2 removed every physical failure but left all sixteen level-18 tracking
cases of the matrix failing in the same way: a 0.32 lateral residual from
turning 60 degrees while accelerating, entered the band at tick 337 and
acquired at 367, seven ticks past the deadline, at every lateral gain from
0.1 to 1.0. Brake-based damping cannot remove a small residual at high speed
without also braking the forward motion, so the fix had to use the nose.

v3 turned the nose toward the direction of the body-frame velocity error
while that error exceeded one unit and called for acceleration, blending in
fully at six. Its first form also turned toward backward errors and swung
the nose around on overspeed starts, slowing aligned acquisition from 67 to
92 ticks; restricting it to errors with a forward component restored 67.
v4 replaces the pursuit angle with a crab angle of 0.15 radians per unit of
lateral velocity error, capped at 0.8 radians, with the same fade. Where the
forward error is large the two agree; where it is small the pursuit angle's
gain diverges, which the small network could not fit. A half-unit deadband
on the lateral brake term, added at v3b, gives the fit a flat zero to land on
at the target; the tolerance absorbs residuals of that size.

## Fits

All fits use the LF-074 pipeline; v3b onward use 400,000 synthetic rows,
40,000 steps then 20,000 per refinement round, three rounds, batch 4,096.

| Fit | Mean absolute error, steering/thrust/brake | Probe slope and bias at 16 | Speed-16 acquisition fast/rest/reverse/slow | Max post-acquisition error, speed-16 | Fixed sets | Seconds |
|---|---|---|---|---|---|---|
| v1 (LF-074) | 0.004 / 0.006 / 0.011 | −4.23, −0.02 | 61 / 67 / 80 / 56 | 0.24 | 104, 104 | 65 |
| v3 | 0.037 / 0.011 / 0.031 | −2.38, −1.61 | 64 / 84 / 100 / 71 | 0.76 | 104, 104, 104 | 71 |
| v3b, deadband and longer schedule | 0.028 / 0.007 / 0.018 | −3.15, +0.16 | 67 / 66 / 80 / 55 | 0.56 | 104, 104, 103 | 297 |
| v4 | 0.020 / 0.005 / 0.014 | −3.56, +0.01 | 62 / 69 / 82 / 58 | 0.38 | 104, 104, 104 | 134 |

The v3 network passed every case while carrying a brake of 0.053 at the
target, a bias of −1.6 units of acceleration, and a post-acquisition wobble
of 0.76 against the 0.5 gate of LF-076; the pursuit angle consumed the
network's capacity. v4 fits as cleanly as the law allows: target bias near
zero, acquisition within two ticks of the law, wobble 0.38, and the velocity
matrix at 104/104 with acquisition from 128 to 181 ticks. Its post-acquisition
error on the matrix is 0.86 for the network and 0.84 for the law itself, so
that set stays a retention guard rather than a precision measure.

## Interpretation

The hand-written law now covers lateral and turning starts across the
envelope, and the fitted actor reproduces it. Two lessons for the navigation
teacher: check every law change by scripted reference on all sets before
fitting, and prefer forms with bounded, state-independent gains, since the
fit's precision at the target is what the gates measure. Fit artifacts are
under `data/training-experiments/learned-flight-motor-teacher-fit-v3-20261025`,
`-v3b-` and `-v4-`; the v4 checkpoint is the warm start for LF-079.
