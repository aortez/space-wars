# Varied worlds for learned arrival guidance

**Status: complete (2026-10-19).** Milestone 5 of the
[match hybrid plan](design/learned-guidance-match-v1.md#milestones).

A frozen training set of 1,370 match starts and a separate holdout of 738,
from lab worlds with and without a sun and with two to eight planets. Native
control's outcome is recorded for every start, and both parents' for every
training start. On the training set the parents complete slightly more starts
than native control but are much slower, almost entirely in worlds without a
sun. See the [results](#results).

## Goal

Build a training set and a separately frozen holdout from worlds whose hazards
vary: with and without a sun, with two to eight planets, and with starts close
to native control's margins. Milestone 6 trains the perception network on the
first and must beat native control on the second.

## Starting point

- **One kind of world.** Every match world comes from the game's generator: a
  sun of radius 200 at the center and planets on circular orbits around it, of
  which the arena keeps the first three. Seats start on the first and third
  planets. The world edge is a circle around the sun, 200 units beyond the
  outermost planet's farthest reach.
- **Known starts.** The 482 known match starts come from 121 such worlds, so
  the network had never seen a world without a sun or with another number of
  planets. Destination sizes already vary: radii run from 15 to 150, median 89,
  the generator's full range.
- **One start per seat.** Each world contributed each seat's first eligible
  approach, under each of two dirt shapes, which are near-duplicates in this
  phase. In the two census worlds of the original approach census, native
  control arrives 2–7 times per seat in the three-minute prefix, so most
  approaches went unused.
- **Few low-margin starts.** In the fresh check, only nine situations started
  within 15 units of native control's sun buffer. Six of them, 0.4–7.2 units
  out, produced 12 of the candidate's 16 misses.

## What native control needs

- Its code treats the sun as optional. The route check, landing safety and
  capture checks all accept a world without one, and its planner handles up to
  eight planets.
- Planet orbits are computed around the sun, and the transfer forecast refuses
  an orbiting world without one. A sunless world therefore needs still planets,
  as in the practice world, or a new orbit model.
- No match had run without a sun or with other than three planets, although
  the two-planet travel fixture flies native control between still planets
  without a sun, under its older mission policy. The [census](#census) showed
  that the Recovery bot plays and approaches planets in every kind of world.

## Design

### Worlds

A lab builder, `SurfaceSortieScenario::init_material_match_layout`, builds a
world from a seed and an `ArenaLayout`. Matches and the launcher are unchanged,
and the standard layout is today's match world exactly.

- **Sun.** Present, with radius 200 as today, or absent. Without a sun the
  planets keep their generated places and spin where they are, so destinations
  are still. Moving destinations then come only from sun worlds and the
  precision markers.
- **Planets.** 2, 3, 5 or 8: the generator's first that many, so sizes keep
  its 15–150 range. When a seed has too few planets in the standard region,
  the generator runs over a wider one. Other planets become obstacles on more
  routes, and eight-planet worlds have more bodies than the sensor has slots.
- **Edge.** 200 units beyond the outermost planet, as today, except in one edge
  cell where it is drawn from 120–320 by a hash of the seed. The practice
  world's edge is the nearest surface in 82% of its arcs, and varying it in
  matches keeps the network from tying the edge to precision stopping.
- **Seats.** On the first and third planets, or the second in two-planet
  worlds. Each ship starts on its planet's side away from the sun or, without
  one, away from the world's center.

That makes nine cells: sun or none, times four planet counts, plus the edge
cell with a sun and three planets. The cell with a sun and three planets is the
standard match.

### More bodies than slots

The sensor has eight body slots. The destination always takes the first, and
the others go to the bodies with the least clearance after two seconds at
their current closing speed. Bodies left out get no record, but the arc ring is
built from every body, so each still shows wherever it is the nearest surface
in an arc, and in fill. The count left out is recorded but is not an input.

The body encoder is one layer shared by every slot and max-pooled, so its
weights do not depend on the number of slots. A sensor with more slots could
reuse a trained network unchanged, and it would behave identically in worlds
with eight bodies or fewer. The cap keeps the cost per decision fixed however
crowded a world gets.

Before this milestone, worlds had at most five bodies. With a sun, an
eight-planet world has ten bodies, so two are left out; without one, nine.
Native control's planner takes at most eight planets, so the sets cannot go
further, although the network has no limit.

### Starts

Discovery runs the Recovery bot in both seats with asteroids off, as before,
for the same three-minute prefix. Each world uses one dirt shape: angular for
even seeds, round for odd. An approach is a destination since native control
last selected it. Each approach contributes:

1. Its first eligible state. Eligibility is the earlier rule: a direct
   transfer to the destination, within 35 units of the entry point, still
   valid after a neutral tick, with no saturated input.
2. Its eligible state closest to the route buffer of the sun or another planet,
   the body's radius plus 65, when that margin is under 30 units, lower than at
   any earlier start of the approach, and the ship is still at least 10 units
   outside native control's capture distance, the destination's radius plus
   105.

No rule looks at any controller's outcome. Each start records its cell and
layout, its margins, including the climb boundary at altitude
70 + falling²/50, the edge's distance, the destination's radius and speed, and
what the sensor reads there.

### Sets

| Set | Size | Seeds | Use |
| --- | --- | --- | --- |
| Census | 4 worlds per cell | 96100001–96100036 | Development only |
| Training | 150 starts per cell | from 96200001 | Milestone 6 training, with the 621 known cases kept for retention |
| Holdout | 60 starts per cell, 240 in the standard cell | from 96300001 | Milestone 6's final evaluation only |

Each cell takes consecutive seeds from its own range of 1,000 until it has its
starts, keeping whole worlds. The standard cell's holdout is larger because
qualification runs in standard matches; 240 is about the size of milestone 1's
fresh set. No training or holdout seed appears in any of the 1,834 earlier
plan, input and source files scanned. Until milestone 6's final evaluation,
only native control runs on the holdout, so no model result can shape
training.

### Checks

- The standard layout reproduces the match world: identical world fingerprints
  for five seeds over 90 steps before and after the builder changed, a unit
  test, and, in the frozen run, the lineage's own runner for four standard
  worlds over 600 ticks.
- The 482 known starts rebuild exactly. Milestone 4's full check also passes on
  the new build, with results identical to its frozen package.
- Unit tests build every planet count with and without a sun, and both edge
  extremes, and check the bodies, seats, edge distance and that sunless planets
  stay still. As in milestone 4's test, the body circles match point
  projection, here in eight-planet worlds with and without a sun and a
  two-planet world without one.
- Every start rebuilds from its seed to its recorded state.
- Every start's perception values are finite and within ±1, and a warm-started
  child flies a sample of the new starts exactly as its parent.

## Census

A development run on the release build: four worlds per cell, the full
three-minute prefix, and native control and both parents from every start. It
took 37 seconds.

| Cell | Starts | Native | `e0f25b68` | `e03d3e29` | Median ticks, native / `e03d3e29` |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sun, 2 planets | 13 | 11 | 13 | 13 | 47 / 54 |
| Sun, 3 planets (standard) | 15 | 15 | 15 | 15 | 46 / 41 |
| Sun, 5 planets | 17 | 17 | 17 | 17 | 44 / 40 |
| Sun, 8 planets | 24 | 24 | 24 | 24 | 47.5 / 53 |
| No sun, 2 planets | 11 | 10 | 10 | 10 | 44 / 46.5 |
| No sun, 3 planets | 15 | 14 | 13 | 13 | 47 / 54 |
| No sun, 5 planets | 23 | 23 | 23 | 23 | 40 / 61 |
| No sun, 8 planets | 19 | 19 | 19 | 19 | 41 / 55 |
| Sun, 3 planets, edge 120–320 | 18 | 17 | 17 | 17 | 46 / 49 |

The controller columns count completions. Median ticks are over the starts all
three complete.

- **Native control plays every cell.** No world raised an error, and each
  world's latest start rebuilt exactly from its seed. Rerunning the census
  reproduced it exactly.
- **Yield varies.** Worlds gave 2.75–6 starts each, 4.3 on average. Two-planet
  worlds give the fewest: six of their eight matches ended early with a
  pilot's death.
- **Overflow.** Every eight-planet start left bodies out, and only far ones.

Two rules changed after the census:

- **Low-margin starts.** As reviewed, the rule took any of native control's
  margins under 30 units. Every approach passes that close to the climb
  boundary as it descends to the entry, so the rule picked a state just before
  capture in nearly every approach, and every controller then arrived within
  a few ticks. The rule now uses only the route buffers, and only states at
  least 10 units outside the capture distance. The climb misses of the fresh
  check started 0.5 and 4.6 units above the boundary, at each approach's first
  eligible state, which the sets keep anyway.
- **Set sizes.** By starts rather than worlds, since yield per world varies.

## Results

The frozen run took 4 minutes for discovery and 2 for the baselines on the
release build. A dry run from a draft plan beforehand produced byte-identical
starts and episodes.

### Sets

| Cell | Training starts | Worlds | Low-margin | Holdout starts | Worlds | Low-margin |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sun, 2 planets | 154 | 38 | 20 | 61 | 20 | 7 |
| Sun, 3 planets (standard) | 150 | 34 | 13 | 240 | 54 | 12 |
| Sun, 5 planets | 151 | 28 | 10 | 66 | 13 | 2 |
| Sun, 8 planets | 156 | 24 | 8 | 61 | 11 | 2 |
| No sun, 2 planets | 151 | 51 | 0 | 61 | 21 | 0 |
| No sun, 3 planets | 152 | 39 | 0 | 64 | 15 | 0 |
| No sun, 5 planets | 152 | 29 | 0 | 61 | 16 | 0 |
| No sun, 8 planets | 153 | 27 | 0 | 61 | 15 | 0 |
| Sun, 3 planets, edge 120–320 | 151 | 36 | 10 | 63 | 15 | 4 |
| All | 1,370 | 306 | 61 | 738 | 180 | 27 |

Low-margin starts come only from worlds with a sun; without one, other planets
rarely come near a route.

### What the sets cover

- **Near the sun.** 142 of the 762 training starts in sun worlds begin within
  15 units of the sun's route buffer, 83 of them inside it while heading out,
  which native control allows. In the holdout, 64 of 491 do.
- **Other hazards.** 16 training starts lie within 30 units of another
  planet's buffer, and 25 within 15 units of the climb boundary.
- **Edge.** The edge is 97–2,103 units from the ship, median 640, and 261–1,021
  in the edge cell.
- **Destinations.** Radii run from 15 to 150 in every cell, median 85.
  Destinations move at 8–22 units/s in sun worlds and are still without one.
- **Bodies.** Starts see 3 to 10 bodies. All 309 eight-planet training starts
  leave bodies out: a planet 378 times and the edge 87 times, each at least
  1,045 units away, median 1,837.

### Baselines on the training set

| Cell | Native | `e0f25b68` | `e03d3e29` | Median ticks, native / `e03d3e29` | `e03d3e29` faster / slower |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sun, 2 planets | 131 | 129 | 129 | 49 / 48 | 66 / 44 |
| Sun, 3 planets (standard) | 135 | 135 | 136 | 46 / 49 | 55 / 67 |
| Sun, 5 planets | 143 | 144 | 143 | 45 / 49 | 57 / 76 |
| Sun, 8 planets | 145 | 152 | 152 | 47 / 53 | 53 / 88 |
| No sun, 2 planets | 147 | 146 | 146 | 41 / 53 | 41 / 98 |
| No sun, 3 planets | 147 | 148 | 148 | 41 / 56 | 35 / 110 |
| No sun, 5 planets | 150 | 150 | 150 | 41 / 57.5 | 29 / 116 |
| No sun, 8 planets | 151 | 151 | 151 | 41 / 52 | 39 / 106 |
| Sun, 3 planets, edge 120–320 | 130 | 133 | 132 | 47 / 49 | 54 / 62 |
| All | 1,279 | 1,288 | 1,287 | 43 / 52 | 429 / 767 |

The controller columns count completions out of the cell's starts. Medians,
faster and slower are over the starts both native control and `e03d3e29`
complete.

- **Completions.** Both parents complete slightly more training starts than
  native control, 1,287–1,288 against 1,279. Each loses 34–35 of native
  control's successes and completes 43 of its failures.
- **Time.** Counting a failed start as 300 ticks, `e03d3e29` takes 6,709 ticks
  more than native control in all, and `e0f25b68` 7,500 more. Worlds without a
  sun account for 7,001 of `e03d3e29`'s deficit. There native control's median
  is 41 ticks in every cell, against the parents' 52–57.5. In sun worlds
  `e03d3e29` is 292 ticks ahead overall, and 17 behind in the standard cell.
- **Failures.** Native control's 91 failures: 48 at its climb guard, 12 avoiding
  the sun, 7 avoiding another planet, 11 when the host switched to hunting the
  opponent, 8 to damage, 3 to contact, one frame change and one loss of
  control. The parents' 82–83: 39 avoiding the sun, 13–15 at the climb guard,
  10 when the host switched to hunting, 7 to damage, 6 avoiding another planet,
  4–5 to contact, one frame change and one loss of control.
- **Near the sun.** All 39 of each parent's sun-avoidance failures come from
  the 142 starts within 15 units of the sun's buffer, including 16–17 of the 83
  that begin inside it. Native control fails 12 of those starts, all 0–5 units
  outside the buffer.

### Native control on the holdout

Native control completes 683 of the 738 holdout starts, with a median of 45
ticks, and 209 of the 240 in the standard cell. Of its 55 failures, 41 are at
the climb guard, 5 avoid another planet, 4 come when the host switched to
hunting, 2 at frame changes, 2 at contact and 1 at damage. No model has run on
the holdout.

### Warm start in the new worlds

Outside the frozen package, both of milestone 4's warm-started children flew
the first two training starts of every cell, 18 in all, exactly as their
parents. All 36 episodes and 1,797 decisions were identical, and every
perception value stayed within ±1, including in the eight-planet worlds.

## Reading

- **The sets are ready for milestone 6.** Every start comes from native
  control's own approaches with no outcome filter, rebuilds from its seed, and
  has native control's result recorded. No model has run on the holdout.
- **The parents are slow without a sun.** They match native control's
  completions but fall far behind in time where the sun is absent. Those
  worlds differ from everything the parents trained on: the gravity vector
  has no sun's pull, and the destination stands still. Why that slows them is
  not yet known. Milestone 6 starts from the warm-started children, which read
  the new inputs.
- **Near-sun starts are now plentiful, and that is where the parents lose.**
  Nearly one in five training starts in sun worlds begins within 15 units of
  the sun's buffer, and every one of the parents' sun-avoidance failures starts
  there.
- **Overflow is tested.** Eight-planet starts always leave bodies out, and the
  slot rule leaves out only bodies at least 1,045 units away.
- **Training will cost more.** With 1,370 new match starts besides the 464
  known ones, a 96-generation arm would take about four times milestone 1's 26
  minutes, unless world batches rotate between generations as the
  [perception design](design/guidance-perception.md#training-set-changes)
  suggests.

## Decisions

Reviewed 2026-10-18:

1. **Sunless motion: still planets.** The practice world already works that
   way, and nothing would pull planets around an empty center.
2. **Lab only.** The layouts serve training and evaluation. Matches keep a sun
   and three planets.
3. **More bodies than slots.** Eight-planet worlds, in both sets, so training
   sees overflow and the holdout measures it. See
   [more bodies than slots](#more-bodies-than-slots).
4. **Low-margin starts.** One extra state per approach, near a route buffer;
   revised after the census.
5. **Set sizes.** Revised after the census, as allowed: 150 training and 60
   holdout starts per cell, and 240 holdout starts in the standard cell.

## Progress

- [x] Review the plan (2026-10-18).
- [x] Build the layouts and discovery, with the checks above as tests.
- [x] Census on the release build: native control in every cell, yield,
  margins and cost.
- [x] Freeze the plan: cells, seeds with a novelty scan, discovery rule and set
  sizes.
- [x] Discover both sets and run the baselines on the release build.
- [x] Write the results and update the match plan.

## Out of scope

- Training, in milestone 6, and physical judging, in milestone 7, which needs
  its own fresh set.
- Planet shapes other than circles, and asteroids or the opponent as perceived
  hazards.
- New precision worlds. The 157 arrivals stay as they are. A practice world
  with a farther edge can follow if milestone 6 shows the network keying on
  the edge.

## Code and evidence

- **Worlds.** `ArenaLayout` and the builder are in
  [`surface_sortie/mission.rs`](../scenarios/spacewars/src/surface_sortie/mission.rs),
  with `init_material_match_layout` in
  [`match_rules.rs`](../scenarios/spacewars/src/surface_sortie/match_rules.rs).
  The match arena now calls the same builder with the standard layout. The
  client's `create_layout_match` sits beside `create_match` in
  [`mission.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission.rs).
- **Experiment.** [`guidance_worlds.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_worlds.rs)
  holds the cells, discovery, the rebuild, the census and the frozen stages.
  Milestone 6 can rebuild any start with its `replay`. The warm-start check
  is in [`guidance_worlds_parity.rs`](../crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_worlds_parity.rs).
- **Package.** `data/training-experiments/recovery-guidance-varied-worlds-v1-20261018/`
  holds the copied inputs, the frozen executable and source patch, the sets in
  `discover/` and the baselines in `baseline/`. From that directory,
  `python3 -I run.py verify` checks the package, and `run.py discover` and
  `run.py baseline` rerun the stages. `analyze.py` writes the coverage. The
  [plan](data/learned-guidance-varied-worlds-v1-plan.json) is frozen, and the
  [results file](data/learned-guidance-varied-worlds-v1-results.json) collects
  the discovery summary, the baselines and the coverage.
