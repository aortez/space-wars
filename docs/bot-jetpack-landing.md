# Prospective jetpack landing routes

This is the next slice after two-sided boarding (`4037ba0`), on
`jetpack-landing-planning`. The opt-in `material_mission_v11` policy can consider
one powered crossing over its proposed parked ship when the existing walk/jump
round trip is disconnected. `material_mission_v9` and `material_mission_v10`
retain their sensor/controller identities. The launcher's existing Planner
choice still selects v10; v11 is currently a headless comparison option.

The measurements below describe checkpoint `e58cdfc`. The follow-up
[moving-objective investigation](bot-moving-objective-work.md) preserves valid
ground routes when a flight hypothesis becomes stale, with new comparison data
and unchanged flight thresholds. The subsequent
[moving-flight model](bot-moving-flight-planning.md) adds calibrated orbital
prediction and a bounded launch window.

## What the planner knows

The landing sensor first measures the existing walk/jump round trip. On failure,
v11 scans at most 512 measured ground nodes for a takeoff and landing footing on
opposite sides of the proposed hull. It uses the exact node positions, not a
nearby-node snap that might hide a gap. Both footings must pass the normal
retained-material, standing-clearance and slope requirements.

A preliminary graph search adds one explicit pair of directed flight edges.
If that pair still cannot connect the exit, a flag footing and either boarding
entrance, there is no reason to run the flight prediction. This hypothetical
result cannot be published on its own.

The flight job predicts both directions using the same lift/cross/descent
commands as the real jetpack controller. It models the initial jump, finite
thrust, powered lateral control, inertial takeoff velocity, surface spin and a
bounded sample of the celestial gravity sources. It checks capsule clearance
against the immutable world snapshot and the proposed hull along the predicted
path. Both flights must reach the landing window within twelve seconds and use
at most 93% of a pack: launch assumes at least 98% charge and reserves 5% for
settling. These percentages are model admission limits, not changes to equipment.

The output carries the crossing, exact endpoint node IDs, and flight time/burn/
arrival-speed estimates. There is at most one flight per outward/return leg.
The return can instead use the opposite entrance without flying again. The
landing score includes the predicted flight time and a full recharge allowance.
No walkable route is replaced simply because flying might be shorter in this
first version.

On foot, the task retains the jointly selected flag footing, checks the real
landed pose and refreshes flight evidence before launch. A fresh invalid
corridor interrupts the maneuver through the existing settling/replan path.
Each flight waits for the ordinary full-charge launch condition. Actual support,
claiming, neutralization, boarding and takeoff remain ordinary world actions.

## Work and validity

The existing shared `PlanningQueue` remains the scheduler. There is no additional
world step or gravity solve. Work is charged as follows:

- One proposal-node inspection is one graph operation.
- Indexing, each graph edge inspection and tracing remain resumable. The two
  virtual flight edges do not require cloning the entire graph inside dispatch.
- Each model integration step is one graph operation over at most 32 gravity
  sources. Each snapshot or hypothetical-hull clearance check is separately
  charged as a physics query. Each direction has at most 720 model steps.
- Flight query bounds join the selected-path dependencies. Whole-region
  validation includes the capped flight height, not just ballistic ground jumps.
  Frame/gravity changes also invalidate the flight hypothesis.

The comparisons retain the existing **16,384 graph operations / 1,024 queries per
shared update** and 120-tick maximum survey age. These are operation quotas,
not wall-clock limits. Snapshot construction, dependency validation, immediate
sensors and on-foot surveys remain outside that objective-work quota. The new
on-foot flight refresh is synchronous at the existing 30-tick survey cadence;
Pi cost for that added work has not been measured yet.

## Controlled physical evidence

`init_material_flag_crossing_trial(seed, seat)` constructs two parked ships on interpolated contour terrain and
an opposing flag on measured retained ground beyond the tested ship. The other
ship prevents the long walk around the planet. Setup installs the initial flag;
subsequent ownership changes use the normal capture rules. The fixture derives
its target from the old physical corridor sensor, independently of the new
forecast.

The tests establish:

- v10 reports no complete trip for the isolated pose; v11 measures one outward
  flight and a walking return to the opposite entrance.
- Completing the incremental job with small or unlimited dispatch allowances
  gives the same result. The original physics snapshot is unchanged.
- The bot plans while aboard, exits, crosses, captures the real opposing flag
  and boards in **both seats**, using the production live planner and actions.
- Separate real crossings succeed in both directions in both seats. The second
  leg waits through recharge and launches with at least 98% charge.
- Missing equipment, excessive fuel demand, obstructing geometry and dirty
  queries reject evidence; endpoint/revision identity is checked on use.
  Existing live tests exercise terrain edits, moving obstacles, age, clone/reset
  behavior and shared-quota accounting.

