# Soil material experiments

This is the continuum-model experiment for [#165](https://github.com/aortez/space-wars/issues/165).
It exercises pouring, loss of support, and repeated blasts before choosing a
gameplay representation. The earlier [dirt blast comparisons](dirt-blast-lab.md)
remain available, including their terrain-cell transfer and removal baseline.

## Run and inspect

```sh
cargo run --locked --release -p scenario-terrain-lab --example soil_mpm_lab

# Run just one experiment, or use another deterministic seed.
cargo run --locked --profile ci -p scenario-terrain-lab --example soil_mpm_lab -- \
  --fixture moving-planet --experiment bank --seed 7

# Solver timings and material audits, without playback capture or replay cost.
cargo run --locked --release -p scenario-terrain-lab --example soil_mpm_lab -- \
  --benchmark --output target/soil-mpm-lab/benchmark
```

The default run records 18 cases: three experiments, two ground fixtures, and
three models. Each gets ten seconds at 60 Hz. Output is a **new** directory
under `target/soil-mpm-lab/`, with a standalone `report.html` and `report.json`.
An existing output directory is rejected. The HTML works offline and provides
play/pause, step, slow playback, scrubbing, fixture/model selectors, and surface
or wide cameras. Playback shows every third simulation step, not an interactive
solver. A link such as `report.html#experiment=blasts&fixture=moving-planet&tick=330`
opens a particular recorded moment. `--benchmark` produces JSON only.

`--model`, `--fixture`, `--experiment`, `--seed`, and `--seconds` select repeatable
runs. These three acceptance fixtures use fixed material and action parameters;
the older blast lab still exposes variable charge position, strength, timing,
and pulse duration. Neither replaces the live Terrain Lab scenario.

## Inputs and comparisons

| Experiment | Conserved soil | Action |
| --- | --- | --- |
| Pour | 288 samples, mass 4.5 | Emit 12 samples every 0.1 s from height 4.5; last row at 2.3 s |
| Bank | 494 samples, mass 7.71875 | Let a bank rest on a raised support; remove its left half at 2 s |
| Blasts | 1024 samples, mass 16 | Radial impulses at 2 and 5 s, centered at height 0.8, radius 1.5, peak speed 12 |

The initial sample spacing is 0.125. Each owns reference area/mass 0.015625 and
one of two material IDs. The colors mark provenance; they currently share one
constitutive law. Unemitted pour material stays in an audited reservoir.
Deleting the bank's prescribed support does not delete any of the soil.

MPM and the zero-internal-friction control use the same solver. The control
changes only the internal friction angle from 35° to 0°; ground friction stays
0.6. The third model uses the existing `TerrainGrain` adapter, with one circular
Rapier body per sample, the same initial positions, point velocities, masses,
material IDs, support geometry, and impulses. Its circular contact footprint,
intrinsic rotation, sleeping, and 60 Hz contact solver differ from MPM. This is
a behavior/cost comparison, not a claim of equivalent material laws.

Flat ground has downward gravity 18. The planetary base has radius 8, center
`(0.3 sin(0.2t), -8)`, and angular speed 0.04. Its gravity is inverse-square
outside and linear inside, with acceleration 18 at the surface, matching the
single uniform-sphere field in `engine-gravity`. Seeds inherit surface point
velocity. The curved seed map uses arc length at each sample's radius, giving
unit area Jacobian instead of stretching the material. The raised planetary
support is a convex polygonal cap used by both contact backends.

## Shared kernel

`engine-soil` owns particles, a bounded grid, and the material law. A caller
supplies gravity and signed-distance contacts with surface point velocity.
It has no downward-gravity assumption, scenario clock, renderer, or Rapier
dependency. Terrain Lab owns the fixtures; Spacewars, Clock, and standalone
Scorched Earth can use the same kernel and supply their own environments.

The implementation uses quadratic MLS-MPM transfers and an affine particle
velocity field, following [Hu et al.'s MLS-MPM work](https://github.com/yuanming-hu/taichi_mpm).
Elastic deformation is decomposed with a 2D SVD. Hencky strain supplies pressure
and shear stress; a constant-angle Drucker–Prager projection retains elastic
compression, releases tension, and yields in shear, based on
[Klár et al. 2016, §§7.1–7.2](https://www.cs.ucr.edu/~craigs/papers/2016-sand/paper.pdf)
and its [technical supplement](https://www.cs.ucr.edu/~craigs/papers/2016-sand/tech-doc.pdf).
Young's modulus is 2000, density 1, and Poisson ratio 0.2, in experimental world units.

An additional scalar remembers stress-free expansion. Subsequent compression
first consumes that dilation before developing pressure. This avoids hollow,
inflated piles observed when every tensile projection erased the expansion
history. The logarithmic volume-memory approach is informed by
[Taichi Elements' sand projection](https://github.com/taichi-dev/taichi_elements/blob/master/engine/mpm_solver.py).
Here the remembered trace is distributed isotropically before projection, so
the deviatoric strain remains traceless. This is a documented prototype variant,
not a reproduction or physical validation of either reference solver.

Each substep projects elastic state, transfers mass/momentum/stress to the grid,
adds gravity, applies separating Coulomb contact in the surface frame, and
transfers velocities back to advect particles. Only occupied grid nodes are
cleared/processed. There is no freeze threshold or artificial particle damping.
APIC transfers and yielding can dissipate energy. A blast is an imposed radial
velocity impulse, not a pressure-wave or energy-conserving explosion model.

## Bounds, diagnostics, and verification

The lab uses a 0.25 grid with 257 × 193 nodes, covering x = -32…32 and
y = -24…24. The kernel allows at most 8192 particles and 128 adaptive substeps
per requested frame in this configuration. Its stability step uses elastic
wave speed, particle speed, and affine velocity variation. No particle is
silently deleted or clamped to the grid. An invalid seed batch is rejected
atomically; grid escape, numerical failure, or exhausted substeps return an
explicit error and restore the kernel's entire pre-frame particle state/time.
The command-line runner aborts on errors.

Contacts are prescribed one-way boundaries. Grid projection is followed by
a particle-position safety correction if interpolation crosses a boundary.
The report counts these corrections and their largest distance; many small
corrections on curved moving surfaces are a limitation to track, not invisible
cleanup. These are material points without circular collision radii. Rendering
shows those points, not an inferred solid surface or terrain collision mesh.

Every frame audits material counts/mass, the reservoir, finite motion, positive
elastic determinants, and finite dilation memory. Playback runs also replay
from fresh seeds and compare all 60 Hz states. MPM hashes include position,
velocity, mass, material identity, elastic deformation, affine velocity,
dilation memory, time, and configuration. Rapier comparisons hash world
snapshots. Same-build replay and clone continuation are tested; cross-architecture
bit identity is not promised.

Tests cover strain projection, recompression after dilation, SVD reconstruction,
quadratic transfer moments, momentum conservation, contact friction, atomic
limits, supported settling, an internal-friction control, bank collapse,
repeat impacts, inherited moving-surface velocity, and the curved seed map.

## Recorded results (2026-10-03)

The later [paired Picade benchmark review](shared-loose-terrain.md#picade-benchmark-comparison-2026-10-03)
repeats all three models three times alongside the shared-terrain integration
and existing terrain benchmarks. The results below preserve the original
single-run experiment and its different background load.

The seed-42, ten-second runs retained every sample and its material mass in
all 18 cases. All desktop cases replayed exactly. The Pi benchmark audited each
step but deliberately omitted replay and capture.

The flat poured soil ended at height 1.41 with RMS speed 0.006, compared with
height 0.26 for the zero-internal-friction control and 0.72 for round grains.
After support removal, 4.17 of the bank's 7.72 mass fell below height 1.5,
while the supported side remained. The first and second flat blasts hit 350
and 312 samples respectively; center height changed from 1.79 before the first
shot to 1.47 before the second and 1.36 at the end. The moving-planet bed also
retained a crater, ending at center height 1.08 and RMS speed 0.006. These height
statistics use the 90th percentile within half a world unit of the center;
they are diagnostics, not a reconstructed terrain surface.

Release simulation cost, in milliseconds per 60 Hz frame:

| Fixture / experiment | Samples | Desktop MPM mean | Pi MPM mean / p95 | Pi grains mean |
| --- | ---: | ---: | ---: | ---: |
| flat / pour | 288 | 0.41 | 2.39 / 2.91 | 4.91 |
| flat / bank | 494 | 0.81 | 4.73 / 5.35 | 12.91 |
| flat / blasts | 1024 | 1.56 | 9.60 / 10.79 | 30.83 |
| moving-planet / pour | 288 | 0.45 | 3.02 / 3.62 | 5.80 |
| moving-planet / bank | 494 | 1.18 | 7.95 / 8.63 | 13.75 |
| moving-planet / blasts | 1024 | 1.76 | 11.90 / 12.89 | 34.99 |

The device was `sw-picade.local`, Raspberry Pi 4 Model B Rev 1.4, aarch64,
with the existing kiosk running. It reported 1.5 GHz at the start, temperature
81.3 → 84.2 °C, and one-minute load 1.67 → 2.59. This is a hot, shared-device
measurement, not an isolated benchmark or a complete game frame budget. The
largest MPM case consumed about 12 ms of a 16.67 ms frame **before rendering
or other gameplay**. A bounded soil region looks worth pursuing; whole-planet
simulation is not established.

Timing includes emission, events, contact preparation, adaptive substeps, and
kernel rollback-buffer allocation. It excludes audits, hashes, recording,
replay, and drawing. The MPM cases used up to 14 substeps/frame and 496 active
grid nodes. Curved-boundary safety corrections were frequent (up to about
127,000 sample/substep corrections over ten seconds), but the largest MPM
correction across these cases was approximately 0.0052 world units. That
boundary treatment deserves further convergence testing.

The review artifacts are retained locally in `target/soil-mpm-lab/release-review/`
and `target/soil-mpm-lab/picade-review/`. JSON records seed, settings, all step
timings, workload/correction counts, machine metadata, and executable hashes.
The measured executable SHA-256 values are:

- Desktop: `e2abbc5746a56adb612db3fc92334b3a9ce0d2dc1e53a6befbdc1962ba2563c3`
- Picade: `135d3cb0395e2653422c60689b0c3ed82690495964a2297f7fa3fe65bbeb4459`

The Picade binary was built with Rust 1.89, release/LTO, and static CRT linkage
because this workstation's dynamic GNU cross-toolchain requires a newer glibc
than the device provides:

```sh
RUSTFLAGS='-C target-feature=+crt-static' cargo +1.89.0 build --locked --release \
  --target aarch64-unknown-linux-gnu -p scenario-terrain-lab --example soil_mpm_lab
```

It was copied into a new temporary directory and run with `--benchmark`.
No kiosk binary, installed files, or service settings were changed.

## Remaining work

This is evidence for continuing the soil experiment, not an integration into
Spacewars or either Scorched Earth mode. Terrain-cell-to-MPM transfer,
durability, renderable surface reconstruction, dynamic/two-way Rapier contact,
and conserved deposition into terrain (#51) are still separate work. Cohesion,
moisture, compaction calibration, hardening, impact fracture, and shock
propagation are absent. The grid introduces scale-dependent contact and
resolution effects, and the material parameters are not calibrated dirt.

The next decision should use both the recorded behavior and the Picade frame
cost. A small stable pile does not establish a budget for a whole planet.
