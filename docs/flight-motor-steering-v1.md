# The motor's steering swing, traced to the teacher and judged in the loop (LF-091)

**Status: done, 2026-10-30.** The saturated steering that
[LF-090](flight-mission-polish-v1.md) left as the next deficiency is the
motor teacher's own attitude law, not the navigation network's command noise
and not the fitted motor's imitation. It is reproduced without any network
by flying the navigation law with the scripted teacher on the practice and
transfer families, which makes a candidate steering law judgeable in about
a minute. The law that passes that judgment removes from the attitude error
the overspeed the brake is already shedding, down to crawling speeds. With
the v5 attitude gain unchanged it keeps the motor matrix at 316 of 320 and
takes the teacher in the loop from 18 to 22 of 24 transfers and from 75 to
96 of 96 practice tasks, with the share of ticks spinning above 1.5 radians
per second down from 0.44 and 0.49 to 0.25 and 0.17. Fitted and flown in
the stack, it halves the spinning share on the families and cuts it in
matches from 45 to 28 percent of flown ticks, with the legs level. The
navigation network refitted over that motor, with the planner's
straight-line waypoint, completes the retained stack. On seeds 25 to 36,
which no candidate was selected on, its first legs are level with the
native transfer law's (9 faster, 9 within five percent and 5 slower of 23),
and its seats hand off 67 times against the native seats' 65. The LF-090
stack, run on the same seeds, was faster on 3 legs and slower on 7, handed
off 63 times and refused 37 ticks.

## Measuring the swing where the motor can be judged

LF-090 found the swing in matches, where the navigation network flies the
retained motor network, and could not reproduce it in the motor's own
reference sets. The identical-start harness writes full traces
(`flight_stack_comparison --traces all`), and the learned arm's traces carry
it: on the two transfer and two practice sets the stack spins above 1.5
radians per second on 36, 38, 37 and 34 percent of its ticks, the native
transfer law on 11, 10, 21 and 21, and the native arrival law on 3 to 4
(`tools/family_spin.py` in the experiment directory).

Two further runs separate the three suspects. The navigation law, not the
network, driving the retained motor network spins 31, 30, 39 and 37
percent, so the network's command noise is not the cause (`reference_v2`
now records the spinning and saturated shares of every case from the
bridge's own trace). The navigation law driving the scripted teacher
instead of the motor network, which two new bridge requests make possible
(`set_command` holds a command without stepping and `step_actions` steps
one tick with raw actions, `reference_v2 --teacher GAINS`), spins 44, 47,
49 and 48 percent and passes only 18 and 18 transfers and 76 and 70 practice
tasks, where the network it was fitted into passes 23 and 23 and 96 and 96.
The fitted network smooths its teacher's swings; the swing is the teacher's.

That last run is the judge used from here on: the teacher in the navigation
loop, about 30 seconds for a transfer set and 55 for a practice set, with
no fit in between. Every failure it shows is a deadline at the goal, the
ship within 3 units at 1.4 to 2.4 units per second with its spin at the turn
limit: parking, not routing. The motor matrix stays as the acquisition
check, since the loop tolerates a slow acquisition that the matrix's
deadlines do not.

## What turns the nose

A per-tick tracer of the loop (`tools/loop_trace.py`: world directions of
the nose, the velocity, the command, gravity's opposite and the attitude
target, the attitude vector's terms, the law's internals, and a class for
every tick at the steering limit) shows four mechanisms.

- **Overspeed reverses the nose.** The v5 attitude vector is the error
  times a gain of 1.8 per unit per second, minus gravity, plus the course
  hold. A ship 3 units per second faster than its command along its own
  velocity has an error pointing backward, so the vector points behind the
  nose and the spin saturates toward a half turn to thrust against a speed
  the brake sheds at up to 40 units per second squared, as strong as the
  thrust, without any turn. The overspeed along the relative velocity now
  leaves the attitude error (`brake_share`), and the share stays on down to
  2.5 units per second (`brake_share_speed`), since the brake's deceleration
  is four times the relative speed; a ship at rest in the frame keeps the
  full error so a command behind it still turns the nose.
- **The goal crawl chases its own thrust.** Near the goal the navigation
  law crawls at 1 to 3 units per second. With the overspeed still in the
  attitude error at those speeds, the deceleration demand turns the nose
  away from the course; the thrust along the rotating nose stirs the
  velocity around, the error rotates with it, and the nose follows it
  through full turns at the turn limit, which is the parking failure above.
  The brake share kept down to crawling speeds removes it: 18 to 22 of 24
  transfers and 80 to 96 of 96 practice tasks between a share that fades
  under 10 units per second and one that fades under 2.5.
