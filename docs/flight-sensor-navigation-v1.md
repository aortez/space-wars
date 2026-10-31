# Actual-hull sensor comparison v1

Status: C1 geometry inspection complete; the task and observation contracts are
still pending (2026-10-22). This extends the
[original sensor prototype](flight-sensor-prototype.md) using the
[measured open-wing hull](flight-calibration-v1.md).

Open the [retained interactive gallery](../data/training-experiments/learned-flight-sensor-navigation-v1-20261022/run/index.html).
The default scene is an open dead end requiring a detour. New ship scenes offer
a movable ghost hull and a paired-scene feature comparison. Moving the ghost
does not move the sensor: the map remains the original observation while native
queries check the hull along an inspection path.

## Scope and method

The gallery has 41 scenes and five sensor configurations:

- The 16 original fixtures remain, including thin walls, real terrain edits,
  moving geometry and the hollow world boundary. Their radius-4 inspection
  disk remains explicitly labelled; it is historical geometry evidence.
- Twenty ship-gap scenes combine distances 12, 20, 40, 90 and 170; aligned and
  approximately 1.41-degree-offset headings; and gap widths 10.8 and 11.2. The
  actual open-wing hull is 11 units wide. These very tight gaps stress encoding
  resolution; they do not establish the intended training clearance margin.
- Two curved corridors compare a wide passage with a central bottleneck.
- An open and a sealed dead end compare a geometric escape route with an
  enclosed start. The open route's first leg moves away from the destination.
  The sealed case is an unreachable diagnostic, not a valid training episode.
- A wall blocks the direct destination path while allowing an inspection path
  around its end.

The ship probe copies the production hull and both rear-foot collider shapes.
Diagnostic barriers and probe shapes use `ALL` collision groups. Every path
segment uses native shape sweeps at a fixed heading, with separate endpoint
overlap checks. The viewer also samples hull poses at at most one-unit spacing.
Continuous translation checks, sampled poses and map values are reported
separately. A clear fixed-heading translation path is geometric evidence; it
does not prove that production controls can dynamically fly it, rotate safely
along it or settle at its endpoint. No inspection path or physical-clearance
answer is supplied to a policy.

All configurations cover 300 units. The new 23-band layouts use edges
`0, 2, 4, 6, 8, 11, 14, 18, 23, 29, 36, 44, 54, 66, 80, 96, 114, 134, 156, 180, 206, 234, 266, 300`.
The original three layouts retain their original band edges.

This run uses a common 4×4 point reference per cell so the largest configuration
stays within the prototype's query-shape budget. C0 used 8×8; compare the new
configurations within this run, not their sampled area figures directly against
C0. Neither reference is exhaustive geometric truth. Curved polyline walls have
zero area, so point occupancy alone is especially uninformative there; declared
solid witnesses and native overlap checks supply separate evidence.

## Findings

Finer radial bands reduce free-space loss at similar query cost to doubling
angular subcell sampling. The following metric counts clear hull poses whose
**center** lies in a cell containing some obstacle overlap:

| Configuration | Clear curved path | Clear wall-detour path | Queries/observation |
| --- | ---: | ---: | ---: |
| 32 sectors, 11 bands, 2×2 | 50.52% | 21.89% | 2,881 |
| 64 sectors, 11 bands, 2×2 | 35.05% | 11.83% | 5,761 |
| 64 sectors, 11 bands, 4×2 | 35.05% | 11.83% | 11,521 |
| 64 sectors, 23 bands, 2×2 | 14.43% | 0.00% | 11,905 |
| 128 sectors, 23 bands, 2×2 | 9.28% | 0.00% | 23,809 |

This measures information discarded by treating an entire touched cell as
blocked. It is not a network failure rate, exact obstacle area, or a complete
clearance classification: fractional overlap values still carry information.
In the sun/boundary fixture, empty reference area inside touched cells drops
from 25.38% of the full mapped area with the original 64-sector layout to 9.34%
with its 23-band counterpart.

Tight-gap pairs expose quantization limits. Both map channels are identical for
six of the ten gap pairs in each 64-sector configuration and the 128-sector
configuration (seven for 32 sectors). More cells do not uniformly preserve
every distinction because the sample locations and angular alignment change.
For example, the refined 64-sector map arrays match for the aligned gap at
distance 12 even though its rays differ; the original 64-sector occupancy
arrays differ there.

