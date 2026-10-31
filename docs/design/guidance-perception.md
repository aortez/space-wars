# Perception for learned arrival guidance

**Status, 2026-10-22:** implementation and design history for the arrival
perception sensor and shared-encoder model. The
[learned flight controller plan](learned-flight-controller-v1.md) governs the
next experiment. Sensor implementations and measurements below are reusable
evidence; warm-start compatibility, encoder widths, native fallback and the
remaining training sequence are no longer requirements for new flight work.

The next observation and network for the learned arrival component. It
applies the [organism perception proposal](organism-perception.md) to the
guidance role in the [component contract](guidance-component-v1.md), prompted
by the fresh-check misses in the
[match hybrid plan](learned-guidance-match-v1.md#current-state). Milestone 4
implemented both, as the observation `arrival_perception_v1` and model version
2; see [implementation](#implementation). Inference costs come from standalone
benchmarks, and sensing costs from the physics worlds of the training matches.

## Why the nine inputs are not enough

`arrival_local_9_v1` supplies target offset and velocity, planet-frame
velocity, gravity and spin. It has no sun position, no other planets and no
surface geometry. In the fresh check, 12 of the candidate's 16 misses end at
sun avoidance. The sun reaches the network only through the total gravity
vector, whose encoded magnitude has a median of 0.08. Gravity is also a poor
cue for "down": it points a median of 23° away from the planet direction. In
three of the six sun situations, native control's bearing drifts away from the
sun and it arrives; the candidate has no input that says why it should.

Narrower host admission rules could avoid those starts instead. This design
aims for a network that sees hazards, because worlds vary: a world may have no
sun, planet counts and sizes vary, and planet shapes may change later. The
host's safety rules remain the hybrid's fallback either way.

## Observation blocks

Keep the nine inputs as the task and self-motion block. Add two blocks in the
same ship-local frame.

### Body records

One record per body: the sun, each planet, and the world edge. Measurements
use the nearest surface point, so a body's size enters only through clearance
and the size field, and nothing assumes a fixed sun radius.

| Field | Values | Meaning |
| --- | ---: | --- |
| Presence | 1 | Separates an empty slot from a body at zero distance. |
| Kind | 3 | One-hot: sun, planet, world edge. |
| Destination | 1 | Set on the target planet's record. |
| Direction | 2 | Unit vector to the nearest surface point. |
| Clearance | 1 | Distance to that point, through a bounded transform. |
| Closing speed | 1 | Relative velocity along the direction. |
| Sideways speed | 1 | Relative velocity across the direction. |
| Size | 1 | Bounded angular size, to tell a small planet from a wall. |

Eight slots give 88 values. The destination always has the first slot. The
rest go to the most threatening bodies: lowest clearance after two seconds at
the current closing speed, then lowest clearance, so closing speed counts and
not distance alone. The sensor counts the bodies left out, but the network
does not read the count yet. Speeds are the ship's velocity relative to the
body's center; planet spin is left out. The world edge's direction points
outward, to its nearest point, and its size is 1. Match worlds keep the
generator's first three planets, so every training world has five bodies: the
sun, three planets and the world edge. Simulating
the ordinary generator at the fresh-check worlds' universe radii, about
800–1,050, gives 1–6 planets per world, usually 2–4, so other worlds may need
the spare slots.

### Arc ring

Sixteen arcs of 22.5° around the ship. Each arc reports the closest surface
inside it, so nothing falls between arcs.

| Field | Values | Meaning |
| --- | ---: | --- |
| Nearest distance | 1 | Closest surface point inside the arc, at any distance, through the same bounded transform. |
| Fill | 1 | Fraction of the arc's angle in which surface lies within 400 units, 0–1. |
| Kind of nearest | 3 | One-hot: sun, planet, world edge. |
| Closing speed of nearest | 1 | Along the direction to that point. |
| Direction | 2 | Unit vector to the arc's center, fixed for each arc. |

The ring has 128 values. Each arc takes the minimum distance rather than an
average, because averaging a small close object with far empty space reads as
far away. Fill carries size: a distant body covers part of one arc and a near
one covers several, so detail is coarse far away and fine nearby. Only fill
has a range: the world edge surrounds the ship, so without one every arc would
be full. The true nearest point always lies in some arc, so the smallest arc
distance equals the exact clearance.

### Why both

The ring covers everything regardless of count or shape, and keeps working
when shapes stop being round. The records give exact values per body: precise
bearing, motion, size, kind, and the destination's own slot. A later
experiment can test whether the ring alone is enough for hazards.

### Encoding rules

- Distances use `100 / (100 + d)` and speeds `v / (|v| + 70)`, with the nine
  inputs' distance and speed scales, instead of a clamp. Far bodies approach
  zero and fast ones approach ±1 rather than saturating. The match harness ends
  an episode when any input saturates, so a clamp would turn ordinary distant
  bodies into failures.
- An empty slot has presence 0 and every other value 0. An arc with no surface
  in range reports fill 0. Both are observed facts. A measurement that could
  not be made needs its own mask; it must not read as empty space.
- The nine existing inputs keep their meaning, order and scales, and come
  first.

Together with the nine inputs, a decision reads 225 values, about 0.9 KB as
`f32`, instead of 36 bytes.

## Network

The V1 model format is a chain of dense layers
([`model.rs`](../../crates/spacewars-ai/src/model.rs)), so shared encoders need
a new model version. Version 2
([`model/shared.rs`](../../crates/spacewars-ai/src/model/shared.rs)) adds set
encoders: each applies one dense layer to every item of a fixed-capacity set
and max-pools the outputs. Existing genomes keep V1 and replay unchanged.

- **Body encoder.** One small layer applied to every present body record, with
  its outputs max-pooled across bodies. Empty slots are excluded from the pool
  rather than fed through the encoder.
- **Arc encoder.** One small layer applied to every arc, including its
  direction, max-pooled across arcs.
- **Trunk.** The nine inputs plus both pooled summaries, a tanh hidden layer,
  and the same three outputs: turn, thrust and brake.

Because the encoders share weights, more bodies or arcs cost time but add no
weights. Start with one-layer encoders of width 4 and 8 hidden units, 255
weights in all, and widen only if the network cannot fit the training cases.

Warm-start from a dense parent, `e0f25b68` or the time-ranked `e03d3e29`. Copy
its weights into the trunk and zero the trunk weights that read the new
summaries, so the new network starts out behaving exactly like its parent.
Give the encoders small random weights rather than zeros: uniform in ±0.5,
from a seed. If an encoder weight and the trunk weight reading it were both
zero, no single mutation could change behavior and the search would stall.

## Cost

These are historical measurements and estimates for the earlier experiment.
The search-effort column scales by parameter count; it is not measured learning
cost and does not predict PPO convergence or justify rejecting a larger PPO
policy. Benchmark the new environment and learner separately.

Measured on the development machine (Ryzen 7 9800X3D, one thread) with the
`ci` profile's optimization settings, using standalone copies of
`DenseModel::infer`'s loop and of circle geometry for the new blocks.

| Design | Weights | Inference per decision | Search effort, rough |
| --- | ---: | ---: | ---: |
| Today, 9→8→3 | 107 | 0.07 µs | 1× |
| Small shared: width-4 encoders, 8 hidden units | 255 | 0.8 µs | ~2.4× |
| Shared: two-layer width-8 encoders, 16 hidden units | 779 | 2.3 µs | ~7× |
| Flat: 193→16→3 | 3,155 | 1.2 µs | ~30× |
| Flat: 193→64→32→3 | 14,595 | 4.9 µs | ~140× |

A simpler standalone version computed the records and the ring from circles
in 0.19 µs with 6 bodies, 1.05 µs with 32 and 3.3 µs with 100. The exact
version measured in match worlds takes 0.9 µs; see
[sensing in match worlds](#sensing-in-match-worlds). In the
[warm-start check](../learned-guidance-perception-warm-start.md#cost), the
implemented sensor took 0.92 µs per decision and the 255-weight network's
inference 0.78 µs, on 32 match states. For scale, a
precision-episode tick costs about 18 µs on one worker, a match tick about
0.5 ms, and a game tick allows 16.7 ms. Precision episodes take 77% of
training time; see the
[training performance report](../learned-guidance-training-performance.md).

**Runtime is not the constraint.** The small shared design adds about 5% to a
precision tick, the 779-weight design about 14%, and neither is noticeable in
a game. Moving from 8 to 32 body slots keeps 779 weights and raises inference
from 2.3 to 4.5 µs.

**The search is.** Recent runs change each weight with probability 0.05, by up
to ±0.05 or ±0.15. With 107 weights, a child differs from its parent in about
five. At
3,155 weights about 158 change at once, and most children get worse; lowering
the rate to keep five changes means each weight is tuned 30 times less often.
The last column assumes, as a rule of thumb from evolution strategies, that
the generations needed grow in proportion to the weights searched. It is an
estimate, not a measurement. This is why the flat networks are rejected even
though some run faster than the shared ones.

**Episodes multiply cost directly.** The mutation-scale run evaluated 16
candidates × 631 episodes × 240 generations per arm, about 2.4 million
episodes; its two arms and final evaluation took about four hours. A training
set that varies worlds needs more cases, and that multiplier may exceed the
search one.

**Traces grow.** A trace row is about 4.5 KB per tick, mostly ship state, and
the nine encoded inputs take about 280 bytes of it. 193 inputs in the same
format would take about 6 KB, more than doubling candidate traces. Replays are
deterministic, so inputs can be recomputed on demand instead of logged.

### Sensing in match worlds

Planets are physically built from terrain chunks
([`TerrainGeometry`](../../crates/engine-terrain/src/lib.rs)), so exact
surfaces could need physics queries instead of circles. Milestone 2 timed
those queries against the physics world of every training world: the 121
worlds behind the known approaches, under both dirt shapes, at 146,540 points.
The points sit around each planet and the sun at fixed altitudes, and across
open space. Each world has the sun, of radius 200, three planets of radius
15–150 built from 1–85 terrain chunks, and the world edge, at universe radius
709–1,282: 29–199 colliders in all. The run used a release build on one thread
of the same machine.

| Sensing per decision | Queries | Mean | 95th percentile |
| --- | ---: | ---: | ---: |
| Arc ring from circles | — | 0.9 µs | 1.0 µs |
| Body records from circles | — | 0.03 µs | 0.04 µs |
| Ray ring, 1 ray per arc, range 1,000 | 16 | 4.5 µs | 6.6 µs |
| Ray ring, 4 rays per arc, range 1,000 | 64 | 16 µs | 23 µs |
| Ray ring, 8 rays per arc, range 1,000 | 128 | 30 µs | 44 µs |
| Ray ring, 8 rays per arc, range 250 | 128 | 21 µs | 38 µs |
| Ball ring, radius 4, range 1,000 | 16 | 43 µs | 84 µs |
| Body records from point projection | 5 | 7.5 µs | 11 µs |

A ray costs 0.16–0.28 µs, a ball cast 2.1–2.7 µs and a point projection onto
one body 1.5 µs. At the approach altitudes, 85 and 120 above a planet, every
method costs about the same or slightly less.

**Circles are exact in these worlds.** The sun is a ball collider. Planet
terrain has an interpolated surface on a circle of 0.99 times the planet
radius. Along the same ray directions, 99% of planet hits agree with that
circle within 0.02 units, and every projected planet clearance does. The world
edge is a 192-segment polygon inside its circle, so its clearance reads about
0.1 units short, at most 0.17. Fewer than 4 rays in 100,000 disagree with the
circles about whether or what they hit, all at grazing angles or at the range
limit.

**Rays cannot give the ring's arc minimum.** Each arc reports its closest
surface, which often lies between rays or on the arc's boundary. With eight
rays per arc at range 1,000, about 1% of arcs that contain a planet miss it,
and 5% misread its distance by more than 14 units. Four rays per arc miss 2%
of those arcs, and one ray per arc misses 12%. The circle ring computes the
minimum exactly.

**Decision: circle geometry for both blocks.** It is exact for the sun, intact
planets and the world edge. It costs 0.9 µs per decision, about 5% of a
precision tick. An eight-ray ring costs 30 µs, more than a whole precision
tick, and still misses bodies. Physics queries are needed only once surfaces
stop following circles, through terrain damage or later non-round planets.
The sensor should then switch per body: point projection for a changed
planet's record and rays only in the arcs it covers, with circles kept for the
sun and the edge. Ball casts cost ten times a ray, so they are not
recommended.

These worlds were sampled two ticks after creation. Impacts dig craters and
release loose dirt during a match, and the benchmark did not measure how much
terrain has changed by the median approach start, tick 1,127. At handover the
ship is 85–120 above the destination's surface, so changes of a few units
there alter its clearance by a few percent.

The benchmark is the ignored test `guidance_sensing_cost` in
[`sensing_cost_tests.rs`](../../scenarios/spacewars/src/surface_sortie/tests/sensing_cost_tests.rs),
which also checks the circle ring against dense circle rays. Its full output,
including every world's composition and the error distributions, is
[`guidance-perception-sensing-cost-v1.json`](../data/guidance-perception-sensing-cost-v1.json).
Point projection was added to the physics world as
[`PhysicsWorld::project_point`](../../crates/engine-rapier/src/world/point_projection.rs)
for this measurement.

## Implementation

- **Bodies.** [`SurfaceSortieState::celestial_bodies`](../../scenarios/spacewars/src/surface_sortie/free_flight.rs)
  reads the sun, every planet and the world edge as circles from the
  completed step, without queries: the sun's ball, each planet at 0.99 of its
  radius and the edge around its center. A test checks each circle against
  point projection onto that body's colliders. In a match world they agree
  within 0.003 units for planets and 0.03 for the edge. The practice world's
  planet is built of square blocks, so its surface lies 0.35–0.45 units
  outside its circle.
- **Sensor.** [`guidance::perception`](../../crates/spacewars-ai/src/guidance/perception.rs)
  builds the records and the ring and encodes them. A test checks every arc
  of 48 generated worlds against 2,049 rays.
- **Model.** Version 2 is
  [`SharedModel`](../../crates/spacewars-ai/src/model/shared.rs).
  `perception::warm_start` builds the 255-weight genome from a nine-input
  parent.
- **Controller.** `Policy::Perception` holds a version 2 model, and
  `Controller::decide_perceiving` passes it the blocks. Other policies ignore
  them, so existing callers are unchanged, and only a perception policy's
  decision records its encoded blocks.
- **Runners.** [`ship_perception::run_episode`](../../crates/engine-agent/src/ship_perception.rs)
  runs precision episodes and
  [`guidance_match::run`](../../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_match.rs)
  match approaches. Both are copies of the pinned lineage runners, extended
  to perception policies, and reproduce them for dense models.

## Training set changes

The current 621 known cases can teach these inputs little:

- All 157 precision starts use one practice world with one planet and no sun.
- All 464 match approaches come from generated worlds with a sun of radius
  200 and three planets, against the other Recovery bot, with asteroids off.
  Most start at the first eligible moment of a native approach.

A training set for this design needs worlds with and without a sun, varied
planet counts and sizes, and approaches where hazards change the right action,
including low-margin starts near the sun's route buffer. Two training changes
help contain the cost:

- Rotate world batches between generations, keeping episodes per candidate
  flat at the price of noisier fitness.
- Give each block its own mutation rate, so the tuned weights move slowly
  while the new ones explore. Training uses one rate for every weight today
  ([`evolution.rs`](../../crates/engine-agent/src/evolution.rs)).

How attempts are judged matters as much as which worlds are used. Today
native control's climb, sun-route and detour rules end training attempts, so
sun inputs could only teach the component to stay out of native control's
route buffer. Judging physical outcomes instead, as the match plan's
[objective](learned-guidance-match-v1.md#objective-beat-native-control)
proposes, is what lets the new inputs pay off.

## Open questions

- Whether a fill range of 400 and a distance scale of 100 suit hazards.
- Whether the threat rule picks the right bodies when more than seven compete
  for slots. The eight-planet worlds of the
  [varied-world sets](../learned-guidance-varied-worlds.md#more-bodies-than-slots)
  have nine or ten bodies, and the rule leaves out only bodies at least 1,045
  units away; whether that suits training is for milestone 6.
- How much terrain changes before and during approaches, and when a planet
  should switch from its circle to queries.
- Whether max pooling is enough or the encoders also need a count channel.
- Whether costly queries can run every few ticks instead of every tick.

## First steps

These steps are milestones 2 and 4–7 in the match plan, whose
[milestone list](learned-guidance-match-v1.md#milestones) tracks progress.

1. Done: time Rapier queries in real match worlds and choose between circle
   geometry and queries. Circles won; see
   [sensing in match worlds](#sensing-in-match-worlds).
2. Write the field-and-frame inventory and the
   [distinction probes](organism-perception.md#checks-before-freezing-an-encoder)
   for these blocks, including identical nine-input states with and without
   the sun near the route.
3. Done: implement the sensor and the new model version with the warm start,
   and check that it reproduces its dense parent, `e0f25b68` or the
   time-ranked `e03d3e29`, on all 621 known cases. See the
   [warm-start report](../learned-guidance-perception-warm-start.md).
4. Done: build the varied-world training set and freeze a separate fresh
   holdout before training. See the
   [varied-worlds report](../learned-guidance-varied-worlds.md).
5. Train the 255-weight network with time saved against native control in its
   ranking, and widen it only if it underfits.
6. Switch training to physical judging, then compare with native control from
   identical starts.
