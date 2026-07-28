# Pizza performance lab

Pizza has a deterministic performance mode for comparing the existing exact
simulation with a Rapier-backed collision world. The visual and headless paths
construct the same fixture and call the same 60 Hz scenario step.

Normal interactive Pizza uses the same canonical Rapier world as the Rapier
benchmark. The scenario computes exact mutual gravity and collision damage,
while Rapier owns ball and wall motion, held-ball kinematics, contacts, and
contact impulses. Classic remains available only as a benchmark reference.

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
  --pizza-benchmark-workload dense
```

`--renderer vector` measures scenario rendering and scene conversion.
`--renderer raster --raster-scale N` measures the software raster path.
The output separates the whole scenario step from workload, lifecycle,
physics, snapshot, and presentation costs. Rapier runs additionally report its
broad phase, narrow phase, island construction, solver, and CCD timers.
Population, awake/sleeping body, candidate-pair, active-contact,
solver-contact, added, and removed counts accompany every one-second sample.

The runner executes 60 fixed steps per sample as quickly as possible.
`throughput_fps` is therefore throughput, not a claim that the program rendered
for one wall-clock second. `avg_total_ms` is the useful per-frame budget number.

## Workloads

- `sparse` distributes small moving bodies through the whole playfield with no
  global gravity. It primarily exercises integration and broad-phase updates.
- `dense` starts overlapping mixed-radius bodies in a central cluster and
  applies constant downward gravity inside fixed Rapier walls. It keeps
  collision detection and contact solving visible.
- `churn` uses the dense fixture and, every 120 ticks, removes and replaces 75%
  of the bodies while keeping the configured population constant.

All benchmark fixtures are populated immediately from the scenario seed.
Collision damage and fragment explosions do not control benchmark population:
Classic bodies receive infinite hit points, and churn follows a deterministic
schedule. This keeps lifecycle pressure comparable after Classic and Rapier
motions diverge.

## Backend boundaries

`classic` retains Pizza's exact all-pairs gravity and collision loops.
`rapier` uses the engine's canonical physics world, which owns body motion,
containment contacts, broad/narrow phase collision detection, and contact
solving. The benchmark workload deliberately does not calculate mutual body
gravity; dense and churn use one external gravity vector. A later Barnes-Hut
experiment can feed custom gravity into the same world without confusing its
cost with the initial collision/lifecycle baseline.

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
