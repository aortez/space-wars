# Dirt blast experiments

These are experiments for [dirt destruction: blast displacement
and settling, #165](https://github.com/aortez/space-wars/issues/165). They compare
the existing crater-removal behavior with material-preserving release into
rigid pieces or individual rounded grains. The same experiment runs on flat
ground, a slope, and a translating, rotating planet, using shared terrain,
Rapier, and gravity code available to Terrain Lab, Spacewars, Clock, and
standalone Scorched Earth.

## Run and inspect

From the repository root, run:

```sh
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab
```

The optimized development run creates a new `target/dirt-blast-lab/<timestamp>`
directory containing `report.html` and `report.json`. Open the HTML file in a
browser; it works offline. Select any two models side by side, switch between
flat, sloped, and planetary fixtures, play/pause, step one 60 Hz tick,
scrub, or replay at 0.25×/0.5× speed. Start resets playback. Surface and whole
fixture views make local contacts and planetary motion easier to inspect.

Each report contains a complete recorded run. Controls inspect that recording;
change simulation parameters by running another experiment. The existing
interactive Terrain Lab scenario remains a separate entry point.

```sh
# Compare larger pieces, which tend to stay wedged together near the charge.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- \
  --patch-cells 4 --output target/dirt-blast-lab/coarse-comparison

# Compare a stronger, deeper charge with a different material seed.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- \
  --seed 7 --depth 1.5 --speed 30 --output target/dirt-blast-lab/buried-comparison

# Exercise explicit population rejection without deleting excess material.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- \
  --max-loose-bodies 1 --output target/dirt-blast-lab/population-limit

# List the bounded settings, including charge position, size, speed, and timing.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- --help
```

An explicit output directory must be new; existing reports are never overwritten.
Use `--release` in place of `--profile ci` for release measurements. Keep the
report alongside its machine/profile details when comparing performance.
These experiments do not yet establish a Raspberry Pi performance budget.

## Reference models

| Model | Material release | Blast coupling |
| --- | --- | --- |
| Crater removal | Deleted cells are explicitly counted | Instant radial kick to any detached remnants |
| Rigid pieces | Connected patches, two cells wide by default | Instant radial kick |
| Rounded grains | One grain per released cell | Instant radial kick |
| Grains + pulse | Same grains | Radial kick budget divided across `--pulse-ticks`, six by default |

The default run lasts 10 seconds, with blasts at 1 and 5 seconds at the same
original ground location. That location follows the planet's motion. Terrain
resolution is 0.5 world units per cell; the default charge is 0.5 units below
the original surface, with radius 3 and a maximum radial velocity change of
18 units/second. The material seed is 42. The default patch width is two cells,
friction is 0.35, and the population limit is 128 loose bodies.

The rigid-piece mode partitions occupied cells inside the quantized circular brush
into regular patches. `Terrain::extract_cells` transfers those cells into
connected fields while preserving their material, remaining durability, local
position, and sampled surface data when present. It prepares all destination
fields before changing the source. Disconnected remnants are also detached.
Shared terrain geometry supplies the visible shapes and collision rectangles.

The grain modes use shared `engine_rapier::terrain::TerrainGrain` bodies. Each
owns the source cell's material, remaining durability, mass, and intrinsic
square-cell inertia. Its center starts exactly at that cell's original center.
The circular contact proxy has radius half the cell width; rendering uses the
same circle. Its footprint occupies π/4 of the old square, introducing pore
space. Mass is supplied independently of that footprint. This changes packing
and allows rearrangement, but does not preserve the solid's occupied area or
model a physical compaction law. The adapter has no gravity or planet policy.

New pieces and grains inherit the source velocity at their center of mass and
its angular velocity. Existing and new loose bodies receive a radial velocity
change that falls linearly to zero at the blast radius, sampled at each body's
center of mass. This supplies an imposed velocity kick. Rapier handles
subsequent rigid contacts. A second blast can subdivide and
accelerate rigid pieces, accelerate existing grains, and release more ground.
Existing grains keep their identities; they are not duplicated on repeat hits.

The optional pulse applies its first fraction at the blast and the remaining
fractions over subsequent ticks, sampling each body's current position against
the original world-space charge center. It never re-cuts terrain. Dividing the
speed by the tick count keeps the maximum velocity-change budget per body
bounded by the configured speed for one pulse. Actual delivered impulses and
energy depend on motion and contacts. This is an external force-field
experiment; gas pressure and shock propagation remain future physics work.
Pulse state survives clones and participates in replay fingerprints.

Flat and sloped ground use constant downward acceleration of 18 units/second².
The slope rises 0.25 units per horizontal unit, sampled into terrain cells.
The moving planet uses the shared spherical-source gravity solver, with surface acceleration
18 at radius 16 and a bounded interior field. The ground's translation and
rotation are prescribed; it does not recoil from the blast. Gravity is sampled
at each piece's center of mass before the fixed physics step.

Before admitting a blast, the fixture prepares the complete result and counts
its loose bodies, including both grains and pieces. If that count exceeds the
configured limit, or eight pulses are already active, it rejects the
whole blast and reports the rejection. Material, body IDs, and velocities remain
unchanged, including any existing pulses. Events record the rejection reason.
`--max-fragments` remains an alias for `--max-loose-bodies` for earlier commands.
Preparing a rejected blast still costs work, which is included in
its timing. The cap bounds the admitted body population, not a guaranteed frame
time. No distance-based cleanup silently destroys released material.

## Evidence and limitations

The default experiment throws some material clear and lets it fall back into
contact. Much remains inside the cut. Larger rigid patches can jam against their
neighbors; lowering friction changes that behavior. Rounded grains rearrange
more readily on the slope, with more bodies to simulate. They are an additional
reference model, not a selected final soil solver.

Initial local comparisons on 2026-10-03 produced the following peak counts of
loose cell centers more than 0.5 units above the original surface. Each run uses
two shots and 600 ticks; other settings remain at their defaults.

| Settings | Fixture | Rigid pieces | Grains | Grains + six-tick pulse |
| --- | --- | ---: | ---: | ---: |
| Seed 42, depth 0.5, speed 18 | Flat | 4 | 5 | 5 |
| Same | Slope | 2 | 7 | 6 |
| Same | Moving planet | 1 | 1 | 1 |
| Seed 7, depth 1.5, speed 30 | Flat | 17 | 20 | 17 |
| Same | Slope | 14 | 16 | 15 |
| Same | Moving planet | 17 | 15 | 13 |

All twelve cases in each run conserved their accounted material and matched
recorded-shot replay. The pulse did not improve peak ejection in these runs.
Grains also did not improve every fixture. These are controlled examples, not
physical calibration or broad performance conclusions. Preserve the comparison
models when investigating cohesion, fracture, and pressure coupling next.

Pieces and grains remain rigid between blasts. Materials retain their identity
and damage, but both currently have the same density and release ignores
material hardness.
The models have no stress propagation, cohesion, compression, grain rearrangement
within a piece, rolling resistance, or impact-driven breakup. Grains can keep
rolling or remain awake even when most of the pile is nearly stationary.
Resting material remains separate bodies; conversion into persistent terrain belongs to [deposition,
#51](https://github.com/aortez/space-wars/issues/51). Gameplay integration and
in-application blast controls remain future work.

Every recorded tick audits material counts by material ID, finite motion,
current geometry, collider mass, and body ownership. Removal mode explicitly
accounts for destroyed cells; all release models must retain original material.
The runner then replays the recorded world-space shots from a fresh fixture and
compares content/motion fingerprints every tick. This checks same-build
repeatability; the diagnostic fingerprint includes pending pulses but excludes
solver caches and is not a portable save format. Browser positions are rounded
only for display.

Reports retain settings, world-space blast events/results, all tick fingerprints,
geometry and motion, piece/grain/contact counts, rejected blasts, and timing
percentiles. The build section records the executable SHA-256 when readable,
architecture/OS, invocation, and the checkout/status observed at runtime. A
runtime checkout alone does not identify the source used to build a binary.
Audit/replay failures are retained in the report and cause a nonzero exit.

The motion diagnostics count centers above the original surface and material
that is both supported and slow. Support requires an upward-facing contact path
back to the actual ground body; a touching airborne cluster does not qualify.
Qualifying contacts have separation at most 0.02 units and normal/up dot product
greater than 0.2. Slow means cell-center speed relative to the moving ground
below 0.2 units/second, with relative angular speed times half a cell width below
the same threshold. This is an instantaneous observation, not freezing,
deposition, or proof of permanent settling. It works even when a moving planet
keeps Rapier bodies awake.

Simulation timing sums blast preparation/commit, pending pulse updates, and the
gravity/physics step. It excludes audit, recording, fingerprinting, replay,
export, and browser
rendering. Capture/audit time includes motion diagnostics and is reported
separately. These numbers describe this small fixture's simulation cost, not an
application's complete frame time.

The relevant automated checks are:

```sh
cargo test --locked --profile ci -p engine-terrain -p engine-rapier -p scenario-terrain-lab
```

Transfer tests cover material/damage conservation, independent disconnected
selections, sampled surfaces, dirty chunks, and atomic errors. Fixture tests
cover ejection and falling, contact/accounting under both gravity fields,
translating/rotating source inheritance, repeat impacts, cloned continuation,
fresh replay, and atomic population rejection. Grain tests additionally cover
source cell positions/motion, mass and inertia, active-pulse continuation and
limits, one-tick pulse equivalence, and exclusion of unsupported airborne clusters.
