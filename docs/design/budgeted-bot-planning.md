# Budgeted planning for material-match bots

Status: steps 1–2 are merged. The first selectable
joint-trip candidate and seat-swapped comparison harness are described in
[mission policy comparison](../mission-policy-comparison.md). Step 3 now has a
[resumable graph job and shared scheduler](../bot-planning-jobs.md) and an
[opt-in live landing-objective adapter](../live-bot-surveys.md). That adapter
resumes coherent physical measurements and graph work under one shared
allowance in the existing comparison runners. An optional
[ground-reuse profile](../bot-ground-reuse.md) retains compatible footing and
walk measurements across requests. A further opt-in
[local route dependency profile](../bot-route-dependencies.md) validates successful
paths despite unrelated obstacle motion and preserves partial measurements while
waiting for candidate refresh. Default v10 remains synchronous; failed routes
under asteroid pressure, broader sensor coverage and strategic planning remain
future work. The experimental v11 [powered landing routes](../bot-jetpack-landing.md)
now have [moving-planet prediction and physical calibration](../bot-moving-flight-planning.md).
This establishes another tactical action for later mission scoring; it does not
yet establish a stronger match policy. The [asteroid-pressure investigation](../bot-asteroid-objective-diagnosis.md)
found useful partial v11 candidates cancelled by scalar jump-gravity changes,
alongside genuinely negative measured v10 routes. The next tactical slice is
now implemented as [candidate validity under changing scalar gravity](../bot-jump-gravity-dependencies.md).
It preserves independent paths but reveals that completed surveys can arrive
between landing scans with no current sites to select. The
[landing-scan handoff](../bot-landing-survey-handoff.md) now joins those results
to current sites and completes physical enemy-flag captures in quiet paired
replays. The optional [early-candidate adapter](../bot-early-objective-candidates.md)
now publishes finished positive routes with bounded fresh landing clearance
before the whole survey completes. In the remaining asteroid case it selects a
powered route at tick 6,604, but still loses the ship at 6,611. Quiet capture and
return remain intact. The [contested-approach investigation](../bot-contested-approach.md)
tested a late damage-triggered switch to combat and rejected it: both triggered
replays shortened pilot survival. An optional
[pursuit disengagement task](../bot-pursuit-disengagement.md) now compares seven
escape directions and establishes separation in four activated physical trials.
The ordinary transfer handoff gives that separation back; one asteroid case
still loses its pilot much earlier. This task remains headless and disabled by
default. The [successor-flight investigation](../bot-disengagement-handoff.md)
now forecasts the actual transfer controller's turning and braking costs.
Its automatic handoff rule is rejected after two win-to-loss regressions; a
read-only probe remains for calibration. The opponent's coasting approximation
can both overstate and understate danger. The subsequent
[opponent-response calibration](../bot-opponent-response-forecast.md) improves
the pursuit forecast and exposes a missing arena-wall constraint. The read-only
probe now flags boundary-margin entry, and mission observations expose the
enclosing arena. A separate [boundary-guidance experiment](../bot-boundary-escape.md)
now avoids the three recorded wall cases, but also loses a quiet acceptance
match. It remains opt-in; the older escape and default policies are preserved.
Braking-only and reflected-direction ablations also regress. The
[remote destination-cover probe](../bot-destination-cover.md) now measures a
four-site shortlist using only query quota left after local planning. Fourteen
complete matched replays preserve controls and local work. It distinguishes
covered, exposed and unavailable ground, and retains explicit sample age and
invalidation. A [budgeted successor comparison](../bot-successor-comparison.md)
now evaluates the first flight leg toward those sites, continued escape within
its deadline, and combat against independent opponent hypotheses. It uses only
remaining graph allowance and leaves controls unchanged. Current revalidation,
capture/return feasibility and combat outcome evidence are still required before
ranking complete successors or promoting a new policy.
The [physical continuation matrix](../bot-successor-continuation.md) now tests
those first legs from three exact handoff states. Pursuit range-entry predictions
are close in the main-seed trials, but no staging point is reached within six
seconds, asteroid edits invalidate two approaches, and immediate survival does
not rank eventual outcomes. The [bounded capture-trip experiment](../bot-successor-sortie.md)
now reaches three staging points and one actual landing in nine contested site
trials. That pilot neutralizes the enemy flag but times out while raising its own;
none completes capture and return. An isolated physical fixture completes the
whole trip. A dense replay identifies repeated self-interruption while raising
the own flag. The [own-flag handoff fix](../bot-claim-handoff.md) now completes
that raise and starts a freshly planned return. Eight fresh paired comparisons
preserve match outcomes and completed-trip counts, with one capture/return
finishing earlier. The recorded long return still exceeds its host task clock.
The [walking investigation](../bot-return-walking.md) rejects a faster steering
experiment: fixed-control clones and a two-body reduction expose sinking and
stalls on moving compound terrain. The [CCD velocity correction](../moving-ground-ccd.md)
now removes those reproduced stalls while retaining the controller. Its
controlled return reaches the final waypoint with no emergency jumps but still
expires before boarding. The [guarded walking retry](../bot-return-completion.md)
now completes that controlled return through boarding and departure with 17.55
seconds left on the unchanged clock. Fresh production comparisons retain a
win-to-loss regression despite faster boarding in the changed match; the extra
held-out cases do not activate the new input. The first
[complete-trip timing and exposure analysis](../bot-trip-calibration.md) now
separates phase costs in dense recordings. It finds a remaining outbound
waypoint slowdown, omitted landing/claim/departure costs and an exposure counter
that stops on foot. The [guarded outbound walking trial](../bot-outbound-walking.md)
now reduces the recorded long outbound leg from 30.33 to 14.30 seconds, with
actual claiming and return. Two activated matched configurations change from
losses to wins; the small known sample does not establish strategic strength.
The first [read-only trip estimator](../bot-trip-estimate.md) now freezes phase
costs at the observed landing choice. Eight independent-world runs yield 29
completed numeric comparisons with 2.48-second median absolute error, mostly
for very short ground trips. Terrain/site changes cause landing underestimates
up to 25 seconds; failed approaches and remote/powered costs remain explicit.
The [rolling diagnostic](../bot-rolling-trip-estimate.md) now separates evidence
refresh from approach restarts, preserves elapsed retry time and exposes native
capture limits. Twelve new runs verify the accounting; timing accuracy remains
mixed when a retry is already close to touchdown. The
[phase-aware diagnostic](../bot-phase-landing-estimate.md) now separates observed
flight phases and retry context. Eight fresh runs reduce the 15-second
checkpoint's paired median trip error from 5.41 to 0.28 seconds. A 29.90-second
underestimate followed by nine further terrain-driven replans preserves the
need for an interruption-risk model. The [long-ground validation](../bot-long-ground-validation.md)
now retains 48 independent matches and all 212 attempts. Only three original
choices are long walks; all exceed the calibration domain and fail, revealing
a blocked posture recovery and a route whose nominal duration exceeds its task
clock. The [controlled-ground fixture](../bot-controlled-ground.md) now retains
32 generated distance/direction/seat trials. Six completed trips remain walks,
with a 4.17-second median ground-cost underestimate; a powered return, unavailable
setups and a shorter posture failure remain separate. The
[independent walking calibration](../bot-walking-calibration.md) now fits fixed
overhead and distance multipliers on eight new worlds, then freezes them before
six-world validation. Fourteen paired ground intervals reduce median error from
2.83 to 0.87 seconds; a newly covered slow return remains underestimated by 15.76
seconds. The [composed estimator](../bot-composed-trip-estimate.md) now reduces
whole-trip error from 3.41 to 1.16 seconds on nine paired controlled completions
at the 15-second checkpoint. Strictly replacing all walking costs loses ordinary
short-walk coverage; initial controlled forecasts also trail the original model.
Explicit short/moderate walking regimes and new-world validation are next,
alongside remote-transfer and interruption-risk evidence before mission selection.
Retry calibration is still sparse and these estimates do not change bot controls.
Longer commitment is not uniformly better, and forecast tails remain unsupported.
Eager route delivery or a successful local escape alone does not establish a
successful powered sortie or justify promoting either experimental adapter.

