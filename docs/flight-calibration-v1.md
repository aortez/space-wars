# Open-wing flight calibration v1

Status: C1 measurement slice complete; C1 calibration remains in progress
(2026-10-22). This is exploratory mechanics evidence for the
[flight-controller plan](design/learned-flight-controller-v1.md), not a trained
policy evaluation or a frozen task envelope.

Open the retained [mechanics viewer](../data/training-experiments/learned-flight-calibration-v1-20261022/run/index.html).
Select a maneuver, scrub or play its trace, compare runs in the table, and
inspect the physical hull, origin and center of mass. The optional circle
shows the existing sensor prototype's 300-unit extent at the current position;
it is not a proposed safety boundary.

## Method and retained evidence

The new `FlightCalibration` diagnostic fixture uses the production material
flight scenario, full open-wing collision hull and rear feet, and ordinary
control actions. It releases the normal neutral-input arming gate before the
measurement and sets the initial velocity and heading. No native pilot chooses
the controls. Subsequent motion uses the ordinary scenario step at 60 Hz.

The 59 prescribed maneuvers include braking at 5, 10, 30, 50, 70, 100 and 140
units/s in three headings, zero-gravity and gravity-on variants, inward and
outward motion, a rotating-frame case, held turns, turn release with and without
braking, coasting and thrust. Initial altitude is 150 in the practice fixture.
Braking cases last five seconds; other cases last three. Each physical tick is
compared exactly with a second independently initialized scenario.

Zero-gravity cases explicitly set the practice planet's gravitational mass to
zero. Most cases also disable its spin to isolate mechanics. Gravity-on cases
retain its normal mass; one case uses 0.015 rad/s spin. These are controlled
fixtures, not a survey of generated-match gravity or world geometry.

Velocity thresholds 0.5, 1 and 2 units/s are diagnostic probes. A reported dwell
requires 15 consecutive completed ticks below the threshold; the report records
both first observed entry and the last qualifying tick. This is a discrete
measurement, not proof of a continuous-time interval or accepted task success.
Velocity is measured at the body origin relative to the planet frame. Path
lengths are accumulated in world coordinates. A threshold crossing does not
establish arrival at a destination or lasting station keeping.

Runs terminate and are flagged if contact, damage, vehicle loss/unavailability,
landing assist or wing sweep occurs. All 59 final runs remained within these
free-flight measurement conditions. Existing landing assist is not disabled in
the production game; these particular traces never enter it.

The retained experiment is `learned-flight-calibration-v1-20261022`, under
`data/training-experiments/`. Its source archive was captured before the final
build/run and includes modified and untracked workspace source. The directory
contains the experiment plan, per-case plan, all traces, physics-only start
snapshots, matching executable, source file hashes and a checksummed manifest.
Full scenario replay uses the recorded setup and action schedule; physics-only
snapshots do not contain all scenario state. Registry dependencies and system
libraries are not bundled. This is local storage, not an off-machine backup.

Compact [results](data/flight-calibration-v1-results.json) and an
[archive receipt](data/flight-calibration-v1-archive.json) are kept with the docs.
To regenerate into a fresh directory:

```sh
cargo run -p scenario-spacewars --profile ci --example flight_calibration_probe --locked --offline -- --out target/flight-calibration-new
```

## Geometry

| Measurement | World units |
| --- | ---: |
| Open-wing width at heading zero | 11.00 |
| Height including rear feet | 8.95 |
| Radius enclosing all colliders about the body origin | 6.281 |
| Center-of-mass offset from body origin | approximately (0, -1.222) |

The old radius-4 inspection disk does not enclose the real ship. Native static
overlap tests agree with the exact projected hull/feet extents away from
tangency: a centered upright ship does not fit a 10.95-unit gap and does fit an
11.05-unit gap. At 90 degrees, the 8.95-unit extent can fit a 9-unit gap only
with appropriate lateral placement; centering the body origin in that gap
fails because the footprint is asymmetric about it. These checks do not prove
dynamic traversal, turning room or a suitable operating margin.