At distance 170, all tested configurations produce identical occupancy,
overlap and ray-distance arrays for the 10.8/11.2 gap pair. **The complete
observations are not identical:** the nearest-distance difference is about
0.00655 units and the nearest-point positions differ by about 0.2 units. At
distance 12, six ray readings differ in the 64-sector configurations and twelve
in the higher angular-sampling configurations. These controlled pairs show why
precise readings and approaching observations matter; they neither prove nor
disprove that a particular network can learn the distinction.

All configurations distinguish the open/sealed dead ends and the wide/narrow
curved corridors through overlap cells. The cell map therefore retains useful
geometry beyond the nearest surface, including behind the observer. Selecting
a route and detecting unreachable tasks remain separate work.

Development-host `ci` profile sensing times, five repeats per scene:

| Configuration | Cells | Range of scene median times |
| --- | ---: | ---: |
| 32 sectors, 11 bands, 2×2 | 352 | 0.041–0.187 ms |
| 64 sectors, 11 bands, 2×2 | 704 | 0.081–0.362 ms |
| 64 sectors, 11 bands, 4×2 | 704 | 0.160–0.694 ms |
| 64 sectors, 23 bands, 2×2 | 1,472 | 0.167–0.668 ms |
| 128 sectors, 23 bands, 2×2 | 2,944 | 0.332–1.289 ms |

Timings exclude shape preparation, reference generation, native route checks,
snapshot verification and rendering. This is not target-hardware qualification
or full training throughput.

## Proposed next contract

Use **64 sectors with 23 radial bands and 2×2 subcells** as the working candidate
for the first policy's geometry map: it substantially reduces the measured
free-space loss at roughly twice the original sensing cost. Keep occupancy and
overlap separate and retain precise distance/nearest-point readings. The
128-sector version remains a measured comparison candidate, rather than a
necessary default. This is a proposal, not a frozen acceptance decision.

Do not interpret a clear center cell as permission for the hull to pass, or a
touched cell as wholly impassable. The final contract must specify precise
channels, coordinate frames, normalization, global object records and decision
cadence. Specify task clearance margins alongside the measured braking envelope;
the stress gaps here are not recommended task margins.

Next is a concrete C1 contract proposal covering the operating envelope,
observation/action encoding, arrival reference point/tolerances/dwell and the
existing landing-assist boundary. These choices should use this report and the
mechanics measurements. Broader simulation learning starts after that contract
and the headless environment checks, not from accumulating more handpicked
training cases from this gallery.

## Validation and provenance

All 205 scene/configuration observations reproduced exactly after native physics
snapshot restoration, left source worlds unchanged, and missed no declared
solid witness or occupied reference sample. Each configuration checked 451
declared solid witnesses across the scene set. All 25 ship fixtures met their
declared start, direct-translation and inspection-path checks. The two example
tests include an obstruction between clear endpoint poses, verifying that the
path check uses a sweep rather than just endpoint overlap.

```sh
cargo test -p scenario-spacewars --profile ci --example flight_sensor_probe --locked --offline
cargo clippy -p scenario-spacewars --profile ci --example flight_sensor_probe --locked --offline
```

Both new tests passed. Clippy completed with existing unrelated warnings and no
warnings in the changed example files. Browser checks passed for resolution and
channel selection, ghost movement, path visibility, paired comparisons, hover,
zoom, original scene playback, and desktop/narrow-screen layouts. Formatting
and whitespace checks passed.

The retained experiment is `learned-flight-sensor-navigation-v1-20261022`.
Source was captured before the final run, including uncommitted and untracked
workspace files. Plans, full reports, the viewer, snapshots, matching executable,
source hashes and validation artifacts are retained under `data/training-experiments/`.
The [compact results](data/flight-sensor-navigation-v1-results.json) omit raw
route samples, which remain in the archived full report. The
[archive receipt](data/flight-sensor-navigation-v1-archive.json) identifies the
retained files and verification. Earlier C0 and mechanics archives are unchanged.
This local archive is outside `target/` but is not an off-machine backup.

The current example generates this expanded gallery. Use a fresh output path:

```sh
cargo run -p scenario-spacewars --profile ci --example flight_sensor_probe --locked --offline -- --out target/flight-sensor-navigation-new
```
