# Budgeted comparison of escape successors

This follows the [destination-cover measurements](bot-destination-cover.md) at
`acda405`. An opt-in diagnostic compares a flight toward each measured site,
continued escape within the existing deadline, and deliberate combat. It reuses
the motor controllers and the existing deterministic scheduler. **No result
changes the live bot's choice.** This is the first flight leg of the successor
comparison; capture/return feasibility and combat outcomes remain unmeasured.

## What a job evaluates

`MaterialMissionPilot::successor_comparison` creates a job immediately after the
successful escape handoff's controls have been emitted. It pins the observation
and copies controller memory. Every advancing branch first applies that already
emitted flight command, then chooses its proposed action on later ticks. It
cannot rewrite the handoff tick or reuse a cached combat command as a new
decision. The authoritative world and controlling pilot remain untouched.

The shortlist contains at most four site proposals, continued escape and combat.
Each eligible option gets three independent paired trajectories, using the prior
coast, brake-to-aim and pursuit hypotheses. The own controller reacts to that
branch's opponent motion; combat is not evaluated against a coasting opponent
and then assigned somebody else's range curve.

- **Site approach:** propose a staging point 85 units above the sampled ship
  origin along its measured normal, transformed from the sample's material
  frame to the predicted planet pose. Reuse obstacle routing and motor guidance.
  Include the destination planet among obstacles, so a far-side site does not
  invite a chord through the planet. Arrival does not start landing or create
  ground permissions.
- **Continue escape:** reuse the current direction and escape flight controller,
  including the optional boundary guard. Stop at the original deadline. Planning
  latency and successful separation do not grant another twelve seconds.
- **Combat:** reuse pursuit, ground-clearance behavior and the combat controller,
  including retained exhibition settings. Steering assumes clear sight to the
  hypothetical enemy. Disable weapon outputs; visibility, damage and future
  weapon consumption are not simulated.

The six-second horizon is an explicit model limit. Branches end earlier on
approach arrival, the escape deadline, or an own/opponent boundary or nominal
obstacle margin. These margins mark where the free-flight approximation is
insufficient; they do not prove physical collision or impossible travel. The
report records evaluated ticks and does not score an unmodelled tail.

This remains the prior approximate motor model: own measured gravity is held
constant, planets translate and spin, and enemy hypotheses begin with open wings
and zero gravity. Contacts, orbital acceleration, asteroid strikes, firing,
changing health and ground travel need other evidence. A forecast that ends far
from its staging point is not a completed transfer.

## Evidence and value stay explicit

Sites retain their exact ID, source sample, age and cover finding. Changed
material revision or vehicle form, missing planets, incomplete measurements, or
new ownership prevent a proposal from supplying an endpoint. A compatible old
sample can propose a flight to investigate; it cannot authorize a surface action.

The comparison records own ship health, observed opponent health, own energy,
rounds, reload progress, laser recharge state, current weapon availability and
the number of owned planets. Each site records whether capture adds ownership
and provides the first rebuilding foothold. Unknown ownership is distinct from
measured neutral ground. These are source facts, not predicted damage or a
strategic utility score. Enemy ammunition is not observed.

Every option names missing evidence. Transfers need current landing geometry
and cover, feasible live flight, and a capture-and-boarding round trip. Further
escape needs a successor at its deadline. Combat needs current firing geometry
and an outcome/damage model. The report is explicitly diagnostic; it does not
select a winner by minimum range or convert stale cover into permission to land.

## Shared work, storage and timing

`SuccessorComparisonJob` implements `PlanningJob`. One graph work unit advances
one own/opponent pair by one motor tick, with no physical queries. At most
eighteen branches and 6,480 paired ticks are retained per job. Branches receive
work in stable round-robin order. The host retains one job per actor, replaces
superseded work and writes completed reports to disk.

Local objective jobs and destination-cover checks dispatch first. The successor
queue gets only their unused global graph allowance: fixed-priority composition
of the existing scheduler, not an independent allowance. Its default per-actor
cap is 128 paired steps per physics tick, adjustable with `--successor-step-cap`.
With no work available, an unfinished job stays pending rather than reporting
failure.

Results describe the pinned source even when completed later; the host logs
completion age. An active policy will need an earlier trigger and live
revalidation. This diagnostic finishes historical comparisons as the match
moves on. Snapshot construction is bounded but synchronous and timed separately.
New construction and dispatch are included in combined planning time; trace IO
is excluded. Existing sensors and the older synchronous handoff probe retain
their documented scope. A paired motor tick does not necessarily cost the same
time as a ground graph node, and these runs do not establish Pi frame-time bounds.

## Results from the matched replays

The final matrix has sixteen complete matches: the previous escape and optional
boundary-aware escape in six cases each, two disabled controls, and two extra
quiet-P2 budget trials. The primary seed is `7725194555774358125`, with quiet and
three-second asteroid variants and additional P2 asteroid cases at seeds 2 and 7.
The previously recorded ten-minute match limit is unchanged.

