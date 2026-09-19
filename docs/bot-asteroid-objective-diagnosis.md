# Why asteroid objective surveys do not publish

This investigation follows `f6f06d3` on `jetpack-landing-planning`. It adds
measurement counters, tests and retained diagnostic evidence. Policy behavior,
validation rules, operation allowances and UI defaults are unchanged.

The main finding is a dependency problem worth fixing next: scalar jump-gravity
changes retire entire unfinished surveys, including completed candidates that
need no ordinary ballistic jumps. Some of those candidates still pass their
own geometry and powered-flight checks. Separately, the old aggregate counts
mostly describe v10 searches that really found no complete measured route.
Increasing the search budget or weakening geometry validation would conflate
these two problems.

## Replays and meaning of the counters

The two retained asteroid comparisons use seed `7725194555774358125`, generated
duel, v11/v10 with seats swapped, a 600-second match limit, and mixed asteroids
every three seconds. Both seats share 16,384 graph operations / 1,024 queries
per update, with ground reuse and route dependencies enabled.

The earlier `completed` counter means **completed and validated for delivery**.
It does not count every finished search. The 280 / 159 `routes_changed`
rejections all concerned completed **v10** surveys containing eight failed
candidates. No positive route was removed from those particular results.

| v11 seat | v10 actor | Surveys reaching route validation | Disconnected candidates | No flag footing | No exit footing |
|---|---|---:|---:|---:|---:|
| P1 | P2 | 280 | 1,986 | 254 | 0 |
| P2 | P1 | 159 | 567 | 696 | 9 |

There were another 13 / 1 finished negative v10 surveys retired by the scalar
gravity check before reaching route validation, giving 293 / 160 raw finished
v10 surveys in total.

These are failures within the measured outer-ground graph, not proof that every
possible maneuver is impossible. A moving obstacle can also invalidate a
negative snapshot, which is why those answers correctly remain unpublished.

Counting candidate results before whole-survey completion reveals different
v11 activity:

| v11 seat | Finished v11 candidates | Successful walking candidates | Successful powered candidates | Whole v11 surveys finished |
|---|---:|---:|---:|---:|
| P1 | 9 | 2 | 1 | 0 |
| P2 | 3 | 0 | 1 | 0 |

All four positive candidates belonged to three requests retired for
`gravity_changed`. None used an ordinary jump in either leg. They had finished
both route selection and path-dependency construction; other candidate searches
were still pending.

## What the retained graphs show

At measurement tick 6,585, planet 1 revision 17, the base graph has two flag
footings: nodes 457 and 458. Boarding footings are present.

- Landing bearing 57 puts the proposed hull over both flag footings. Neither
  survives the hull overlay; the closest remaining actor-center position is
  4.46 units from the flag, outside the 2.8-unit planning range.
- Bearing 56 leaves both flag footings intact and returnable to the opposite
  entrance, but the hull disconnects them from exit node 442. The existing v11
  crossing between nodes 442 and 454 restores the complete trip and passes the
  physical forecast. This is an outward obstruction, not missing return access.
- Bearing 61 is already disconnected from the flag in the base graph, before
  inserting the hypothetical hull. A ship-only crossing does not address every
  gap in damaged ground.

Later in the opposite-seat replay, measurement tick 27,240, revision 70, only
flag footing 458 remains and it has **no incoming or outgoing edges** even in
the base graph. At tick 28,020, revision 71, that node is rejected for capsule
obstruction. Nearby samples also report missing retained floor and steep floor.
The closest accepted actor-center position is now 5.60 units from the flag.
Those failures arise before the proposed hull is added. More graph search alone
cannot manufacture an acceptable landing footing or a missing physical edge.
The retained data does not identify which collider obstructed the capsule;
that requires a focused clearance probe if we pursue this terrain case.

`retained-maps.json` saves six complete base/overlay map pairs. The accompanying
`inspect_maps.py` independently traverses their directed edges, measures exit,
flag and boarding eligibility, and checks the explicit approved crossing.
These are graph checks against original snapshots, not physical replayed jumps.

## Recheck at the exact cancellation tick

A separate read-only probe passed each partial result through the existing
geometry/flight validator at retirement, bypassing only the earlier scalar
jump-gravity rejection. It issued no controls and did not change the live queue.
The ordinary objective, age and landed-hatch checks precede that rejection.

