# Remaining enemy approach: route topology

The [destination failure experiment](capture-destination-retry.md) preserves a
useful neutral capture, then returns to the sole remaining enemy destination.
That approach still fails. In the retained cover-response trace, both visits
probe sites 0–7 and all eight report disconnected outbound ground routes. Each
leaves 25 candidates unmeasured. With cover response disabled, both visits
repeat exposed site 41 until the native eight-cover-replan limit.

Earlier [exhaustive route diagnostics](capture-cover-alternatives.md) found no
sheltered round trip at the first visit's observed clocks. Repeated probes are
therefore not sufficient evidence that rotating the probe list would enable
this capture. We must distinguish a disconnected base graph from a path cut by
the bot's proposed parked hull, and remeasure the later visit.

## Diagnostic contract

`--probe-cover-topology true` requires the existing fixed-clock cover probe.
It records the native 512-bearing outer-contour graph, its rejected footings,
and each observed site's hull-induced node/edge removals. The native joint
round-trip search supplies outbound/return paths and diagnostics. Every result
must exactly match the separate native eight-site diagnostic batches.

An additional route on the base graph omits the bot's proposed parked ship.
This is a counterfactual used to locate a constraint. Other physical obstacles
remain; it is never admitted to gameplay and cannot certify landing, walking,
jetpack traversal or a completed capture. Graph disconnection describes this
measurement model, not every physically possible route. The probe does not
remove the opponent, terrain, debris or any world object.

All additional queries execute on a cloned world after the ordinary control
decision, with separate timing and sensor profiles. No diagnostic result enters
the bot, sensor demand or shared planner. Omit the option to retain the existing
cover-probe output shape. No gameplay code, policy, default, weight or deadline
changes in this investigation.

## Frozen replay plan

Freeze the diagnostic, analysis, tests and this plan before inspecting new
route measurements. Replay these three existing destination-memory-enabled
runs from `target/destination-retry/v1`, preserving every gameplay argument:

| Source | Cover response | World ticks |
| --- | --- | --- |
| Directed failure, flag -0.8 | On | 3,270; 3,930; 8,580; 9,030 |
| Directed failure, flag -0.8 | Off | 9,270; 9,720 |
| Successful directed control, flag +0.8 | On | 3,510 |

All are seat 0, seed 42. These are seven known observations in three correlated
replays, not fresh strength trials. Keep unavailable observations in the
denominator. Require byte-exact native controller/planner streams, matching
physical/mission/progress reports, unchanged sensor counters and shared work
allocations. Bind each diagnostic observation and mission record to its exact
trace row. Audit graph identities, removed edges/nodes, directed paths, route
lengths, joint endpoints and native route parity. Retain source/raw hashes and
any failed audit instead of overwriting it.

Use base-graph components, footing rejection reasons and hull counterfactuals
to choose the next intervention. A wider survey, new traversal method or altered
combat/capture handoff would require its own declared gameplay experiment.

```sh
python3 tools/probe-capture-topology.py \
  --study target/destination-retry/v1 \
  --binary target/capture-topology/surface_mission_soak-COMMIT \
  --out target/capture-topology/v1
```

The first audit stopped after the second replay because it incorrectly required
the optional `cover_response` report field when that option was disabled. The
failed `v1` output is preserved. The corrected audit checks both field presence
and value; rerun the same plan and frozen binary into `v2`. No gameplay, clocks,
source cases or diagnostic measurements change for this correction.
