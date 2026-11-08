# Training the perception network against native control

**Status: superseded 2026-10-22 by the
[learned flight controller plan](design/learned-flight-controller-v1.md).**
This unexecuted recipe and its partial revision are preserved as design history.
The native-relative objective, legacy retention sets, evolutionary run budget
and proposed takeover distances below do not govern the new flight skill.
It was milestone 6 of the
[match hybrid plan](design/learned-guidance-match-v1.md#milestones).

Train the 255-weight perception network on the varied-world training set with
the time-saved ranking, then test it once on the frozen holdout against native
control. A matching run of the nine-input network shows what the new inputs
add. See the [design](#design) and the [decisions](#decisions) to review.

## Revision in progress

Reviewing the training set showed that its cases are far less varied than
their count suggests, so the set is being redesigned. The sections after this
one describe the first draft and will be rewritten.

**Findings, from development measurements outside any package:**

- The 464 known match approaches are 244 distinct starts. The other 220 are
  dirt-shape copies with bit-identical ship states that fly identically.
- Every match set starts where native control's descent first comes within 35
  units of the entry point. 83–85% of starts lie within 1 unit of the same
  height, and about 89% fall within 15° of straight down.
- From there native control needs a median of 44 ticks. From 100 above the
  entry it needs 126, including all of its braking, and the nine inputs stay
  in range to capture in 158 of 163 census approaches.
- Handed over at 100 above, the parents complete about half of 149 census
  approaches, against 142 for native control. Most of their failures coast
  into native control's climb guard without braking.
- Over full ten-minute matches, only 163 of native control's 308 census
  approaches come in the first three minutes, the window milestone 5 searched.

**Decided (2026-10-22):**

- Train on handovers at 35, 70 and 100 above the entry, and choose the
  takeover point afterwards from where the network beats native control.
- Match approaches only. The precision arrivals leave training: the component
  becomes a match-approach specialist, and existing networks keep the practice
  exercise.
- No legacy sets in training or reports: not the 464 known approaches, the
  earlier fresh and inspected starts, or milestone 5's sets. New sets replace
  them.
- No asteroids, in training or the holdout.

**Open:** which kinds of world and how much weight standard worlds get;
searching full matches; one handover point per training approach; and keeping
the low-margin starts.

## Question

Trained on the varied-world starts, does the perception network beat native
control on the frozen holdout: more completions, and less arrival time on the
starts both complete?

## Starting point

- **Networks.** The time-ranked `e03d3e29` completes every case `e0f25b68`
  completes, in every set evaluated, and arrives sooner, so it is the parent.
  Its warm-started child, genome
  `2fb59897993886fbf6ddfd59c279b0465f77abcbc689c3ba7d049a1d22373def`, reads
  the perception blocks but flies exactly as the parent does, because the
  trunk weights that read them are zero.
- **Training set.** 1,370 varied starts from 306 worlds of nine kinds, with
  and without a sun. Native control completes 1,279 and `e03d3e29` 1,287,
  losing 35 of native control's successes and completing 43 of its failures.
  Counting a failed start as 300 ticks, `e03d3e29` takes 6,709 ticks more than
  native control.
- **Known cases.** `e03d3e29` completes all 464 known match approaches and 157
  precision arrivals. Native control completes 430 of the approaches.
- **Holdout.** 738 starts from 180 worlds, 240 of them standard. Native
  control completes 683, with a median of 45 ticks. No model has run on it.

### Where the time goes

A development measurement outside any frozen package, over the varied training
starts that both native control and `e03d3e29` complete:

| | Starts | Mean ticks | Speed at capture | Thrust ticks | Brake ticks |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sun worlds, native control | 650 | 47.6 | 16.6 | 11.9 | 32.6 |
| Sun worlds, `e03d3e29` | 650 | 50.4 | 14.1 | 19.1 | 36.6 |
| No sun, native control | 594 | 41.9 | 17.8 | 8.4 | 26.1 |
| No sun, `e03d3e29` | 594 | 53.7 | 13.3 | 20.1 | 38.8 |

Speed at capture is relative to the destination, and capture requires less
than 18.

- **Without a sun native control gets faster** and reaches capture just under
  the limit. Destinations stand still there. `e03d3e29` gets slower and
  arrives more gently than in sun worlds.
- **`e03d3e29` works harder.** It thrusts about twice as often as native
  control and brakes more, in both kinds of world.
- **Speed is where native control leads.** `e03d3e29` already completes more
  training starts.
- **Near the sun.** All 39 of `e03d3e29`'s sun-avoidance failures start within
  15 units of the sun's buffer.

## Design

### Arms

- **Perception arm.** 16 copies of the child.
- **Nine-input arm.** 16 copies of `e03d3e29`, with the same cases, ranking
  and settings.

Both arms start out flying identically. Comparing them separates what the new
inputs add from what the new worlds add.

### Cases

Every candidate flies every case in every generation: the 1,370 varied training
starts, the 464 known match approaches and the 157 precision arrivals, 1,991
episodes in all. The
[perception design](design/guidance-perception.md#training-set-changes)
proposed rotating world batches to contain cost. The whole set takes about a
minute per generation (see [cost](#cost)), so batches do not rotate and
fitness compares every candidate on the same cases.

### Ranking

Both arms rank candidates in this order:

1. Precision arrivals completed.
2. Match approaches completed, known and varied alike.
3. Total match arrival ticks, lower first, counting a failed start as the
   300-tick attempt bound.
4. Gentle arrival: precision position and velocity points, then match speed
   and position points.
5. Precision spare ticks.

This is milestone 1's time ranking with one change. Milestone 1 counted all
completions together before precision and matches separately. Every candidate
kept all 621 known cases, so that order never mattered. Here match completions
can rise on the varied starts, and counting completions together would let a
candidate trade a precision arrival for a match approach. Qualification reruns
the retained arrival cases, so precision comes first.

Known approaches count like varied ones, so a candidate may trade a known
approach for a varied one. Reports show known successes lost. Native control's
arrival times are fixed for each start, so the third key ranks candidates by
time saved against native control.

Attempts are still judged by native control's rules: its climb guard, route
detours, sun avoidance and switches to other tasks end them. Under those rules
the sun inputs can only teach the network to stay outside native control's
buffer. [Milestone 7](design/learned-guidance-match-v1.md#milestones) judges
physical outcomes instead.

### Settings

As in milestone 1, but twice as long: evolution seed 1, 16 candidates, two
elites, tournament size 3, mutation probability 0.05, mutation scale 0.05 and
parameter limit 8, for 192 generations per arm.

- **One mutation rate.** At milestone 1's rate the trunk's 107 inherited
  weights change as often as before, about five per child, and the 148 new
  weights add about seven. A change to an encoder weight has no effect until a
  trunk weight that reads that encoder moves off zero. Separate rates per
  block, which the perception design suggests, can follow if the new weights
  stay unused.
- **Selection.** Each arm's final is the best candidate of its last
  generation. No evaluation set chooses among checkpoints.
- **Evolution code.** `evolution.rs` handles only dense genomes and is pinned
  by three earlier packages. A copy generic over the genome kind runs both
  arms. For dense genomes it must reproduce `evolution.rs` exactly.

### Checks before training

On the frozen release build:

- The 1,370 varied training starts rebuild to their recorded states. Native
  control and `e03d3e29` reproduce their recorded outcomes and ticks on every
  one.
- The child flies every varied start, known approach and precision arrival
  exactly as the parent does.
- Both reproduce milestone 4's records on the 482 known match starts and the
  157 precision arrivals.
- The parent's fitness matches the same fields rebuilt from its recorded
  episodes.
- Only the evaluation stage flies models on the holdout, and only once both
  finals exist. A development run of two generations per arm tests the stages
  and resuming, and evaluates on training starts instead.

### Evaluation

After both arms finish, the evaluation flies native control, `e03d3e29`, both
finals, and the perception final with the trunk weights that read the new
inputs set to zero. That last one shows how much of the final's behavior rests
on the new inputs.

| Set | Starts | Use |
| --- | ---: | --- |
| Holdout | 738 | The gate |
| Varied training | 1,370 | Fit |
| Known match approaches | 464, and 18 excluded | Retention |
| Milestone 1's fresh starts | 239 | Standard worlds outside training |
| Precision | 157 known, 96 inspected | Retention |

Milestone 1 evaluated its fresh starts, so they are not a holdout, but nothing
has trained on them or been selected with them. Each match set reports, by cell
where it has cells: completions, native successes lost and gained, mean and
median ticks on shared successes, starts faster, slower and tied, time saved
with 300 ticks for a failure, speed at capture, thrust and brake ticks, and
host reasons for failures.

### Gate

Milestone 6 is done when, on the holdout, the perception arm's final:

1. completes more starts than native control, and
2. takes less time in total than native control on the starts both complete.

Both are measured over the whole holdout. The standard cell, where
qualification will run, is reported but not gated.

### Reading the result

- **Pass.** The final becomes milestone 7's starting point.
- **The perception arm leads the nine-input arm but not native control.** The
  inputs help, but the network is not yet good enough. Where it still loses
  time chooses the next step: a wider network, longer training or separate
  mutation rates.
- **The arms end alike.** Any gain comes from the worlds, not the inputs.
  Under native control's rules the inputs may have little to teach until
  milestone 7.
- **Neither arm gets faster without a sun.** Something that more worlds do not
  fix holds speed back. The precision cases are the first suspect, since the
  same weights must stop precisely in the practice world, which has no sun.

## Cost

Measured on the development machine with the release build, outside any
package:

- Rebuilding the 1,370 varied training starts takes 55 seconds, and holding
  them takes 3.9 GB of memory. With the known approaches it is 6.2 GB of the
  machine's 60.
- One candidate flies all 1,991 cases in about 4 seconds: 3 for the varied
  starts, under 1 for the known approaches and 0.1 for the precision arrivals.
- A generation of 16 candidates takes about 65 seconds. 192 generations take
  about 3.5 hours per arm and 7 for both, about 6.1 million episodes per arm.
  The evaluation takes a few minutes.

## Decisions

To review:

1. **Nine-input arm.** Recommended. Without it a gain cannot be credited to the
   new inputs. It adds about 3.5 hours.
2. **192 generations per arm.** Milestone 1's time arm was still improving at
   generation 96, and this gap is larger. 96 generations would halve the time.
3. **Precision first.** Recommended, since qualification reruns the retained
   arrival cases. Leaving the precision cases out might free the network's
   speed, but would give up a capability that qualification checks.
4. **Gate over the whole holdout.** Alternatively the standard cell must also
   pass on its own, which is stricter.

## Progress

- [ ] Review the plan.
- [ ] Generic evolution, the ranking, the stages and the checks, with tests.
- [ ] Development run: two generations per arm from a draft plan.
- [ ] Freeze the plan, and run the preflight on the release build.
- [ ] Train both arms.
- [ ] Evaluate, write the results and update the match plan.

## Out of scope

- Physical judging and component-owned safety, milestone 7.
- The hybrid with native fallback, milestone 3.
- Wider networks, separate mutation rates per block and new precision worlds.

## Code and evidence

- **Measurement.** The cost and the table in
  [where the time goes](#where-the-time-goes) come from the ignored test in
  [`guidance_training_cost.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_training_cost.rs).
- **Planned.** The generic evolution in `engine-agent`, the experiment in
  `mission/guidance_training.rs`, the frozen plan in
  `docs/data/learned-guidance-perception-training-v1-plan.json`, and the
  package `data/training-experiments/recovery-guidance-perception-training-v1-20261020/`.
