# Dirt blast experiments

This is the first experimental slice of [dirt destruction: blast displacement
and settling, #165](https://github.com/aortez/space-wars/issues/165). It compares
the existing crater-removal behavior with material-preserving release into
coarse rigid pieces. The same experiment runs on flat ground and a translating,
rotating planet, using the shared terrain, Rapier, and gravity code that is
available to Terrain Lab, Spacewars, Clock, and standalone Scorched Earth.

## Run and inspect

From the repository root, run:

```sh
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab
```

The optimized development run creates a new `target/dirt-blast-lab/<timestamp>`
directory containing `report.html` and `report.json`. Open the HTML file in a
browser; it works offline. Compare crater removal and material release side by
side, switch between flat and planetary fixtures, play/pause, step one 60 Hz tick,
scrub, or replay at 0.25×/0.5× speed. Start resets playback. Surface and whole
fixture views make local contacts and planetary motion easier to inspect.

Each report contains a complete recorded run. Controls inspect that recording;
change simulation parameters by running another experiment. The existing
interactive Terrain Lab scenario remains a separate entry point.

```sh
# Compare larger pieces, which tend to stay wedged together near the charge.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- \
  --patch-cells 4 --output target/dirt-blast-lab/coarse-comparison

# Exercise explicit population rejection without deleting excess material.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- \
  --max-fragments 1 --output target/dirt-blast-lab/population-limit

# List the bounded settings, including charge position, size, speed, and timing.
cargo run --locked --profile ci -p scenario-terrain-lab --example dirt_blast_lab -- --help
```

An explicit output directory must be new; existing reports are never overwritten.
Use `--release` in place of `--profile ci` for release measurements. Keep the
report alongside its machine/profile details when comparing performance.
These experiments do not yet establish a Raspberry Pi performance budget.

## Reference model

The default run lasts 10 seconds, with blasts at 1 and 5 seconds at the same
original ground location. That location follows the planet's motion. Terrain
resolution is 0.5 world units per cell; the default charge is 0.5 units below
the original surface, with radius 3 and a maximum radial velocity change of
18 units/second. The material seed is 42. The default patch width is two cells,
friction is 0.35, and the population limit is 128 loose bodies.

The release mode partitions occupied cells inside the quantized circular brush
into regular patches. `Terrain::extract_cells` transfers those cells into
connected fields while preserving their material, remaining durability, local
position, and sampled surface data when present. It prepares all destination
fields before changing the source. Disconnected remnants are also detached.
Shared terrain geometry supplies the visible shapes and collision rectangles.

New pieces inherit the source velocity at their center of mass and its angular
velocity. Existing and new loose pieces receive a radial velocity change that
falls linearly to zero at the blast radius, sampled at each body's center of
mass. This is an imposed velocity kick, not a pressure or energy calculation.
Rapier handles subsequent rigid contacts. A second blast can subdivide and
accelerate already displaced material as well as remaining ground.

Flat ground uses constant downward acceleration of 18 units/second². The moving
planet uses the shared spherical-source gravity solver, with surface acceleration
18 at radius 16 and a bounded interior field. The ground's translation and
rotation are prescribed; it does not recoil from the blast. Gravity is sampled
at each piece's center of mass before the fixed physics step.

Before admitting a blast, the fixture prepares the complete result and counts
its loose bodies. If that count exceeds the configured limit, it rejects the
whole blast and reports the rejection. Material, body IDs, and velocities remain
unchanged. Preparing a rejected blast still costs work, which is included in
its timing. The cap bounds the admitted body population, not a guaranteed frame
time. No distance-based cleanup silently destroys released material.

## Evidence and limitations

The default experiment throws some pieces clear of the surface and lets them
fall back into contact. Much of the released material remains inside the cut.
Larger patches can jam against their neighbors and barely escape; lowering
friction changes that behavior. Keep those comparisons when evaluating the next
model. Choosing small pieces alone does not establish convincing dirt physics.

Pieces remain rigid between blasts. Materials retain their identity and damage,
but both currently have the same density and release ignores material hardness.
The model has no stress propagation, compression, grain rearrangement within a
piece, or impact-driven breakup. Resting fragments remain separate bodies;
conversion into persistent terrain belongs to [deposition,
#51](https://github.com/aortez/space-wars/issues/51). Gameplay integration and
in-application blast controls remain future work.

Every recorded tick audits material counts by material ID, finite motion,
current geometry, collider mass, and body ownership. Removal mode explicitly
accounts for destroyed cells; release mode must retain all original material.
The runner then replays the recorded world-space shots from a fresh fixture and
compares content/motion fingerprints every tick. This checks same-build
repeatability; the diagnostic fingerprint excludes solver caches and is not a
portable save format. Browser positions are rounded only for display.

Reports retain settings, world-space blast events/results, all tick fingerprints,
geometry and motion, fragment/contact counts, rejected blasts, and timing
percentiles. The build section records the executable SHA-256 when readable,
architecture/OS, invocation, and the checkout/status observed at runtime. A
runtime checkout alone does not identify the source used to build a binary.
Audit/replay failures are retained in the report and cause a nonzero exit.

Simulation timing sums blast preparation/commit and the gravity/physics step.
It excludes audit, recording, fingerprinting, replay, export, and browser
rendering. Capture/audit time is reported separately. These numbers describe
this small fixture's simulation cost, not an application's complete frame time.

The relevant automated checks are:

```sh
cargo test --locked --profile ci -p engine-terrain -p engine-rapier -p scenario-terrain-lab
```

Transfer tests cover material/damage conservation, independent disconnected
selections, sampled surfaces, dirty chunks, and atomic errors. Fixture tests
cover ejection and falling, contact/accounting under both gravity fields,
translating/rotating source inheritance, repeat impacts, cloned continuation,
fresh replay, and atomic population rejection.
