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