Before extending the powered-route model, the
[cockpit and spaceling scale slice](../spaceling-cockpit-scale.md) integrates
#95's smaller physical pilot with both existing bot policies. New route
measurements and comparisons must use that shared geometry.

The earlier measurement baseline is `ground-route-profiling` at `1e8ce10`;
see [ground-route profiling](../ground-route-profile.md). The first optimization
is implemented and measured in [indexed ground-route queries](../indexed-ground-routes.md).
The comparison checkpoint retains the subsequently merged v9 behavior at `d574b8d`.

The goal is better decisions with predictable computation on the Picades. Treat
navigation results as reusable evidence for choosing a mission, and request
detailed evidence only when it can affect a decision. Preserve the existing
flight, combat, capture, ground and recovery controllers as action executors.

The user also wants lightweight comparisons with retained policies, scalable
budgets for several bots, and a foundation for bot control beyond this specific
mission. These remain design constraints for extending the initial comparison
registry and implementing the shared planning scheduler.

## Current boundary and missing information

`MaterialMissionPilot` already coordinates persistent mission tasks. Recovery,
solar avoidance and opportunistic pursuit take precedence; selection among the
remaining capture destinations largely uses straight-line distance. Tactical
landing selection considers cover and a measured ground round trip. Legacy v9
first chooses the cheapest outbound arrival, then tests that arrival's return;
v10 now chooses the endpoint using both legs together.

