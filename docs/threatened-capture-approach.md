# Threatened transfer and capture approach

This follows the rejected [hull-parity gate](pursuit-health-gate.md). The known
world-1 P1 powered match declines pursuit at 12521, reaches planet 1 at 13746,
loses its ship at 13921 and loses the match at 14138. This checkpoint diagnoses
that alternative; the health option remains disabled by default. It changes no
controller, weapon, physics constant, sensor cadence or live planning quota.

**Finding:** covered round trips exist in the diagnostic model, but none is in
the published route evidence when the bot commits to the exposed site. The
existing cover penalty would prefer a covered alternative if that evidence
were available. The next test should address early covered-route requests and
initial commitment, without changing the hull threshold or penalty weight.

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

## Replay retention

The runner, six tests and original plan were frozen in `0f81dd3`. The existing
binary reproduces the match exactly with the diagnostics enabled:

- All six non-trace streams retain their hashes, including every capture/control
  evidence row. Physical outcomes, mission histories and visit records match.
- All 3,208 previously recorded trace rows match. The dense interval contains
  17,300 rows covering every update for both players.
- All 28,276 ordinary sensor observations retain their work counters. The full
  allocation ledger and non-timing planner telemetry match.
- The ship is still lost at 13921 and the pilot still dies at 14138. Completed
  sorties remain `[2, 3]`; these are diagnostic measurements of the same loss.

All seven probes bind to exact replay observations and mission records. Three
have full surveys and complete paired route measurements; four selected-site
snapshots explicitly report that a full route probe is unavailable. Their
missing alternative-route counts remain unknown rather than zero.

## Covered routes were not published at commitment

The table counts observations at each snapshot, not distinct missions. Native
walking and powered jobs agree with independent powered batches for every site.

| Snapshot | Observed sites | Ground + approach cover | Covered modeled round trips | Also clear of the sun in the independent reconstruction |
| ---: | ---: | ---: | ---: | ---: |
| 5490, earlier capture | 60 | 9 | 9 | 2 |
| 13770, threatened capture | 64 | 13 | 13 | 13 |
| 13800, threatened capture | 64 | 13 | 13 | 13 |

At both threatened snapshots, bearings **8 through 20** have grounded and
approach cover and valid walking round trips. Powered equipment adds no route
to these thirteen candidates. Their measured round-trip scores are about
1507.64 controller units. Cover is a current first-hit terrain-ray measurement
at heights 7 and 30 above the proposed landing pose; departure uses height 60.
It does not measure protection along the entire flight to that site.

At **13819**, the live planner publishes its first positive route, measured at
13770: planet 1, bearing **42**. The current observation contains only that
validated site. Its native score is 81.25 approach + 4000 cover penalty + 0
ground-trip score = **4081.25**. Both eligible directions refer to the same site.
The route puts the flag within hatch interaction range, which explains the zero
ground-trip score; it says nothing about flight exposure. There is no current
cover measurement for this newly delivered site until the following tick.

At **13820**, all three cover flags are false. The controller retains the
selected site, and the cover response remains unarmed throughout this approach.
The required-cover filter is activated by a later witnessed rejection, so it
does not prevent this initial choice.

As a separately labeled **post-replay calculation**, applying the existing
native score formula to the complete diagnostic evidence gives:

| Same-clock full-evidence calculation | Best covered candidate | Covered score | Exposed site 42 score |
| ---: | ---: | ---: | ---: |
| 13770 | bearing 10, positive circling direction | 1723.98 | 4081.77 |
| 13800 | bearing 10, positive circling direction | 1724.10 | 4081.65 |

These are hypothetical scores with information the playing controller did not
have. They are not native choices or physically executed alternatives. They
show why increasing the cover penalty is not the first supported change: at
these snapshots the existing formula already favors the covered route when
given its completed evidence.

The earlier capture is a useful counterexample to a broad failure claim. It
also chooses an exposed initial site, bearing 56 at 5539, with no current cover
entry. It eventually replans and completes the previously documented powered
crossing, claim, boarding and departure. At its earlier full survey, nine covered
round trips exist, but seven fail the independent solar-clearance check. Only
bearings 0 and 63 pass that check. Requiring cover alone would not qualify all
nine, and changing this earlier decision could remove the later successful trip.