## Braking and turning

Zero gravity, stationary frame, full brake, no turning; measurements at the
end of 15 consecutive ticks at body-origin relative speed at most 1 unit/s:

| Initial speed | Elapsed seconds | World-space origin path |
| ---: | ---: | ---: |
| 5 | 0.633 | 1.083 |
| 10 | 0.800 | 2.248 |
| 30 | 1.300 | 12.080 |
| 50 | 1.800 | 31.916 |
| 70 | 2.300 | 61.749 |
| 100 | 3.050 | 125.249 |
| 140 | 4.050 | 244.916 |

Braking does not require turning to oppose the velocity. The production brake
acts against origin velocity relative to the planet frame, with acceleration
capped at 40 units/s² and a low-speed gain of 4/s. The 70-unit/s open-wing cruise
target is a thrust governor, not a hard speed cap; inherited momentum can exceed
it. The 100/140 cases probe that possibility without selecting it as the first
training envelope.

Under practice gravity, inward braking from 70 units/s needed 2.667 seconds and
65.664 units to meet the same diagnostic threshold. Speed remained around
0.729 units/s at five seconds, and never met the 0.5-unit/s dwell. Brake-only
behavior under gravity is not exact station keeping. A learned controller may
need thrust as well; these measurements do not establish its achievable floor.

Full turn from rest reached 1.8 rad/s, changed heading by 90 degrees after
1.017 seconds and by 180 degrees after 1.900 seconds. This rotates the hull;
it does not reverse the existing velocity. After one second of held turn:

| Released-turn action | Additional rotation magnitude | Time to spin at most 0.01 rad/s |
| --- | ---: | ---: |
| Neutral/coast | 61.02° | 1.20 s |
| Brake | 14.61° | 0.30 s |

The turn-from-rest case also exposes the frame distinction: its center of mass
barely translates, but the offset body origin reaches about 2.20 units/s while
rotating. Arrival velocity semantics and observations must account for this.
Final orientation can remain free without treating origin motion as COM motion.

## Implications and next checks

- Use the full hull and feet in the next gallery cases. Preserve distinctions
  around ship-sized gaps and curved corridors; a disk-only clearance label is
  insufficient. Static geometric fit is separate from a maneuver's feasibility.
- A 300-unit map comfortably contains the measured zero-gravity braking path at
  70 units/s, while a 140-unit/s stop consumes about 245 units before hull extent
  or reaction delay. This is evidence for calibrating range against the declared
  speed/gravity envelope, not a guarantee that 300 is enough everywhere.
- Keep angular velocity visible. Turn-release behavior and the COM offset matter
  even when final orientation is unrestricted. Choose arrival reference-point
  semantics explicitly before freezing the task contract.
- The existing near-surface landing assist needs an explicit boundary in the
  learned environment. This measurement slice excludes assisted behavior. Do
  not silently let it qualify learned obstacle navigation or settling.

Next: expand the sensor comparison with actual-hull narrow gaps, curved
corridors, dead ends and required detours. Use those results plus these mechanics
measurements to propose C1's sensor layout, flight envelope and arrival
tolerances. No numerical acceptance criteria are adopted by this report.

## Validation

`cargo test -p scenario-spacewars --profile ci --lib flight --locked --offline`
passed 21 tests, including two new diagnostic tests for reset/replay, read-only
measurements, COM/origin frame consistency, momentum, heading-independent
braking and explicit gravity/frame settings. The final probe checks all 59
replays and 20 native gap configurations. Browser checks passed for selection,
scrubbing, playback, range overlay, comparison-table navigation and desktop/
narrow-screen layouts, with no JavaScript exceptions.

Formatting and whitespace checks passed. Clippy completed with existing
warnings in unrelated code; it reported none in the new files. This is not a
claim that the existing strict lint baseline is clean.
