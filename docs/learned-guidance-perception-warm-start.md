# Warm-starting perception networks

**Status: complete (2026-10-18).** Milestone 4 of the
[match hybrid plan](design/learned-guidance-match-v1.md#milestones).

A 255-weight network that reads the perception blocks, warm-started from
either dense parent, flies all 621 known cases exactly as that parent does,
decision for decision. Sensing and inference cost about 1.8 µs per decision,
against 0.09 µs for the parent's inference. See the [results](#results).

## Question

Can a network that reads the new perception blocks start out flying exactly
like a dense parent on all 621 known cases? Perception training, milestone 6,
starts from such a network. If the warm start already changed behavior, its
results would mix the effect of the new inputs with an unrelated change of
starting point.

## Design

**Observation.** `arrival_perception_v1` follows the
[perception design](design/guidance-perception.md#observation-blocks): the
nine inputs, then eight body records of 11 values and 16 arcs of 8 values, 225
in all. The sun, planets and world edge are read as circles, which
[milestone 2](design/guidance-perception.md#sensing-in-match-worlds) found
exact in the training worlds.

**Network.** Model version 2 puts two set encoders in front of a dense trunk:

- a body encoder, one tanh layer from 11 values to 4, max-pooled over the
  occupied slots;
- an arc encoder, one tanh layer from 8 values to 4, max-pooled over the 16
  arcs;
- the trunk, the parent's 9-8-3 network widened to read the nine inputs and
  the eight pooled values.

That is 255 weights, against the parent's 107.

**Warm start.** The trunk copies the parent's weights, and its weights that
read the eight pooled values are zero. The encoders get deterministic uniform
weights and biases in ±0.5, from seed 1 for `e0f25b68` and seed 2 for the
time-ranked `e03d3e29`, so a single mutation of a zero trunk weight can
change behavior. The trunk adds the nine inputs in the parent's order before
the pooled values, so its sums, and the child's raw outputs, should match the
parent's bit for bit.

**Runners.** The match and precision runners of the frozen lineage are pinned
by earlier packages, so this milestone adds copies that also feed perception
policies: one for match approaches and one for precision episodes.

## Checks

The frozen [plan](data/learned-guidance-perception-warm-start-v1-plan.json)
takes its inputs from the [time-ranking](learned-guidance-time-ranking.md)
package: the 482 recorded match states, the two parents, and the evaluation's
records of both parents on all known sets. For each parent:

1. **Runner parity.** The copied runners, flying the parent, must reproduce
   its recorded episodes: 464 training and 18 excluded match approaches and
   157 precision arrivals.
2. **Child parity.** The child's episodes must equal the same records.
3. **Decision parity.** Every child decision must have the same raw output
   bits as the parent's.

The 464 training approaches and 157 precision arrivals are the 621 known
cases. The run also records every perception value the children read, to
check that each stays finite and within ±1, and times sensing and inference
on 32 real match states.

## Results

Both children reproduce their parents exactly. The check took 22 seconds on
the release build.

| | `e0f25b68` child | `e03d3e29` child |
| --- | ---: | ---: |
| Training approaches reproduced | 464/464 | 464/464 |
| Excluded known approaches reproduced | 18/18 | 18/18 |
| Precision arrivals reproduced | 157/157 | 157/157 |
| Decisions with identical outputs | 123,697/123,697 | 116,144/116,144 |

Each count holds for all three checks: the copied runner flying the parent,
the child's episode, and the child's decisions. The children are genomes
`a2951655e72076252786a0ee8adbd93f28a14959c66a4790b5f916ed1162f6b6` and
`2fb59897993886fbf6ddfd59c279b0465f77abcbc689c3ba7d049a1d22373def`. Like their
parents, both complete all 464 training approaches and 157 precision
arrivals. Of the 18 excluded approaches, the `e0f25b68` child completes none
and the `e03d3e29` child two, as recorded for the parents.

### What the children read

- **Bodies.** Every match decision had five bodies, the sun, three planets
  and the world edge, and every precision decision two, the planet and the
  edge. No body was left out of the eight slots.
- **Bounds.** Every perception value stayed finite and within ±1. Encoded
  distances reached at most 0.71 and speeds 0.49, so neither transform came
  near its bound.
- **Matches.** The destination's clearance ran from 75 to 150 units. The sun's
  surface was 41–672 units away and was the nearest surface in 17% of arcs;
  planets were nearest in 35% and the world edge in 49%.
- **Practice world.** It has no sun, and its world edge, 258–380 units away,
  is the nearest surface in 82% of arcs. In matches the edge is 194–986 units
  away.

The [results file](data/learned-guidance-perception-warm-start-v1-results.json)
has every field's range by kind of body.

### Cost

Single-threaded on 32 match states:

| Per decision | Mean |
| --- | ---: |
| Reading the bodies | 0.09 µs |
| Sensing and encoding | 0.92 µs |
| Child inference | 0.78 µs |
| Parent inference | 0.09 µs |

Together the perception decision costs about 1.8 µs, close to the
[design's estimate](design/guidance-perception.md#cost) of 0.9 µs for
sensing plus 0.8 µs for inference.

### First attempt

The first frozen attempt stopped at a check on the comparison code itself.
The recorded precision files had been written straight from typed episodes,
which print each f32 at its shortest. The comparison converted episodes
through JSON values first, which widen each f32 before printing, so the
numbers read back differently. Even the pinned lineage runner failed that
check. The comparison now reads typed records the way their files were
written. Match records had been written through JSON values and passed on the
first attempt. That attempt is kept in the package's `attempt-1/` and does not
enter the results.

## Reading

- **The warm start is exact.** A network with 225 inputs and 255 weights
  starts out flying exactly like either dense parent on every known case.
  Perception training can start from either child, so any change in behavior
  will come from training.
- **The new inputs are not used yet.** The trunk weights that read them are
  zero. Whether they help is the question for milestone 6.
- **The known cases vary hazards little.** Every match world has the same
  five bodies, and the practice world has no sun and an edge closer than in
  most matches. The varied worlds of milestone 5 should vary the edge's
  distance as well as the sun and planets.

## Code and evidence

- **Sensor and model.** [`guidance/perception.rs`](../crates/spacewars-ai/src/guidance/perception.rs)
  reads and encodes the blocks and builds the warm start.
  [`model/shared.rs`](../crates/spacewars-ai/src/model/shared.rs) is model
  version 2. [`free_flight.rs`](../scenarios/spacewars/src/surface_sortie/free_flight.rs)
  supplies the body circles. See the design's
  [implementation](design/guidance-perception.md#implementation).
- **Runners.** [`ship_perception.rs`](../crates/engine-agent/src/ship_perception.rs)
  runs precision episodes and
  [`guidance_match.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_match.rs)
  match approaches. Both copy the pinned lineage runners and add perception
  policies.
- **Experiment.** [`guidance_perception.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_perception.rs).
  It and `guidance_match.rs` are declared in `mission.rs`, beside
  `guidance_approach`, so adding them edited no pinned file and the earlier
  packages still verify.
- **Package.** `data/training-experiments/recovery-guidance-perception-warm-start-v1-20261018/`
  holds the copied inputs, the frozen executable and source patch, and the
  output. From that directory, `python3 -I run.py verify` checks the package
  and `python3 -I run.py reproduce` reruns the check. The
  [results file](data/learned-guidance-perception-warm-start-v1-results.json)
  is a copy of `reproduce/results.json`.
