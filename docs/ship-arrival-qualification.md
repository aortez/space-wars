# Fresh-case arrival qualification

`material_arrival_qualification_v1` evaluates the frozen seed-3 genome from the
[first training experiment](ship-training-experiment.md). The schedule and
pass gate are declared before either controller runs on these cases. There is
no training, mutation, seed reselection, reference retuning, or exclusion of
cases on which the reference fails.

Candidate ID:
`6c0ef53fb27e190243c84903855544a2cb9b97f4f32c92499dd37e541bbc41fb`.
Reference: `arrival_reference_v1`. Both run fresh ordinary material physics
with the same observation/action contract and restrictions. The original
20-second deadline, 2-unit position tolerance, 1-unit/s relative-speed tolerance,
30-tick continuous dwell, and all physical failure gates remain fixed.

## Frozen scope and schedule

There are 96 new cases, in four groups of 24. All are physically distinct from
the 11 development cases, which are evaluated again as a separate regression
group. The four development training cases are not expanded or changed.

The first three groups use altitude 90 and four bearings: 0, π/2, π, and -π/2.
Each combines those bearings with the six profiles below. Distances and target
velocities use the initial radial frame (+x tangential right, +y away from the
planet); angles are radians. Target velocity becomes constant in world space.

| Geometry profile | Target offset | Heading offset |
| --- | --- | ---: |
| p0, near | (0, 8) | 0 |
| p1, far | (0, 35) | 0 |
| p2, left | (-30, 8) | -0.6 |
| p3, right | (30, 8) | 0.6 |
| p4, inward | (0, -12) | 0 |
| p5, reversed | (10, 18) | π |

Geometry cases start with zero radial/lateral velocity and a stationary target.

| Initial-motion profile | Target offset | Heading | Initial lateral / radial speed |
| --- | --- | ---: | --- |
| p0, rising | (18, 12) | 0.25 | 0 / 6 |
| p1, falling | (-18, 12) | -0.25 | 0 / -6 |
| p2, rightward drift | (-18, 12) | 0.25 | 6 / 0 |
| p3, leftward drift | (18, 12) | -0.25 | -6 / 0 |
| p4, diagonal | (18, 12) | 0.75 | -4 / 4 |
| p5, diagonal | (-18, 12) | -0.75 | 4 / -4 |

Initial-motion cases have stationary targets.

| Target-motion profile | Target offset | Heading | Target velocity |
| --- | --- | ---: | --- |
| p0, right | (18, 8) | 0.3 | (3, 0) |
| p1, left | (-18, 8) | -0.3 | (-3, 0) |
| p2, outward | (0, 18) | 0 | (0, 1.5) |
| p3, inward | (0, -8) | 0 | (0, -1.5) |
| p4, diagonal | (15, 10) | 0.5 | (2, 1) |
| p5, diagonal | (-15, 10) | -0.5 | (-2, -1) |

Target-motion cases start with zero radial/lateral velocity.

The combined group uses altitudes 70, 100, and 125, bearings π/4 and -3π/4,
and four profiles:

| Combined profile | Target offset | Heading | Initial lateral / radial speed | Target velocity |
| --- | --- | ---: | --- | --- |
| p0 | (25, 8) | -0.8 | -3 / -3 | (2.5, 0) |
| p1 | (-25, 8) | 0.8 | 3 / 3 | (-2.5, 0) |
| p2 | (0, -8) | π/2 | 3 / 0 | (0, -1) |
| p3 | (0, 25) | -π/2 | 0 / -3 | (0, 2) |

Every case uses the same material ship, open wings, world seed 42, and one
planet fixture. Altitude changes the actual shared gravity and control frame;
it does not prescribe a fake gravity input. This is a finite sampled envelope,
not exhaustive coverage of every combination or a new world/build family.
There are still no obstacles, terrain contacts, landing, recovery, opponents,
swept wings, or multiple controlled seats in this qualification.

## Pass gate and interpretation

The strict gate requires **96/96 fresh arrivals and 11/11 regression arrivals**
from the frozen candidate under unchanged physical criteria. Timeouts count as
failures. Invalid controls and physical failures also count as failures; host
or invalid-fixture errors abort the run rather than inventing fitness.

Report both-arrive, candidate-only, reference-only, and neither-arrive counts
for every group. Reference failures do not remove cases or lower the gate. A
reference-only arrival demonstrates a missed opportunity for the candidate in
that case. A joint failure alone cannot establish that the task is physically
impossible. A candidate-only arrival can expose a reference limitation.

After the first comparison, retain representative traces from each failure
category and inspect distance, relative speed, continuous dwell, controls,
clipping, and physical failure reasons. Any further probes are diagnostics,
reported separately from this frozen score. Once these results inform changes,
the suite becomes development evidence; a revised model needs fresh
qualification. No result automatically assigns the genome to a live bot.

## Reproduce

```sh
cargo +1.89.0 build --offline --locked --profile ci \
  -p engine-agent --example ship_training
mkdir -p target/ship-qualification
target/ci/examples/ship_training qualification-manifest \
  > target/ship-qualification/manifest.json
target/ci/examples/ship_training qualify \
  > target/ship-qualification/report.json
target/ci/examples/ship_training qualification-episode \
  --case qualification_geometry_b0_p0 --policy model \
  > target/ship-qualification/candidate.trace.json
target/ci/examples/ship_training qualification-episode \
  --case qualification_geometry_b0_p0 --policy reference \
  > target/ship-qualification/reference.trace.json
```

`qualify` evaluates the pinned saved model and reference on identical case
specifications. It never invokes evolution. A completed evaluation exits
successfully even when the qualification gate fails; inspect `passed` in its
JSON report. This separates a valid negative result from an execution error.
The original `manifest`, `train`, `resume`, and `evaluate` commands preserve
their experiment and case partitions.

