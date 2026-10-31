# Navigation teacher fit v1

LF-083, 2026-10-26. The supervised-fit step of the
[navigation plan](design/learned-navigation-v1.md): the hand-written
[reference law](flight-navigation-reference-v1.md) is fitted into the C3
navigation encoder, refined on the network's own visited states, and verified
closed loop with native inference and the v4 motor network flying. Exploratory
under the two-tier process: saved task sets, per-case results, traces and a
summary, no frozen protocol. Implementation: `tools/flight_navigation/fit.py`
and `model.py` with tests, the `learned_navigation` module in `spacewars-ai`,
and three new operations on the composed bridge.

## Contract and runtime

The network is the inspected C3 encoder unchanged: 4,009 inputs, polar
convolutions over the 23-band by 64-sector map, ray and nearest-surface
blocks, object attention, two 64-unit fusion layers, 39,447 parameters. Its
three heads are read as a body-relative direction pair through tanh and a
speed through a sigmoid scaled to 20. The export is format version 4 under the
`navigation-command-v1` contract; the flight policy loader refuses that
version and the navigation loader requires it, so neither side can misread the
other's heads. The composed bridge gained `load_navigation_policy`,
`infer_navigation` and `step_policy`: the loaded network issues the command
from the current observation, the bridge holds it as an absolute heading for
six ticks while the motor network flies, and a command whose pair has no
direction is refused rather than defaulted. Training uses task-local input
scaling, goal times 25 and relative velocity times 17.5, folded into the
exported query and fusion weights as the C3 pipeline already does.

## Data and fit

Tasks come from the C3 generator with gravity off and the planet stationary,
static families only: 120 per family at seed 7710001 for the first round and
60 per family at seeds 7710002 and 7710003 for the refinement rounds, with
LF-082's development seed 7700002 regenerated and checked against the archived
set, and seed 7700003 held out from every fit. The law labels every visited
observation with the command it would issue. In the first round the law
drives, with random commands held for one to six decisions at a rate of one in
thirty, so the data covers states off the law's own path; the labels are still
the law's. In each refinement round the previous export drives 180 fresh tasks
without perturbation and the law labels what the network visited. The loss is
squared error on the tanh pair plus cross-entropy on the speed logit against
the speed over 20; Adam fits the encoder and actor with the critic and
deviations untouched, 8,000 steps of batch 512 at 1e-3 with cosine decay, then
4,000 steps at 5e-4 per round. Twelve collection workers each own a bridge.
The whole run, three collections, three fits, two evaluations and the law's
held-out run, took 492 seconds.

| Round | Driver | Episodes and outcomes | Rows | Heading error | Speed error | Obstructed heading | Near-goal heading and speed |
|---|---|---|---|---|---|---|---|
| 0 | perturbed law | 360: 348 arrived, 10 deadline, 2 collision | 58,161 | 4.25 deg | 0.14 | 5.33 deg | 3.48 deg, 0.20 |
| 1 | round-0 network | 180: 173 arrived, 5 collision, 2 deadline | 84,238 | 4.39 deg | 0.13 | 5.81 deg | 3.38 deg, 0.19 |
| 2 | round-1 network | 180: 177 arrived, 3 deadline | 108,209 | 4.21 deg | 0.11 | 5.68 deg | 3.11 deg, 0.17 |

Errors are mean absolute values over all rows fitted so far, heading wrapped,
speed in units; near goal means within the law's eight-unit hold radius, 36
percent of the rows. The network's own driving already arrives on 96 percent
of fresh tasks after the first fit and on 98 percent after the second, with
no collisions in the last round.

## Closed-loop result

Native inference matches Python within 6.7e-6 on every task's first
observation. The exported network issues every command; the law is not run
during evaluation.

| Set | Family | Network | Law | Mean ticks, network | Mean ticks, law |
|---|---|---|---|---|---|
| development, seed 7700002 | clear | 24/24 | 24/24 | 668 | 634 |
| development | obstructed | 24/24 | 24/24 | 864 | 819 |
| development | altered | 24/24 | 24/24 | 735 | 731 |
| held out, seed 7700003 | clear | 24/24 | 24/24 | 774 | 751 |
| held out | obstructed | 23/24 | 23/24 | 857 | 841 |
| held out | altered | 24/24 | 24/24 | 718 | 724 |

The network passes 72 of 72 development tasks and 71 of 72 held-out tasks,
the same count as the law on the held-out set but not the same task. Its
mean arrival is four percent later than the law's on the development set
and one percent later on the held-out set.

**Held-out task 25, curved corridor, network collision at tick 445.** The
network took a different line through the corridor from early on; the law's
command at the states the network visited differs from the network's by 34
degrees on average over the run. In the last two seconds before contact the
law, evaluated at those same states, asked for a left turn of 8 to 52 degrees
while the network held a slight right heading at nine units per second and
clipped the inside of a bend. The clearance along the network's commanded
heading read clear throughout, so the ray corridor did not warn it; the
law's own run passes this task in 888 ticks with a minimum clearance of 17
units because its route rounds the bend on the outside. This is a heading
fit error in the obstructed family, whose mean error is the largest at 5.7
degrees, on geometry from a seed the fit never saw.

**Held-out task 54, wall, law deadline.** The law's failure is a route
ambiguity, not an approach problem: for the first 1,170 ticks its A* waypoint
alternates between the two ends of the wall, the commanded heading flips
between 177 and minus 110 degrees every decision, and the motor network,
which thrusts only when the nose is near the command, never leaves the start.
The ship is still at its starting position at tick 1,170, moves off only when
the alternation breaks, and reaches eight units from the goal at tick 1,728 of
1,800. The fitted network commits to one side at once and arrives at tick
768. The fit smooths the law's discontinuity, and here that is an improvement
over the teacher; a stable tie-break belongs in the law before any refit.

## Interpretation and next step

The 39,447-parameter encoder reproduces the routing law to about four
degrees and a tenth of a unit, generalizes to a held-out generation seed at
the law's own pass rate, and runs natively through the composed stack. The
capacity question is settled in favour of keeping the encoder as it stands;
a smaller trunk would be an inference-cost choice, not a learning one. The
fitted export is the initialization for navigation polishing: PPO at the
command rate with six-tick decisions, per-tick discounting, critic warm-up,
bounded arrival shaping and physical terminal terms, pilots first and then one
frozen comparison with the gates listed in the plan, including arrival on
every development family, no collision, held-out goals not lost and faster
arrival than the law. Limits: one fit seed, one held-out seed of 24 cases per
family, static families only, no gravity or planet spin, and one corridor
collision that polishing is expected to address through the collision term
rather than through more imitation data. Per-case results, traces, task sets,
the three exports, the checkpoint and the 1.7 GB of labelled observation
shards are under
`data/training-experiments/learned-flight-navigation-fit-v1-20261026`, with the
summary copied to [docs data](data/flight-navigation-fit-v1-summary.json).