## Why the retry cannot rescue this approach

| Tick | Physical/control evidence |
| ---: | --- |
| 13746 | Capture handoff; 53.77 hull, opponent about 221 units away. |
| 13777 | First new laser damage, before any landing route is published. |
| 13819 | Selects exposed bearing 42; opponent 184.64 units away. |
| 13820 | Current ground, approach and departure cover all measure false. |
| 13887 | First thrust command after site selection. |
| 13898 | Cannon hit removes 48.31 hull, leaving 1.01. |
| 13920 | 0.018 hull remains; opponent 134.71 units away. |
| 13921 | Laser damage finishes the ship. |

Across the **102 control updates** after selection, the bot is never aligned
with the site and never satisfies the current descent gate. Its angular error
only changes from about -0.856 to -0.825 radians; alignment requires magnitude
below 0.2. It issues thrust on 29 updates, no brake and no weapon controls. These
are control counts, not evidence that more thrust or firing would be safe.

The `seek_cover` rejection requires both alignment and more than 120 ticks in
that phase. Neither happens before ship loss. The ten-second no-progress timer
also has not expired. Thus the later retry mechanism cannot address this
initial exposed commitment in time. The final pod impact remains downstream of
the cannon/laser damage, with no completed landing at this destination.

## Next change to test

Test an opt-in initial cover requirement while exposed, paired with an early
request for current covered-route evidence under the existing shared budget.
Use observed candidate geometry and native route results; never inject the
offline full-survey routes into play. Keep unsupported/missing routes unknown,
preserve the solar gates, and retain fixed acquisition/search deadlines.

Measure the complete continuation, including waiting under fire, successful
landing/claim/boarding/departure and all later ship/pilot losses. Include the
earlier successful capture, both near-full-hull regressions and the complete
retained corpus. Start the policy throughout each match rather than forcing a
change only at 13819. The rejected health rule should stay disabled in ordinary
comparisons, with this health-enabled losing trajectory retained as a separate
diagnostic case. No default promotion follows from this investigation.

Timely evidence remains an open requirement. The offline probe uses three
complete observed-site sets and **188 paired site measurements**. Those paired
jobs alone consume 7,624,936 graph operations and 5,236,058 physics queries,
excluding the independent native batches and other probe overhead. A covered
site's full-model job here takes roughly 19,500 graph operations. These are not
the costs of the live focused planner and cannot be used to claim that its
4-graph / 384-query quota can deliver a covered route before the incoming hit.
A covered endpoint also does not prove that the ship can safely fly around the
planet while its opponent moves. Both timing and physical continuation need
their own controlled comparison.

## Validation and retained evidence

All **675 Python tests pass**, including six new checks for missing versus
negative cover, unavailable route probes, translating/rotating planet frames,
the grounded low-height exception, complete sparse-trace retention and frozen
command construction. The previously tested Rust binary is unchanged; no new
Rust build or full-workspace test run is claimed here. All physical, powered
route, flight-continuation, hull-gate and allocation auditors pass. Live charged
work stays within 4 graph / 384 queries, and publication age stays within 120
ticks. Extra diagnostic work is isolated from the ordinary sensor counters.

[The result manifest](data/threatened-capture-approach-v1.json) retains all seven
snapshots, comparisons, physical outcomes and derived findings.
[The compressed evidence archive](data/threatened-capture-approach-v1.json.gz)
contains 16 exact documents, including native paired measurements, the dense
derived timeline, physical witnesses, full report, original plan, hypothetical
score calculations and validation logs. Hashes for 18 raw files and every
embedded text were verified. Full streams remain under
`target/threatened-approach/v1`; the original input files and frozen binary
retain their hashes.

- Frozen summary SHA-256:
  `92ab7a667abb4e1af53306b7a28e6c3f67640b888a8c76db8f0b3c3726c33444`
- Evidence archive SHA-256:
  `8273740d539f6c94ba7a118ce89121e799fa7b48e1116b595a667a757a573881`

The follow-up [initial-cover admission experiment](initial-cover-admission.md)
implements and compares the proposed check. It remains disabled after mixed
complete outcomes, loss of the earlier successful capture, and evidence that an
early sensor request does not necessarily reprioritize an already pending route
job.
