# Qualified cover response experiment

The [alternative-route investigation](capture-cover-alternatives.md) found three
constraints after native cover rejection: an already measured sheltered route
can lose on score, a useful sheltered route can be omitted by the shortlist,
and some observed approaches have no qualified sheltered route. This experiment
responds to those distinctions without changing initial selections or defaults.

## Opt-in contract

`--cover-response-seats none|0|1|both` defaults to `none` and requires v13 for
an enabled seat. It is separate from the rejected cover-retry cooldown; both
trial arms leave that cooldown disabled. The profile is
`qualified_cover_response_v1`.

Only a witnessed native `replan_for_cover` arms the response. While exposure
persists, selection requires grounded and approach cover. Ground-only cover also
qualifies at the native local descent gate: angular error below 0.2 radians,
tangential speed below 18, and projected height below 40. A distant site's
negative projected height does not establish this exception. Existing required
site, prior rejection, current route, material and solar checks still apply.
The native score orders qualifying candidates without a new coefficient.

A known eligible covered route is used immediately. Otherwise a full ordinary
survey supplies a queue of missing route IDs in native site order. The queue
requests at most eight additional sites, one at a time through the existing
selected-site sensor interface. Requests never populate the retained landing
site. Each result must pass current geometry, cover, solar and objective checks;
no positive or negative route permission is cached. The native route cadence
and per-survey site limit remain unchanged. The exhaustive diagnostic is unused.

The search has a 600-tick (ten-second) deadline from the cover failure, including
malformed evidence and unavailable queries. Material or flag changes discard
pending IDs without restoring the deadline or probe allowance. A later search
after a selected route has its own bounded window; the native 150-second capture
and eight-cover-retry limits remain hard ceilings for the whole attempt.

The first ordinary survey retains native waiting controls, preserving successful
covered retries. While probing missing routes, local altitude holding uses the
existing ground and solar clearance guards. Exposure clearing releases the
restriction and pending requests. A failed bounded search exits through ordinary
mission failure/reconsideration with distinct reasons for exhausted observed
candidates, remaining unmeasured candidates at the probe limit, and a missing
evidence deadline. None claims global or permanent unreachability.

The main planner's shared 4 graph / 384 query allowance is unchanged. Native
synchronous sensing remains separately measured, outside that dispatch quota;
frequency and candidate bounds are not a Pi frame-time guarantee.

## Frozen validation plan

Freeze implementation, tests, this plan and runner before viewing candidate
outcomes. Preserve the binary, commands, source commit and hashes. Keep failed
runs and negative outcomes; do not fit settings to these results.

1. Replay both arms of the five recorded predecessor trajectories in
   `target/cover-alternatives/v1`: directed P1 -0.8 failure, +0.8 successful
   control, recorded asteroid/P2, and the preceding cooldown study's fresh
   world-3 asteroid cases for each seat. Remove only diagnostic probes and
   add the new option. Require exact disabled physical/mission/progress reports,
   controller/planner streams and sensor calls/counters. Require candidate trace
   parity until the first recorded response effect, using native world ticks.
2. Run the existing 16 directed cases in both arms (32 runs): two destination
   worlds, both seats and bearings 0, +0.8, +1.2, -0.8. Retain failed attempts.
3. Run 16 paired finished matches (32 runs) with seeds derived from
   SHA-256 `qualified-cover-response-v1:0` through `:3`, both asteroid settings
   (off and three seconds), and both tested seats. These reuse four correlated
   worlds; they are not 16 independent world samples. Alternate arm order using
   the existing plan. Both arms retain published-flag and current-neutral cost
   experiments against the same v10 opponent.

Audit physical invariants, visit endings, progress, search bounds, native time
and retry limits, option identity and aggregate shared allocations. Report
captures/departures, losses, recoveries, match outcomes, time without progress,
interventions and separate desktop timings. Compare the successful control and
all failures, not only wins. No default promotion or Pi deployment is authorized
by this desktop study.

```sh
python3 tools/validate-cover-response.py \
  --study target/cover-alternatives/v1 \
  --binary target/cover-response/surface_mission_soak-COMMIT \
  --out target/cover-response/v1
```

## Audit correction

The first validator completed all 74 gameplay runs but rejected the final
comparison for `world2-asteroids3-p1`: physical results changed while retained
report snapshots contained no `first_effect_tick`. The failed
`target/cover-response/v1/summary.json` remains intact. Sparse report snapshots
can miss a short response immediately before recovery clears the capture task.
Absence from that sampling is not evidence that the response never ran.

