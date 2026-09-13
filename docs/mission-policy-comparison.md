# First comparable material mission planner

This is the first implementation slice of [#81](https://github.com/aortez/space-wars/issues/81)
and the [budgeted planning design](design/budgeted-bot-planning.md). It adds a
selectable candidate and a small comparison runner. The candidate improves one
decision: where to land and stand to reach an enemy flag **and get back to the
ship**. Strategic selection, aim, weapons, combat breaks and recovery continue
to use the existing shared tasks.

## Selecting the policies

Ordinary Spacewars exposes `human`, `legacy bot` and `planner bot` for each
player. Any human/bot or bot/bot combination is supported, including two planner
bots. Choices persist across restart. Automatic matches retain selected bots
and fill human seats with the legacy bot. Historical lab registrations remain
pinned to their existing controllers. Classic Spacewars does not support this
material planner and reports an error if its P2 selection is the planner.

| Selection | Concrete mission policy | Objective sensor semantics |
| --- | --- | --- |
| Legacy bot | `material_mission_v9` | Cheapest outward endpoint, then check return |
| Planner bot | `material_mission_v10` | Joint outward and return endpoint selection |

The persisted legacy value remains `rule-bot`; `rule bot` is still accepted by
the launcher parser as a legacy label. The new saved value is `planner-bot`.
Existing defaults are retained. `MaterialMissionPilot::new` remains pinned to
v9. Interactive and headless hosts select through `MissionPolicy`/`MissionBot`;
the same selection supplies the sensor request, controller and report identity.

## Joint trip and physical execution

For each shortlisted landing pose, sensors measure the same directed ground
graph with the proposed hull present. The candidate runs a forward shortest
path search from the hatch and a reverse, multiple-source search from all
boarding footings. Each eligible flag endpoint is scored by the sum of both
distances, using edge length plus two units per jump. Reverse search retains
the original direction of every physical edge. A cheap one-way jump therefore
does not become an invented return jump. Prospective jetpack edges are excluded.

The heap searches and incoming index belong to this map snapshot. They do not
cache collision results across ticks. Candidate selection retains the existing
eight-site shortlist, landing cost conversion, cover weights and sensor cadence.
The legacy profile still calls the original route implementation. Additive
candidate fields are omitted from legacy observation and telemetry JSON.

At actual touchdown, a fresh joint survey selects the flag footing to execute;
the airborne forecast does not authorize exiting by itself. The ground task
walks to that selected footing before falling back to normal claim handling.
Being inside the flag radius is insufficient to finish that approach. It then
uses the existing hatch task to return and physically board. This is a
revalidated endpoint commitment, not an uninterruptible replay of an old path.

Fresh ground maps check the footing and directed return route. A terrain
revision, changed flag, displaced hatch or obstructed return invalidates the
plan. The task can select another complete trip from current measurements; it
waits at most five seconds for a missing trip and retains the overall traversal
deadline. Outward edges, posture, displacement and jetpack crossings retain the
existing movement checks. After reaching the endpoint, existing claim-footing
recovery remains available. Neither the plan nor its diagnostics grants support,
ownership or boarding. Destruction and pilot-death rules are unchanged.

## Reproduction

Build the shared runners with Rust 1.89:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --example surface_flag_soak --features sensor-profile

# First validate the comparison harness with identical policies.
python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --candidate material_mission_v9 --seconds 600 \
  --seeds 9216675843324634618 7725194555774358125 \
  --out /tmp/mission-baseline-check

# Default: v9 versus v10, four fixed fresh seeds, each with swapped roles.
python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --seconds 600 --out /tmp/mission-policy-comparison

# Repeat with sustained random asteroid pressure.
python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --seconds 600 --asteroid-interval 3 --out /tmp/mission-policy-asteroids

# Direct contested flag approach with real landing, capture, boarding, departure.
target/release/examples/surface_flag_soak \
  --policy material_mission_v10 --seed 42 --seat 0 --offset -0.8 \
  --mode capture --jetpacks true --survey-landing true --landing-threat false \
  --edit none --expect complete --out /tmp/planner-flag-trip
```

Output directories must be new. The Python harness runs sequentially, records
the executed binary's SHA-256 and exact commands, checks initial worlds across
role swaps, and checks deterministic results for same-policy comparisons.
Individual reports retain physical audits, ownership, visits, claims, boarding,
departures, recoveries, abandonment reasons and sampled task histories. Timing
CSV records each seat's sensors and controller separately. The optional
`sensor-profile` feature writes sensor counters and stage timings; the summary
aggregates those counters per seat. Long phase durations are review candidates,
not an automatic assertion that the bot is stuck.

Direct runner selection is also available as `--p1-policy` and `--p2-policy`.
Use `--mode duel` to give both seats to those policies. Other runner modes retain
their existing subject/interceptor arrangements. `--seconds` is the simulation
budget; generated matches keep their actual ten-minute match clock and normal
pilot-death termination. Shorter runs may report `unfinished`, separately from
a draw. Equal planet ownership at match expiry remains a draw.

## Validation checkpoint, 2026-09-13

The reference was built from merged main `d574b8d` before implementation. Its
runner SHA-256 is
`db3b582ee5cc8dcf697c40a81aaef7a4a734a68fd34b0ca44fb2b7d59770b4cc`.
The comparison runner for this checkpoint is
`a442ee3173efbc1f1cc133318dbcc836fa700ef86b650fd37483e93f19266f4c`.
Local artifacts are under
`/home/oldman/.codex/visualizations/2026/09/13/bot-sortie-planning/`.

- Two legacy replays match all **23,786** dense player records byte-for-byte:
  observations, encoded actions, controller telemetry and physical diagnostics.
  All 41 non-timing report fields and 12 non-timing CSV columns also match.
- Four same-policy, seat-swapped matches pass deterministic and initial-world
  checks. Both roles record two wins and three completed sorties.
- Directed graph fixtures select a returnable alternate when the cheapest
  outward endpoint is trapped, reject every-blocked-return cases and exclude
  jetpack edges. Sparse-graph tests compare joint cost to exhaustive endpoint
  enumeration using the frozen independent route implementation.
- Controller tests execute the selected endpoint even when already within the
  flag region, follow the directed return path, revoke arrival when a moving
  obstruction removes the return, and bound waiting. Actual-touchdown endpoint,
  profile validation, clone/replay/reset and per-seat policy identity are tested.
- Both mission policies pass the physical two-planet capture/board/departure
  matrix across two seats, two reflections and three starting bearings.
- **24** contested physical trials compare both policies across two seats,
  offsets ±0.8 and no edit / flag destruction / crater. Each completes **9/12**;
  all physical audits pass. All clean and flag-destruction cases complete. The
  same three documented crater cases remain incomplete, with their completion
  assertions still failing; see the [landing investigation guide](landing-investigation-guide.md). The candidate
  reaches its selected footing in all ten trials where it captures, including
  the known case that captures and departs without finishing the tactical task.
- All **551** AI/scenario tests and **306** client tests pass. The rendered
  player-choice workflow passes under private Xvfb through launch, both
  renderers, pause, restart and persisted selection. Clippy completes with
  existing scenario warnings; formatting and whitespace checks pass.

The development seeds are `9216675843324634618` and `7725194555774358125`.
Held-out seeds are generated before examining outcomes from the first eight
SHA-256 bytes, big-endian, of `mission-policy-comparison-v1:0` through `:3`:
`2979685229031930359`, `10230489255960971764`, `2557666543469500907`, and
`1075914996979132958`. Both roles play both physical seats in each world.

All 24 comparison matches (including the four baseline checks) finish within
the real ten-minute clock and pass their physical audits. The 20 candidate
versus legacy matches give this small-sample result:

| Suite | Matches | Legacy wins | Planner wins | Legacy completed sorties | Planner completed sorties |
| --- | ---: | ---: | ---: | ---: | ---: |
| Development, no asteroids | 4 | 2 | 2 | 3 | 3 |
| Held-out, no asteroids | 8 | 3 | 5 | 30 | 28 |
| Held-out, asteroid every 3 seconds | 8 | 4 | 4 | 10 | 13 |

There are no draws or budget-truncated matches in this set. The planner's 11–9
record is not evidence of a reliable strength advantage. On quiet held-out
worlds it wins more matches but completes fewer sorties; under asteroid pressure
it completes more sorties with equal wins. This is useful comparison evidence,
not a reason to promote it to the default. Raw summaries preserve per-world
regressions and abandoned visits rather than hiding them in aggregate wins.

## What remains

This slice establishes a selectable, executing candidate and reproducible
comparisons. It does **not** establish stronger play across the game. Neutral
planet capture, combat and much of each match remain shared with v9; a changed
footstep can also change later collision and combat outcomes.

Both policies remain cadence-limited and **unbounded in planning work**.
Reports explicitly record a null work quota. Timings are instrumented desktop
diagnostics on different trajectories, not equal-budget scores or Pi frame-time
guarantees. Ground-connection physics still dominates expensive observations.
The next [graph-job checkpoint](bot-planning-jobs.md) adds a resumable v10 solver,
shared scheduler and offline quota probe while preserving synchronous live
behavior. Coherent physical surveys and the live budget adapter still precede
broader strategic choice. Prospective jetpack
resource planning, cross-update sensor caches and improved combat stay separate
experiments. Keep v9 selectable and the default while those comparisons grow.