- **Gravity's error turns the nose, then the hover attitude turns it
  back.** In a cruise around a planet the error gravity itself is removing
  points along gravity; the attitude vector adds it to gravity's opposite,
  the two cancel, the vector's direction swings through 150 degrees as its
  magnitude passes near zero, and the nose follows. Removing the error's
  component along gravity (`gravity_share`) takes the transfers from 22 to
  13 of 24, because a launch climb's large error toward the planet is then
  left to gravity for 14 seconds; capped at what gravity removes in a second
  (`gravity_share_time`) it keeps the transfers and gains two practice tasks
  at the cost of 11 matrix passes, not worth it on top of the brake share.
- **A fixed error gain against a variable bias.** The v5 gain of 1.8 per
  unit per second overpowers a weak gravity bias (1.7 at a practice goal)
  and the course hold (2.0), so an error of 1 unit per second already turns
  the nose 45 degrees from its rest attitude. A gain relative to the rest
  bias (`attitude_relative` 0.5: the same tilt for the same error whatever
  the bias) reaches the loop's ceiling too, 22 of 24 and 96 of 96, at 300
  of 320 on the matrix; so does the fixed gain lowered to 0.03 (312 on the
  matrix, 22 and 91) and 0.02 (296, 21 and 94).

A fifth finding is the navigation law's. Its local planner returned the
first cell center of an A* path beyond the lead distance even when the
straight line to the aim was free, so the command heading was quantized to
the polar map's 64 sectors and alternated by a sector from one decision to
the next as the nose turned. The planner now returns a waypoint on the
exact goal direction at the lead radius when the line is free
(`route.straight_free`). With the retained motor network the law's spinning
share on transfers falls from 0.31 and 0.30 to 0.27 and 0.27; it passes 21
and 23 transfers against 23 and 23, the two lost being marginal parkings at
the goal, and 96 of 96 practice tasks on both seeds as before.

## The sweep

Forty-six candidate gain sets were run through the matrix, the v1 sets
and the loop on one transfer and one practice set (`tools/sweep.sh`). Every
candidate keeps the v1 sets at 312 of 312. The ones that matter:

| Candidate | Matrix | Transfers in the loop, spinning share | Practice in the loop, spinning share |
|---|---|---|---|
| v5 teacher | 314 | 18, 0.44 | 75, 0.49 |
| brake share fading under 10 units per second | 316 | 18, 0.35 | 80, 0.34 |
| plus the forward-error cut | 316 | 19, 0.34 | 80, 0.33 |
| brake share fading under 2.5 | 316 | 22, 0.25 | 96, 0.17 |
| brake share under 2.5 without the forward-error cut | 315 | 22, 0.27 | 96, 0.17 |
| brake share under 1.5 | 316 | 22, 0.24 | 96, 0.16 |
| brake share under 2.5, overspeed brake gain 0.2 | 316 | 22, 0.24 | 96, 0.19 |
| brake share under 2.5, attitude 0.03 | 312 | 22, 0.23 | 96, 0.17 |
| brake share under 2.5, capped gravity share | 305 | 22, 0.24 | 96, 0.17 |
| relative gain 0.5 | 300 | 22, 0.25 | 96, 0.18 |
| relative gain 0.5 with the brake share under 2.5 | 300 | 22, 0.24 | 96, 0.16 |
| attitude 0.03 | 312 | 22, 0.27 | 91, 0.25 |
| attitude 0.02 | 296 | 21, 0.22 | 94, 0.20 |
| hold kept under gravity | 319 | 15, 0.27 | 79, 0.32 |
| attitude ramp 0.5 to 2 | 277 | 20, 0.30 | 92, 0.29 |
| uncapped gravity share, any combination | 273 to 289 | 13, 0.08 to 0.11 | 84 to 96 |

The hold kept under gravity, LF-090's rejected candidate, has the best
matrix and the worst loop, the regression LF-090 saw after a fit; the loop
shows it before one. A soft thrust gain at small forward errors and a
free-space stop that holds its heading change nothing the brake share has
not already fixed and cost matrix passes. The retained law is the brake
share fading under 2.5 units per second with the forward-error cut: the
v5 law with two added terms and every gain unchanged.

## The fit and its selection

The retained law was fitted four times with the LF-087 recipe (the 64-wide
actor, steering weight 3), differing only in the seed, and each fit was
flown in the stack with the LF-090 navigation network unchanged
(`tools/fit_seeds.sh`). The fits vary far more than the law does: the
teacher passes 316 of the matrix, the four fits 233 to 276 on the
development matrix and 242 to 268 held out, as the v5 fits varied in
LF-087. The families select among them:

| Fit seed | Matrix, development and held out | Transfers, two seeds | Practice, two seeds | Spinning share, transfers and practice |
|---|---|---|---|---|
| retained v5 fit | 287, 299 | 20, 18 | 88, 91 | 0.36 and 0.38, 0.37 and 0.34 |
| 1 | 265, 245 | 16, 15 | 88, 88 | 0.17 and 0.16, 0.12 and 0.12 |
| 2 | 263, 260 | 18, 21 | 89, 92 | 0.19 and 0.20, 0.14 and 0.14 |
| 3 | 276, 268 | 17, 17 | 84, 88 | 0.18 and 0.19, 0.15 and 0.15 |
| 4 | 233, 242 | 18, 19 | 88, 91 | 0.19 and 0.20, 0.15 and 0.14 |

Seed 2 is retained: level with the v5 fit on the families, with the
spinning share halved on the transfers and cut to a third on the practice
tasks. The navigation law itself, flying that fit, passes 21 and 22
transfers and 96 and 96 practice tasks with spinning shares of 0.17 and
0.14 to 0.15, against 21 and 23 and 96 and 96 at 0.27 and 0.37 to 0.39
with the v5 fit.

## Matches

Each candidate motor flew the learned seats of LF-090's match comparison
on both seed ranges with the navigation network unchanged
(`SPACEWARS_LEARNED_MOTOR`, `tools/evaluate_motor.sh`). Four fits of two
laws, the retained law's seeds 1 and 2 and two fits of the relative-gain
law, agree: the spinning share of flown ticks falls from 45 and 43 percent
to 26 to 29, the identical first legs stay level with the native transfer
law, and the learned seats make 58 to 67 handoffs against the retained
stack's 55 and 61. The retained fit:

| Seeds | Faster | Within 5% | Slower | First-leg median, learned against native | First-leg mean | Handoffs | All legs, median | Spinning share |
|---|---|---|---|---|---|---|---|---|
| 1 to 12 (24 pairs) | 5 | 12 | 7 | 1216 against 1218 | 1186 against 1202 | 62 against 53 | 835 against 833 | 0.29 |
| 13 to 24 (23 pairs) | 4 | 11 | 8 | 1023 against 989 | 1098 against 1059 | 58 against 55 | 678 against 911 | 0.29 |

The per-seat handoff counts differ from the retained stack's by one either
way on most seats, which is the variation of a 180-second match after its
first leg; the spinning share is the consistent change. What remains of it
is the share the native transfer law also shows on the families, 0.09 to
0.12, plus the turns a transfer leg needs.

## The navigation refit

The navigation network imitates its law with a motor network flying under
it, and both have changed: the motor above, and the law's planner, whose
straight-line waypoint replaces the quantized cell center. The LF-090
recipe (`fit_v2 --round-transfers 72`) was rerun for two seeds with the
retained motor fit in the loop. The law's parameters are unchanged. The
second fit's process stopped during its closing evaluation. That evaluation
was rerun from the saved checkpoint (`tools/resume_nav_evaluation.py`), once
the fit's sources, bridge and motor were confirmed unchanged and the
checkpoint was shown to export the saved network byte for byte. Its first
case reproduced the interrupted run's.

| Navigation network over the retained motor | Development (72) | Held out (72) | Transfers, two seeds | Practice, two seeds | Spinning share, transfers and practice |
|---|---|---|---|---|---|
| LF-090 network | | | 18, 21 | 89, 92 | 0.19 and 0.20, 0.14 and 0.14 |
| refit seed 1 | 65 | 68 | 21, 23 | 87, 92 | 0.18 and 0.20, 0.16 and 0.16 |
| refit seed 2 | 63 | 67 | 19, 23 | 91, 91 | 0.18 and 0.19, 0.14 and 0.15 |

The law the refits imitate passes 69 of the 72 held-out tasks with the
retained motor network under it. The LF-090 network's own fit, with the
LF-087 motor in its loop, scored 62 and 63.

Each network then flew the learned seats on both seed ranges over the
retained motor. Its first legs from identical starts, against the native
seats:

