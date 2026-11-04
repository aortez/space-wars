# Learned-flight task generator v1

The generator now produces reproducible candidates for all four flight families,
checks them independently of a controller, and fills equal accepted quotas while
retaining every attempted rejection. Three development seeds produced **192
accepted tasks from 240 attempts**, with 48 accepted tasks per family. This is a
scoped task-generation checkpoint; no policy was trained or flown.

[Interactive inspection](../data/training-experiments/learned-flight-tasks-v1-20261022/run-20261022/index.html)
· [Compact results](data/flight-task-generator-v1-results.json)
· [Archive receipt](data/flight-task-generator-v1-archive.json)
· [Contract](design/learned-flight-contract-v1.md)

The gallery filters accepted/rejected attempts, draws geometric route witnesses,
animates the prescribed target position, and compares geometry before and after
alterations. It also displays rotation bounds, sampled gravity and the deadline
estimate. Route lines are evaluator evidence and never enter policy inputs.

## Declared diagnostic recipe

`FlightCandidate::generate(seed, index)` deterministically generates one
candidate with a versioned SplitMix64 sequence. Index modulo four selects clear,
moving, obstructed or altered tasks. Each family has a declared procedural
recipe; individual failures are not appended to a hand-maintained dataset.

| Family | Recipe and initial speed |
| --- | --- |
| Clear | Stationary goals 30–200 units away; independently sampled velocity direction and heading; initial speed 0–70 units/s. |
| Moving | Same initial distance/speed range, with constant world-space target velocity of 5–20 units/s. |
| Obstructed | Cycles through walls, open U-shaped obstacles and curved corridors; initial speed 0–15 units/s. The direct route must be blocked. |
| Altered | Alternates a carved bay providing a passage through formerly solid terrain and a newly built protruding wall; initial speed 0–12 units/s. |

Headings cover the full circle and spins −1.8 to +1.8 rad/s. Ordinary starts use
altitudes 100–150; carved passages use 15–25. Goal distances for the structured
families follow their geometry rather than a uniform 30–200 draw. Obstructed and
altered tasks deliberately start slower near geometry; this is an explicit
conditional distribution, not coverage of all obstacle starts at 70 units/s.

Worlds use the existing one-planet practice fixture, with seeded zero/production
gravity and planet spin selected from −0.015, zero and +0.015 rad/s. Carved bays
use stationary terrain. Wall height, width, orientation, corridor curvature and
cut location/size vary procedurally. The source snapshot and exported recipe
metadata retain the exact sampling implementation. No generated-match planet
sizes/masses, sun, moving debris, planet-attached targets or ongoing edits are
part of this initial navigation recipe.

`FlightTaskBatch::generate(seed, per_family, max_rounds)` admits up to one
candidate per unfilled family each round. Filled families skip later indices;
other candidate identities do not change. Every attempted candidate and its
first rejection reason are retained. A bounded run that cannot fill a quota
returns `complete: false`; it does not silently change family weights or loop
forever. The inspection used 16 accepted tasks per family and a 128-round limit.
All three seeds are development evidence, not a held-out evaluation partition.

## Admission guarantees and limits

The completed reset must expose a supported policy observation and a clear full
hull/feet placement. Bounds are recomputed from actual enabled solid shapes about
the physical COM. The conservative enclosing radius is approximately **5.982**
units, larger than the earlier tight outline measurement of 5.507. The 12-unit
goal-clearance requirement therefore leaves approximately **3.018** units beyond
the bound plus the 3-unit arrival tolerance. The required extra margin is at
least one unit.

A native capsule intersection query checks the target's entire straight-line
motion over its deadline with radius 12, together with inward world-edge
clearance. Clear endpoints alone cannot admit a trajectory through a thin wall.
Static geometry is queried in its current edited form. Rotating planets also
contribute a conservative full-rotation bound about their stationary origin.
This covers future rotating geometry without pretending the initial shape is
stationary, but excludes some physically safe routes through that envelope.
Unsupported moving body origins and independent dynamic bodies are rejected.

Routes use a COM-centered enclosing disk plus 0.25 units of margin. Every edge
is checked by a continuous native capsule query. An 8-unit A* grid finds a
witness when the direct segment is blocked; checked visible shortcuts remove
grid zigzags. A failure to find this conservative witness is **not proof of
physical disconnection**: narrower, orientation-dependent or off-grid paths may
exist. No partial path, route direction, safety flag or native action reaches
the learned policy. A witness is not an action sequence or a dynamic trajectory.