Default planning still happens synchronously while constructing an observation.
At the original profiling checkpoint, the slowest observations took 51.88 and 53.69 ms, including
roughly 19 ms of ground connection construction and 14–16 ms of route searches.
Connections dominate accumulated sensor time; graph searches amplify the worst
pauses. Improving only Dijkstra will leave substantial work in world queries.

The current mission observation exposes whether match rules apply, but not a
remaining match time field. Clock-aware strategy therefore needs an explicit
observation-contract extension. Detailed ground evidence currently concerns
the approach planet, not every possible destination. A remote planet must
remain an estimated opportunity until the bot obtains relevant measurements.

## Decision layers

| Layer | Responsibility | When it runs |
| --- | --- | --- |
| Safety and controls | React to hazards; validate landing, transfer, footing and other immediate permissions; execute the current action | Every simulation update |
| Tactical planning | Choose a landing and complete ground trip; plan a measured crossing or recovery approach | On relevant changes or scheduled refresh, with bounded work |
| Mission planning | Compare capture, intercept, defend, disengage and recovery opportunities | On mission events and a slower periodic review |

The existing two/four-Hz survey cadence is a starting point, not a reason to
rebuild every map on every scheduled review. Retain useful plans while checking
the part that is about to be executed. If that part becomes unsafe, the existing
controller should stop, hold or lift clear as appropriate while replanning.
Ship loss and imminent hazards can interrupt commitment immediately.

Separate cheap observed facts from expensive planning requests/results. The
scenario owns physical measurements; pure graph search and plan evaluation can
operate on the returned data without access to the mutable world. Keep the
observation-to-canonical-controls authority boundary and one physics/gravity
step described in [surface AI integration](surface-ai-integration.md).

Planning results need distinct pending, ready, stale and no-measured-route
states, dependency versions and diagnostic reasons. An unfinished query is not
a failed route. Absence of an outer-contour route does not establish that caves,
mining or an unmeasured jetpack crossing cannot provide access.

## Search over missions rather than button presses

Make an extended action such as “capture planet B and return aboard” expose:

- Preconditions and the evidence supporting them.
- Estimated travel, landing, ground, claim and return time.
- Exposure, resource demand and remaining escape/recovery options.
- Expected ownership or opponent-state change.
- Progress, completion, interruption and failure conditions.
- Confidence, age and dependencies of the estimates.

Initially, use these records for a one-step utility comparison. Distinguish
known infeasibility from uncertain estimates. Score survival, ownership and
time using actual match rules: pilot death ends the round, ship loss alone
does not; the time limit compares owned planets and equal ownership draws.
Losing a flag can remove ownership. Asset value must not override terminal
outcomes. Avoid treating an uncalibrated risk score as a death probability.

This should allow choices such as taking a slightly farther planet with a
shorter exposed ground trip, delaying descent while an armed opponent can
interfere, or defending a lead near the time limit. Waiting and disengaging
need progress conditions and bounded review; caution must not become permanent
inaction. Preserve commitment unless new evidence materially changes the choice.

Use a cheap estimate to rank all destinations and refine a small shortlist.
Detailed measurements may initially require approaching the destination. Let
uncertainty motivate a bounded approach/probe rather than classifying every
unsurveyed planet as unreachable. Remember why an attempt failed, with relevant
terrain/obstacle context and an expiry, so changed circumstances permit a retry.

Once estimates predict real executions reasonably well, explore a few actions
ahead with a bounded beam or best-first search over an approximate mission
state. Actions have different durations and both players move simultaneously;
ordinary alternating-ply minimax is not automatically the correct model.
Consider a small set of plausible opponent responses, such as continuing its
sortie or intercepting. Use modelled responses as predictions, not privileged
knowledge of its controller or future inputs. Execute the first action and
replan from observations. Full physics rollouts and learned control are not
prerequisites for this architecture.

Planning competence should be independently adjustable from aim, reaction,
aggression and the existing configurable exhibition breaks. Better mission
choices should not implicitly remove the human's opportunities to play.

## Lightweight policy comparisons

