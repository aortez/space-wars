# Frozen requalification of the revised motor teacher v2

LF-079, 2026-10-25. Frozen before any run, with hashes in the experiment's
`plan.json`. Results are appended below. This requalifies the motor stage's
method, teacher initialization plus PPO polishing with six-tick decision
holds, after the teacher revision of LF-078 (`flight-motor-teacher-v4.md`),
which adds lateral-velocity damping and a capped crab angle so that heading
changes while moving no longer leave sideways drift to chance.

## Question and bounded decision

Starting from the LF-078 v4 teacher-fitted actor, does PPO polishing under
six-tick holds, the arm selected in [LF-076](flight-motor-polish-v1.md),
keep every development case on four sets, keep the held-out random result,
and beat its teacher on acquisition and braking time without over-sharpening?

One arm, `hold6`, seeds **17/29/43**, 84 updates after eight critic-only
warm-up updates, configuration unchanged from LF-076 except the warm
checkpoint. There is no LF-075 reference export for this teacher, so the
reproduction check is not applicable; the run records that explicitly. No
second arm, parameter sweep, fallback, confirmation use or promotion.

## Evaluation and gates

Deterministic evaluation before and after training with the six-tick hold on:

- the LF-073 104-case speed-16 set;
- the aligned 6/12/18 set, 104 cases;
- the LF-055 velocity matrix, 104 cases, with lateral and diagonal tracking
  starts and 60-degree turns at 6/12/18, which the v1 teacher failed 40 of;
- 200 held-out random tasks, seed 7600001, as in LF-076.

A seed qualifies only if all ten gates pass on its final policy: the nine
LF-076 gates with the lateral matrix added to the development physical-failure
gate, plus no lateral-matrix case lost against the initial policy. The arm
qualifies with at least two of three seeds. The v4 fitted policy enters at
104/104 on each fixed set and 200/200 held out with the scripted law; the
pre-freeze check of the fitted network on the held-out set is part of the
run's initial evaluation rather than a separate claim.

## Verification, budgets and provenance

Native/Python inference parity on every evaluation case's first inputs;
source hashes of the pilot, polish, teacher, model, PPO and bridge modules
recorded at freeze and checked before every cell. The test suites for the
teacher, pilot and polish modules run before freeze with the exit status
taken from the test runner, correcting the LF-076 process defect.

Budget: three cells at 847,872 training physics ticks and 1,104 Adam steps
each; evaluation at most 3 cells by 2 policies by 512 episodes, under 3
million ticks. Expected under five minutes of wall time. Artifacts under
`data/training-experiments/learned-flight-motor-polish-v2-20261025`.

Stopping rule: a failed parity or source check is recorded and the arm does
not qualify; nothing is retried, patched or extended after freeze.

## Results — complete; no seed qualifies, steering drift

All three cells ran to completion in 201 seconds of wall time with source
hashes unchanged and parity on every first input; the pre-freeze tests passed
with the runner's own exit status. No seed passes the gates.

| Seed | Gates | Tracking | Stopping | Aligned | Lateral matrix | Held out | Tracking acquisition | Braking acquisition | Post-acquisition error |
|---|---|---|---|---|---|---|---|---|---|
| 17 | fail, seven gates | 0/24 | 0/80 | 6/104 | 104 to 8 | 200 to 36 | 61.8 to 60.8 | none acquired | 0.34 to 1.10 |
| 29 | fail, lateral and random lost by one case each | 24/24 | 80/80 | 104/104 | 104 to 103 | 200 to 199 | 61.8 to 57.2 | 161.3 to 116.0 | 0.34 to 0.33 |
| 43 | fail, six gates | 24/24 | 0/80 | 24/104 | 104 to 48 | 200 to 137 | 61.8 to 63.5 | 161.3 to 107.6 | 0.34 to 0.34 |

The failure is steering drift. Seed 17's final policy settles about 10.3
degrees off every commanded heading, so each holding, turning and braking case
times out with velocity error near zero, and the tracking cases acquire and
then exit near tick 400 as the heading crosses the 5-degree bound. Seed 43
settles at 5.6 to 5.8 degrees: holds acquire at tick 30 and exit at 63. The
steering response became asymmetric: at rest with a 30-degree error, seed 17
steers −0.59 one way and +0.24 the other against the initial ±0.54. Training
shows it: stopping episodes in seed 17 go from all succeeding to nearly all
timing out between updates 18 and 30, in seed 43 after update 60, while speed
feedback stays intact throughout. Seed 29 drifted least and improved both
acquisition measures, losing one lateral and one held-out case.

The cause is structural. Polishing keeps steering at its learned mean and
samples only the actuators, so the likelihood ratio, the KL limit and the
reward's heading cost never touch the steering output; it moves only through
the shared hidden layers as a side effect of actuator updates, with nothing
to pull it back. LF-076 survived this in three of three seeds with the v1 fit;
the v4 fit's richer steering, with lateral damping and crab, is more exposed.
Per the stopping rule nothing is retried; the method needs a recipe element
that anchors steering, piloted before any new freeze.
