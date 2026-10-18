# Pizza live-ball budget and capacity lab

Normal Pizza has two separate settings:

- **Desired balls** is the population the ordinary spawner tries to maintain (default 24).
- **Live ball limit** is the shared budget for all live balls, fragments and the held ball (default 500, maximum 5,000). Desired balls cannot exceed this budget. Raising the budget does not raise the desired population.

The launcher offers budgets of 100, 250, 500, 1,000, 2,500 and 5,000. Saved settings accept any integer from 1 through 5,000. Older settings without this field retain the former 500-ball limit. Lowering it clamps the desired population for the next run. Spawning stops at the budget, pointer creation retains the existing replace-one behavior, and explosions admit fragments in their original order until capacity fills.

**5,000 is an experimental ceiling, not a recommended sustained Pi workload.** A small desired population alone does not limit cascades. The default budget is conservative relative to the old behavior; it is not a CPU-utilization guarantee. [#354](https://github.com/aortez/space-wars/issues/354) measures useful populations, CPU headroom, frame pacing and burst/recovery behavior on the supported Pis before selecting optimizations. Ordinary use should avoid continuously saturating one core.

## Reproduce a normal physical cascade

```sh
cargo +1.89.0 run --locked --profile ci -p scenario-pizza --example capacity_lab -- 5000 5000 120 7 800 480 > /tmp/pizza-capacity.json
```

Arguments are `POPULATION LIMIT [STEPS=120] [SEED=7] [WIDTH=800] [HEIGHT=480] [FIELD=off] [KIND=mixed]`. `FIELD=on` requests the normal directional-gravity event at tick 120; `KIND=ordinary` uses normal desired=24 spawning instead of the mixed fixture. Steps are bounded to 1–3,600. Population must be 64 through the chosen limit; unsupported narrow viewports fail explicitly. Compare 500/500, 1000/1000, 2500/2500 and 5000/5000 for capacity pressure, and e.g. 500/5000 for growth with headroom. These are different physical workloads, not interchangeable performance samples.

Fixture version 1 starts healthy dynamic bodies on separated grids in three bands. Small radii are 0.002–0.0035, medium 0.005–0.008, and larger 0.012–0.020 world units. These are valid normal ball/fragment sizes. Larger bands counter-move to provoke impacts; health starts at the ordinary radius-based value. Nothing forces deaths, disables damage, shrinks bodies to fit, or selects benchmark mode. Automatic events and ordinary spawning are off so the fixture isolates normal mutual gravity, contacts, damage and fragment lifecycle. No user settings are read or written.

Each update uses the normal 16,666,667 ns step (including its existing world-time scale). JSON includes the version, seed, viewport, actual initial/final/peak counts, cumulative lifecycle totals, contact samples, explosion-bearing updates and a bounded per-update trace. The lab checks actual body/collider counts against metadata after every step. Screenshot export uses the same fixture and production raster renderer:

```sh
SPACEWARS_PIZZA_CAPACITY_ARTIFACTS=/tmp/pizza-capacity-images RUST_MIN_STACK=16777216 \
  cargo +1.89.0 test --locked --profile ci -p engine-client --bin engine-client \
  pizza_capacity_captures -- --ignored --nocapture
```

The example stores a bounded vector of typed samples and serializes JSON only after all updates. Timings are microseconds: `step_us` covers the positive normal update after actions, `spawning_us` covers the held ball and spawner, `gravity_us` includes the optional field, `physics_total_us` includes canonical contact collection, `physics_us` is the nested Rapier solve, and `snapshot_us`, `damage_us` and `lifecycle_us` cover metadata projection, contact damage and removal/fragment creation. Broad phase, narrow phase and solver times are nested in physics. Do not add nested timers. Clearing the field and event bookkeeping are in the small remaining total. The older baseline lacks `physics_total_us`; its `physics_us` has the same narrower meaning.

`capture_us` measures the diagnostic scan after the simulation timer: size bands, fragment ancestry and a state fingerprint. Ancestry is diagnostic metadata only; it never influences damage, fragmentation or RNG. `exploded_by_generation` counts original parents, first-generation fragments, second-generation fragments and generation 3+; size bands use radius <0.005, <0.01 and >=0.01. These are actual deaths, including repeated generations hidden by an unchanged live count. The fingerprint includes ordered IDs, positions, velocities, health, radius, color, generation and held/moving state. Compare it only within the same target/toolchain/workload; it is not a cross-platform floating-point guarantee.

The runner is unpaced and excludes rendering. Headless throughput does not establish production frame pacing or CPU headroom. Use the [live profiling procedure and device results](pizza-cascade-profile.md) for those measurements.

The live client's existing `status` response also includes `pizza.population`: budget, metadata/body/collider counts and lifetime birth totals. `exploded` and `fragments_created` count actual removals/insertions, including work hidden by a net-zero population. `fragment_capacity_stops` records an early stop after filling the budget; `fragment_parents_skipped` counts remaining whole dead parents whose candidates were not sampled. It deliberately does not claim an exact refused-fragment count: evaluating unsampled random candidates would change subsequent simulation behavior. The separate Classic/Rapier benchmarks retain their 10,000-ball ceiling and do not establish normal explosion performance.

## Local functional evidence

At source `ff21581`, Rust 1.89 release, seed 7, 800×480, 120 updates, all four normal fixtures completed with coherent physical counts:

| Initial / budget | Final live balls | Exploded | Fragments created | Updates with explosions |
| --- | ---: | ---: | ---: | ---: |
| 500 / 500 | 434 | 90 | 24 | 46 |
| 1,000 / 1,000 | 805 | 242 | 47 | 82 |
| 2,500 / 2,500 | 1,685 | 1,135 | 320 | 113 |
| 5,000 / 5,000 | 3,263 | 3,196 | 1,459 | 119 |

These are actual bodies, not render-only particles. Desktop timing collected alongside compilation is not a controlled baseline and is not used as a Pi performance claim. The initial 5,000-body snapshots and subsequent cascade snapshots in `docs/screenshots/pizza-capacity` identify their precise population and tick in the accompanying manifest. The menu screenshot comes from the live local client at the target dimensions; the physical scene images are fixture exports through the production raster renderer. The refreshed captures include the viewport and continuous collision detection corrections. Temporary Pi 4 GPU runs and their limitations are recorded in the [device investigation](pizza-cascade-profile.md).