## Results

The frozen candidate **did not pass** the declared gate: it arrived on 85/96
fresh cases and all 11 regressions. The reference arrived on all 107 cases.

| Group | Candidate arrivals | Reference arrivals |
| --- | ---: | ---: |
| Geometry: distance, direction, heading | 24/24 | 24/24 |
| Initial ship motion | 24/24 | 24/24 |
| Target motion | 18/24 | 24/24 |
| Combined altitude, heading, and motion | 19/24 | 24/24 |
| Original development regressions | 11/11 | 11/11 |

Every miss is a reference-only arrival, so each has a demonstrated successful
controller under the same physics and deadline. There were nine candidate
timeouts and two `outside_region` failures. Neither controller clipped an
input or produced invalid controls in any of the 214 evaluated episodes.
No case was dropped or rerun with a relaxed gate.

### Moving-target precision

Six timeouts were in the target-motion group: three of the four right-moving
3-unit/s cases (p0), and three of the four diagonally outward-right cases (p4).
Three more were in combined right-moving cases (p0): both bearings at altitude
100, and the -3π/4 bearing at altitude 125.

Four of those nine cases never entered the 2-unit position tolerance. The
other five entered both tolerances, but their longest continuous dwells were
28, 15, 19, 21, and 18 ticks, below the unchanged 30-tick requirement. This is
insufficient tracking precision or settling within the allotted time, not an
evaluation error where a completed arrival was overlooked. Some approaches
were still converging at the deadline; no longer-deadline success is claimed.

For example, `qualification_target_motion_b0_p4` reached a distance of 1.956
units and held both tolerances for 28 ticks before losing them. Its paired
reference completed the full dwell at tick 873. The successful candidate
contrast `qualification_target_motion_b1_p0` took 990 ticks and also crossed
the tolerances before finally settling. Near-boundary behavior is sensitive
to these starting poses; a few successful moving cases did not establish
reliable transfer to the larger grid.

### Lower-altitude inward arrival

The two region failures were the combined p2 profile at altitude 70, at both
bearings: sideways initial heading, lateral speed 3, and a target moving
inward at 1 unit/s. Both left through the **lower** boundary, at body-origin
altitudes approximately 39.94 and 39.89. Their closest target distances were
3.48 and 3.19 units, so neither had previously achieved arrival. They ended
at ticks 1188 and 1192 with relative speeds 6.43 and 6.68 units/s.

The reference completed those same cases at tick 593. The corresponding
candidate profiles at altitudes 100 and 125 both succeeded at both bearings;
paired traces retained for π/4 show arrivals at ticks 465 and 641. This narrows
the coverage gap to the lower-altitude inward approach among these sampled
profiles. Altitude changes position, gravity, and the control frame together;
this comparison does not isolate a single physical cause or prove that the
network architecture or input representation is insufficient.

### Evidence and checks

The [frozen plan](data/ship-arrival-qualification-v1-plan.json) was saved before
evaluating either controller. The [complete compact results](data/ship-arrival-qualification-v1-results.json)
retain all 107 paired case specifications, outcomes, metrics, clipping counts,
and trace diagnostics. Both controllers' initial physical state and target
were verified equal for every pair, and the report's manifest matched the
pre-run plan exactly.

All 11 misses were replayed with both controllers, plus three successful
contrast cases: 28 full traces. Removing only trace samples gives exactly the
original scored episode records. No model, reference, input contract, physics,
deadline, tolerance, or qualification case changed during the comparison.

Local execution used Rust 1.89.0, Linux x86_64, and the `ci` profile. The 214
scored episodes took approximately 2.67 seconds on this host, excluding source
archiving and later traces. Runtime identity:
`diagnostic_v1/linux/x86_64/a35cfe1aab033ce196607c964875ccd63e593d3886a01bb09c13f89f91199a6a`.
The manifest SHA-256 is
`678cc0d5de92f1964564a22b206688441810ab1aaf12016cb139892e1b874e01`.

The source snapshot, executable, manifest, full report, traces, and analysis
scripts are retained locally in
`target/ship-qualification/qualification-v1-pwlmpukz/`, and archived outside the
build directory at
`data/training-experiments/material-arrival-qualification-v1-20261013.tar.gz`.
The archive has an internal checksum inventory and a sibling SHA-256 file.

All 64 engine-agent tests pass, including new checks for fresh/development
separation, valid fixture paths, counting reference failures without lowering
the gate, and matching the schedule to its saved pre-run plan. Strict AI/runner
lint and formatting checks pass. CI does
not demand that this frozen candidate pass the qualification it demonstrably
failed.

## Recommended next experiment

Keep this genome as the baseline for a small mixed training recipe: preserve
stationary-arrival cases while adding moving-target tracking and lower-altitude
inward arrivals. Retain the existing physical criteria. The evidence supports
trying broader training coverage before increasing network size or changing
the nine-feature interface; it does not prove those choices will suffice.

Define that recipe and budget before running it, use compatible saved weights
explicitly as seeds, and compare stationary retention as well as the new tasks.
These 96 cases are now inspected development evidence and may guide diagnosis;
any revised candidate needs a separately frozen fresh qualification set.
Managed jobs, the Training view, and genome browsing remain independent product
work and can use the already retained successful and unsuccessful artifacts.

This follow-up is now complete in the
[seeded mixed experiment](ship-mixed-training-experiment.md). All three
descendants pass the 107 previous cases and 48 separately frozen new cases,
using the same network architecture and physical arrival criteria. The
historical failed qualification above remains unchanged.
