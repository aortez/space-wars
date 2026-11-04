# Frozen requalification of the revised motor teacher with anchored steering v3

LF-080, 2026-10-25. Frozen before any run, with hashes in the experiment's
`plan.json`; results are appended below. This repeats [LF-079](flight-motor-polish-v2.md),
which failed in two of three seeds by steering drift, with one declared
recipe element added: a supervised steering anchor.

## Recipe change and pilot evidence

Polishing keeps steering at its learned mean and samples only the actuators,
so nothing in PPO pulls steering back when the shared layers move it. The
anchor adds a supervised term to the PPO loss on every minibatch: the squared
difference between the policy's mean steering and the v4 teacher law's
steering at the same visited states, scaled by a weight. Implementation:
`steering_anchor` in `tools/flight_motor/pilot.py`, with the `auxiliary`
hook added to `tools/flight_ppo/ppo.py`.

Unfrozen pilot, six cells from the v4 fit, six-tick holds, seeds 17/29/43,
LF-073 speed-16 set only:

| Anchor weight | Seed | Passed, final | Max final heading error on stopping cases | Speed acquisition fast/rest/reverse/slow | Post-acquisition error |
|---|---|---|---|---|---|
| 1 | 17 | 104/104 | 0.3° | 42 / 66 / 79 / 54 | 0.34 |
| 1 | 29 | 104/104 | 1.0° | 42 / 60 / 74 / 49 | 0.24 |
| 1 | 43 | 104/104 | 0.4° | 44 / 65 / 78 / 53 | 0.22 |
| 10 | 17 | 104/104 | 0.3° | 42 / 65 / 78 / 53 | 0.23 |
| 10 | 29 | 104/104 | 0.2° | 43 / 64 / 77 / 53 | 0.59 |
| 10 | 43 | 104/104 | 0.1° | 42 / 63 / 76 / 51 | 0.18 |

Against LF-079's 10.3 and 5.7 degree offsets, both weights hold steering,
and the anchor term stays near zero throughout. Weight **1** is chosen: the
lighter leash, with all three seeds under the 0.5 post-acquisition bound in
the pilot. Pilot artifacts:
`data/training-experiments/learned-flight-motor-anchor-pilot-v1-20261025`.

## Question, arms, evaluation, gates, budgets

Identical to LF-079 except for the anchor: one arm, `hold6`, seeds 17/29/43,
84 updates after eight critic-only warm-up updates from the LF-078 v4 fit;
evaluation before and after on the speed-16 set, the aligned set, the LF-055
velocity matrix and the 200 held-out random tasks; the same ten gates; the
arm qualifies with at least two of three seeds. Verification, provenance and
the stopping rule are as declared in LF-079, with the pre-freeze tests taking
their status from the test runner. Artifacts under
`data/training-experiments/learned-flight-motor-polish-v3-20261025`.

## Results — complete; one seed qualifies, the arm does not

All three cells ran with source hashes unchanged, parity on every first
input and the pre-freeze tests passing with the runner's status.

| Seed | Gates | Tracking | Stopping | Aligned | Lateral matrix | Held out | Tracking acquisition | Braking acquisition | Post-acquisition error |
|---|---|---|---|---|---|---|---|---|---|
| 17 | pass | 24/24 | 80/80 | 104/104 | 104 to 104 | 200 to 200 | 61.8 to 60.2 | 161.3 to 96.3 | 0.34 to 0.34 |
| 29 | fail: aligned, lateral, held out | 24/24 | 80/80 | 96/104 | 104 to 84 | 200 to 154 | 61.8 to 56.2 | 161.3 to 99.8 | 0.34 to 0.24 |
| 43 | fail: held out, by two cases | 24/24 | 80/80 | 104/104 | 104 to 104 | 200 to 198 | 61.8 to 60.0 | 161.3 to 94.5 | 0.34 to 0.22 |

The anchor did what it was declared to do: steering survives in three of
three seeds, every stopping, holding and turning case passes, and braking
acquisition falls from 161 to 94 to 100 ticks in every seed. The arm still
fails to qualify, one seed short. Seed 43 misses only the held-out gate, by
two of 200 tasks. Seed 29 loses eight aligned, twenty lateral-matrix and 46
held-out cases while keeping both speed-16 sets whole.

What the losses have in common is that they sit outside the polishing
distribution. The movement pools are the LF-069 speed-16 tasks only: aligned
starts, command 16, no turns. The aligned set commands 6, 12 and 18, the
lateral matrix adds sideways starts and 60-degree turns, and the held-out
tasks vary everything. The anchor holds steering only on visited states, and
PPO's actuator updates reshape the shared layers for command 16 while
nothing in the loss speaks for the rest of the envelope. Per the stopping
rule nothing is retried. The next recipe element to pilot is the obvious one:
polish on the training matrix of commands and start modes that the
evaluation sets draw from, with the anchor kept. Until a polished actor
qualifies, the LF-078 v4 fitted actor, which passes every set unpolished, is
the motor controller for navigation work; the interface does not change
when a polished actor later replaces it.