The material mission registry now retains v9 and v10, and the existing
seat-swapped comparison runner can select a live objective sensor for its
candidate role. Before introducing further changed behavior, retain the current
checkpoint and explicitly select the baseline/candidate in each seat.
Share behavior-preserving helpers; version changed decision rules and planning
inputs rather than copying an entire engine or silently changing a baseline's
sensors. Keep defaults separate from concrete evaluation policy IDs.

One existing headless runner, a small set of workload descriptions and JSON
reports are sufficient. Record engine/build identity, policy identities,
planning/sensor configuration, total and per-bot work budgets, seeds, seats and
exhibition settings. First compare policies at equal budgets, then vary budget
to measure quality versus computation. Report role-based results after swapping
seats. Frozen historical traces apply to a pinned world/sensor contract; after
physics changes, old and new policies should also run in the same new world.
Retain a few useful checkpoints rather than making every experiment a permanent
production option. No tournament service or new evaluation dependency is needed.

## Navigation improvement that also improves choices

First add direct node lookup and indexed outgoing edges to the measured graph,
sharing those indexes between queries. Preserve ordering, arithmetic, ties,
partial-route behavior and diagnostics in this optimization step. Consider a
priority queue separately after measuring the remaining 512-slot minimum scan.

Then evaluate the complete ground round trip. For a candidate landing graph,
let `s` be the exit start, `F` the eligible flag-interaction nodes and `H` the
boarding region. A forward shortest-path search from `s` gives `d_out(f)`.
A multi-source search from `H` on the reversed graph gives the cost `d_back(f)`
of returning along the original directed edges. Choose a feasible `f` minimizing
`d_out(f) + d_back(f)`. Reconstruct both routes and carry the chosen goal into
execution. Reversing the search graph does not authorize a reversed physical
jump. This addresses a cheapest outbound endpoint with a poor or impossible
return when another flag endpoint permits a complete sortie.

Define the optimized cost explicitly. The current edge penalties and landing
selector's time conversion differ; moving to a common time estimate is a
separate policy change from indexing or preserving existing shortest paths.
Exposure and resource margins can remain additional candidate-level terms at
first. If future jetpack plans depend on energy spent on preceding edges, a
position-only graph is insufficient for exact feasibility: use a resource-aware
state or a conservative executable envelope.

Each proposed parked hull changes the candidate graph. Do not reuse a single
flag distance field as proof for every landing. A shared base index with
candidate-specific node/edge masks is a possible later representation, provided
it preserves the real clearance tests and selected-site revalidation.

## Cache dependencies explicitly

Start with immutable, within-survey reuse: graph indexes, query workspaces and
candidate-independent geometric samples. Profile exactly repeated capsule poses
and floor rays before adding their lookup cache; reversed geometric paths can
differ in floating-point details.

For longer-lived reuse, separate:

1. Planet-local material geometry, keyed by body identity and material revision.
2. Traversal eligibility, which also depends on actor dimensions, mobility and
   gravity assumptions.
3. Moving-obstacle and proposed-ship overlays, tied to the relevant body state
   and candidate pose.
4. Route/mission estimates, tied to those dependencies and their goal regions.

The current `GroundMap` combines material, gravity and live obstacle checks; it
cannot safely become a cross-update terrain cache by keying only on revision.
Rigid planet motion preserves intrinsic geometry while changing world obstacles
and possibly gravity in the local frame. Compatible geometry can be shared
between bots; their complete clearances, mobility and plans need not match.

Begin with conservative invalidation and bounded storage. Only add regional
terrain invalidation after identifying which nodes and crossing edges depend
on an edit. Stale estimates can guide which option to investigate; they cannot
authorize landing, jumping, boarding, claiming or rebuilding.

## Bound work and retain determinism

Schedule planning work for both bots under a shared deterministic work quota,
with urgent revalidation first and fair progress for remaining jobs. Charge
expansions and world queries separately and calibrate their quotas against Pi
timings. Record milliseconds, but do not let a wall-clock cutoff choose different
plans merely because one machine is slower. Spreading work bounds latency;
reuse and selective queries are still needed to reduce total computation.

The scheduler should accept an active set of bot/job identities rather than
assuming two seats. Keep one global allowance, configurable per-bot caps/weights,
and deterministic fair ordering; adding bots must not silently multiply the
global work allowance. Reallocate unused work according to a recorded rule.
Low-budget bots keep executing their current valid action as planning takes
longer. Immediate safety/control work remains separate and still costs time:
the planning budget does not make an arbitrary number of bots free to simulate.

