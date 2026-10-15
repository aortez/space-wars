# Material-ship arrival diagnostic

This is the first real-physics integration of the
[evolutionary training framework](design/evolutionary-training.md). It evaluates
one guidance component on eight fixed cases, using the existing material ship,
normal gravity, and ordinary turn/thrust/brake actions. It is a local developer
fixture; the Training UI, genome browser, and match-brain integration remain
planned work. No useful evolved ship controller has been established yet.

## World and component

The fixture uses `SurfaceSortieScenario::init_material_flight` with seed 42,
one pilot, a stationary-center material planet with its ordinary spin, block
terrain, and open wings. The unused second ship slot is not simulated. There
are no opponents, weapons, sun, or spawned asteroids in these cases. Only the
initial placement is prescribed; the evaluator never moves the ship afterward.

The target is a non-colliding marker with constant world velocity. Its starting
offset and velocity are specified in the initial radial frame: +x tangential
right, +y away from the planet. The marker begins its motion when scoring starts.
The entire prescribed marker path must stay inside the allowed region.

Before activation, the runner sends neutral controls and waits for the actual
arming gate and a completed physics step, with a 30-tick setup deadline. Each
fixed case arms in one tick. Invalid starting conditions are setup errors. Once
active, the same conditions becoming invalid are recorded episode failures.

`free_flight_observation` reads body-origin position and point velocity, the
last shared gravity result, the planet's velocity at that point, flight limits,
identity, eligibility, and retained landing/contact state. It requests no
landing/access surveys and performs no additional gravity or physics solve.
Tests compare it with the existing pilot observation and check that reading it
preserves the solver snapshot and subsequent trajectory.

The experimental genome contract is `material_ship_open_v1` /
`arrival_guidance_diagnostic_v1`, with nine inputs and three outputs at 60 Hz.
Vectors rotate into ship-local coordinates; +y is forward and +x is right.

| Inputs, in order | Fixed divisor |
| --- | --- |
| Target minus ship position, x/y | 100 units |
| Target minus ship velocity, x/y | 70 units/s |
| Ship minus planet-frame velocity, x/y | 70 units/s |
| Observed gravity, x/y | 45 units/s² |
| Ship angular velocity | 1.8 rad/s |

Each scalar clamps to [-1, 1], and reports count saturated ticks per feature.
No feature saturated in the reference runs below. These scales describe the
bounded diagnostic; broader training envelopes still need calibration. This
arrival vector does not represent terrain, combat, or the whole world.

The model outputs turn, thrust, and brake. Turn decodes with `tanh`; each held
button uses `raw > 0`. Zero is neutral, and thrust plus brake is legal. The
coordinator fixes wings open and interaction, weapons, and mining off. It adds
no throttle modulation to learned outputs.

The reference rule uses the same physical features, bounded positional velocity
correction, observed gravity, and brake compensation at the target's velocity
in the planet frame. Its own fractional thrust accumulator emits ordinary held
button pulses. Keeping the actual brake's damping of velocity error made the
moving-left case settle without changing the success thresholds.

## Outcomes and evaluation

Success requires distance at most 2 units and target-relative speed at most
1 unit/s for 30 consecutive post-step states. A flyby resets dwell. Cases allow
1200 scored ticks (20 seconds); arrival wins a deadline tie.

The allowed ship body-origin altitude is 40–220 units above the planet's nominal
radius. Contact, landing-assistance entry, damage, loss of the supported ship or
control frame, and leaving that region are explicit failures. Assistance is
checked by strength as well as phase so held thrust cannot hide its activation.
An incompatible model is a load error; a non-finite model output is an
`invalid_output` episode failure. Broken world observations remain host errors.

The fixture records a practice-brain definition and hash: target-source version,
single-owner coordinator version, model contract, and exact rule/model identity.
Changing the component changes that brain identity. A controller instance owns
one episode binding and epoch. Identical repeated requests return the cached
decision; conflicting, skipped, stale, or wrong-owner requests fail. A new target
identity resets reference memory. Restart creates a fresh component instance.
The host is synchronous; asynchronous worker ownership is not implemented here.