| Network | Seeds | Faster | Within 5% | Slower | First-leg median, learned against native | First-leg mean | Handoffs | All legs, median | Deadlines, refused ticks, budget replans |
|---|---|---|---|---|---|---|---|---|---|
| LF-090 | 1 to 12 | 5 | 12 | 7 | 1216 against 1218 | 1186 against 1202 | 62 against 53 | 835 against 833 | 1, 0, 1 |
| LF-090 | 13 to 24 | 4 | 11 | 8 | 1023 against 989 | 1098 against 1059 | 58 against 55 | 678 against 911 | 3, 0, 0 |
| seed 1 | 1 to 12 | 4 | 12 | 8 | 1224 against 1218 | 1212 against 1202 | 61 against 53 | 847 against 833 | 0, 0, 0 |
| seed 1 | 13 to 24 | 5 | 15 | 4 | 1013 against 998 | 1099 against 1064 | 67 against 55 | 752 against 911 | 2, 9, 1 |
| seed 2 | 1 to 12 | 4 | 12 | 8 | 1236 against 1218 | 1258 against 1202 | 53 against 53 | 900 against 833 | 2, 0, 0 |
| seed 2 | 13 to 24 | 7 | 14 | 3 | 972 against 998 | 1010 against 1064 | 71 against 55 | 767 against 911 | 0, 0, 2 |

Between ranges and fits the medians and means move by tens of ticks, more
than they separate the networks. Steadier measures pooled over both ranges
(`tools/steering_summary.py`), where the typical first-leg ratio is the
geometric mean of each learned first leg over its native counterpart:

| Stack | Typical first-leg ratio | First legs over 1.25 times native | Legs over 2000 ticks | Handoffs, learned seats against native | Seats with fewer handoffs than native |
|---|---|---|---|---|---|
| LF-090 network over the LF-087 motor | 1.00 | 4 of 47 | 2 | 116 against 108 | 11 of 48 |
| LF-090 network over the retained motor | 0.99 | 2 of 47 | 5 | 120 against 108 | 11 of 48 |
| refit seed 1 | 1.00 | 2 of 48 | 1 | 128 against 108 | 4 of 48 |
| refit seed 2 | 0.99 | 4 of 48 | 6 | 124 against 108 | 10 of 48 |

Every network keeps the first legs level with the native law. The seed 1
refit is retained: it completes the most legs, falls behind its native
seat's handoffs in the fewest seats and has the fewest long legs. Its nine
refused ticks all came in one seat, the first seat of seed 17, where the host found
the ship's control state outside the stack's envelope (`unsupported flight
policy control state`). The pilot's own controls fly a refused tick, and
the leg resumes.

Selecting on both ranges favors the selection, so the retained stack also
flew seeds 25 to 36, which no candidate was chosen on, alongside the LF-090
stack:

| Stack, seeds 25 to 36 | Faster | Within 5% | Slower | First-leg median, learned against native | First-leg mean | Typical ratio | Handoffs | Longest leg | Refused ticks | Spinning share |
|---|---|---|---|---|---|---|---|---|---|---|
| LF-090 stack | 3 | 13 | 7 | 1261 against 1181 | 1259 against 1211 | 1.02 | 63 against 65 | 3285 | 37 | 0.48 |
| retained | 9 | 9 | 5 | 1212 against 1181 | 1175 against 1211 | 0.97 | 67 against 65 | 2071 | 4 | 0.30 |

The retained stack had one deadline and two budget replans on these seeds,
and the LF-090 stack none.

## The retained stack

The fixtures in `crates/spacewars-ai/fixtures/learned-flight-stack-v2` now
hold the seed 1 navigation refit over the seed 2 motor fit, with their
hashes and qualification in `provenance.json`. The v2 parity test replays
development case 45 of the navigation refit through the gameplay driver
tick for tick. That case has gravity on and a spinning planet, and takes
323 ticks. A release build with the fixtures embedded, and no candidate
exports named, reproduces the candidate's match file for seeds 1 to 12
byte for byte (`matches-retained-v2-1..12`).

## Evidence

The experiment directory
`data/training-experiments/learned-flight-mission-polish-v1-20261029` holds
the traced family runs (`stack-retained-traces`), the law runs with the
network and the teacher (`law-spin`, `law-spin-straight`, `law-teacher`,
`law-teacher-share`), the sweeps (`sweep1` to `sweep8`), the motor fits and
their stack and match runs (`motor/fit-*`, `stack-*`, `matches-*`), the
navigation refits (`nav-fit-seed1` and `nav-fit-seed2`, which keeps its
interrupted evaluation as `evaluation-development-interrupted`), their
stack and match runs (`stack-nav*-m2`, `matches-nav*-m2-*`), the validation
runs (`matches-final-25..36`, `matches-nav1-m2-25..36`), the reproduction
with the embedded fixtures (`matches-retained-v2-1..12`) and the analysis
scripts (`tools`). `tools/steering_summary.py` writes the
[summary](data/flight-motor-steering-v1-summary.json) from those runs.
