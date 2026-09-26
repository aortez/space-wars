# Explaining rejected flag surveys

The [flag-survey study](capture-flag-survey.md) produced two completed walking
results in generated worlds that failed publication geometry validation. That
check covers an entire planet plus 80 units. Its rejection did not identify the
changed objects or distinguish nearby query changes from unrelated motion.

This extension keeps the circular acceptance gate and planner allocation
unchanged. It adds diagnostics after a geometry rejection. No new evidence
enters the value evaluator or controls, and no rejection becomes acceptance.

## Diagnostic boundary

`source_envelope_overlaps_v1` records source/current collider IDs, relative
poses, bounds, filters, sensor/enabled state and the collider-extent motion
bound. It uses the same change predicate as the acceptance gate and checks both
poses in their respective planet frames. Shape replacement remains removal plus
addition, even when the semantic ID is reused. Shape intersection errors count
as potential overlap and have their own counters.

Eight conservative source-frame envelopes describe landing material probes,
landing hull placements, boarding/exit probes, the three sampled climb heights,
the entire walking patch, and the separate nearby-vehicle rule's neighborhood.
The boarding envelope includes both entrances, lateral floor search, drift,
tilt and both capsule orientations. The walking patch includes full rays to the
center, failed nodes and unused edges. It is **not the selected walking path**.
ALL collision groups deliberately make the envelopes broad. Overlap means
potential relevance, not a blocked query, an impassable path or a causal failure.

The vehicle envelope does not evaluate the body-center proximity predicate.
Climb samples do not constitute a swept flight. Hypothetical self-hull tests
and historical opponent-cover rays are not certified by this diagnostic; cover
rays may extend outside the circular region. Zero envelope overlaps cannot
authorize a trip or establish combat safety.

Each scan permits at most 8,192 source and current colliders, eight envelopes
and eight retained change records. It scans past the record cap to retain total
region and per-envelope counts, with an explicit omitted count. Collider-kind
summaries describe retained records only. Invalid frame/bounds or capacity
produce an unavailable report, not a valid empty explanation. Dirty material
queries are separately flagged; their rejection can coexist with an accepted
circular gate and zero reported geometry changes.

The original acceptance counters are a first-rejection prefix, preserved as
`acceptance_prefix`. Full diagnostic counts and synchronous time are separate:
`diagnostic_setup_ms` covers bounded envelope construction;
`diagnostic_ms` and `diagnostic_area_tests` cover the additional publication
scan. They are outside graph/query fuel, like the existing snapshot validation,
and included in headless dispatch timing. This is not a whole-bot CPU guarantee.
There is no extra physics step or replanning of the trip.

## Predeclared engineering replay

`tools/investigate-flag-geometry.py` replays all five enabled conditions from
the completed optimized study: the v10/v13 regression and both v13/v13 generated
worlds with quiet/three-second asteroid pressure. These are the same worlds,
not independent samples or a strength trial. The complete plan is written
before execution; existing output directories are refused.

Every run must finish under the same ten-minute deadline and retain identical
full controller traces, evaluator bytes, mission telemetry, physical outcomes,
survey samples (excluding the new geometry field), old non-timing survey
telemetry and every per-tick allocation/charge. New diagnostics must reconcile
with raw samples and their own total counters, including truncation and
unavailable reports. No geometry tolerance or timing constant is tuned.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/investigate-flag-geometry.py \
  --reference target/capture-flag-survey/optimized \
  --out target/capture-flag-survey/geometry
```

The reference is the complete `a4bfee1` study retained by the previous runner.
Its commands, seeds and hashes are committed in
[capture-flag-survey-v1.json](data/capture-flag-survey-v1.json). Recreate its raw
logs with that source if local artifacts are absent. Use a new output directory
for additional investigation; never reuse these worlds as fresh strength data.

## Results at `7a0bdf6`

All five runs finished with healthy physics, totaling 30.81 simulated minutes.
All five comparisons retained exact controller/evaluator hashes, physical and
mission outcomes, survey samples apart from the new diagnostic field, old
non-timing telemetry and allocation logs. The
[comparison record](data/capture-flag-geometry-v1.json) retains commands, hashes,
the complete diagnostic samples and counter reconciliation.
An independent raw-data audit checked all 221,826 per-seat controller rows,
sample/cancellation accounting and reference hashes. Total planner work remains
2,199 graph operations and 3,444 physical queries; maximum combined tick use is
four graph operations and 127 queries, within the shared 4/384 allowance.

Both generated-world rejections have complete diagnostics with no unsupported
shape tests or dirty material queries. Both belong to v13 in player 1:

| Recorded condition | Source → completed tick | Changed colliders scanned / overlapping circle | Retained / omitted detail | Overlaps with any of the eight envelopes |
| --- | --- | ---: | ---: | ---: |
| World 0, three-second asteroids, planet 0 / bearing 55 | 5193 → 5950 | 123 / 21 | 8 / 13 | 0 |
| World 1, quiet, planet 1 / bearing 6 | 3471 → 3814 | 75 / 1 | 1 / 0 | 0 |

The asteroid case retains the sun collider and seven parts of another planet.
The thirteen omitted collider details have no inferred type; they **are**
included in the zero-overlap totals. The quiet case retains debris entity
100022, moving from `(55.43, 80.84)` to `(180.95, 61.44)` in the target planet's
frame, outside all eight envelopes. The first circular gate scanned only two
changed colliders before rejecting; the second scanned 72. Those prefix counts
must not be mistaken for the diagnostic totals above.

The measured outward/return distances are 0/0 units in the asteroid case and
0.379/0 units in the quiet case. These are flag-adjacent landing/boarding cases,
not evidence for long walking trips. The earlier regression's two positives
remain unchanged on the v10 seat, and generated v13 positives remain zero.

The extra scans performed 31 and six actual shape intersection tests, taking
0.0247 ms and 0.00656 ms on this desktop. Envelope construction totaled 0.00874 ms
across the five runs. These sparse, instrumented desktop observations do not
establish a Pi FPS improvement or a worst-case cost bound.

The evidence supports a more local spatial dependency check as the next step.
It does not support loosening the collider-motion tolerance: the recorded
changes are real motion elsewhere in the circular region. Before promoting any
previously rejected result, prove coverage of the queries supporting its
positive route, both hatch access paths, hull/landing and sampled climb checks,
and separately revalidate non-query predicates and relevant actor state.
Keep the same source age, shape-replacement and long-rotation protections, add
physical blockers in those dependencies, and retain unknown outcomes when
coverage is incomplete. These diagnostic envelopes themselves remain unsuitable
as an acceptance certificate.

## Validation and device

Local validation passed 99 physics tests, 476 scenario tests, 155 AI tests,
342 Python analysis tests, the three-minute native parity test, formatting and
strict AI Clippy. Strict physics Clippy encountered the pre-existing
`collapsible_else_if` warning in `spaceling.rs`; it passed with only that lint
allowed. No unrelated movement code was changed. Independent review found no
correctness or allocation issue in the diagnostic path.

Runtime `7a0bdf6` was deployed to `sw-picade.local`. Executable hashes were
verified, the kiosk is active without automatic restarts, and the saved P1 v10 /
P2 v13 automatic matchup remains selected. Live status exposes the new separate
diagnostic counters. There is no new bot version or launcher setting.
