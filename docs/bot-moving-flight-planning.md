# Jetpack forecasts on moving planets

This follows `3e03fca` on `jetpack-landing-planning`. The opt-in v11 sensor
profile is now `live_jetpack_objective_v3`; the policy remains
`material_mission_v11`. v9/v10 and the launcher's Planner choice are unchanged.
This slice has not been deployed to the Pi.

## Why the earlier forecasts expired

The [previous checkpoint](bot-moving-objective-work.md) preserved independent
walking routes when flight evidence went stale. Its quiet P2 replay approved
120 flight forecasts, but all 1,544 publication-time environment checks failed.
The old comparison treated expected orbital motion as an unexpected field
change, and its flight model approximated the planet's motion with a straight
line. Relaxing that comparison alone would have admitted inaccurate flights.

The new model follows the known kinematic orbital trajectories in fixed inertial
axes. It includes the planet's moving center, absolute takeoff velocity,
rotating footings and the changing locations of the gravity sources. It uses
the same physical jump, thrust and flight commands as before. It predicts a
controller trajectory; it does not run another Rapier world or apply another
gravity solve to the real world.

A first curved model exposed two numerical effects in completed motion samples:

- Rapier computes kinematic velocity from two `f32` world poses. Comparing
  samples includes pose-rounding error divided by the small physics timestep.
  The velocity check now adds an explicit coordinate-scaled rounding envelope
  to the existing 0.01-unit/second tolerance.
- The engine accumulates orbital angle in `f32` every tick. At larger angles,
  repeated rounded additions differ from a closed-form rotation. One retained
  failure occurred at tick 23,294: a source's predicted position differed by
  0.01013 units after only 29 ticks. Prediction now uses the engine's actual
  per-tick phase recurrence, retaining the 0.01-unit position tolerance.

Field validation compares the newly observed frame with the prediction for
that tick. Source mass, radius, orbital rate/center, spin, age and physical
motion still participate in validation. Tests deliberately change these
inputs and check rejection. Predictable source motion alone does not authorize
an old instantaneous flight estimate.

## Launch window and bounded work

Forecast format version 2 carries a measurement tick and a launch deadline,
with the existing maximum age of 120 ticks (two seconds). Time-dependent
fields require both crossing directions to succeed at launch offsets of zero,
one and two seconds. A single-source linear, axisymmetric field needs one
start. The result carries the largest flight time, burn and arrival speed
observed for each direction. These are componentwise bounds over the samples,
not a proof covering every intermediate launch time.

Every sampled flight retains the existing twelve-second horizon, at least 98%
launch charge and 5% landing reserve. Capsule queries check the immutable world
snapshot and hypothetical parked hull; their bounds join the route's geometry
dependencies. Real on-foot observations refresh the forecast before launch.
Consumers reject expired launch evidence and retain the existing interruption
and settling behavior. Actual support, capture and boarding remain ordinary
world permissions.

The shared allowance remains **16,384 graph operations / 1,024 physics queries
per update**. Each integration advances at most 32 source trajectories and
costs one graph operation; each clearance query is charged separately. Warming
up a future launch also costs one graph operation per tick, plus one operation
to initialize the flight. Across six flights the delays add 360 graph
operations. Small and unlimited dispatch allowances produce identical results,
query bounds and charged totals, without changing the real physics snapshot.

Publication validation is separately bounded by 32 sources and 120 phase
additions per source, with only the final two poses evaluated. As before,
snapshot construction, validity checks and immediate/on-foot sensors are
outside the objective scheduler's operation quota. On-foot forecast refresh
remains synchronous at the existing 30-tick survey cadence. Its increased Pi
cost has not been measured; operation quotas are not wall-clock guarantees.

## Controlled physical calibration

`init_material_moving_crossing_trial` makes an interpolated-terrain planet,
two parked ships and an opposing flag beyond the tested hull. The sun mass
matches the scripted orbital acceleration; local surface gravity is about 18.
Setup derives the flag footing from the older geometric corridor sensor and
installs only the initial flag. Subsequent flight, ownership and boarding use
normal actions and physics.

The physical tests cover radii 30, 60 and 80, positive and negative orbit/spin,
both seats and both crossing directions: **24 real flights**. They compare the
forecast available at actual launch with fuel use, first supported touchdown
time and touchdown position. Six additional moving-world sorties exercise
planning aboard → exit → fly → capture the opposing flag → board, using the
production incremental planner. The two stationary versions still pass.

The radius-100 and radius-128 variants deliberately exceed the same forecast
fuel allowance and are rejected. This is a limit of these particular hull,
gravity and motion fixtures, not a rule that large planets are inaccessible.
The environment tests also cover a generated multi-planet field early in its
orbit and after adding ten full turns to its accumulated phase.

