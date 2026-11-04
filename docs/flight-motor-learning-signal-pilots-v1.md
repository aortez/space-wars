# Learning-signal pilots around the teacher-fitted actor v1

LF-075, 2026-10-25. Eighteen unfrozen pilot runs under the two-tier process:
saved configuration, seed, checkpoint, per-update history, outcome summary and
per-case traces, with no frozen protocol, replay audit, confirmation use or
promotion. The frozen C4 attempt counters stay at 35 learning attempts and 25
audits; these runs are pilots, not qualification evidence.

## Questions

1. Does the current PPO recipe preserve or erode the [LF-074](flight-motor-teacher-fit-v1.md)
   teacher-fitted controller, which passes all 104 development cases?
2. Can a changed recipe learn velocity feedback from fresh weights?
3. Tested separately: six-tick decision holds, and a cost that stays steep
   inside the tolerance band.

Grid: start in {warm, fresh} by recipe in {current, repeat, steep}, seeds
17/29/43. Implementation: `tools/flight_motor/pilot.py`, tests in
`tools/flight_motor/pilot_tests.py`; the PPO update, likelihoods and GAE form
are the shared `tools/flight_ppo/ppo.py` implementation. Bridge: the LF-073
archived `flight_motor_bridge`.

## Recipes

- **current**: the LF-069 settings on the plain format-1 actor. Six persistent
  slots on the LF-069 speed-16 and stopping pools, 256 decisions per slot per
  update, 84 updates, gamma .99 per tick, lambda 1, four epochs, minibatch 256,
  Adam 3e-4, clip .2, entropy .001, value weight .5, gradient clip .5, KL stop
  .03, independent Gaussian exploration on the actuator logits with trainable
  log deviation -.7. Warm starts keep steering at its learned mean, as LF-069
  did; fresh starts sample steering too, as LF-053 did, since deterministic
  steering cannot be learned from nothing.
- **repeat**: each decision is held for six ticks. Rewards inside a hold are
  accumulated with the per-tick discount and the per-decision discount is .99
  to the number of ticks taken, so the roughly 100-tick horizon is unchanged.
  256 decisions per slot per update means six times the physics. Evaluation
  holds decisions the same way.
- **steep**: the LF-069 cost plus |velocity error| / 5 per tick, so the slope
  of the cost no longer vanishes inside the one-unit band.

Warm starts load the LF-074 weights, including its untrained critic, and run
eight critic-only updates with the actor frozen before PPO. Fresh starts use
the standard format-1 initialization with thrust bias -4. Every cell is
evaluated on the LF-073 104-case development set, deterministically, before and
after training, with its own hold. A fixed-state probe records the feedback
slope and bias at command 16 after every update.

## Results

Development passes are initial to final; the teacher-fitted actor starts at
104/104 with feedback slope -4.23 and bias -0.02 at command 16, and every fresh
start at slope about 0 and bias about -19, a constant brake at command 16.

| Start | Recipe | Passed, seeds 17/29/43 | Tracking | Final slope at 16 | Final bias at 16 | KL early stops of 84 | Seconds per run |
|---|---|---|---|---|---|---|---|
| warm | current | 104, 89, 104 | 24/24 all | -4.5, -4.7, -6.7 | -0.09, -0.19, -0.93 | 54, 51, 55 | 44 |
| warm | repeat | 104, 104, 104 | 24/24 all | -4.4, -6.3, -10.4 | +0.29, +0.12, -3.71 | 71, 71, 74 | 90 to 99 |
| warm | steep | 104, 104, 86 | 24/24 all | -5.2, -6.1, -5.1 | +0.06, -0.36, +0.09 | 47, 56, 51 | 44 |
| fresh | current | 0, 0, 0 (from 0, 0, 32) | 0/24 all | -0.03, +0.00, +0.07 | -20.7, -16.2, -19.7 | 0 | 42 |
| fresh | repeat | 0, 50, 0 (from 0, 0, 32) | 0/24 all | +0.05, +0.02, +0.00 | -18.7, -18.6, -14.3 | 0 | 74 |
| fresh | steep | 0, 0, 0 (from 0, 0, 32) | 0/24 all | -0.01, -0.02, +0.08 | -21.7, -16.1, -20.6 | 0 | 43 |

### Warm starts