A separate plan in `target/cover-response/activation-replay/plan.json` replays
both arms with the same frozen binary and commands, adding only trace output
and a dense window at loop ticks 21,500–23,500. The new audit requires exact
original physical/mission/progress reports, non-trace controller/planner streams
and sensor calls/counters before admitting the supplementary capture telemetry.
It also requires identical retained trace behavior up to the newly witnessed
first effect. No controller, seed, deadline or score changes accompany this
correction. Counters from other sparse snapshots remain observed lower bounds.

```sh
python3 tools/audit-cover-response.py \
  --study target/cover-response/v1 \
  --replay target/cover-response/activation-replay \
  --out target/cover-response/v2
```

## Results and decision

**Do not promote this response.** It preserves the successful control and can
end blocked attempts earlier, but the fresh matches regress. Defaults remain
unchanged; the opt-in is retained for reproduction.

The frozen gameplay implementation is `757efb5`, with preserved binary
`target/cover-response/surface_mission_soak-757efb5`, SHA-256
`07955d51846d790797ef9a7c24cf1d792da5922d2536f674c74001efa82fd9ae`.
All 74 original runs completed. The corrected audit from `9ca9bda` completes in
`target/cover-response/v2`; it does not rerun or retune the 74 trials. The
[results archive](data/capture-cover-response-v1.json) binds the original failed
validation, corrected audit, all commands/raw hashes and two supplementary
activation replays. A compressed companion retains the two witnessed covered
selection observations from the recorded candidate traces.

| Measure | Predecessor | Cover response |
| --- | ---: | ---: |
| Directed completed capture sorties, 16 runs | 20 | 20 |
| Fresh wins, 16 runs | 8 | 6 |
| Fresh losses | 8 | 10 |
| Fresh completed capture sorties | 45 | 43 |
| Fresh ships lost | 9 | 11 |
| Fresh completed recoveries | 4 | 4 |

Four of 16 directed pairs and three of 16 fresh pairs change physical outcomes.
Fresh time accumulated after 20 seconds without progress increases from 73,767
to 83,317 ticks (1,229.5 to 1,388.6 seconds), while eligible transfer/capture time
falls from 297,299 to 290,102 ticks. Different match endings also change total
observation time; these are paired aggregate outcomes, not independent samples
or a calibrated estimate of win probability.

### Recorded controls and failures

The +0.8 control has the same physical results and exact claim/departure timing:
the existing covered site remains selected at tick 3,510, with no extra probes.
The directed -0.8 failure requests eight omitted covered sites before reporting
that its probe budget ended with other candidates unmeasured. Its first visit
ends at tick 4,171 instead of 4,953, but later returns repeat the blocked attempt.
Neither arm completes a capture there.

In the recorded asteroid/P2 case, the new selector chooses covered site 4 at
tick 2,475 and physically lands at tick 3,675. The actual hatch round trip is
valid at landing, on material revision 5. Subsequent revisions reach 9; fresh
ground surveys then report a disconnected route and the visit is abandoned at
4,187. Later captures happen earlier, but this original attempt still fails,
and the overall match changes from a win to a loss.

The preceding study's fresh P1 regression remains a loss and loses one completed
capture sortie (three to two); P2 retains its win and three sorties, with fewer
ships lost. Those recorded trajectories are regression cases, not part of the
four new strength-test worlds. No capture using a newly probed omitted route is
witnessed in this study. Unit tests establish that evidence-acquisition path;
the gameplay evidence does not establish a benefit from it.

### Verification and limits

All five disabled replays preserve the original physical, mission, progress and
controller/planner streams. Sensor parity covers 236,246 player observations.
Candidate retained traces match until each first recorded response effect.
The supplemental replay independently finds the formerly omitted activation at
tick 22,620 and verifies 3,669 identical retained rows before it. Both replay
arms reproduce their original reports, non-trace streams, sensor calls/counters
and planner allocations exactly. The sparse report snapshot had missed a
response shortly before recovery cleared the capture task; there is no evidence
of an unexplained pre-activation behavior change.

The original 74 runs audit 1,254,074 main dispatch ticks, never exceeding four
graph operations or 161 physics queries in one tick. These are dispatch counts;
native synchronous sensing remains outside that allowance. Desktop timings are
retained separately and do not establish Raspberry Pi performance.

302 AI unit tests, four physical destination tests, 39 harness tests and 599
Python tests pass (944 total). Formatting, strict AI Clippy with `--no-deps`,
and both profiled and normal release harness builds pass. A read-only trace audit
also verifies current cover and usable route evidence for the two witnessed
covered choices. Other sparse per-capture counts remain observed lower bounds.

The next useful investigation is destination reconsideration after a blocked
approach: the bounded search can end an attempt earlier without making the next
objective achievable. Inspect the repeated returns and failure context before
changing another retry limit, score or default.