Across the 24 moving flights, the maximum absolute burn error was **0.03335
seconds**, the maximum touchdown-time error **0.16667 seconds**, and the largest
touchdown position error **0.14366 world units**. Minimum observed charge was
**9.44%**. These measurements cover the controlled envelope above, with ordinary
on-foot forecast refresh; they do not certify arbitrary terrain or impacts.

The full scenario/AI regression run passed **620 tests**, with none failed or
ignored. The five targeted forecast/environment tests also passed in debug.
Client compilation, formatting and diff checks passed. Clippy completed with
existing warnings in unchanged code.

## Matched generated-world replays

Baseline: retained `3e03fca` binary. Seed `7725194555774358125`, generated duel,
600-second match limit, live objective work for both seats, ground reuse and
route dependencies. Asteroid runs use mixed severity every three seconds.
All five matched runs finished with clean physics audits, and every allocation
and charged total passed the unchanged quota audit.

Counts below combine both seats. A delivery can repeat an already completed
survey. Changed match trajectories and durations make these replay totals,
not isolated CPU savings or a policy-strength ranking.

| Conditions / v11 seat | Duration before → after (s) | Completed surveys before → after | Deliveries before → after | Queries before → after |
|---|---:|---:|---:|---:|
| Quiet / P1 | 598.35 → 598.35 | 243 → 243 | 4,428 → 4,428 | 932,384 → 932,384 |
| Quiet / P2 | 600 → 438.73 | 356 → 226 | 6,629 → 3,523 | 1,906,736 → 1,726,504 |
| Asteroids / P1 | 538.87 → 538.87 | 0 → 0 | 0 → 0 | 3,643,681 → 3,654,991 |
| Asteroids / P2 | 600 → 600 | 0 → 0 | 0 → 0 | 1,956,217 → 1,957,996 |

The updated quiet P2 run started 172 forecasts, approved 128 and rejected 41 for
fuel reserve; three starts did not finish. All **420 field checks passed**,
and **630 powered entries were delivered** after field/geometry validation.
Entries can be delivered repeatedly and are prospective alternatives; 630 is
neither a count of unique plans nor of actual flights. The controlled physical
trials establish execution, separately from these publication measurements.

Quiet P1 and both asteroid outcomes stayed the same. Quiet P2 changed from a
v11 2–1 ownership win at the time limit to a v10 win by pilot death at 438.73
seconds. This is a usable planning capability, **not evidence that v11 is a
stronger match player**. Keep it opt-in. The retained-v10 asteroid replay has
1,531 identical sparse trace records and 5,522 identical allocation rows
(excluding dispatch time) compared with `3e03fca`; this is not dense every-tick
equivalence. Concurrent desktop timing is not a Pi benchmark.

## What remains and how to investigate

The subsequent [asteroid investigation](bot-asteroid-objective-diagnosis.md)
separates negative v10 results from unfinished positive v11 candidates and
identifies scalar gravity invalidation as the next dependency fix.

Asteroid runs still publish no completed objective surveys. They have the same
280 / 159 route-change rejections as the prior checkpoint. Before adding more
maneuvers, retain a negative survey at its original snapshot and distinguish
missing exit/flag/return nodes, disconnected graph components, overlay failures
and actual field/fuel denials. Count successful routes before invalidation so
we do not mistake a negative answer for a usable path lost to an asteroid.

The launch window samples three times, the spin prediction remains continuous,
and clearance uses a retained geometry snapshot. Unexpected future collisions
and terrain changes are not forecast. Normal dependency checks and immediate
controls must continue to gate execution. Additional curved-surface crossings,
mining and resource-aware multi-flight routing remain separate work. Measure
on-foot sensor cost on the Pi before promoting this profile to the UI. The
higher-level destination/mission utility layer can then consume these measured
capabilities without changing the low-level flight and combat controllers.

## Reproduction and retained evidence

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p scenario-spacewars -p spacewars-ai -- --nocapture
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 0 --trace true --out /tmp/moving-flight-quiet-p2
```

Swap seats and use interval 3 for the other comparisons. The retained-v10
comparison uses v10 in **both seats with interval 3**. The extra v10 quiet run
is not the retention comparison.

`target/moving-flight-planning/` retains the evidence. Final discrete-phase
results are `discrete-*`, including `discrete-mission-soak`, regression logs,
calibration JSON, comparison summary and `summarize_discrete.py`. The older
`final-*` files are an intermediate closed-form candidate, not the final result.
`field-diagnostic.log` and its separately retained instrumented source/binary
record the rounding diagnosis; production contains no diagnostic logging.
The older checkpoint's evidence is linked from
[bot-moving-objective-work.md](bot-moving-objective-work.md).

A verified compressed copy, manifest and summaries are retained at
`/home/oldman/.codex/visualizations/2026/09/18/bot-moving-flight-planning/`.
Large per-tick sensor logs remain in the target directory. The archive retains
reports, sparse traces, allocation tables, binaries, tests and the source patch
so this investigation can be resumed without rebuilding the original baseline.
