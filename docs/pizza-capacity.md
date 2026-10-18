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

Arguments are `POPULATION LIMIT [STEPS=120] [SEED=7] [WIDTH=800] [HEIGHT=480]`. Steps are bounded to 1–3,600. Population must be 64 through the chosen limit; unsupported narrow viewports fail explicitly. Compare 500/500, 1000/1000, 2500/2500 and 5000/5000 for capacity pressure, and e.g. 500/5000 for growth with headroom. These are different physical workloads, not interchangeable performance samples.

Fixture version 1 starts healthy dynamic bodies on separated grids in three bands. Small radii are 0.002–0.0035, medium 0.005–0.008, and larger 0.012–0.020 world units. These are valid normal ball/fragment sizes. Larger bands counter-move to provoke impacts; health starts at the ordinary radius-based value. Nothing forces deaths, disables damage, shrinks bodies to fit, or selects benchmark mode. Automatic events and ordinary spawning are off so the fixture isolates normal mutual gravity, contacts, damage and fragment lifecycle. No user settings are read or written.

Each update uses the normal 16,666,667 ns step (including its existing world-time scale). JSON includes the version, seed, viewport, actual initial/final/peak counts, cumulative lifecycle totals, contact samples, explosion-bearing updates and a bounded per-update trace. The lab checks actual body/collider counts against metadata after every step. Screenshot export uses the same fixture and production raster renderer:

```sh
SPACEWARS_PIZZA_CAPACITY_ARTIFACTS=/tmp/pizza-capacity-images RUST_MIN_STACK=16777216 \
  cargo +1.89.0 test --locked --profile ci -p engine-client --bin engine-client \
  pizza_capacity_captures -- --ignored --nocapture
```

The example's microsecond timings cover `PizzaScenario::step`, excluding initialization, JSON construction/serialization, validation and rendering. Existing stage timers are reported as components, not an exhaustive or mutually exclusive breakdown; contact damage, for example, is in total step time but lacks its own timer here. The example runs unpaced and collects a JSON value between updates, so it is a functional capacity tool and preliminary headless timing aid, **not** low-overhead production frame-pacing or CPU-headroom qualification. Use identical source/build, budget, seed and dimensions for comparisons. Screenshot runs are outside timing. Full normal-path profiling remains #354.

The live client's existing `status` response also includes `pizza.population`: budget, metadata/body/collider counts and lifetime birth totals. `exploded` and `fragments_created` count actual removals/insertions, including work hidden by a net-zero population. `fragment_capacity_stops` records an early stop after filling the budget; `fragment_parents_skipped` counts remaining whole dead parents whose candidates were not sampled. It deliberately does not claim an exact refused-fragment count: evaluating unsampled random candidates would change subsequent simulation behavior. The separate Classic/Rapier benchmarks retain their 10,000-ball ceiling and do not establish normal explosion performance.

## Local functional evidence

At seed 7, 800×480, 120 updates, all four normal fixtures completed with coherent physical counts:

| Initial / budget | Final live balls | Exploded | Fragments created | Updates with explosions |
| --- | ---: | ---: | ---: | ---: |
| 500 / 500 | 452 | 103 | 55 | 53 |
| 1,000 / 1,000 | 796 | 242 | 38 | 80 |
| 2,500 / 2,500 | 2,269 | 3,378 | 3,147 | 117 |
| 5,000 / 5,000 | 4,426 | 8,434 | 7,860 | 119 |

These are actual bodies, not render-only particles. Desktop timing collected alongside compilation is not a controlled baseline and is not used as a Pi performance claim. The initial 5,000-body snapshots and subsequent cascade snapshots in `docs/screenshots/pizza-capacity` identify their precise population and tick in the accompanying manifest. The menu screenshot comes from the live local client at the target dimensions; the physical scene images are fixture exports through the production raster renderer. No deployment was performed.