All nine warm cells keep 24/24 speed-16 tracking and end with stronger
feedback than the teacher. Speed acquisition is mostly a few ticks faster than
the teacher's 56 to 80 ticks; for example warm/repeat seed 29 acquires the four
tracking conditions at 47/60/73/48 against the teacher's 55/62/76/51 under the
same six-tick hold. Post-acquisition velocity error rises from 0.24 to between
0.21 and 0.44 in the per-tick cells and from 0.19 to between 0.30 and 0.70 in
the repeat cells, inside the one-unit tolerance. Mean KL per update is about
.01 with clip fraction .13, so the policy moves substantially every update, and
the exploration deviations stay at -.7.

Stopping drifts in both directions. In seven of nine warm cells braking gets
faster: warm/repeat seed 29 brakes lateral starts in 66 to 115 ticks instead
of 119 to 321, and forward or reverse starts in 61 to 116 instead of 142 to
189. In two per-tick cells, warm/current seed 29 and warm/steep seed 43,
lateral braking still stops but acquires at ticks 410 to 600, past the 360
deadline, costing 15 and 18 cases; both cells also time out 37 stopping
episodes in training. The three repeat cells keep every one of the 104 cases.
Warm/repeat seed 43 sharpens to slope -10.4 with bias -3.7, a near bang-bang
controller with a visible offset that still passes but overshoots more (post
error 0.70, 39 training movement deadlines). With three seeds these are
indications about side effects, not separations between recipes.

### Fresh starts

No fresh cell learns velocity feedback. The probe slope stays within 0.08 of
zero in all nine cells and the bias stays between -14 and -22: the policy
brakes constantly at command 16, every movement episode times out (69 per
per-tick cell, 429 per repeat cell), and the policy barely moves per update
(mean KL .002 to .003, clip fraction .01 to .03, no KL stops). The steeper
cost cannot act on a policy that never enters the band, and six-tick holds
do not change the picture. Fresh/repeat seed 29 does learn holding and most
braking with sampled steering (holds 8/8, braking 36/48, turns 9/24) but ends
every tracking case at rest with velocity error 15.3.

### Cost and provenance

6,488,064 training physics ticks and 1,023 seconds of compute across the
eighteen cells, about thirty minutes of wall time with six cells in parallel.
A collector bug in the first attempt carried a six-tick hold counter across
collection windows and crashed the repeat cells at their second window; it
was fixed with a regression test and the six repeat cells were rerun from
scratch, their partial first-attempt directories discarded. Artifacts, per-cell
logs and the grid summary are in
`data/training-experiments/learned-flight-motor-learning-signal-pilots-v1-20261025`;
the grid summary is copied to
[docs data](data/flight-motor-learning-signal-pilots-v1-summary.json).

## Interpretation

The erosion hypothesis from the LF-073 diagnosis is falsified for this budget.
Near a competent controller the current recipe's gradient is informative: it
keeps the trained direction, sharpens it and mostly speeds up acquisition and
braking. The noise-floor account was therefore incomplete. What the recipe
cannot do, with or without holds or a steeper cost, is reach that basin from a
constant-output start. Every failure to date, from the LF-053 stopping parents
with a residual to the fresh weights here, begins at a constant brake that
never produces target-band experience, and the per-update change stays tiny
until it does. The barrier is exploration and initialization, not the gradient
inside the basin.

Two consequences follow. Teacher initialization plus PPO polishing is a
working path to a learned motor controller that exceeds its teacher on
acquisition and braking time, with stopping retention as the regression to
guard. Learning from scratch needs a way into the basin: a command-speed
curriculum starting near zero, where the LF-059 and LF-061 successes all sat,
or an initialization that coasts rather than brakes, are the candidates to
pilot before any frozen run. The six-tick hold kept all 104 cases in three of
three seeds and produced the fastest braking, at six times the physics per
decision; it is a candidate default, not an established one.

Limits: three seeds per cell, 84 updates, one budget, the four-condition
tracking set with six placements each, and no held-out random tasks. The two
recipe changes were not separated from the current recipe on the primary
measure, because every warm cell passes and every fresh cell fails. Nothing is
promoted or qualified for navigation.

Rerun one cell:

```
PYTHONPATH=. data/python-envs/flight-ppo/bin/python -m tools.flight_motor.pilot \
  --out <directory> --seed 29 --start warm --recipe repeat
```