| Actor / v11 seat | Request → retirement tick | Scalar gravity before → after | Result of the independent recheck |
|---|---|---|---|
| P1 / P1 | 10,890 → 10,894 | 20.68507 → 20.67409 | Both walking candidates still pass their path checks |
| P1 / P1 | 11,400 → 11,408 | 19.66994 → 19.65876 | Powered candidate passes field check but **fails geometry** |
| P2 / P2 | 6,585 → 6,605 | 19.87776 → 19.88818 | Powered candidate passes field and path checks |

Each change is just over the existing 0.01 scalar tolerance. Request age differs
from measurement age when ground data was reused: the first two measurements
were made at ticks 10,800 and 11,310. All remained within the 120-tick source-age
limit at retirement. The surviving candidates were still only hypothetical
landing plans; this probe does not establish that the bot would choose or
successfully execute them in combat.

The geometry rejection is an important counterexample: a broader “keep all
successful partial results” shortcut would admit a stale path. Keep the real
geometry check, powered-field check, equipment, actual-touchdown requirement,
original measurement age and immediate action permissions.

## Instrumentation kept in production

`LivePlanningTelemetry.measurements_by_actor` records finished surveys,
finished candidates, successful candidates, powered candidates and the existing
bounded failure categories. Candidate counts include work in cancelled requests.
`retired_partial_successes_by_actor` counts requests retired before finishing
all candidates despite finding a successful one. Neither counter implies live
validation, publication or physical execution. `completed` and `published`
retain their previous meanings.

Counters update at existing charged transitions and accumulate only new work
per dispatch. They add no query, graph search or solver step; failure maps have
four fixed possible keys. Repeated publication cannot recount a measurement,
and reset clears the counts. Full graph dumps and diagnostic logging remain
only in separately retained investigation binaries.

## Next implementation slice

The [scalar-gravity dependency follow-up](bot-jump-gravity-dependencies.md)
implements the first slice below and records its controlled tests and matched
replays. It also identifies a separate landing-site handoff gap after valid
routes begin publishing. The measurements above describe the earlier baseline.

First separate scalar jump-gravity dependencies from candidate validity. In the
opt-in route-dependency path, let coherent work finish after a scalar gravity
change, then preserve only successful candidates that remain justified: routes
with no ordinary ballistic jumps can retain their geometry evidence; powered
segments must independently pass their flight environment and launch-window
checks. Changed-gravity negative answers and routes using ordinary jumps must
remain unknown unless explicitly remeasured. Whole-region mode can retain its
strict contract. Preserve the actual-landed-route requirement and never renew
the source snapshot's age.

Test that rule against the three retirement cases above, including the geometry
rejection, before changing launch thresholds or adding terrain flight primitives.
If waiting for every candidate still loses otherwise usable opportunities,
consider publishing individually validated positive candidates while the rest
of the job continues; mark incomplete alternatives as unknown. That is a
separate scheduling/selection change, not required merely to relax the coarse
scalar dependency.

After this, the damaged flag footing is a distinct navigation problem. It needs
physical clearance/contact inspection and possibly a terrain-crossing or mining
primitive. Higher-level mission selection should distinguish a temporary lack
of measured access from permanent impossibility and avoid endlessly retrying
an unchanged failed approach.

## Verification and reproduction

The scenario/AI suite passed **621 tests**; all **20 live-planning tests** also
passed in debug. New assertions cover negative measurements withheld at
validation, positive candidates retired mid-survey, repeated delivery, and
reset. Client compilation, formatting and diff checks passed. Clippy completed
with existing warnings in unchanged code.

The final two asteroid replays match `f6f06d3`: 1,531 / 1,499 sparse trace
records and 5,522 / 2,764 allocation rows respectively (excluding dispatch time),
with clean physics and unchanged quota audits.
The final counter values also match the separately instrumented raw results.
This verifies observation of existing behavior, not improved match outcomes or
Pi performance.

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v11 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 3 --trace true --out /tmp/asteroid-objectives-p1
```

Swap policies for the P2 comparison. `target/asteroid-objective-diagnosis/`
contains `final-p1/p2`, raw `lifecycle-p1/p2` logs, retained maps, counterfactual
probe logs, before/after binaries, tests, diagnostic patches and audit scripts.
The counterfactual runs stop after 195 / 120 seconds, retaining the ordinary
600-second match rule. The late flag-footing probe stops after 470 seconds.
Their purpose is inspecting those intervals, not completing a match.

A verified copy of the reports, sparse traces, allocation tables, raw diagnosis,
source patches and scripts is archived under
`/home/oldman/.codex/visualizations/2026/09/18/bot-asteroid-objective-diagnosis/`.
Large per-tick sensor logs remain in the target directory. Baseline evidence is
also retained with [moving-flight planning](bot-moving-flight-planning.md).
