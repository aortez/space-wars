# Learned-flight policy observation v1

The C2 policy adapter now supplies normalized, versioned inputs from current
physics at 60 Hz. It is shared by the headless environment and the runtime state
API. This is an observation-correctness checkpoint, not a trained controller or
a demonstration that the representation is sufficient for navigation.

The [contract](design/learned-flight-contract-v1.md) defines the task;
[LF-009](learned-flight-journal.md#lf-009--2026-10-22--implement-the-policy-observation-adapter)
records this slice. [Results](data/flight-policy-observation-v1-results.json) and
an [archive receipt](data/flight-policy-observation-v1-archive.json) accompany it.

## Input boundary

`FlightEnvironment::policy_observation()` calls the public
`SurfaceSortieState::flight_policy_observation()` adapter. Only its `inputs`
record is intended for the network. Stable collider/body IDs and the physics
tick remain outside those tensors for trace association. The raw diagnostic
`observation()` API remains available separately, including evaluator state and
legacy landing telemetry that must never be fed wholesale to the policy.

| Branch | Shape | Values |
| --- | --- | ---: |
| Self and goal | 35 | 35 |
| Polar occupancy and overlap | 23 bands × 64 sectors × 2 | 2,944 |
| Precise ray hits | 128 × 4 | 512 |
| Nearest solid | 6 | 6 |
| Global rigid bodies | 32 × 16 | 512 |
| Total | Five separate branches | 4,009 |

`flight_policy_observation::schema()` exports feature order, scales, radial
edges and ray bearings. These must accompany future model checkpoints. Flattening
is available for inspection; it does not prescribe a dense neural architecture.
The planned map encoder still needs circular angular padding, and the object
encoder must consume a masked set. Its permutation invariance is a C3 test.

All vectors use the ship frame. The goal is relative to the actual center of
mass (COM); polar geometry and object locations use the body origin. Both COM
and body-origin velocities are exposed, since rotation makes them different.
Ray and nearest-hit velocities use the physical velocity at the hit point,
including surface rotation, minus the ship-origin velocity. Fixed solids have
zero world velocity. Raw polar schema v2 retains the source identity needed for
these velocities; exact distance ties choose the smallest stable collider ID.

Missing rays/nearest hits and unused object slots have zero payloads and zero
masks. A valid hit at distance zero has a one mask. Fixed normalization scales
do not clip large valid values. Remaining time and required dwell are divided
by 30 seconds even for shorter diagnostic tasks; dwell progress is separately
encoded as completed dwell ticks divided by required dwell ticks. Non-finite
values and unsupported control states return explicit errors.

## Current global geometry

Records include every planet, sun, enclosing edge and other enabled solid rigid
body, excluding all bodies of the controlled ship. Attached terrain colliders
stay with their parent body; independently moving bodies in an articulated
assembly get separate records. Each record carries type, body origin, origin
point velocity, relative orientation, spin, conservative current shape bounds,
hazard boundary/state and the active-control-frame flag. The edge has its own
type because its interior is navigable.

Bounds use current enabled non-sensor shapes in body-local coordinates. They
survive world rotation and expand for added colliders; they are conservative
context, not exact geometry. Bodies beyond the 300-unit local sensing range
remain in the global set. More than 32 records is an explicit unsupported-state
error. No nearby-object truncation or controller handoff occurs.

Solar records use production `solar_hazard()` and `solar_exposure()`. Disabling
combat removes the active corona and exposure availability while retaining the
physical sun. The headless diagnostic tasks have no sun; a separate live-match
unit fixture checks both enabled and disabled solar state. This does not claim
heat-avoidance competence.

## Validation and retained probe

Six new adapter tests cover shared runtime/environment output, read-only sensing,
finite values, masks, no clipping, COM/origin math, rotating surface velocities,
whole-scene rotation/translation, added/removed solids, attached-shape bounds,
capacity overflow, fresh terrain edits, replay, and solar state. Three new
physics tests cover hit motion and misses/inside hits, deterministic ties across
insertion order, and local solid bounds excluding sensors.

The retained probe records initial raw and normalized observations, final policy
inputs, schema metadata and JSON traces for four diagnostic scenes: stationary,
rotating ship/terrain, an independent wall, and gravity with a moving goal. Each
advances 30 neutral-action ticks and verifies identical final policy observations
after independent initialization and serialized trace replay. These are fixtures,
not the four-family training distribution or a policy performance evaluation.

The probe also times 100 full-adapter reads of each unchanged initial scene,
after five warmups. The `ci` build is optimized with debug assertions enabled.
Timing excludes physics, reset, serialization, replay and network inference;
host/build identity and every sample are retained. The native-query counter now
includes nearest-source candidate projections, and rays resolve all native hits
to make exact ties deterministic. Earlier schema-v1 query counts and timing are
historical measurements, not an equivalent cost baseline.

| Scene | Median full observation | 95th percentile | Objects |
| --- | ---: | ---: | ---: |
| stationary | 0.960 ms | 0.990 ms | 2 |
| rotating-surfaces | 0.758 ms | 0.811 ms | 2 |
| independent-wall | 0.794 ms | 0.813 ms | 3 |
| gravity-moving-goal | 0.757 ms | 0.795 ms | 2 |

These small fixed scenes do not establish throughput for generated worlds,
many bots, a full training collector or target hardware. Keep 60 Hz; measure
those costs when their workloads exist.

Full regression results: **642 scenario tests passed (two ignored), 175 physics
tests passed (one ignored), and two sensor-example tests passed**. Formatting and
whitespace checks passed. Clippy completed with 18 existing warnings and none in
the new adapter/probe. All 1,471 selected source files match the retained archive;
the four earlier experiment manifest hashes remain unchanged.

## Handoff and reproduction

C2 remains in progress. Next, implement the declared four-family task generator
and independent admission checks for safe moving goals, full-hull clearance,
route connectivity, gravity coverage and task duration. Freeze sampling and
exclusions before training. The neural encoders, PPO, learner checkpoint/resume
and any claim of competent flight remain later work.

The local archive is
`data/training-experiments/learned-flight-observation-v1-20261022/`. It contains
the source snapshot (including uncommitted/untracked build inputs), source hashes,
optimized executable, schema, full observations/traces/timing samples, host/build
identity, test logs, documents and a checksummed manifest. No model weights exist.
This is local retention, not an off-machine backup; Cargo registry dependencies
and toolchain binaries are not included.

To inspect fresh output without overwriting evidence:

```sh
cargo run --profile ci -p scenario-spacewars --example flight_policy_probe --locked --offline -- --out /tmp/flight-policy-inspection-new
```

The output directory must not already exist. The retained executable can be run
with the same `--out` argument on a compatible host.
