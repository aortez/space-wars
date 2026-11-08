# Motor teacher v5: the planet-frame contract

LF-087, 2026-10-28. The motor step of the
[match envelope v2 plan](design/learned-flight-match-envelope-v2.md): a v2
motor contract that commands heading and speed relative to the pilot's planet
frame under gravity and spin at speeds to 60, a scripted teacher law that
flies it, and a supervised fit of that law into a motor network. The law
passes 314 of 320 cases of a gravity matrix and all 312 cases of the v1
development sets under the v2 gate; the fitted network passes 287 and 299 of
320 on the development and held-out matrices and all 312 v1 cases, with a
native-inference parity of 1e-5. Implementation: `motor.rs` (contract,
inputs, arena tasks), `learned_motor.rs` (loader, the 64-wide actor),
`tools/flight_motor/teacher_v2.py` (law, matrix, fit), with every result under
`data/training-experiments/learned-flight-motor-teacher-v5-20261028`.

## The contract

The v1 motor command was a world heading and a speed to 20 in a stationary
zero-gravity world, and arrival required the heading too. The physical brake
decelerates the ship toward the point velocity of the pilot's planet frame,
so the v2 command is relative to that frame: the same eighteen inputs with
the two velocities taken relative to the frame and scaled by 70, the
command speed to 60, gravity as before. Arrival is relative velocity within 1
for thirty complete intervals, with spin within 0.2 at a zero-speed command
and within 0.5 while moving; retention after acquisition judges velocity
only. The heading requirement is gone because holding a point under gravity
tilts the nose against it, and holding an attitude against a planet's gravity
while cruising around it means turning. v2 tasks fly the practice planet with
gravity and spin, or any planet of a generated match arena, where the
surface gravity reaches 18 and orbital frames move at 12 to 21 units per
second. Every bridge, loader and the gameplay runtime dispatch on the
contract; v1 exports and traces are unchanged, and the v1 parity test still
passes.

## The law

The stopping branch is the v4 law: brake in proportion to speed, face the
commanded heading, and under gravity turn against it and hold the ship on
thrust. The moving branch is new. The nose steers toward an attitude vector:
a slow gain, 1.8 per unit of velocity error, times the error, minus gravity,
plus a course-hold term of 2 units per second squared along the commanded
course that fades out under gravity. With no demand the nose rests on the
course; under gravity it rests against gravity; an error turns it
continuously, with no thresholds to hunt across, and the slow gain tilts it
toward gravity's opposite while the error is still large instead of swinging
there after the error is gone. Thrust is the projection of the required
acceleration onto the nose: the v4 proportional term plus a feedforward of the
gravity component along the nose. The brake fires on the component of the
error that braking removes, on backward motion, and on drift across the
course, the last fading out under gravity, where braking the drift throws away
the speed along the course and lets the ship sink.

Fifteen versions of this branch were run as references before the fit. The
switching formulations hunted at the spin tolerance or stalled; a course hold
referenced to the current nose walked the nose around a residual error; an
attitude pursued at the thrust gain swung the nose through up to 155 degrees
after acquisition and lost the velocity to gravity on the way. The final law
has none of these modes.

| Reference set | Passed | What fails |
|---|---|---|
| v2 matrix: gravity off and at altitudes 150, 100, 60 and 30, spin on and off, speeds 0, 10, 30 and 60, four starts, two headings | 314/320 | four acquisitions 2 to 87 ticks past deadlines sized to keep fast commands inside the practice world, two retention exits at 1.1 and 1.4 in the spinning frame under the strongest gravity |
| v1 development sets under the v2 gate: speed 16, aligned, lateral | 312/312 | |

## The fit

The LF-078 pipeline on the v2 contract: 400,000 synthetic rows with gravity
to 18, 300 perturbed teacher episodes on random v2 tasks, a third of the
gravity ones above arena planets, then three rounds of 200 episodes on the
network's own states, with the law labelling everything. The 32-unit actor
fitted the law unevenly across runs, between 236 and 287 of 320 on the
matrix for the same law, so the fit uses a 64-unit actor
(`motor18-actor64x64-critic32x32-v4`, format 4) with the steering term
weighted three times in the loss. Its mean absolute errors against the law
are 0.059 steering, 0.019 thrust and 0.058 brake.

| Evaluation | Passed | Parity |
|---|---|---|
| matrix, development seed | 287/320 | 8.8e-6 |
| matrix, held-out seed | 299/320 | 1.0e-5 |
| v1 development sets under the v2 gate | 312/312 | 7.6e-6 |

Of the 21 held-out failures, 18 are retention exits after acquisition with
final velocity errors between 1.05 and 1.25, most at speed 60 under gravity,
and 3 never acquire; the gap to the law is the fit's precision, not the
law's. The export is the motor of the v2 stack:
`data/training-experiments/learned-flight-motor-teacher-v5-20261028/fit/policy-final.json`.
