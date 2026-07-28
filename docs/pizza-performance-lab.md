# Pizza performance lab

Pizza has a deterministic performance mode for comparing collision mechanics
and gravity independently. The visual and headless paths construct the same
fixture and call the same 60 Hz scenario step.

Normal interactive Pizza uses the same canonical Rapier world as the Rapier
benchmark. The shared gravity system computes mutual gravity, while Rapier owns
ball and wall motion, held-ball kinematics, contacts, and contact impulses.
Classic remains available only as a collision benchmark reference.

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
and CCD timers. Population, awake/sleeping body, candidate-pair,
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
