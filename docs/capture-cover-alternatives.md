# Reachable alternatives after cover rejection

The [cooldown experiment](capture-cover-retries.md) changes selected sites but
does not reliably improve capture progress. The next question is whether a
measured alternative supplies both usable cover and a route to the objective.
This is a diagnostic study; no controller, scoring coefficient or default changes.

## Observed questions

In the recorded P1 -0.8 failure at world tick 3,930, 37 of 59 observed sites have
ground and approach cover. The native eight-site objective shortlist contains
three usable exposed round trips, one footing failure and four sheltered sites
with disconnected outbound routes. The other 51 sites have no native route
measurement. This does not establish that every sheltered landing is unreachable.

In the successful +0.8 control, the same retry has four sheltered round trips and
the native controller selects one. In the recorded asteroid case at tick 2,475,
one sheltered round trip is already measured, but its long walk contributes a
large score. These suggest different boundaries: shortlist coverage, actual route
failure and ranking a covered but longer approach.

## Diagnostic contract

`--probe-cover-ticks` takes at most 16 unique **world ticks**;
`--probe-cover-seat` identifies the observed player without changing the trial's
existing observer seat. Absent options preserve the harness output contract.
Each requested tick records the exact immutable observation and post-intent
mission telemetry. Fresh choices use the existing native read-only comparison,
which must reproduce the selected site, direction, solar plan and rejection
counters. Retained sites and unavailable choices remain explicitly unknown.

The route probe measures every site in that observation on a cloned physical
world, in batches of at most eight using the unchanged JointRoundTrip sensor.
It rejects stale/wrong-actor inputs, duplicate sites and partial landing surveys.
The combined diagnostic output never becomes a controller survey. Native route
rows must match their independently batched counterparts exactly. It does not
discover sites absent from the observed survey or supply a future cover guarantee.

Diagnostic physical queries and wall times are outside live planning quotas and
are profiled separately. No diagnostic result enters controls or live demand.
The ordinary 4 graph / 384 query accounting must remain unchanged on replay;
the extra probe work is not claimed to fit that allowance or a Pi frame budget.

The analysis distinguishes ground-and-approach cover from the ranker's stronger
ground/approach/departure preference. Ground-only cover is retained in the raw
record; the native below-40-height exception also requires reaching the approach
alignment and speed gate. A distant site's projected height alone is not proof
that the ship can descend there. A diagnostic round trip remains a route
hypothesis, not a physically completed approach, claim or departure.

## Frozen replay plan

Freeze the probe, tests, analysis and this plan before measuring additional routes.
Use the cooldown study's existing commands and both arms, keeping its seeds,
deadlines, policies, observer seats and native query cadence. Add only the probe
options and ordinary traces where the preceding study did not retain them.

| Recorded case | Tested seat | World ticks, both arms |
| --- | ---: | --- |
| P1 value-destination -0.8 | 0 | 3,270; 3,930; 4,080 |
| P1 value-destination +0.8 control | 0 | 3,510 |
| Recorded world-3 asteroid/P2 | 1 | 2,475; 2,895; 9,383; 10,875 |
| Fresh world-3 asteroid/P1 from cooldown study | 0 | 9,750 |
| Fresh world-3 asteroid/P2 from cooldown study | 1 | 13,365 |

These are ten diagnostic replays and twenty snapshots of known trajectories,
including both favorable and unfavorable cooldown outcomes. They are not fresh
strength trials. Unavailable snapshots and changed capture tasks remain in the
denominator, including the candidate that never reaches the original failed trip.

Require exact physical reports, mission/visit records and all previously retained
controller/evaluator/planner streams. Require sensor stage calls/counters and
combined planner allocations to match, apart from wall time. Bind each successful
native diagnostic to the replay's exact trace observation and mission record.
Audit batch completeness, native route parity, round-trip validity, cover
penalties and native score ordering. Preserve unknowns and all source/raw hashes.

```sh
python3 tools/probe-cover-alternatives.py \
  --study target/cover-retry-cooldown/v2 \
  --binary target/release/examples/surface_mission_soak \
  --out target/cover-alternatives/v1
```

Use the results to choose a separately declared next experiment. Do not retune
the rejected cooldown or claim a landing-policy improvement from diagnostic
route availability alone.