Extract only the reusable mechanics once there is an actual bounded job:
identity, reset/cancellation, dependency validity, charged work and pending/ready
results. Keep Spacewars observations, mission values and movement rules in its
adapter. This provides a general bot-control boundary without building a generic
game-state language or a plugin system before a second use case exists.

Incremental graph search can hold an immutable map. Incremental physical surveys
also need a coherent measurement contract: pin compatible query data where
possible, or version and restart affected work. Do not silently combine queries
from changing live physics steps and label them one current survey. Measure the
largest indivisible physical query as well as job totals before claiming a frame
budget. Start with indexed synchronous queries before introducing this scheduler.

## Implementation and evaluation sequence

1. **Preserve behavior while indexing routes.** Compare complete routes and
   diagnostics, then replay both measured Pi seeds with identical non-timing
   output. Record graph construction separately from lookup savings.
2. **Improve one complete sortie.** Implement joint outbound/return evaluation,
   pass the chosen goal through execution, and expose its time/exposure evidence
   to landing selection. Add directed-route cases with multiple goal nodes and
   a misleading cheapest outbound endpoint. Version the changed policy.
3. **Make planning demand explicit.** Introduce request/result dependencies,
   within-survey reuse and bounded graph jobs. Add cross-update geometry reuse
   only with tested invalidation for excavation, detached ground, moving ships
   and debris, gravity and actor mobility changes.
4. **Choose missions using the same evidence.** Extend match context, implement
   time/risk-aware selection with cheap estimates and a refined shortlist, then
   compare predicted and actual sortie outcomes. Add shallow mission search
   after these estimates are useful.

The [asteroid route-failure investigation](../bot-objective-route-failures.md)
identifies the next bounded behavior extension alongside step 3: a prospective
crossing over the parked ship, measured in both directions and carried through
joint-trip execution. Fresh walk/jump surveys fail for all current landing sites
in that saved state, but an existing jetpack controller physically captures and
returns from one of them. This gives the flight extension a positive fixture and
a nearby placement that must remain rejected before broadening mission search.

Keep an optimization-only comparison separate from new-policy comparisons.
For behavior, run against the previous policy with paired seeds and swapped
seats, controlled interrupted sorties, generated asteroid matches, and held-out
worlds. New-vs-new self-play alone cannot establish an improvement. Preserve
human playtesting and exhibition settings alongside competitive comparisons.

Measure completed sorties and recoveries, exposure during landing/on foot,
pilot deaths, time without progress, objective switching, wins/draws, prediction
error, and per-update planner/sensor mean, p95, p99 and maximum. Keep the longer
known reproductions: the first profiled spike occurs after three minutes.
Decision traces should include selected and rejected options, cost components,
estimate age/confidence, invalidations, pending work and actual milestones.

The first deliverable is a cheaper route service plus a better choice of a
complete capture trip. It supplies evidence and interfaces for strategic search
without requiring a replacement of the working physical controllers.

## Acceptance gates for the next steps

Before changing sortie behavior, retain a constructible baseline with its
current sensor/planning semantics and explicit per-seat policy selection in the
existing runner. Record the concrete policy and configuration that actually
emits controls. A baseline-vs-baseline run with swapped seats checks the runner
before using it to compare a candidate. Changed physics requires a new shared
world comparison; it does not justify silently replacing historical traces.

For the first joint sortie candidate, optimize the existing walk/jump graph
score: total edge length plus two units per jump, across both legs. Keep the
existing landing-level cost conversion and cover weights for this comparison.
Prospective jetpack access, a common time model and changed strategic weights
are separate behavior experiments. Acceptance requires a directed fixture where
the cheapest outbound endpoint cannot return but another interaction endpoint
can, no claimed access when every return is blocked, and execution through the
selected endpoint to real boarding. Destroying support or obstructing the next
leg must invalidate execution through the existing controls and permissions.

For the scheduler, test zero and exhausted allowances, cancellation/reset,
deterministic ordering, unused-work redistribution and sustained progress for
several active jobs under one fixed total quota. Use lightweight synthetic jobs
to test more identities than the game's current two seats. Record per-bot
allocations, job age and charged work, plus separate safety/control time and
cache memory bounds. Calibrate numerical work quotas on the Pi at that step;
query counts alone do not guarantee a fixed millisecond cost in every world.

Promote a behavior candidate only after it demonstrates the intended fixture
improvement, passes the existing physical acceptance cases, and has a recorded
comparison against its predecessor on paired and held-out worlds. Report
regressions and stalled cases as well as successes. Keep equal-budget policy
comparisons separate from budget sweeps and preserve human playtesting with the
configured exhibition behavior. No universal win-rate threshold or strategic
weight tuning is needed to publish the current optimization checkpoint.