- **53,026 recorded trace rows match byte-for-byte**, including observations,
  controls and previous handoff forecasts. Complete round outcomes, cover traces
  and all **30,928 local allocation rows** also match; only allocation timing is
  excluded. Physical audits pass.
- At the default cap, **eleven comparisons consider 64 options**. Ten site
  proposals lack a measured landing endpoint; the remaining options produce
  **162 paired branches** and consume **41,393 graph-work units**. The boundary
  seed-7 run enters recovery before a successful handoff and submits no job.
- Of those branches, 99 reach the six-second horizon, twelve stop at the escape
  deadline, 42 reach an own boundary margin, four reach an opponent boundary
  margin and five reach an opponent obstacle margin. **None reaches its site
  approach point within this horizon.** This cannot yet certify a whole transfer.
- Default jobs finish 0–50 physics ticks after their source. The seven-step-cap
  quiet-P2 job gives the identical result after **893 ticks**, versus 48 at the
  default cap. That is already beyond the source escape deadline: deterministic
  completion is not timely permission to act. The zero-cap version remains
  pending and leaves the match unchanged.
- Every per-tick sum of local work and successor work remains within the one
  global allowance. Desktop construction averages 0.108 ms (maximum 0.240 ms);
  active dispatch averages 0.025 ms (maximum 0.033 ms). These are small samples
  from concurrent desktop runs, not Pi calibration or a complete bot timing bound.

The following are conditional model results, not observed alternative outcomes:

| Handoff | Forecast evidence | What remains unresolved |
| --- | --- | --- |
| Boundary-aware quiet P2 | Pursuit enters the 300-unit diagnostic range during all four transfers, after 4.02–4.55 s. Further escape stays at least 413 units away for its remaining 4.77 s. All sampled endpoints are exposed. | What to do at the unchanged deadline; preserving separation briefly does not finish a mission |
| Boundary-aware main asteroid P2 | The covered planet-2 site is still about 810 units away after six seconds. Pursuit enters range after 2.32 s. Continued escape also re-enters range before its 3.67 s remaining deadline. | How to survive the flight to cover; another covered endpoint alone does not solve the approach |
| Boundary-aware seed 2 P2 | Evaluated branches remain separated, but several opponent hypotheses enter a boundary/obstacle margin before six seconds. The nearer covered site initially requires a detour and remains distant. | Contact-aware opponent prediction and a physically executable site approach |
| Original quiet P2 and seed 2 P2 | The free-flight model reaches an own boundary margin after about 0.4 s, or begins within one. Both physical baselines ultimately win. | A model-validity stop is not evidence that the real option loses |

Current weapons and capture value are visible alongside these tradeoffs. For
example, quiet P2 has one owned planet, two rounds and full energy, but about
39.5 ship health versus 98.3 observed opponent health. Main asteroid P2 has no
owned planet and thus values a first rebuilding foothold. These facts motivate
different questions; they do not supply a calibrated fight-versus-capture score.

Validation passes **102 AI unit tests**, including nine new comparison tests,
and **21 physical mission/combat tests**. Tests cover read-only repeatability,
two resumable jobs under one small quota, zero work, the committed control tick,
unchanged deadlines, stale/incomplete evidence, material edits, ownership,
different bearings on one planet, model-margin stops and reactive combat paths.
Client compilation, formatting and AI/core all-target clippy with `--no-deps`
and warnings denied pass. A dependency-wide clippy invocation encounters the
existing `collapsible_else_if` warning in `engine-rapier/src/spaceling.rs:395`;
that file is unchanged by this work.

## Reproduction and evidence

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --disengagement-boundary true \
  --probe-disengagement-handoff true --probe-destination-cover true \
  --probe-successors true --successor-step-cap 128 \
  --seed 7725194555774358125 --asteroid-interval 0 --trace true \
  --trace-start-tick 10718 --trace-end-tick 11800 \
  --out /tmp/successor-comparison-quiet-p2
```

Remove `--probe-successors true` for the destination-cover baseline.
`successors.jsonl` holds comparisons with source/completion ticks;
`successor-work.csv` records charges and the allowance left by local work.
`report.json.successor_comparison` retains counts, timings and unfinished jobs.
Combine those charges with `live-planning.csv` to audit the global allowance.
Request identities are scoped to their respective queues/files.

`target/successor-comparison/` retains commands, reports, traces, allocation
records, comparison scripts and tested binaries. Its earlier `pre-prefix/`
results predate the explicit committed control tick and are superseded by the
final matrix.
A verified evidence archive is retained under
`/home/oldman/.codex/visualizations/2026/09/19/bot-successor-comparison/`.

## Before changing the policy

Exercise a selected alternative in a controlled physical continuation from the
same handoff, comparing predicted and actual motion up to each model limit. Keep
the quiet P2 regression, successful seed 2 escape and losing main asteroid P2
match. Include a measured site approach and the remaining escape interval, with
the existing deadline intact.

Obtain current geometry before any surface commitment and preserve the local
joint-trip planner's authority over capture and return. Record weapon supply and
damage as well as separation and match outcome. The current resource snapshot
does not justify choosing combat. Use validated components to rank complete
successors before testing a new opt-in policy against the preserved baseline.
