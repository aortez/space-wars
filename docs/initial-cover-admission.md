# Initial cover admission and early route requests

The [threatened approach diagnosis](threatened-capture-approach.md) found covered
modeled round trips that had not reached the controller when it selected an
exposed first site. This experiment changes initial admission and request timing.
The option is disabled by default; the hull rule and cover penalty stay unchanged.

## Frozen candidate

`--initial-cover-seats none|0|1|both` enables `initial_qualified_cover_v1` only for
the selected value-planner seat, which must also enable the existing bounded cover
response. Each capture task checks the first airborne choice. Current exposure
uses the native rule: an observed opponent within 300 units with no ground
occlusion. It requires current grounded and approach cover, retaining the native
low-height exception only when aligned within 0.2 radians, with planet-relative
tangential speed below 18 and projected height below 40.

A full current candidate scan can seed a covered-site request before the ordinary
shortlist has published a route. IDs come from native observed order after the
existing site, solar and cover gates. A queued request is only a request: first
selection still needs current cover, material and solar evidence plus a native
positive objective route when a flag is present. A later queued ID was qualified
at the seed observation; it gains no permission from that old cover measurement.
Missing and unsupported route evidence remains unknown.

The existing cover-search guidance, 600-tick deadline and eight-probe cap apply.
Initial arming does not fabricate a witnessed cover rejection. Exposure relief
releases the filter; if exposure returns before a first choice, a new current
scan must seed requests within the original deadline and remaining probe budget.
Material and objective changes also retain those limits. A full scan with no
qualified unknown candidate ends this observed search and falls back through the
existing capture failure handling. It does not prove no future route exists.
Physical support or on-foot entry ends this initial check and preserves the
existing surface handoff. Existing recovery, solar escape and match rules retain
priority. There are no added physics queries, route injections or higher quotas.

Optional bounded telemetry records initial arming, the latest seed/request and
first choice. Event witnesses save the exact consumed observation; the logger
performs no additional sensing or model rollout. Configuration propagates through
capture creation/reset and invalidates incompatible transfer forecasts.

## Frozen comparison plan

Freeze implementation, tests, runner and this plan before reading candidate match
outcomes. Use the fourteen cases in `target/flight-continuation/v1/summary.json`
with the rejected health option disabled. Retain the two cover-response-disabled
cases as exact disabled controls; enable initial cover in the other twelve.
Separately replay three health-enabled cases from
`target/pursuit-health/v1/summary.json`: world 0 P1 walking, world 0 P1 powered,
and world 1 P1 powered. These include both near-full-hull regressions and the
damaged-ship loss with its earlier successful capture.

Run **17 disabled retention replays and 17 comparisons**, with at most two
concurrent processes. Preserve each source command except binary/output paths
and the initial option. Start at match initialization, never at a hand-picked
failure tick. Keep full 180-second directed and 600-second armed horizons, subject
to existing early match termination. Do not tune the candidate after outcomes.

Require exact disabled controls, physical results, seven retained streams,
ordinary sensor work, non-timing planner telemetry and allocation ledgers. For
candidate runs, independently audit source-bound cover/solar/request witnesses,
native positive route delivery, physical landing/exit/claim/boarding/departure,
powered flight continuation and every final loss. Confirm the shared allowance
remains 4 graph operations / 384 queries and route age at most 120 ticks. Keep the
health gate's own audit in its three diagnostic cases. Retain the first physical
control difference and exact normalized state/sensor parity for unarmed runs.
Hash frozen inputs and binary before and after comparison.

```sh
python3 tools/validate-initial-cover.py \
  --prior target/flight-continuation/v1/summary.json \
  --health target/pursuit-health/v1/summary.json \
  --binary target/initial-cover/surface_mission_soak-FROZEN_COMMIT \
  --out target/initial-cover/v1
```

The primary questions are whether live requests deliver covered evidence soon
enough, whether the earlier successful capture survives, and whether complete
physical outcomes improve. Covered endpoints do not certify a safe flight past a
moving opponent. These known, correlated development cases cannot establish a
general win-rate improvement or justify a bot default change.
