# Conditional first-scan capture references

This extends the [scan schedule](capture-scan-clock.md) for #141 and supplies
evidence for the destination-policy evaluation in #142. Both belong in the
same PR. The scan clock itself still predicts an opportunity, not a usable
landing site or successful capture.

## Composition

When scan scheduling and arrival-local references are both enabled, the final
charged comparison step publishes `conditional_first_scan_success_v1`. For
each retained usable site it adds the existing transfer reference, the native
handoff-to-scan delay and the existing successful neutral-trip phase medians.
It records each component, the original measurement and geometry epochs, and
whether the conditional total exceeds the remaining match time.

The condition is explicit: the first scan selects that site and an eligible
solar direction, neutral material/claim/form and survey history persist, the
arrival geometry remains applicable through the scan, and local execution is
successful and unexposed. Geometry is not moved to the later scan tick. A
sample that expires before the scan cannot supply a duration. Missing,
negative, incompatible or incomplete evidence remains unknown.

This is an optional diagnostic. Ordinary acquisition and whole-trip fields
remain unknown, and live destination selection does not consume the new sum.
Existing controls, physical queries and graph charges are unchanged. The
composition handles the existing bounded candidate/site lists in the final
comparison operation.

## Frozen replay plan

Commit the implementation, audit and this plan before running. Reuse all eight
`neighbors3` conditions in the hash-bound arrival corpus, with scan/composition
off and on: sixteen recordings, four ordinary and four controlled source
windows from seed 3491156488288037499. Preserve all original commands, model
coefficients, query/graph budgets, source epochs and termination rules.

Verify full controls/observations, native sensors, physical outcomes, original
forecasts and work against the archived corpus. Audit every public report
copy. Compare later scans and capture milestones retrospectively, only when
the frozen reference covers the actual material/site/direction and its
conditions hold. Preserve nonmatching, failed and interrupted cases. These
four source windows are correlated, not a strength test or a new calibration.

```sh
CARGO_TARGET_DIR=target cargo +1.89.0 build --locked --release \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/compare-scan-clocks.py \
  --baseline /home/data/workspace/space-wars2/target/capture-flag-survey/arrival-neighbors-v1 \
  --out target/capture-first-scan-success/replay-v1
```

The baseline path names retained local evidence; the runner verifies its
manifest and every consumed file. Use a new output directory. Results will be
recorded separately after this plan is frozen. Bot-policy promotion requires
the separate directed and held-out evaluation under #142.
