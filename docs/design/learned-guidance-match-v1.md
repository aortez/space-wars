# First learned-guidance match hybrid

**Status: superseded 2026-10-22 by the
[learned flight controller plan](learned-flight-controller-v1.md).** The new
direction uses physical task outcomes, continuous skill ownership and a fresh
PPO-trained flight policy. Native-relative qualification, temporary delegation
and native fallback below are historical proposals, not current requirements.
The remaining milestones are retired; the experiment history is preserved.

This was the integration plan for [#291](https://github.com/aortez/space-wars/issues/291).
Use Recovery bot v1 as the host and test a retained arrival component during
one short, unobstructed approach to a destination planet. The
[current state](#current-state) summarizes where the work stands, the
[objective](#objective-beat-native-control) defines what success must mean, the
[milestones](#milestones) track progress, and the
[experiment history](#experiment-history) links every report.

The Training workflow, saved practice assemblies, and history browser are
delivered in #287, #308, and #319. This slice connects those component artifacts
to a match host; it does not require whole-brain evolution or another organism.
See the [guidance contract](guidance-component-v1.md) and the
[larger training design](evolutionary-training.md).

The [observability delivery plan](training-observability.md) provides checkpoint
fitness graphs and a synchronized trace replay for the reviewed checkpoints.
Full world replays and Training-screen integration remain subsequent stages.
Freeze diagnostic replay cases alongside each recipe and keep monitoring
separate from training fitness and final qualification.

## Current state

As of 2026-10-20:

- **Candidate.** Genome
  `e0f25b68492df8d1a2e334b2b30da2a5fa8a45bfde8d7dd674cb7ee655c46de1`, a 9-8-3
  tanh network with 107 parameters and no memory, is frozen as the research
  baseline. It passes all 621 known cases: 464 match approaches and 157
  precision arrivals. Milestone 1's time-ranked final, `e03d3e29`, completes
  every case it completes, in every evaluated set, and arrives sooner. Live
  delegation is not qualified and bot defaults are
  unchanged. Learned control in ordinary matches and a selectable match brain
  remain gated on further evidence.
- **Fresh check.** On 64 unused worlds the candidate completes 222/238
  available match approaches and 96/96 new precision cases. The older learned
  reference completes 220 approaches, native mission control 218 and
  handwritten arrival guidance 210. The frozen gate required every available
  approach, so it failed. All 16 misses end at host preemption: 12 for sun
  avoidance and 4 at the climb guard. See the
  [fresh-check report](../learned-guidance-fresh-check.md).
- **Post-hoc reading.** An exploratory pass over the same traces, not a frozen
  result:
  - The two dirt shapes are near-duplicates in this phase. All 117 world/seat
    situations available under both shapes share the candidate outcome, so the
    238 approaches are about 121 distinct situations and the 16 misses are 8.
  - Against native control the candidate gains 7 situations and loses 4. On
    shared successes it is slower, with median arrivals of 54 and 48 ticks.
  - Native control would fail the same gate. Every approach that only the
    candidate completes (12 approaches, 7 situations) ends for native at its
    own climb guard. Preemption ends a component attempt for either
    controller; it is not a task outcome.
  - Ten of the twelve sun misses are route detours, not escapes. Each tick,
    native control's planner draws the line from the ship to its own entry
    point, 85 above the planet at the ship's current bearing, and detours when
    that line passes within 263 of the sun's center: the 198 sun radius plus a
    65-unit buffer. The other two, both in world 94000044, come from the escape
    rule's two-second look-ahead.
  - The six sun situations approach inner planets orbiting 266–328 from the
    sun's center and start only 0.4–7.2 units outside that buffer. They are 6
    of the 9 lowest-margin starts in the set; the next margin is 15. In three
    of them every controller loses the margin within 4–30 ticks. In the other
    three, native control's bearing drifts 1.5–2.8° away from the sun and it
    arrives, while the candidate holds the adapter's frozen bearing and loses
    the margin.
  - The nine inputs carry no sun position. The sun reaches the network only
    through the total gravity vector, whose encoded magnitude has a median of
    0.08. The candidate and the older reference miss the same six fresh sun
    situations, although the repairs between them fixed a known sun case.
  - All four climb misses come from the two lowest-margin starts in the set,
    0.54 and 4.57 units above the climb guard.
- **Objective.** Training rewards finishing native control's approaches by
  native control's rules, and until milestone 1 it never rewarded beating
  native control. The candidate completes more approaches than native control
  but arrives more slowly. Ranking by arrival time made it about one tick
  faster on fresh starts, while native control arrives about six ticks sooner.
  See [Objective: beat native control](#objective-beat-native-control).
- **Next.** [Milestone](#milestones) 6, perception training on the
  varied-world set. Its [plan](../learned-guidance-perception-training.md)
  awaits review. Milestone 3, the hybrid measurement, has no unfinished
  dependency either.
- **Hybrid measurement, milestone 3.** Measure the hybrid rather than the
  component alone. From identical states, run native control and the
  candidate with latched native fallback until eventual arrival within the
  original transfer budgets, reporting component completion separately and
  arrival time against native control for every start. Add a variant whose
  admission and early handback use host-computed climb and sun margins. The
  inspected fresh cases cannot serve as a holdout, so final qualification needs
  a newly frozen set. The harness checks native control's route to its
  current-bearing entry,
  whereas the admission design above asks for the frozen entry's route;
  reconcile the two before the hybrid test. In the six sun situations the two
  lines lose margin at nearly the same rate, so the mismatch does not explain
  those misses.
- **Perception.** The [guidance perception design](guidance-perception.md)
  adds sun, planet and surface inputs for a more capable network. Milestone 4
  implemented the sensor and a shared-encoder model version. Warm-started
  255-weight children of `e0f25b68` and `e03d3e29` fly all 621 known cases
  exactly as their parents do; see the
  [warm-start report](../learned-guidance-perception-warm-start.md).
- **Varied worlds.** Milestone 5 froze 1,370 training and 738 holdout starts
  from lab worlds with and without a sun and with two to eight planets. On the
  training set both parents complete slightly more starts than native control,
  1,287–1,288 against 1,279, but take 6,709–7,500 more ticks in all, almost
  entirely in worlds without a sun. All their sun-avoidance failures start
  within 15 units of the sun's buffer. See the
  [varied-worlds report](../learned-guidance-varied-worlds.md).
- **Code.** The headless harness lives under
  `crates/engine-client/src/client_scenarios/surface_sortie/mission/guidance_approach/`.
  Each experiment module nests under the one whose helpers it reuses and
  inherits them through `use super::*`, so the tree mirrors experiment lineage,
  for example `training/match_retention/graded/priority/repair_focus/run/fresh_check.rs`.
  Entry points are ignored tests. Reshaping that tree would rewrite frozen
  experiment code and break the current package's source-hash check.
  Milestone 4 copied the reusable pieces, the frozen starts and an episode
  runner for every policy, into the flat module `mission/guidance_match.rs`.
  New experiments are flat modules beside it, such as
  `mission/guidance_perception.rs` and `mission/guidance_worlds.rs`, declared
  in `mission.rs` so that adding one edits no pinned file.
  Long training and evaluation stages run from release builds, after a
  preflight reproduces the recorded results on that build.
- **Evidence.** Each report names its raw package under
  `data/training-experiments/`, which is local and ignored by Git. The
  fresh-check package reproduces with `python3 run.py all`.

## Objective: beat native control

A learned component that only matches native control adds risk to the hybrid
without adding value. The goal is to beat native control from identical
starts: complete more approaches and arrive sooner, without losing native
successes. Training has not asked for that so far.

### What training rewards today

The latest runs rank candidates in this order:

1. Keep every case the parent already completes.
2. Fix the failures the run targets.
3. Arrive gently: lower speed relative to the planet and a smaller distance to
   the marker in matches, smaller distance and velocity error in precision.
4. Spare ticks.

No term compares the component with native control. An earlier stage of the
lineage added a hand-written reward for braking on the first tick at eight
diagnosed starts; the latest runs do not use it.

Native control shapes the task anyway:

- It flies the ship to every match start.
- Success is its capture check.
- An attempt fails when native control would climb, avoid the sun or detour,
  even if the component's path is physically safe.
- Nothing after capture is scored.

### Where the candidate stands

After [milestone 1](../learned-guidance-time-ranking.md), from identical
starts:

| | `e0f25b68` | Time-ranked `e03d3e29` | Native control |
| --- | ---: | ---: | ---: |
| Training approaches completed | 464/464 | 464/464 | 430/464 |
| Inspected approaches completed | 222/238 | 224/238 | 218/238 |
| Fresh approaches completed | 221/239 | 221/239 | 209/239 |
| Mean ticks, 430 shared training starts | 53.5 | 51.8 | 46.9 |
| Mean ticks, 210 shared inspected starts | 53.4 | 52.8 | 50.3 |
| Mean ticks, 197 shared fresh starts | 55.5 | 54.5 | 48.5 |

Inspected approaches are the earlier fresh check's, which no longer serve as
a holdout; fresh approaches come from 64 new worlds. Shared starts are those
every controller in the milestone 1 report completes. On fresh starts both
complete, the time-ranked final is slower than native control on 113 and
faster on 80, with median arrivals of 54 and 47 ticks. Native control reaches
capture at a mean relative speed of 16.7, near the limit of 18, and both
models at about 14.2. Both models complete 20 fresh approaches that native
control misses and miss 8 that it completes. Of native control's 34 training
failures, 31 end at its climb guard, 2 at route detours and 1 has no recorded
reason. Native results for the training approaches are the `mission` runs of
the same frozen states, recorded in earlier experiment packages.

The slowness follows from the ranking. Gentle arrival ranks ahead of spare
ticks, so a slow success outscores a fast one. Milestone 1 confirmed it: under
that ranking the control arm got slower, while ranking time first made the
network faster but not as fast as native control.

### Changes

1. **Reward time saved against native control.** Rank total match arrival
   time right after the completion counts, counting a failed start as the
   300-tick attempt bound. Native control's arrival times are fixed for each
   start, so this ranks candidates exactly as total time saved against native
   control would; its times serve reporting, not selection. Precision cases
   keep their current ranking, since native control has no precision
   counterpart. This stays inside native control's rules and needs no new
   inputs. [Milestone 1](../learned-guidance-time-ranking.md) found that it
   speeds up fresh approaches too, by about one tick, so later training keeps
   it. On its own it does not close the gap to native control.
2. **Judge physical outcomes once the component sees hazards.** Count an
   approach as successful when the ship reaches native control's capture
   window without contact or damage, and stop ending training attempts on
   native control's climb, sun-route and detour rules. This lets the component
   out-fly native control's margins, for example near the sun. It needs the
   inputs in the [guidance perception design](guidance-perception.md), since
   the nine inputs cannot see the sun. Live play needs a matching ownership
   change: while the component flies, native control's planning margins stop
   preempting it, and a narrower hard safety check remains as fallback. That
   replaces the [first phase](#first-phase-direct-destination-approach) rule
   that a new detour ends delegation, so it needs its own frozen evidence.

Every evaluation, including qualification, reports completions and arrival
time against native control from identical starts, along with the native
successes lost.

## Milestones

This work ships as a single PR, and nothing is delivered until the hybrid
qualifies. Check items off here as experiments finish, link each report in the
[experiment history](#experiment-history), and update the
[scoreboard](#where-the-candidate-stands) after every evaluation.

- [x] **1. Time-saved ranking.** Rank candidates by time saved against native
  control right after completions, on the current nine-input network. Done
  when a bounded run from `e0f25b68` reports completions and time against
  native control on the training set and on a newly frozen fresh set.
  Done 2026-10-18 ([report](../learned-guidance-time-ranking.md)): about one
  tick faster on fresh starts with no completions lost, and still about six
  ticks slower than native control.
- [x] **2. Sensing cost.** Time ray casts, `cast_ball` and point projection in
  a real match world. Done when the
  [perception design](guidance-perception.md) records the per-query cost and
  chooses between circle geometry and queries. Done 2026-10-18
  ([results](guidance-perception.md#sensing-in-match-worlds)): circles are
  exact in the training worlds and cost 0.9 µs per decision. An eight-ray
  ring costs 30 µs and misses about 1% of planet arcs.
- [ ] **3. Hybrid with native fallback.** The measurement in the
  [current state](#current-state). Done when the report gives component
  completion, eventual arrival and time against native control, with the
  frozen-entry route check reconciled.
- [x] **4. Perception sensor and model.** Body records, the arc ring and a
  shared-encoder model version. Done when the warm-started network reproduces
  its dense parent, `e0f25b68` or the time-ranked `e03d3e29`, on all 621 known
  cases. Needs 2. Done 2026-10-18
  ([report](../learned-guidance-perception-warm-start.md)): children of both
  parents reproduce them on every known case, decision for decision. Sensing
  and inference cost about 1.8 µs per decision.
- [x] **5. Varied-world training set.** Worlds with and without a sun, varied
  planet counts and sizes, and low-margin sun starts, plus a separately frozen
  holdout. Needs 4. Done 2026-10-19
  ([report](../learned-guidance-varied-worlds.md)): 1,370 training and 738
  holdout starts from nine kinds of lab world, with native control's outcome
  for every start. The parents match native control's training completions but
  are much slower, almost entirely without a sun.
- [ ] **6. Perception training.** Train the 255-weight network with the
  time-saved ranking. Done when it beats native control on completions and
  time on the frozen holdout. Needs 1 and 5.
  [Plan](../learned-guidance-perception-training.md) written 2026-10-20 and
  being revised: training set redesigned 2026-10-22.
- [ ] **7. Physical judging and component-owned safety.** Judge attempts by
  physical outcomes and let the component own safety while it flies, with a
  hard safety fallback. Done when it beats native control from identical
  starts on a fresh frozen set. Needs 6.
- [ ] **8. Qualification.** [Qualification](#qualification-sequence) steps
  2–4: host tests, frozen paired matches, the decision and the opt-in selector.
  Needs 3 and 7.

## Experiment history

Each report records its frozen plan, results and raw evidence.

| Experiment | Result |
| --- | --- |
| [Approach census](../learned-guidance-approach-census.md) | Short approaches exist in both seats; none met a conservative distance-and-speed filter based on the retained practice starts. |
| [Eight-case approach probe](../learned-guidance-approach-probe.md) | The retained genome completes 4/8 native handoffs against a frozen 8/8 gate. The arrival reference completes 8/8 with the same nine inputs, and ordinary mission guidance 6/8. |
| [Bounded continuation](../learned-guidance-match-training.md) | Approaches improve to 6/8 and reserved approaches from 6/16 to 12/16, but three old moving-target arrivals regress. Gate failed. |
| [Broader retention](../learned-guidance-match-retention.md) | 16/16 on four new validation worlds and 20/24 inspected approaches, but five other original arrivals regress. |
| [Failure diagnostic](../learned-guidance-failure-diagnostic.md) | Adds tested full-suite retention support. Forcing the brake completes all 43 inspected match starts but only 9/157 older arrivals. |
| [Full retention](../learned-guidance-full-retention.md) | 157/157 older arrivals; 8/15 training, 26/40 development and 12/14 available validation approaches. Every parent success kept; gate failed. |
| [Brake readout](../learned-guidance-brake-readout.md) | Full-network continuation reaches 10/15 training and 42/54 development approaches, the brake-only arm 8/15 and 38/54. Both keep 157/157 precision and pass 16/16 fresh starts; full-network training loses two match successes to sun avoidance. |
| [Protected matches](../learned-guidance-protected-matches.md) | Recovers both regressions, keeps all 61 known match successes and adds four: 65/73 inspected and 9/9 available fresh approaches. Eight brake-off failures remain. |
| [Focused progress](../learned-guidance-focused-progress.md) | 76/82 inspected approaches versus 74/82 under unchanged scoring; both runs reach 14/16 fresh and keep 157/157 precision. The remaining failures apply no brake. |
| [Brake bias](../learned-guidance-brake-bias.md) | All eight failures can recover with changed braking, but every uniform shift that adds a match success loses precision arrivals. |
| [Graded braking](../learned-guidance-graded-braking.md) | 92/98 inspected and 16/16 fresh, versus 90/98 and 14/16 for the matched control, with no lost successes. Six brake-off failures remain. |
| [Brake neighborhood](../learned-guidance-brake-neighborhood.md) | No fully retained gain among 108 single brake-parameter changes. The best recovers five failures but loses 44 precision arrivals. |
| [Joint brake-readout fit](../learned-guidance-brake-joint-fit.md) | No readout preserves all 98,336 successful reference brake decisions while braking initially; exact certificates confirm the conflict. |
| [Long run](../learned-guidance-graded-long.md) | 240 generations solve every training case by generation 135, finishing at 114/114 inspected and 157/157 precision. Fresh stays at 12/16, with two gains and two regressions. |
| [Checkpoint review](../learned-guidance-checkpoint-review.md) | Monitoring holds 14/16 from generation 29 through 183 and falls to 12/16 at 184. Both handwritten controls hit the same sun-avoidance intervention. |
| [Broader coverage](../learned-guidance-coverage.md) | The final model keeps 124/128 known approaches and reaches 24/32 final-evaluation starts. Its generation-135 parent does better: 128/128 and 28/32. |
| [Reward priority](../learned-guidance-priority.md) | Either score order reaches all 317 training successes by generation 8. Selected checkpoints reach 28/32 validation and 32/32 final starts; the parent also passes all 32. |
| [Fixed-model comparison](../learned-guidance-broad-evaluation.md) | On 64 fresh worlds and 256 starts, the parent and progress-first generation 24 complete 240; brake-first generation 36 completes 238. |
| [Mutation scale](../learned-guidance-mutation.md) | Small steps keep every success in 18.23% of offspring versus 3.77%, but both arms plateau at two added arrivals. Small-step selection matches the seed's 250/256 final test. |
| [Single-case isolation](../learned-guidance-isolation.md) | The retained sun-route approach is learned within 4–6 generations, at the cost of all 157 precision arrivals and 9–30 protected matches. |
| [Precision repair](../learned-guidance-repair.md) | Recovers 143–156 of 157 precision arrivals with the focal pair, but keeps only 435–453 of 460 protected matches. |
| [Joint retention](../learned-guidance-joint-retention.md) | Two runs improve to 614 and 612 retained successes, plateauing by generation 12. Five original matches still omit braking before the climb guard. |
| [Repair focus](../learned-guidance-repair-focus.md) | The focused arm reaches 619/621 known cases versus 616/621 for the broad control. One unselected population member scores 620/621. |
| [Near-miss reconstruction](../learned-guidance-near-miss.md) | Reproduces that 620/621 member. Its one failure holds both arrival limits for 24 of the required 30 ticks. |
| [One-weight repair](../learned-guidance-near-repair.md) | Moving hidden weight 67 halfway back toward its parent passes all 621 known cases, giving genome `e0f25b68`. |
| [Fresh check](../learned-guidance-fresh-check.md) | 222/238 fresh approaches and 96/96 precision cases; the strict match gate fails. See the [current state](#current-state). |
| [Time ranking](../learned-guidance-time-ranking.md) | Ranking match arrival time right after completions keeps all 621 known cases and is faster than its seed on 124 of 221 fresh approaches, slower on 18. Native control still arrives about six ticks sooner on fresh starts. Under the old ranking the control arm gets slower. |
| [Perception warm start](../learned-guidance-perception-warm-start.md) | 255-weight networks that read the perception blocks, warm-started from `e0f25b68` and `e03d3e29`, reproduce their parents on all 621 known cases and 18 other known approaches, with every decision identical. |
| [Varied worlds](../learned-guidance-varied-worlds.md) | 1,370 training and 738 holdout starts from lab worlds with and without a sun and with two to eight planets. Both parents complete slightly more training starts than native control but take 6,709–7,500 more ticks, almost all without a sun. |

Lessons from this sequence:

- More training improves known cases without reliably improving fresh ones.
- A validation-selected checkpoint does not establish unseen retention.
- Smaller mutation steps preserve skills more often but still plateau.
- Focused training learns a hard case quickly while badly damaging precise
  stopping.
- Until milestone 1, no score rewarded beating native control, and the
  speed-first tie-breaker ranked slow successful approaches above native
  control's faster arrival. See the [objective](#objective-beat-native-control).
- Ranking arrival time right after completions speeds up fresh approaches as
  well as known ones, but by about one tick of the seven by which native
  control led.
- The existing network and inputs can express the complete known set.

## Host and candidate

The handwritten reference is the current **Recovery bot v1**: Value v13 mission
policy, `terrain_recovery_supported_hatch_ordinary_offsets_v1`, and
`material_hunt_clearance_handoff_v1`. Use its real paired recovery-observation
driver and the client host's destination/flag planning schedule. Other opt-in
mission experiments stay disabled. Two humans remain the launcher default.

Start with the retained mixed-arrival **seed-1** finalist. Choosing the lowest
declared training seed gives a reproducible choice without using match results
to select among the three successful finalists.

| Identity | Value |
| --- | --- |
| Artifact | [`seed-1.json`](../../crates/engine-agent/fixtures/ship-arrival-mixed-v1/seed-1.json) |
| Genome | `693447c6df8315017a67f8dba8809568cf08f3ec66131302bfd026fa49f343fb` |
| File SHA-256 | `a1d127eb9eb96f190616d5d3ee377490d5a06ac2023302c9eaeb27a971945199` |
| Organism / role | `material_ship_open_v1` / `arrival_guidance_diagnostic_v1` |
| Observation / action | `arrival_local_9_v1` / `turn_tanh_thrust_brake_v1` |
| Cadence | One decision per physics tick |
| Proposed match adapter | `recovery_arrival_match_v1` |

[Original provenance](../../crates/engine-agent/fixtures/ship-arrival-mixed-v1/provenance.json)
records all three finalists and their original executable identity. Its arrival
results concern one stationary material world and build. Moving planets, match
speeds, combat, and destructible approaches require new evidence. Rerun the
retained arrival suite on the eventual integration executable; keep that receipt
separate from the original results and from match evaluation.

## First phase: direct destination approach

The proposed phase is the end of `MissionGoal::Transfer`, after the nearest
physics frame has become the selected destination and before native capture
starts. Launch, long-distance travel, obstacle detours, combat, landing,
on-foot work, and recovery remain handwritten.

The existing mission path already supplies an entry point at the destination
radius plus 85 world units. It starts capture only when the ship is in the
destination frame, within radius plus 105, slower than 18 units/s relative to
the planet center, and has ready queries. These are existing host gates, not
learned success declarations. The ordinary transfer has a 60-second overall
budget and a 20-second progress budget; preserve their original clocks.

At admission, the proposed adapter freezes the radial bearing of that entry
point. Thereafter the marker follows the destination center with that fixed
world-space offset, and its velocity is the center velocity. This gives a
consistent position/velocity pair independent of subsequent model steering.
It differs from the ordinary entry's continuously recomputed radial bearing;
version and test that difference explicitly. Planet acceleration is also new
relative to the constant-velocity practice marker.

The host must still verify that the frozen entry has a direct unobstructed route
each tick. A new detour, frame change, destination change, or native arrival
ends delegation. Do not pass a landing site, opponent, or clearance waypoint
through this adapter as if it were the same task.

### Admission is an experiment gate

Require a living pilot aboard the assigned available full ship, armed controls,
the trained open-wing flight limits, no wing transition, no contact or supported
feet, no landing assistance, finite observations, and no saturated features.
Require the destination frame, direct transfer, and no higher-priority host
task. Losing any condition ends the attempt.

The practice episode permits radial altitude 40–220, but those limits are
failure boundaries, not evidence that every state inside them was trained.
Likewise, the encoder's distance and speed scales are normalization constants,
not a supported flight envelope. The retained development starts include
short offsets up to 35 units and initial speed up to 6 units/s; their sampled
paths and moving-target combinations need separate inspection.

The directed comparison now establishes a gap beyond the old practice starts:
in both distinct failed network starts, it turned under full thrust without
braking until the native host required a climb. Add a match-relevant arrival
curriculum using the existing framework and nine-input contract. Treat the
inspected starts as development cases, retain the original arrival regressions,
and freeze separate validation cases before evaluating a new candidate. Freeze
numerical admission bounds only after development measurements, and do not widen
them simply to obtain activations in the eventual qualification matches.

## Control ownership and fallback

Put the optional delegation at the mission's transfer-guidance branch, after
ordinary priority, deadline, arrival, and route decisions. Do not calculate a
complete handwritten movement command and then replace it in the client: that
would advance the handwritten steering accumulator for controls never applied.
The coordinator continues observing physical progress every tick; only the
selected low-level guidance owner advances its steering memory.

The learned phase owns steering, thrust, and brake together. Emit open wings
and neutral weapons, interaction, and mining every tick through canonical
actions. The current mission encoder emits three channels while guidance emits
four, including mining; the integration must deliberately handle the complete
control set. Combat aiming cannot be combined with the learned turn output.

Use an explicit neutral control tick when changing owners. Create a fresh
guidance controller after handoff, reset, vehicle/actor replacement, target
replacement, or physics-frame change. Bind requests to episode, actor,
spaceling, vehicle, planet, target, control epoch, and tick. Pause consumes
neither a decision nor a handoff tick. The existing guidance controller rejects
skipped/conflicting ticks, so it cannot simply resume after a handwritten gap.

The initial proposed bound is one attempt per mission selection and vehicle,
at most 300 physics ticks (five seconds), further limited by the original
mission deadline. This bound must be frozen with the admission profile before
candidate testing. Timeout, model error, damage during the attempt, lost
eligibility, or a higher-priority task releases control and latches handwritten
fallback for that selection. No reset of mission progress, retries, capture
failure history, recovery/scuttle state, or native support/transfer gates is
allowed. Fallback does not turn a failed learned attempt into an arrival success.

Record the host and adapter identities, exact genome, active control owner,
target position/velocity and identity, admission tick/deadline, binding epoch,
raw and encoded features, saturation, applied actions, and exit/fallback reason.
Count eligible opportunities, admissions, owned ticks, successful native capture
handoffs, and failures separately. A match in which the model never ran is not
a successful learned-guidance trial.

## Development census

The ignored client diagnostic
`recovery_guidance_approach_census` uses the normal `spacewars` launcher with
Recovery v1 in both seats. It records direct-transfer geometry through the
completed-step free-flight projection without running a model or changing
controls. Each observed run has an unobserved control: compare mission telemetry
and free-flight state each tick, and scenario payloads every second and at a
native match finish.

The fixed development sample is seeds 42 and 10101, weak Angular and Round dirt,
ordinary combat, environmental asteroids off, and the first 180 seconds of a
native ten-minute match or an earlier native finish. These inspected prefixes
measure opportunities; they are neither held-out cases nor completed-match
strength evidence. Record the full settings, source diff, executable hash,
case termination, and output checksums alongside the report.

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_GUIDANCE_CENSUS_OUT=/tmp/spacewars-guidance-census-NEW \
cargo +1.89.0 test -p engine-client --bin engine-client \
  --locked --offline --profile ci recovery_guidance_approach_census \
  -- --ignored --nocapture
```

The output directory must be fresh. `approaches.jsonl` contains every transfer
decision already in its destination frame, including rejected physical states;
`summary.json` preserves phase coverage and the control comparison. The entry
position is the current ordinary radial entry, with center velocity recorded
for comparison. This measurement is not yet the frozen-bearing adapter or a
certificate of obstacle/asteroid safety.

## Qualification sequence

1. **Establish capability.** Complete for the known cases, not yet for fresh
   matches. Genome `e0f25b68492df8d1a2e334b2b30da2a5fa8a45bfde8d7dd674cb7ee655c46de1`
   passes all 621 known cases but fails the fresh match gate at 222/238; see
   the [current state](#current-state) and the
   [experiment history](#experiment-history). Investigate the native-success,
   candidate-failure approaches before another training run or rollout,
   keeping climb and sun-avoidance causes separate. Any adapter or policy
   change needs a new frozen experiment and an independently reserved final
   holdout. No joint solution is qualified for live delegation yet.
   Retain contact, assistance, damage, timeout, and failed native handoffs as
   outcomes. Require the development gate and fresh validation before building
   a match selector; the current result does not qualify live delegation.
   [Milestones](#milestones) 1–7 track the route to this capability.
2. **Test the component and host.** Rerun retained arrival cases on the exact
   new executable. Exercise the directed match phase against the original host
   and the handwritten arrival reference using identical initialization. Include
   both seats, target/frame changes, reset/pause, wing transition, assistance,
   contact, damage, vehicle loss, recovery, saturated/invalid features, model
   failure, timeout, and native capture handoff. Add checks that disabled or
   never-eligible operation preserves the original host's actions and deadlines.
3. **Freeze and run paired matches.** Predeclare four new world seeds, two dirt
   shapes, and asteroid-off/8-second-mixed conditions. Each of those 16 physical
   configurations runs a Recovery-v1 control and two hybrids, placing the
   candidate in one seat at a time: 48 native ten-minute matches, ending earlier
   only by native outcome. Keep all cases, including zero-admission ones. Freeze
   exact settings, seed manifest, source/build, genome, admission/fallback
   profile, and quantitative retain/reject thresholds before any outcomes.
4. **Decide and expose.** Retain the candidate only with correct ownership,
   useful activation coverage, and acceptable mission, recovery, survival,
   win/draw, no-progress, and cost results. It must also beat native control
   from identical starts, measured by completions and arrival time, with a
   frozen limit on native successes lost. Inspect paired differences by seat,
   dirt shape, and asteroid condition; report the shared-world dependence.
   Rejection or a narrower supported scope is a valid result. A retained
   candidate then gets an explicit per-seat selector, clear host/genome identity,
   client tests, and a controlled 1× device check. Changing defaults is a separate
   evidence-backed decision.

Zero stale actions, unauthorized ownership, or budget resets is a hard gate.
Directed supported cases must complete the native arrival handoff without new
damage or recovery failures; adversarial cases must fall back as specified.
The quantitative match and device-cost thresholds remain to be fixed with the
case manifest after development measurements, before qualification. None of the
steps above establishes general superiority from replay agreement alone.
