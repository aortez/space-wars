# Recovery bot approach measurements

The first measurement for [#291](https://github.com/aortez/space-wars/issues/291)
found a gap between ordinary match approaches and the retained arrival practice
starts. Across four development runs, 1,019 unassisted direct-transfer ticks
were within 35 world units of the destination entry. None also had both
planet-relative and braking-frame speed at or below 6 units/s.

This is a reason to test the component in match-like approach fixtures before
enabling it in matches. No learned controller ran, so this measurement says
nothing about whether the retained genome would succeed or fail. The
[integration plan](design/learned-guidance-match-v1.md) now puts that directed
comparison before the match adapter and selector.

## Method and coverage

The diagnostic runs the normal client `spacewars` host with Recovery bot v1 in
both seats, including its recovery profile and hunt-clearance handoff. Seeds
42 and 10101 were already inspected development worlds. Each runs with weak
Angular and Round dirt, default combat breaks, ordinary combat enabled, and
environmental asteroids disabled. The native time limit is ten minutes; the
census stops after 180 seconds or a native round finish.

Read the completed-step free-flight projection immediately before each client
step, then associate it with the mission decision made from that state. Record
transfer decisions whose nearest physics frame is already the destination.
The candidate entry geometry is the planet radius plus 85 along the current
ship-to-planet bearing, and its proposed marker velocity is the planet-center
velocity. No mission target, input, controller state, or physical body is changed.

An independent unobserved control runs alongside each case. Mission telemetry
and free-flight observations match on every tick; scenario payloads match every
second and at native termination. This checks the measurement path, not learned
control or candidate strength.

| Seed | Dirt | End tick | Termination | Destination-frame transfer ticks P1 / P2 | Unassisted direct-transfer ticks P1 / P2 |
| ---: | --- | ---: | --- | --- | --- |
| 42 | Weak Angular | 10,800 | Prefix limit | 272 / 371 | 272 / 371 |
| 42 | Weak Round | 10,800 | Prefix limit | 273 / 371 | 273 / 371 |
| 10101 | Weak Angular | 10,800 | Prefix limit | 184 / 734 | 184 / 640 |
| 10101 | Weak Round | 8,896 | Native round finish | 184 / 362 | 184 / 362 |

Of 2,751 destination-frame transfer ticks, 2,657 pass the physical free-flight
and no-detour screen. The screen requires the live full ship, armed controls,
fully open wings and expected limits, no contact/support, and no landing
assistance. It is not a full obstacle or asteroid safety certificate.

## Short approaches

The following statistics concern the 1,019 screened ticks within 35 units of
the entry. Percentiles describe sampled ticks, not independent encounters.

| Quantity | Minimum | Median | 95th percentile | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Radial altitude | 77.62 | 105.95 | 118.13 | 119.98 |
| Distance to entry | 0.14 | 20.95 | 33.13 | 34.98 |
| Ship speed relative to planet center | 7.65 | 21.27 | 30.45 | 39.11 |
| Ship speed relative to braking frame | 5.22 | 21.15 | 29.87 | 38.80 |

These ticks form 17 continuous windows; the longest lasts 119 ticks, about
1.98 seconds. A conservative development filter requiring both speeds to be
at most 6 leaves zero ticks. Raising that diagnostic filter to 18 leaves
98 ticks in two 49-tick windows: the same P2 approach in the Angular and Round
versions of seed 10101. This is not a proposed qualification threshold.

The retained development starts used offsets up to 35 and initial ship speed
up to 6 units/s in a stationary world. Those are initial-condition ranges, not
maximum speeds reached during learned trajectories or a proof of a supported
continuous envelope. No feature saturates during these short match approaches;
fitting inside the encoder's numeric range does not establish trained coverage.

Planet translation alone is not a neural-input mismatch: both velocity inputs
subtract their respective reference velocities. The moving, rotating braking
frame still matters. The proposed marker's speed relative to that frame ranges
from 2.72 to 9.57 units/s in these short approaches; practice marker motion was
at most 3 in a stationary frame. Directed cases should reproduce the actual
frame, gravity, and target kinematics rather than add world velocity to a
stationary fixture and assume it represents the same problem.

There are only two world seeds, and the dirt variants share long identical
prefixes. Neither 17 windows nor 1,019 ticks represents that many independent
trials. Three runs are truncated prefixes. Keep them separate from future
held-out, completed-match comparisons.

## Retained evidence and next step

Base source: `1df24f2b574400a7b9ae78145c67402721aae907` plus the retained
test-only patch. Rust 1.89.0, CI profile, Linux x86_64. The explicit census test
passed in 22 seconds; compilation reported existing client dead-code warnings.
No production policy, setting, or device was changed.

The local evidence package is
`data/training-experiments/recovery-guidance-approach-census-v1-20261016.tar.gz`
(655,996 bytes), SHA-256
`bdafc62b2660a20078a3f1703ec37cd681603dbcd26fa3c8c37876a27bfc9e95`.
It contains all approach rows, settings and run summaries, the source patch,
executable identity, and the analysis script/output. Every archived payload was
verified against the manifest. As with prior experiments, `data/` is local and
ignored by Git.

Executable SHA-256:
`42303ee1c60736ca832c12565353269fbe560a478fbaca4fe320ab2a5b423883`.
Patch SHA-256:
`c8c8a87cc794d06a432b6ae2e4d0d25edd9938d9ac270f7c48113e061438251f`.
The [plan's reproduction command](design/learned-guidance-match-v1.md#development-census)
runs the diagnostic with a fresh output directory.

Next, freeze a small set of moving-frame approach fixtures from these measured
ranges. Compare the seed-1 genome, handwritten arrival reference, and original
mission guidance from identical starts. Establish whether the existing component
can handle them before choosing further training or a match admission envelope.