Every terminal outcome sends a complete neutral control set for one cleanup
tick. The report keeps that observation separate from the scored endpoint.
This releases held controls before any continuing trial; it cannot earn arrival.

`ArrivalTask` implements the same `EvaluationTask` interface used by the line
mover. Its manifest pins cases, build/flight limits, timestep, thresholds,
setup deadline, region, and score comparison. The diagnostic tuple sums:

1. Completed cases.
2. Terminal position points: `1000 * clamp(1 - distance / 100, 0, 1)`.
3. Terminal velocity points: `1000 * clamp(1 - relative_speed / 70, 0, 1)`.
4. Unused ticks on successful cases.

Point values truncate to integers. Only arrival and timeout receive shaping;
other failures score zero. The order is lexicographic. This is an integration
recipe, not a calibrated training curriculum or qualification score. Tests run
material evaluations through the generic mutation/selection/checkpoint loop and
compare uninterrupted and resumed results without requiring learned improvement.

## Run and inspect

```sh
cargo build --profile ci -p engine-agent --example ship_arrival
mkdir -p target/ship-arrival

target/ci/examples/ship_arrival suite --policy reference \
  > target/ship-arrival/reference.json
target/ci/examples/ship_arrival episode --case moving_left --policy reference \
  > target/ship-arrival/moving-left-trace.json

target/ci/examples/ship_arrival genome --out target/ship-arrival/neutral.json
target/ci/examples/ship_arrival suite --policy model \
  --genome target/ship-arrival/neutral.json \
  > target/ship-arrival/neutral-results.json
```

`genome` exports a known-failure neutral model. Loading it checks the real model
adapter in stationary and moving cases; it reproduces the idle rule's physical
trajectory. Diagnostic line-mover genomes are rejected before world creation.

Episode traces contain the initial specification, complete practice-brain
identity, executable/platform identity, completed observations, current marker,
physical and encoded features, clipping flags, raw outputs, ordinary controls,
next observation, and physical dwell metrics. A missing ship has an absent
final observation and `lost_control`, rather than an invented final pose.
Trace retention does not change the trajectory or score. Reports are bounded
by the 1200-tick episode cap; this tool does not yet provide a visual replay UI.

## Reference results

Local Rust 1.89.0 `ci` profile on Linux x86_64, with the fixed cases above:

| Case | Scored ticks | Final distance | Relative speed |
| --- | ---: | ---: | ---: |
| `stationary_radial` | 752 | 1.797 | 0.377 |
| `stationary_right` | 887 | 1.802 | 0.110 |
| `stationary_left` | 757 | 1.812 | 0.404 |
| `stationary_rotated` | 863 | 1.813 | 0.437 |
| `stationary_reversed` | 844 | 1.796 | 0.362 |
| `moving_right` | 944 | 1.804 | 0.686 |
| `moving_left` | 816 | 1.793 | 0.435 |
| `moving_rotated` | 911 | 1.806 | 0.180 |

All eight arrive after the full dwell without assistance or contact. These are
reachability checks in one fixed world/build, not evidence of general flight,
landing, recovery, or match competence. The initial unsuccessful moving-left
reference run was retained during calibration; its miss was not excluded by
relaxing the evaluator.

Relevant checks:

```sh
RUST_MIN_STACK=16777216 cargo test --profile ci -p engine-agent --lib ship_arrival
RUST_MIN_STACK=16777216 cargo test --profile ci -p spacewars-ai --lib guidance
RUST_MIN_STACK=16777216 cargo test --profile ci -p scenario-spacewars --lib surface_sortie::free_flight
```

The [first ship-training experiment](ship-training-experiment.md) declares a
9→8→3 model, three bounded training runs, and separate stationary-validation
and moving-transfer cases. It uses this evaluator without changing the original
eight-case diagnostic or its physical arrival criteria.
The [larger design](design/guidance-component-v1.md) still covers managed jobs,
Training and Genomes views, richer world/build coverage, and match integration.
