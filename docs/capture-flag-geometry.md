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
