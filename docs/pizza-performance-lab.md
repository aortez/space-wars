# Pizza performance lab

Pizza has a deterministic performance mode for comparing collision mechanics
and gravity independently. The visual and headless paths construct the same
fixture and call the same 60 Hz scenario step.

Normal interactive Pizza uses the same canonical Rapier world as the Rapier
benchmark. The shared gravity system computes mutual gravity, while Rapier owns
ball and wall motion, held-ball kinematics, contacts, and contact impulses.
Classic remains available only as a collision benchmark reference.

Normal gameplay has a saved 500-ball budget by default, adjustable up to an
experimental 5,000. See the [capacity lab](pizza-capacity.md) for the normal
contact/damage/fragmentation fixture and CPU-headroom qualification boundary.

## Event runtime

Normal Pizza now owns a small deterministic event scheduler (#356). The first
kind is directional gravity, with the physical field and direction cue from
#357. Launcher settings and pause-menu controls are implemented in #358.

`PizzaConfig::automatic_events` defaults to true for normal runs. The scheduler
samples 3,600–7,200 ticks of ordinary play on initialization and after each
event ends. Only positive fixed-timestep updates advance it, at 60 Hz before
the physics `WORLD_TIME_SCALE`. After the quiet gap's last update, the next
update admits an event. An event owns exactly 900 updates, including its
admission update, then starts a fresh gap. There is no wall-clock catch-up.

Headless callers and host controls use the ordinary action boundary:

```rust
use engine_common::Scenario;
use scenario_pizza::{PizzaAction, PizzaEventKind, PizzaScenario};

let request = PizzaAction::TriggerEvent(PizzaEventKind::DirectionalGravity).encode();
// Queue without advancing the scenario; the next positive step admits it.
PizzaScenario::step(&mut state, &[request], std::time::Duration::ZERO);
```

A manual request bypasses the quiet gap, takes precedence over a due automatic
event and works with automatic events disabled. Repeated pending requests and
requests during an active event are ignored without extending the visit,
queuing another visit or consuming random choices. Admission chooses one of
left/right/top/bottom uniformly and retains that screen direction through
resize. The scheduler uses its own seed-derived RNG; spawning and fragmentation
continue using their existing stream.

`PizzaState::event_state()` exposes Disabled, Idle or Active, including the
pending request, automatic wait, selected direction, admission source and
elapsed/remaining ticks as applicable. `PizzaScenario::observe()` returns a
bounded JSON `PizzaObservation` with version 1, simulation tick and that state.
Reading observations or rendering frames advances neither time nor RNG.
Pause preserves event state; resize preserves both pending and admitted
requests; restarting with the same seed/configuration recreates the initial
schedule. Leaving the scenario drops the scheduler and any pending request.

Sparse/dense/churn benchmark configurations always disable both automatic and
manual events, regardless of the requested setting. Normal-path cascade labs
can instead set `automatic_events: false` without selecting benchmark mode,
then send a typed manual request with a recorded run seed when needed.

## Interactive controls and diagnostics

Pizza settings include **Automatic events**, default On (including older settings
without this field). Starting a game saves the choice through the normal launcher
settings lifecycle. It applies to fresh runs/restarts; Off still permits manual
admission. Existing grab/drag/fling controls remain available during the field.

Pause, then choose **Run gravity & resume** to start one gravity event in the same
world. The menu closes, and the next positive simulation step admits it. During an
active event the action is disabled; **Resume** continues the paused phase. Menu
navigation uses the usual keyboard, controller and pointer paths. Benchmark mode
and other scenarios do not expose this action. Restart and launcher transitions
clear requests; a busy refusal never queues an event for later.

Public UI IDs are `launcher.settings.pizza.automatic-events.previous` / `.next`
and `pause.pizza.gravity`. The existing control socket `status` response includes
one bounded `pizza=<JSON>` line while Pizza is hosted. It contains the existing
version-1 observation (`tick`, `event`), `scenario_revision`, `paused`, `ball_count`
and `held_ball_id`. `event` reports idle wait/pending request or active
kind/direction/trigger/elapsed/remaining ticks; benchmarks report `disabled`.
Status is sampled from the latest host update, including paused updates, without
serializing the world or changing the UI revision on every simulation tick.

The short `pizza::pizza_event_controls_settings_and_lifecycle_smoke` public UI
case runs on PRs. The real-time completion/GPU case runs in the full nightly/manual
UI suite. Deterministic host tests cover build-up/hold/fade pause and resize, busy
refusal, input cancellation, restart/benchmark/revision guards and remaining idle
past the maximum automatic gap with the setting Off. Target-size UI tests dispatch
real keyboard/pointer events; optional menu PNGs use
`SPACEWARS_PIZZA_CONTROLS_ARTIFACTS`. [Live target-size captures](screenshots/pizza-controls/validation.json)
record the public UI workflow at 800×480, 1024×768 and 480×800.

## Directional gravity

The extra field builds for 120 ticks (2 seconds), holds for 660 (11 seconds),
then fades for 120 (2 seconds). Both ramps use smoothstep, `t²(3−2t)`, with
zero strength at time 0 and 15 seconds. The existing scheduler counts unpaused
60 Hz updates; the field uses that same phase and selected screen edge.

Full strength is **0.30 world units per physics-second squared**, independent
of ball mass. The longest playfield dimension is one world unit. The existing
Rapier acceleration binding applies force proportional to each body's actual
mass, alongside unchanged mutual attraction. Rapier integrates it over the
existing `dt * WORLD_TIME_SCALE` step (0.9), including solver substeps. Thus a
full-strength 60 Hz update contributes 0.0045 world units per physics-second
of velocity before contacts. There is no additional time-scale multiplier.

Held kinematic balls receive no field force. A released or cancelled ball
responds on its first dynamic update, including during fade-out. Spawned balls
participate in their creation update; fragments created after physics join on
their next update. Affected dynamic bodies are woken. The normal update clears
the field force immediately after physics, including the event's last update.
Completion removes the field and cue while retaining actual positions,
velocities, health, spawning and ordinary mutual gravity.

A small arrow badge sits inside the selected edge, above the balls, with
opacity following the same envelope. Its diameter is 7% of the shorter
playfield dimension (about 34 pixels on an 800×480 display). It uses ordinary
shared circle/polygon primitives and has no input handler or physical collider.

### Calibration and captures

The initial October 18, 2026 local sweep used all four directions, 800×480, 1024×768
and 480×800 viewports, and target populations of 24 and 120 with normal
spawning, damage and fragmentation. Each run warmed up for 300 ticks, admitted
one typed manual event, and continued through 60 ticks after completion.
Actual starting populations ranged from 24 to 500 due to normal cascades.
At six seconds, the mass-weighted position toward the chosen edge ranged
from 0.61 to 0.86 (0 is the opposite edge, 1 is the target edge), compared with
0.42–0.58 before admission. All runs remained finite and continued producing
contacts and new bodies. Captures show the gathering effect and readable cue;
focused gesture tests also verify a fling can travel against full-strength pull.
This calibrates behavior at the default 500-ball budget. The [capacity lab](pizza-capacity.md)
covers functional integrity up to 5,000; #354 and #359 retain device performance
and CPU-headroom qualification.
Representative [native captures and validation records](screenshots/pizza-gravity/validation.json)
are checked in alongside this document, including paired software/GPU images.

The sweep was rerun at `ff21581` after the viewport/CCD corrections. Updated ranges and raw calibration data are linked from the [cascade investigation](pizza-cascade-profile.md); the captures above retain their original source identity.

Reproduce the normal-path sweep and export native software captures:

```sh
RUST_MIN_STACK=16777216 SPACEWARS_PIZZA_EVENT_ARTIFACTS=/tmp/pizza-event-captures \
  cargo test --locked --profile ci -p engine-client --bin engine-client \
  pizza_event_calibration_and_captures -- --ignored --nocapture
```

The existing offscreen GPU geometry probe has four opt-in fixtures,
`pizza-event-left`, `pizza-event-right`, `pizza-event-top` and
`pizza-event-bottom`. Each replays the normal 120-target-ball scene to six
seconds into the event, then captures identical frames through the production
GPU and software renderers. The first seed whose real manual admission chooses
the requested direction is recorded as `pizza_event_fixture` in the log.
The standard sparse/dense/churn fixtures and default probe list are unchanged.

```sh
cargo build --locked --profile ci -p engine-client --bin engine-client \
  --no-default-features --features pi-kiosk-gpu
target/ci/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-device /dev/dri/renderD128 \
  --geometry-cases pizza-event-left,pizza-event-right,pizza-event-top,pizza-event-bottom \
  --geometry-modes triangles --presentation-frames 2 --presentation-repeats 1 \
  --benchmark-width 800 --benchmark-height 480 \
  --geometry-output /tmp/pizza-event-gpu-800
```

Repeat with 1024×768 and 480×800 and a fresh output directory. These are local
offscreen scene captures at target dimensions; they do not deploy to a device
or measure its frame rate. The longer calibration sweep is opt-in, while the
focused field and interaction tests run with the ordinary Pizza test suite.

## Visual verification

Start the default 300-ball dense Rapier workload:

```sh
cargo run --release -p engine-client -- --scenario pizza --benchmark
```

The launcher also exposes the same workload through Pizza's **Benchmark**
button or the `B` shortcut. A label in the rendered world identifies the
backend, workload, population, active bodies, and current contact count.

Use CLI options to inspect another configuration:

```sh
cargo run --release -p engine-client -- \
  --scenario pizza \
  --benchmark \
  --pizza-benchmark-balls 2000 \
  --pizza-benchmark-backend rapier \
  --pizza-benchmark-gravity fast \
  --pizza-benchmark-workload churn
```

The benchmark population is clamped to 10,000. The interactive default remains
300 so individual bodies and collision behavior are still visually legible.

## Headless measurements

Run the same scenario without opening a window:

```sh
cargo run --release -p engine-client -- \
  --scenario pizza \
  --benchmark-headless \
  --benchmark-seconds 30 \
  --benchmark-report pizza-rapier-dense-300.csv \
  --pizza-benchmark-balls 300 \
  --pizza-benchmark-backend rapier \
  --pizza-benchmark-gravity fast \
  --pizza-benchmark-workload dense
```

`--renderer vector` measures scenario rendering and scene conversion.
`--renderer raster --raster-scale N` measures the software raster path.
The output separates the whole scenario step from workload, lifecycle,
gravity, physics, snapshot, and presentation costs. Gravity reports validation,
tree construction, mass aggregation, and traversal time plus source, target,
node, exact-interaction, approximation, and applied-source counts. Rapier runs
additionally report its broad phase, narrow phase, island construction, solver,
and CCD timing availability. Rapier 0.34 does not populate its aggregate CCD
timer, so `avg_rapier_ccd_ms` is an empty CSV field rather than a measured zero.
`PhysicsStepMetrics::ccd_time` returns `None`. Population, awake/sleeping body, candidate-pair,
active-contact, solver-contact, added, and removed counts accompany every
one-second sample.

The runner executes 60 fixed steps per sample as quickly as possible.
`throughput_fps` is therefore throughput, not a claim that the program rendered
for one wall-clock second. `avg_total_ms` is the useful per-frame budget number.

## Workloads

- `sparse` distributes small moving bodies through the whole playfield. It
  primarily exercises integration, mutual gravity, and broad-phase updates.
- `dense` starts overlapping mixed-radius bodies in a central cluster. Mutual
  gravity keeps the population interacting while collision detection and
  contact solving remain visible.
- `churn` uses the dense fixture and, every 120 ticks, removes and replaces 75%
  of the bodies while keeping the configured population constant.

All benchmark fixtures are populated immediately from the scenario seed.
Collision damage and fragment explosions do not control benchmark population:
Classic bodies receive infinite hit points, and churn follows a deterministic
schedule. This keeps lifecycle pressure comparable after Classic and Rapier
motions diverge.

## Backend boundaries

Collision and gravity selections are orthogonal:

- `--pizza-benchmark-backend classic` retains Pizza's all-pairs collision loop.
- `--pizza-benchmark-backend rapier` uses the canonical physics world for body
  motion, containment contacts, broad/narrow phase collision detection, and
  contact solving.
- `--pizza-benchmark-gravity exact` uses the symmetric O(n²) correctness oracle.
- `--pizza-benchmark-gravity full` uses Barnes-Hut with θ=0.5.
- `--pizza-benchmark-gravity fast` uses Barnes-Hut with θ=0.7 and is the
  interactive default.

Both mechanics backends consume the same gravity result. This permits
Rapier+exact versus Rapier+Barnes-Hut comparisons without also changing the
collision implementation.

Rapier benchmark bodies use one dynamic body and one circular collider, four
solver iterations, CCD disabled, and sleeping disabled. These choices keep the
moving/dense comparison explicit. Sleeping, CCD, solver iteration, SIMD, and
parallel variants should be introduced one at a time and recorded with their
Cargo features.

## Benchmark discipline

Run release builds on an otherwise quiet machine. Record:

- repository commit and dirty state;
- CPU/target hardware and operating system;
- Rust and Rapier versions and enabled Cargo features;
- exact command, seed, backend, workload, population, renderer, and duration;
- raw CSV rather than only a summarized FPS number.

Treat visual runs as behavior checks. Use headless stage timings to decide
whether a change improved physics, lifecycle, state projection, or rendering.

## Gravity-only benchmark

The reusable kernel also has a release-mode benchmark that excludes Rapier and
rendering:

```sh
cargo run --release -p engine-gravity \
  --example gravity_benchmark -- \
  --sizes 300,1000,5000,10000 \
  --scenarios jittered,clustered \
  --samples 20 \
  --theta 0.7
```

Cases up to `--oracle-limit` (1,000 by default) are checked against exact
gravity outside the timed samples. `--full` adds 25,000 and 50,000 bodies.

On the development host on 2026-07-27, the θ=0.7 kernel took approximately
5.6 ms for 10,000 jittered bodies and 10.0 ms for 10,000 clustered bodies. In
the full dense Pizza scenario, a stabilized 5,000-ball frame was approximately
3.75 ms gravity, 6.66 ms Rapier, and 5.16 ms vector rendering (about 16 ms
total). At 10,000 balls, gravity was approximately 9.1 ms while Rapier contacts
and solving rose to 34.7 ms and rendering to 10.8 ms. These are local reference
measurements, not portable performance guarantees.
