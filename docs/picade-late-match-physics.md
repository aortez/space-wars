# Picade late-match physics investigation

The first retained headless slow case spends most of its native physics time
in continuous collision detection (CCD). Its worst measured 120-tick window
averages 46.237 ms of physics work, including 35.879 ms of CCD. This identifies
a focused performance target without changing recovery policy or physics.

This investigation is separate from #178, which is ready for review with its
candidate still opt-in. Runtime source here is unchanged from `ffee5ba`;
`e82b2a4` adds the final publication notes. The only Rust addition is the
read-only `picade_physics_probe` example.

## Live settings and scope

The Picade is at 1× raster scale, 1024×768. Its saved automatic-match preset is
heavy asteroids every second, Round grains, and 15/4-second combat breaks.
The saved human seat becomes Legacy v9 for automatic play; the second seat
retains Planner v10. `Round` is the original grain mode with strength and
collapse Off, rather than Weak/Round. The recovery candidate is disabled.

Two live samples motivated the investigation:

| Seed | Tick | FPS / UPS | Native physics mean |
| --- | ---: | ---: | ---: |
| `3057324812542282494` | 12,971 | 5.9 / 29.4 | 19.057 ms |
| `1978168328740471875` | 10,736 | 3.6 / 17.8 | 43.893 ms |

The headless probe follows the launcher's initialization, seat order, four-Hz
observation cadence, destination planner and shared work budgets. It uses the
same seed and saved gameplay settings, recording every action and native timer.
It excludes rendering, camera updates and evidence I/O from physics timings.

**These are new retained stress cases, not exact replays of the old live
states.** The original input streams and solver snapshots were not captured.
The first recreation's P2 remains on its mission path near the live sample's
rebuild period; the second recreation is substantially cheaper than its live
sample. The diagnostic uses Rust 1.89 with static host glibc, while the shipping
client uses Yocto Rust 1.94. Compiler/library and host differences have not been
isolated. The exact 43.893 ms live sample remains unpartitioned; CCD dominance
below is measured on the newly retained headless cases.

## Measurements

The kiosk stayed paused while both probes ran sequentially on the Pi. These
are means within each window, not an optimization comparison. Timers are nested:
CCD is part of physics; first-impact search and clamping are parts of CCD.

| Case / window | Physics | CCD | CCD share | Contact narrow phase | Solver |
| --- | ---: | ---: | ---: | ---: | ---: |
| First seed, ticks 12,852–12,971 | 32.888 ms | 23.738 ms | 72.2% | 3.136 ms | 3.889 ms |
| First seed, ticks 9,061–9,180, worst sampled window | 46.237 ms | 35.879 ms | 77.6% | 3.706 ms | 4.165 ms |
| Second seed, ticks 10,617–10,736 | 12.904 ms | 9.729 ms | 75.4% | 1.180 ms | 1.104 ms |

In the worst window, first-impact search averages 18.472 ms and motion clamping
17.326 ms, with three CCD substeps. This does not establish which shape pairs
or tree traversals account for the expensive sweeps. Final contact-pair counts
are not CCD query counts, and do not prove terrain self-collision.

At tick 12,960 of the first case, the world has 253 physics bodies, 128 loose
grains and four fragments. Grain counts remain within the configured capacity.
Both runs pass all conservation, finite-state/cache and speed audits: 14,040
plus 12,000 ticks, with 234 plus 200 audits. This is not evidence of a body leak.
Device temperature samples are retained with the raw logs; every captured CPU
clock sample reads 1.5 GHz. No whole-game FPS improvement is claimed.

The first dynamic probe could not start because the host linker required
`GLIBC_2.43` from the Pi's `libm`. No simulation tick ran in that attempt. The
successful static binary, failed invocation and all source hashes are retained.
The kiosk resumed automatic play afterward, with PID 2429, zero restarts,
unchanged settings bytes and 1× scale. A later read confirms ticks advancing.

## Reproduction and next gate

The [compact receipt](data/picade-late-match-physics-v1.json) records timings,
populations, checksums, exact configuration and compiler limitations. Full input
and cost traces are retained in `spacewars-handoffs/picade-physics-20261011`.
Use a fresh output directory with the frozen static binary on the Picade:

```sh
./probe --seed 3057324812542282494 --ticks 14040 --out retained-seed-new
./probe --seed 1978168328740471875 --ticks 12000 --out current-seed-new
```

Build the diagnostic with locked dependencies, Rust 1.89, the release profile,
default features and target `aarch64-unknown-linux-gnu`; its target Rust flags
are `-C target-feature=+crt-static`. Strict example Clippy and formatting pass.
No gameplay source, bot defaults, solver limits or historical recovery fixtures
change in this branch.

The next bounded experiment should reconstruct the first case through tick
9,060, retain its native physical state, and inspect the fixed inputs for ticks
9,061–9,180. Count the expensive CCD shape pairs and compound-tree queries before
choosing a pruning or reuse change. Compare it against an unchanged control on
the same physical state and require matching motion/contact outcomes, material
audits and later gameplay checks. Existing moving-ground and compound-sweep
regressions remain required. Additional live backend timers are needed to
confirm the same breakdown directly in a slow shipping-client match.