The deadline screen uses this inspectable reference estimate, in seconds:

```text
(5 + 1.9 × route_segments + route_length / 35) / (1 − target_speed / 35) + dwell
```

The five-second reserve covers acceleration/braking/settling approximately; the
segment allowance uses the calibrated approximately 1.9-second half-turn. This
is an engineering screen, **not a proven upper bound**. It does not simulate
acceleration at each corner, gravity compensation, initial momentum or timed
interception. The accepted estimates ranged from 8.50 to 25.56 seconds against
the 30-second deadline. Actual deadline adequacy remains a behavior-level risk
to inspect during the bounded learner smoke; passing this screen is not a claim
that every admitted task is dynamically feasible.

Gravity samples use the production spherical-source solver along each witness
and the full goal path at intervals of at most eight units. A separate sample
checks the game's actual legacy ship gravity anchor, which is offset from the
body origin by `SHIP_PIVOT`; that existing physics convention is unchanged.
Route points sample the field, not a heading-dependent flown trajectory. Terrain
carving retains the practice fixture's ordinary gravity model. The largest
accepted sampled magnitude was **18.178 units/s²**, with zero-gravity cases and
both rotation signs represented. This is sampled coverage, not a global bound
on every point or a claim about generated-match gravity.

## Inspection results

| Family | Attempted | Accepted | Rejected |
| --- | ---: | ---: | --- |
| Clear | 52 | 48 | 4 unsafe goal trajectories |
| Moving | 91 | 48 | 43 unsafe goal trajectories |
| Obstructed | 48 | 48 | 0 |
| Altered | 49 | 48 | 1 unsupported independent moving body |
| Total | 240 | 192 | 48 |

An early development cut through the planet diameter detached a large terrain
piece. The final recipe carves a bay instead, while still rejecting any edit
that creates an unsupported independent moving body. One such rejection remains
in the retained sample. The counterfactual tests verify that carved passages
change a blocked direct route into a clear one, and construction changes a clear
route into a blocked one. All 15 retained U-shaped cases require initial movement
away from the goal; curved corridors and wall detours also appear in every seed.
The accepted set contains all seven subrecipes, 2–14 global objects, initial
speeds up to 69.922 units/s and target speeds up to 19.294 units/s.

Every accepted task reconstructed identical policy inputs after task JSON
serialization and independent reset. Eight focused generator tests cover seed
identity, admission replay, sealed/open U cases, continuous moving-goal and edge
clearance, meaningful edits/additions, rotation envelopes, time rejection,
gravity agreement, read-only inspection, unsupported body motion, equal quotas
and budget exhaustion. The physics-bound test also checks an offset center and
non-finite input. Full regressions passed **650 scenario tests (two ignored)
and 175 physics tests (one ignored)**. Targeted formatting and whitespace checks
passed. Clippy completed with the same 18 existing warnings and none in the new
generator/probe. Desktop/mobile viewer interactions passed without JavaScript
errors or horizontal overflow.

## Handoff and reproduction

The generator and admission machinery are ready for a scoped C3 integration
smoke using this practice-world envelope. Before that run, record its task
sampling, reward, model/optimizer settings, evaluation metrics, budgets and
checkpoint/restart semantics. A short smoke must test the learning machinery;
it should not be promoted to a navigation success claim. Inspect physical
outcomes and trajectories before spending a long training budget. Broader world
coverage and dynamic feasibility remain explicit limitations.

```sh
cargo run --profile ci -p scenario-spacewars --example flight_task_probe --locked --offline -- --out /tmp/flight-tasks-new --seed 20261022 --per-family 16 --max-rounds 128
```

The directory must be new. Open its `index.html` locally. Each `task-NNNN.json`
retains its candidate, rejection/evidence and geometry; `results.json` contains
compact coverage, accounting and the recipe metadata. A short quota returns a
nonzero CLI exit after retaining partial evidence.

The archive under `data/training-experiments/learned-flight-tasks-v1-20261022/`
retains all three batches, the exact executable, source snapshot including dirty
and untracked build inputs, browser checks/screenshots and regression logs. It
contains no model weights, toolchain binaries or Cargo registry dependencies.
This is local retention, not an off-machine backup.
