# Flight sensor prototype

Status: inspection prototype, 2026-10-22. This implements the sensor-inspection
part of the [flight-controller plan](design/learned-flight-controller-v1.md).
It does not choose the final observation contract or deliver a trained policy.
This report describes the retained C0 run. The current example now generates
the [expanded actual-hull gallery](flight-sensor-navigation-v1.md); use the
archived C0 viewer below when reviewing the original 16-scene measurements.

## Run and inspect

From the repository root, choose a fresh output directory:

```sh
cargo run -p scenario-spacewars --profile ci --example flight_sensor_probe --locked --offline -- --out target/flight-sensor-inspection
```

Open `target/flight-sensor-inspection/index.html` in a browser. The page is
self-contained and needs no server. Select a scene and resolution, switch
between overlap and occupancy channels, enable rays, zoom, and hover over cells.
Use **Play sequence** on the rotation, motion or terrain-edit scenes.

The retained C0 [viewer](../data/training-experiments/learned-flight-sensor-v1-20261022/inspection/index.html)
is now outside `target/`, alongside the reports, snapshots, matching executable
and an archive-time source snapshot. See the
[archive receipt](data/flight-sensor-prototype-v1-archive.json) for hashes,
verification and provenance limits. This is local storage under the existing
ignored `data/` directory, not an off-machine backup.

The output also includes `report.json`, `summary.json`, and one native physics
snapshot per scene. Snapshot hashes, four relevant source-file hashes and the
running executable hash are recorded. These identify the inspection artifacts;
they are not a full source archive. The separate C0 archive adds a source
snapshot, but cannot retroactively prove every build-time input. `target/` is
disposable; preserve future runs separately as well. A compact copy of this run's
[measurements](data/flight-sensor-prototype-v1-results.json) is kept with this
document. The default seed is 42, with five timed observations per scene and
configuration; `--seed` and `--repeats` can override them.

## What is implemented

- `engine_rapier::world::polar::PolarSensor` prepares the polar query shapes once
  and reads current collision geometry after a completed physics step.
- `SurfaceSortieState::flight_surroundings` combines ship state, coarse global
  celestial-body records and the polar observation. It excludes the sensing
  ship and sensor-only colliders. It makes no native pilot decisions.
- The gallery contains 16 controlled scenes and three configurations, plus a
  separate read-only observation of an ordinary live match.

The default candidate has 64 sectors and 11 radial bands with edges at
`0, 4, 8, 12, 18, 27, 40, 60, 90, 135, 200, 300` world units. Each cell has two
angular and two equal-area radial subdivisions. These are experimental values.
The gallery also compares 32 sectors and twice the angular sampling density.

Each cell supplies two deliberately separate signals:

1. **Sampled occupancy:** the fraction of subcell centers inside solid geometry.
   Thin geometry may miss all centers.
2. **Conservative overlap:** the fraction of circumscribed subcell polygons
   intersecting geometry. This catches thin features but can mark a mostly
   empty subcell. It is not a measured occupied-area fraction.

Directional rays retain native first-hit distances along sampled bearings.
They do not guarantee the nearest distance within an angular sector. A separate
native nearest-point query reports the nearest solid within the map range,
including obstacles between rays. Circle outlines are tessellated only for
display; the sensor queries native shapes. Neither map channel is a clearance
permission for a ship of nonzero size.

The controlled scenes cover occluded obstacles, thin walls, two gap widths,
a filled sun and hollow boundary, rotation through the angular seam, physical
structure motion, and real generated terrain before and after a crater edit.
Adding and removing an ordinary wall collider exercises future construction
visibility. This last fixture is a stand-in, not a construction gameplay system.

## First inspection findings

All 48 scene/configuration observations reproduced exactly after native
snapshot restoration and left the source world unchanged. No declared solid
witness or occupied dense-reference sample was missed by the overlap signal.
The dense reference uses 8×8 point samples per cell; this is a sampled audit,
not an exhaustive geometric proof or a test of learned navigation.

- Both thin-wall fixtures were entirely missed by the directional rays, while
  overlap queries and the nearest-point query detected them. Rays alone are
  insufficient for this representation.
- The farther of two obstacles on the same bearing survives in the map.
- At 32 sectors, the passable gap's center cell is still flagged. At 64 sectors,
  that cell is clear, while the too-narrow gap's center cell remains flagged.
  This single fixture is useful evidence for resolution choice, not a general
  passability test. The independent physical check uses a radius-4 disk, not
  the actual open-wing ship hull.
- A real terrain removal changes the observation. Adding a wall changes it
  again; removing the wall restores the crater's cell values exactly.
- Coarse far bands lose substantial free-space detail. In the sun/boundary
  scene, approximately 25.39% of the entire mapped area consists of empty
  reference samples in cells with some overlap (64 sectors). Treating every
  touched cell as fully blocked would discard that space. Increasing angular
  subcell sampling alone does not fix this radial-resolution problem.

Development-machine timings with the optimized `ci` profile:

| Configuration | Cells | Native queries/observation | Range of scene median times |
| --- | ---: | ---: | ---: |
| 32 sectors, 2×2 samples | 352 | 2,881 | 0.042–0.188 ms |
| 64 sectors, 2×2 samples | 704 | 5,761 | 0.081–0.359 ms |
| 64 sectors, 4×2 samples | 704 | 11,521 | 0.161–0.700 ms |

For the default candidate, generated-terrain scenes took approximately
0.26–0.27 ms per observation. Timings exclude sensor construction, dense
reference generation, snapshot checks, outline export and rendering. These
small fixtures and five-repeat measurements are not a full-match throughput
benchmark or a target-hardware latency qualification.

## Validation and remaining work

Validation completed:

```sh
cargo test -p engine-rapier --profile ci --lib --locked --offline
cargo test -p scenario-spacewars --profile ci --lib flight_sensing --locked --offline
cargo clippy -p scenario-spacewars --profile ci --lib --example flight_sensor_probe --locked --offline
```

The physics suite passed 172 tests with one ignored; the live-match adapter test
passed. Six new physics tests cover thin geometry, occlusion, coordinate-frame
invariance, updates, query filters and invalid inputs. The export command also
checks every scene's snapshot parity and sampled coverage. Browser checks
exercise scene/resolution/channel selection, hover, zoom, overlays and sequence
playback, with desktop and narrow-screen layout inspection.

Clippy completed with existing warnings in unrelated code and no warnings in
the new files. A strict `-D warnings` invocation stops on existing warnings in
`engine-core/src/plants/mod.rs`; this change does not claim a clean strict lint
baseline.

Before freezing the network input, calibrate radial resolution and range against
the actual ship footprint, stopping distances and intended task envelope. Add
representative curved corridors and demanding live-world cases. The current
adapter's global records describe celestial bodies; a full capacity/ordering
contract for other moving objects, channel normalization and update cadence
remain to be specified. Choose how to preserve thin features and free-space
detail from these measurements, then connect the inspected representation to
the headless task environment and bounded learning experiment.
