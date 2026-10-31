# Supervised teacher fit of the plain motor actor v1

LF-074, 2026-10-25. Exploratory pilot under the two-tier process agreed after
LF-073: saved configuration, seed, checkpoint, outcome summary and per-case
traces, but no frozen twenty-page protocol, replay audit, confirmation use or
promotion. It is not a PPO learning attempt; the C4 counters stay at 35
learning attempts and 25 audits.

## Question

Can the plain 18-input, 32x32 motor actor (`motor18-actor32x32-critic32x32-v1`,
format 1, no frozen parent and no residual) represent and execute a heading and
speed controller that passes the development gates, when fitted to an
independent proportional teacher? A good static fit alone does not count; the
exported network must fly the cases itself through the native bridge.

## Teacher and feasibility reference

The teacher is a hand-written proportional law over the 18 motor inputs, with
no knowledge of the native guidance code. With forward error
`e = cos(heading error) * command - forward speed`:

- steering = -clip(2 * heading error, -1.8, 1.8) / 1.8;
- command > 0: thrust = clip(0.1 * e, 0, 1); brake = 1 while moving backward,
  otherwise clip(-0.1 * e, 0, 1);
- command = 0: thrust = 0; brake = clip(0.1 * |velocity|, 0, 1).

As a scripted controller it passes all 104 LF-073 development cases, acquiring
speed 16 in 57 to 81 ticks against the 360-tick deadline. Gains of 0.015 still
pass all 24 speed-16 cases; 0.012 loses the reverse starts to the deadline and
0.005 passes none. The fast-start collisions seen in LF-073 were policies
flying above 20 into the universe boundary at radius 500; the teacher travels
about 240 units in fifteen seconds and never collides. The teacher is a
training target only. The exported network owns every control during
evaluation, with no scripted mask, override, fallback or takeover, and the
teacher defines neither the success criteria nor a ceiling for later learning.

## Method

Implementation: `tools/flight_motor/teacher.py`, tests in
`tools/flight_motor/teacher_tests.py`. Bridge: the LF-073 archived
`flight_motor_bridge`. Seed 1, eight Torch threads.

Data, labelled by the teacher in every case:

- 300,000 synthetic zero-gravity, open-wing rows; half concentrate on the
  aligned regulation regime (heading error within 0.3 rad, forward speed within
  6 of the command, lateral speed within 2), a quarter use command 0, and the
  previous-action inputs are uniform random so the fit cannot copy them.
- 180,000 rows from 300 teacher-driven episodes on random tasks (speed 0 or 2
  to 20, heading delta 0 or up to 90 degrees, start speed up to 30 in nine
  directions, spin -0.6/0/0.6), with random held actions of 1 to 30 ticks
  injected about once per 90 ticks to spread the visited states.
- Two refinement rounds, each adding 120,000 rows visited by the fitted network
  itself on 200 further random episodes with the same perturbation.

Fit: actor parameters only, Adam 1e-3 with cosine decay, batch 2,048, 20,000
steps then 10,000 per refinement round at half the rate. Loss is cross-entropy
on the thrust and brake logits plus squared error on the tanh steering. The
critic and log deviations stay at their initialization. Export is format 1
with the bridge schema; native and Python inference agree to 5.8e-6 on every
development case's first inputs.

## Results

Mean absolute action error over the accumulated data:

| Round | Rows | Steering | Thrust | Brake |
|---|---:|---:|---:|---:|
| 0 | 480,000 | 0.0070 | 0.0096 | 0.0155 |
| 1 | 600,000 | 0.0052 | 0.0072 | 0.0128 |
| 2 | 720,000 | 0.0041 | 0.0058 | 0.0113 |

Fixed-state probe of the exported network at command 16, aligned, zero spin,
previous-action inputs iterated to the network's own fixed point:

| Speed | Thrust | Brake | Net acceleration |
|---:|---:|---:|---:|
| 0 | 1.000 | 0.637 | +44.99 |
| 8 | 0.845 | 0.000 | +38.04 |
| 12 | 0.389 | 0.000 | +17.50 |
| 15 | 0.100 | 0.000 | +4.50 |
| 16 | 0.004 | 0.005 | -0.02 |
| 17 | 0.000 | 0.099 | -3.96 |
| 20 | 0.000 | 0.395 | -15.82 |
| 24 | 0.000 | 0.824 | -32.95 |

Local slope at 16 is -4.23 units of acceleration per unit of speed with a bias
of -0.02, against -0.004 to -0.143 and biases up to 1.4 for the LF-073
policies. The brake value at exactly zero forward speed interpolates the
teacher's backward-motion discontinuity; brake force vanishes at zero speed, so
it has no physical effect, and it is 0.000 by speed 8.

Closed-loop development evaluation of the exported network:

| Set | Passed | Speed-tracking acquisition ticks | Max post-acquisition velocity error |
|---|---:|---|---:|
| LF-073 speed-16 set, 104 cases | 104/104 | 56 to 80 | 0.24 tracking, 0.90 braking, 0.47 turns |
| Aligned 6/12/18 set, 104 cases | 104/104 | 44 to 85 | 0.32 tracking, 0.90 braking, 0.47 turns |

Every brake, hold and turn case passes in both sets, including all 24 turns
and all eight stationary holds without a frozen parent. The teacher itself
acquires in 57 to 81 ticks with a tracking error bound of 0.12, so the network
is marginally less precise but well inside the 1.0 tolerance. On 200 unseen
random tasks without perturbation, retention horizon 600, the teacher succeeds
in 99 and the network in 99; the remaining tasks exceed what this teacher can
acquire by tick 360, such as 30-unit lateral starts with 90-degree turns.

Cost: 65 seconds in total, including 420,000 rollout ticks, 40,000 optimizer
steps and 187,200 evaluation ticks. Per-case traces, exports, the checkpoint,
plan and summary are in
`data/training-experiments/learned-flight-motor-teacher-fit-v1-20261025`; the
summary is copied to [docs data](data/flight-motor-teacher-fit-v1-summary.json).

## Interpretation and next step

Representation, fitting and native execution are not the obstacle. The plain
3,463-parameter network holds a controller whose feedback is thirty to a
thousand times stronger than anything thirty-five PPO attempts produced, and
it preserves stopping, turning and holding without any protection machinery.
The remaining gap is the learning signal. This fitted network is the positive
control for the next pilots: does PPO under the current recipe preserve or
erode this feedback, and do temporally extended decisions or a steeper
near-target reward let learning improve it? Reward accumulation and discounting
must be handled explicitly when the decision interval changes.

Limits: the speed-tracking portion of each set is four motion conditions with
six placements each, since zero gravity and aligned starts make the placements
physically identical. The teacher's gains were chosen by hand; a learned
controller may exceed it, and the physical gates remain the only success
criterion. No confirmation set was used and nothing is promoted or qualified
for navigation.

Rerun:

```
PYTHONPATH=. data/python-envs/flight-ppo/bin/python -m tools.flight_motor.teacher \
  --out <directory> --threads 8 \
  --archived-development data/training-experiments/learned-flight-motor-effective-batch-v1-20261025/tasks/development.json
```
