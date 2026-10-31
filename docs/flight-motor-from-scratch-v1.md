# From-scratch motor learning pilots v1

LF-077, 2026-10-25. Time-boxed unfrozen pilots on the question LF-075 left
open: can PPO enter the basin of a working speed controller without a
teacher? Two ways in are tried, each as six cells of fresh weights under the
current per-tick recipe and under six-tick holds, seeds 17/29/43, 84 updates,
with the LF-075 pilot trainer (`tools/flight_motor/pilot.py`). Evaluation is
the LF-073 104-case set before and after, plus the aligned 6/12/18 set after.
These are pilots: saved configurations, histories, exports and traces, no
frozen protocol, confirmation use or promotion.

## Pilot A: command-speed curriculum

The movement pools start at command 4 and step to 8, 12 and 16 every 21
updates; start velocities scale with the command from the speed-16 matrix.
Stopping pools are unchanged. The only earlier from-scratch successes, LF-059
and LF-061, were all at speed 6, so a low first command was the obvious way
in.

| Recipe | Seed | 104-set passed, final | Tracking | Movement training outcomes | Final feedback slope at 16 | Final bias at 16 |
|---|---|---|---|---|---|---|
| current | 17 | 0 | 0/24 | 69 deadlines | -0.00 | -22.1 |
| current | 29 | 0 | 0/24 | 69 deadlines | +0.00 | -19.5 |
| current | 43 | 0 | 0/24 | 69 deadlines | +0.02 | -20.5 |
| repeat | 17 | 80 | 0/24 | 429 deadlines | -0.04 | -18.0 |
| repeat | 29 | 0 | 0/24 | 429 deadlines | -0.02 | -15.8 |
| repeat | 43 | 0 | 0/24 | 429 deadlines | -0.06 | -13.8 |

No movement episode is acquired at any stage, including command 4, and no
cell ends with velocity feedback. The six-tick seed 17 learns holding,
braking and some turning with sampled steering, 80 of the 104 cases, and
still tracks nothing. A reporting flaw: the per-stage probe was evaluated at
speeds 15 to 17 rather than around the stage's own command, so it reads the
policy's response to a large overspeed and not its feedback at the stage
speed; the conclusion rests on the training outcomes, zero acquisitions at
every stage, and on the final command-16 probe. The probe is corrected for
later runs.

Why the curriculum cannot help: the standard fresh initialization is a
low-thrust prior, thrust logit -4 and brake near .5. Thrust samples under the
exploration deviation reach a few percent at most, the brake damps the ship to
about one unit per second, and no sample ever enters the one-unit band around
command 4. Without that experience the dense cost's gradient through the
saturated thrust output is tiny, and the policy change per update stays near
zero, as in every fresh LF-075 cell.

## Pilot B: coasting initialization

Fresh weights with thrust and brake logits at -2 instead of the low-thrust
prior, so the untrained policy cruises near eleven units per second under the
brake physics and produces target-band experience from the first rollout.
Same recipes, seeds and budget; command 16 throughout, no curriculum, so the
cells compare directly with the LF-075 fresh cells. The untrained policy
passes none of the 104 cases, with feedback slope 0 and bias about +0.6.

| Recipe | Seed | 104-set passed, final | Tracking | Tracking acquisition fast/rest/reverse/slow | Movement training outcomes | Final slope at 16 | Final bias at 16 |
|---|---|---|---|---|---|---|---|
| current | 17 | 0 | 0/24 | none | 64 deadlines, 7 collisions | -0.05 | -0.15 |
| current | 29 | 0 | 0/24 | none | 65 deadlines, 6 collisions | -0.00 | -2.27 |
| current | 43 | 0 | 0/24 | none | 69 deadlines | -0.01 | +0.37 |
| repeat | 17 | 18 | 18/24 | 253 / 299 / 419 / 238 | 397 deadlines, 36 collisions | -0.58 | -0.04 |
| repeat | 29 | 0 | 0/24 | none | 375 deadlines, 63 collisions | -0.29 | +1.32 |
| repeat | 43 | 12 | 12/24 | 237 / 401 / 565 / 312 | 403 deadlines, 28 collisions | -0.52 | -0.17 |

This is the first speed tracking learned from fresh weights in the project.
Under six-tick holds, seed 17 retains all fast, rest and slow starts and seed
43 the fast and slow starts, with learned feedback of -0.58 and -0.52 units
of acceleration per unit of speed and near-zero bias, against zero in every
other fresh cell to date. The slopes are still seven to eight times weaker
than the teacher's, acquisition is late, reverse starts miss the deadline,
and no stopping, holding or turning case passes: the coasting prior gives up
the stopping competence that the low-thrust prior supplied for free, and 84
updates do not recover it. The per-tick recipe learns nothing from the same
initialization in any seed, the clearest separation between the two recipes
so far. Training collisions, 28 to 63 per hold cell, are overspeed into the
universe boundary while the policy learns to slow down.

## Interpretation

The from-scratch barrier is the initialization prior, and the decision rate
matters once that prior is fixed. A brake prior learns stopping and never
moves; a coasting prior learns tracking under six-tick holds and forgets to
stop; a speed curriculum cannot rescue a prior that never samples motion.
Learning feedback from nothing is therefore possible with this recipe, but at
this budget it is partial and seed-dependent, while teacher initialization
plus polishing qualified in [LF-076](flight-motor-polish-v1.md) with braking
twice as fast as the law. The stated method for the motor stage remains
teacher initialization plus polishing. A longer coasting-initialization run
under holds, with a stopping-friendly prior for the zero-command cases, is the
one from-scratch follow-up worth its cost later; it is not a prerequisite for
navigation.

Cost: twelve cells, 5,419,008 training physics ticks and 846 seconds of
compute in total, under two minutes of wall time per six-cell batch.
Artifacts: `data/training-experiments/learned-flight-motor-curriculum-v1-20261025`
and `data/training-experiments/learned-flight-motor-coast-init-v1-20261025`.
