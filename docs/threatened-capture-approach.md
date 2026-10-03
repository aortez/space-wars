# Threatened transfer and capture approach

This follows the rejected [hull-parity gate](pursuit-health-gate.md). The known
world-1 P1 powered match declines pursuit at 12521, reaches planet 1 at 13746,
loses its ship at 13921 and loses the match at 14138. This checkpoint diagnoses
that alternative; the health option remains disabled by default. It changes no
controller, weapon, physics constant, sensor cadence or live planning quota.

## Observations before the diagnostic replay

The existing trace shows a route delivered at 13819, 73 ticks after handoff.
Only site 42 is published. The controller starts `seek_cover` with no current
cover entry, then measures all three cover flags as false at 13820. The native
ranker permits an exposed initial site with a finite penalty; qualified cover
is required only after an observed cover rejection arms the response.

At 13920 the site is still about 0.825 radians away in the planet frame. The
descent gate requires alignment within 0.2 radians. Cover replanning from
`seek_cover` also requires alignment and more than 120 ticks in that phase.
The ship is destroyed 102 ticks after selection, so this retry has not fired.
A cannon hit at 13898 takes hull from 49.32 to 1.01; subsequent laser damage
finishes it. This is an exposed maneuver, not a ten-second evidence timeout.

The earlier successful capture also initially selected a site with missing
cover data. Treat that as a retained comparison when interpreting whether
missing initial cover alone explains the later failure.

## Frozen diagnostic plan

Use the existing profiled binary from `639dbde`, SHA-256
`bf1da1296f349a7abb59953f31206e947e28d0b2c23ed49d110c2cae5a8868a5`, and the
immutable `target/pursuit-health/v1/summary.json`. Replay the full original
600-second-limit command for `shared-armed-world1-p1-powered`; it may finish
early under the existing match rules. Keep the losing health option enabled
only to reproduce that recorded trajectory.

Add a dense trace from 5488 through 14138 and seven read-only cover probes:

| Purpose | Observed world ticks |
| --- | --- |
| Earlier capture: full survey, first choice, next cover measurement | 5490, 5539, 5540 |
| Threatened capture: two full surveys, first choice, next cover measurement | 13770, 13800, 13819, 13820 |

The existing paired walking/powered probe measures only the sites in each real
observation, in native batches of at most eight on an isolated world clone.
Missing full surveys remain unavailable; no synthetic scan, future opponent
position or diagnostic route enters the playing controller. Keep both native
single-site work and independent powered batches, and verify agreement. The
older walking-only probe rejects this powered runtime profile; its explicit
unknown is retained rather than interpreted as no available routes.

Require exact physical/mission results, all six non-trace control/evaluator
streams, all previously retained trace records, ordinary sensor work, allocation
ledgers and non-timing planner telemetry. Verify the dense interval is complete
for both players and bind every probe to the same observation and mission in
the trace. Hash the old input files and frozen binary before and after replay.

Audit current cover separately from route availability. Reconstruct solar
clearance using the existing independent checker with its 0.01-unit uncertainty
band; check native fresh choices against that reconstruction. A covered local
round trip with solar clearance still supplies no safe transfer, future cover,
completed landing or survival guarantee. Diagnostic work remains outside live
quotas and is not a performance result.

```sh
python3 tools/probe-threatened-approach.py \
  --prior target/pursuit-health/v1/summary.json \
  --binary target/pursuit-health/surface_mission_soak-639dbde \
  --out target/threatened-approach/v1
```

Use the findings to distinguish an initial-choice qualification gap, missing
route evidence and an unsafe transfer that needs a different successor. Do not
change the hull threshold or infer a stronger bot from an offline route count.