For the stationary radius-60 block-terrain test planet, the four physical flights predicted
2.58–2.62 seconds of burn. Actual burn was 2.58–2.62 seconds, with the largest
prediction difference approximately 0.03 seconds. The observed minimum charge
was 11.7–13.9%. Those results validate this controlled envelope, not every
possible orbit, crater, gravity field or impact during flight. The separate
end-to-end flag trial passes on interpolated contour terrain in both seats.

The ordinary enemy-flag capture runner also completed a real landing, claim,
boarding and departure with v11. That run selected a walkable landing and did
not need the new flight primitive; it is a compatibility check, not evidence of
a powered-route improvement.

The scenario/AI regression run passed 611 tests during implementation. Final
checks covered the updated live planner (17 tests), both physical crossing tests,
the blocked-flight sensor test and 35 client tests (one opt-in test ignored).
Formatting, client compilation and Clippy passed; Clippy reports existing warnings.

## Retained failure and match comparisons

The September 13 failure in [bot-objective-route-failures.md](bot-objective-route-failures.md)
is historical. Replaying its seed from the accepted boarding checkpoint at tick
26,799 now finds planet 2 already owned by player 1, at revision 53, and bearing
36 unavailable. The old saved capture failure cannot be used as a current
positive fixture. Its original binaries and physical continuations remain
available as documented there.

The retained v10 replay on this branch matches the boarding checkpoint's **1,531
sparse trace records and 5,522 planning-allocation rows**, excluding dispatch
time. This is not a dense every-tick control comparison. It also retains the
same charged totals and zero completed/published objective surveys for this
asteroid replay.

Four matched-policy runs used seed `7725194555774358125`, swapped seats and a ten-minute match limit. All completed with clean physics audits and every allocation within the unchanged shared quota.

| Conditions | v11 seat | Finish (s) | Winner | Surveys completed / published |
|---|---|---:|---|---:|
| Asteroids every 3s | 1 | 482.67 | v10 | 0 / 0 |
| Asteroids every 3s | 2 | 298.67 | v10 | 0 / 0 |
| Quiet | 1 | 598.35 | v11 | 243 / 4428 |
| Quiet | 2 | 600.00 | v10 | 221 / 3804 |

Completed/published counts cover both seats and include ordinary ground routes. A publication is a delivery of validated evidence, not necessarily a new search or a powered route. This one-seed sample is not a strength ranking. It provides **no basis to promote v11 over v10 yet**: the controlled maneuver works, but generated matches still expose incomplete objective work and motion-related invalidations.

An intermediate comparison exposed an unnecessary restriction: flight-environment validity was being required even before a walk-only job needed a flight model. The final adapter applies it once a job starts a flight prediction; an explicit test covers this boundary and equipment removal. The final results above are from that corrected version.

## Limits and the next investigation

This is a conservative, opt-in controller model. It is not a copied Rapier
simulation or a proof that a future landing will survive disturbances. In
particular, it projects source motion at constant velocity over the flight and
requires gravity close to the local outward normal. Surface spin is modeled,
but curved orbital motion, disturbances, orientation/contact transients and
settling energy need a wider calibration matrix. Fresh physical observation and
the unchanged world permissions remain authoritative.

The strict flight-environment check can reject a request before it finishes in
a moving generated world. Do not interpret fewer graph operations in a different
match trajectory as a speedup. Before promotion, classify these invalidations
and compare predicted/actual flights across larger planets, positive/negative
spin and orbital motion. Model the known curved frame trajectory where the
linear approximation is inadequate; preserve coherent snapshot identity and
recharge requirements rather than increasing the quota or relaxing validity
without evidence.

Also measure the added on-foot refresh on the Pi before enabling v11 in the
launcher. General terrain-gap chains, resource-state search across several
flights, mining routes and new strategic weights remain subsequent work.

## Reproduce

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p spacewars-ai \
  --test jetpack_forecast -- --nocapture
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p scenario-spacewars \
  --lib live_planning

cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --example surface_flag_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v11 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 3 --trace true --out /tmp/jetpack-objective-p1
```

Swap policies for the opposite seat; use `--asteroid-interval 0` for the quiet
comparison. Each report records both policy identities, per-seat objective
profiles, the shared allowance and invalidation/charged-work totals. For a
single-landing investigation, `surface_flag_soak --landing-bearing N` filters
the available measured sites; it never moves the ship or waives landing rules.

Local reports, commands, build/test logs, the retained baseline binary and replay
instrumentation are under `target/jetpack-landing-planning/`. Its manifest and
comparison summary identify the retained artifacts. Timing measurements from
concurrent runs are not hardware-performance benchmarks.

A compressed copy of the comparison reports, sparse traces, allocation tables,
source patch and binaries is retained at
`/home/oldman/.codex/visualizations/2026/09/14/bot-jetpack-landing/evidence.tar.gz`.
The manifest distinguishes intermediate investigations from the final
`comparison-*` runs.
