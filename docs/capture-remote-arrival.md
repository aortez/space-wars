# Conditional remote arrival screen

## Plan

The next comparison gap is the local acquisition step after a transfer. This
slice answers a narrower question: do the limited sites measured before a trip
have a solar-clear approach at the model's arrival pose? It does not estimate
acquisition duration, predict enemy movement, choose the native future site,
or change a playing controller. Empty or rejected samples do not establish
that a planet has no usable landing ground.

`--compare-remote-arrival true` extends the existing fixed-source comparison.
It snapshots the remote planner's existing evidence before the source command,
without advancing work or querying physics. This separate snapshot never
changes the observation used to generate ordinary or hypothetical controls.
The option is exclusive with the neutral-timing extension and cannot be used
in nominated physical replays. Default serialization and work stay unchanged.

Each known neutral destination retains up to four original samples, including
their generation, measurement tick, material identity, geometry, cover and
opponent. A stale status describes history; missing/incomplete/negative samples
remain unknown. Geometry needs a finite unit normal, consistent local surface
position, and at least one measured boarding hatch and clear sampled climb.
Duplicate IDs and incompatible identities are refused. The screen's numerical
domain bounds positions, velocities, radii, angles and rates to magnitude 1e6
to prevent overflow in solar sampling; this does not limit gameplay physics.

Only an explicit source capture or kinematic forecast handoff supplies an
arrival pose. Points and normals transform rigidly from each measurement's
planet pose; velocity uses the projected vehicle origin. The destination's
source-known orbit rate comes from the frozen environment, independently of
the original ship frame. Both circling directions use the existing solar
assessment, one charged graph operation per direction. At most eight additional
operations apply per comparison (four source samples, two directions), under
the same shared residual allowance of 64. No new physics queries are issued.
Construction, projection, validation and serialization still consume CPU outside
that operation counter.

Source and projected evidence age must remain within 1800 ticks. Every admitted
sample's current age and the local solar hazard are also pinned while the queue
is Pending or Ready, alongside existing 120-tick queue and material guards.
The report is historical and conditional on unchanged terrain and external
blockers. Source cover/opponent observations are retained without extrapolation;
future threat, fresh native acquisition and its duration remain explicit unknowns.
Screening never fills a whole-trip time or participates in destination ranking.

Before new study outcomes, commit code, runner, tests and this plan. The earlier
raw archives are absent in this workspace, so regenerate both arms of every
one of the 13 fixed commands in `docs/data/capture-local-composition-v1.json`:
21 source groups and 62 destination candidates, with their original seeds,
sources, durations, two seats and quiet/asteroid settings. No replacement or
selection based on outcomes. The committed summary has 20 remote numeric local
references, including four source capture entries; actual raw-snapshot coverage
is measured, not presumed. Use `sensor-profile` in the release build.

`tools/screen-remote-arrivals.py` compares complete controls/observations,
sensor counts, physics, playing work and old comparison payloads. The off arm's
full trace digest and queue summary must also equal the committed historical
results. Added direction work may delay publication or lose a result to expiry;
retain and explain such cases rather than treating better publication as a
precondition. Reconstruct each projected site, source provenance, destination
orbit and solar plan from raw records. Independently reconstructed clearance
signs within 0.01 units stay unresolved. Retain all unknowns, refused requests,
publication delays, work and file hashes. Tests cover malformed and aged
samples, motion/projection, different orbit frames, solar failure/overflow,
zero budget, current invalidation and ordinary-report parity.

This validates a conditional source-geometry screen, not bot strength or Pi
performance. Deployment remains paused.

## Results

Runtime, runner and plan froze at `fe2935ae2dabc96a9f9765c91dc3ebf0be7b1439`.
The profiled binary SHA-256 is
`539b8179c82b5dd006356fa56a17a1bdde4e6b28cdda3482e95ddd7d5aed31b1`.
All **26 runs** completed: 13 fixed cases with the option off and on. Every
off-arm full controller trace digest and queue summary matches the committed
historical archive. Each on-arm preserves its paired full controller trace,
physical outcomes, sensor counts, upstream allocations, and old comparison
payloads. The runs cover 49,440 physical ticks (13.733 simulated minutes) and
98,880 controller rows and sensor rows apiece. All 21 requests publish in each
arm; 17 publish at the same tick and four one tick later with screening enabled.
Cancellation remains 18 source expirations, two ends of unassisted flight,
and one material/ownership change per arm.

The 62 on-arm candidates retain their original denominator. Sixteen have
matching raw source evidence: two sites each, giving **32 projected sites and
64 charged direction checks**. The other 46 remain unknown: nine have no active
remote survey snapshot, five destinations are outside the neutral domain, and
32 have no sampled sites for that destination. No site-age or numeric-domain
refusal occurs in this corpus. Four screened candidates are explicit source
captures; twelve have forecast handoffs 1–120 ticks later. Projected sample
ages are 10–129 ticks. These are nearby arrival screens, not validation of
long-distance landing geometry.

Of the 64 directions, **58 pass and six fail the conditional solar screen**.
Every sampled site retains at least one solar-clear direction. The six failures
are three geometries repeated under quiet and asteroid settings:

| Source | Site bearing | Rejected side | Approach clearance |
| --- | ---: | ---: | ---: |
| fresh0, seat 0, tick 132 | 31 | +1 | -34.551 |
| fresh1, seat 0, tick 131 | 33 | -1 | -50.238 |
| holdout0, seat 0, tick 131 | 24 | +1 | -59.830 |

Thus source-measured ground alone can conceal a bad approach direction at the
projected pose. This corpus does not justify rejecting an entire destination,
predicting combat survival, or filling acquisition time. Independent numerical
reconstruction has maximum solar error 0.000031 units and no uncertain clearance
sign. There are 38 equal or nearly equal departure-corridor comparisons within
0.02 units; their preferred order remains unresolved by the audit.

The off arm spends 56,085 diagnostic graph operations and the on arm 56,149,
exactly the 64 added direction assessments. Both use zero diagnostic physics
queries and retain the shared residual limit of 64 after unchanged playing
work. Operation counts do not establish wall-clock cost or Pi headroom.

One useful limit emerged from the historical cases. The evaluator still has
four numeric remote local references, but the active survey no longer retains
their raw geometry. The new screen leaves those candidates unknown instead of
reconstructing sites from their costs or borrowing later measurements. The next
bounded step is retaining the small set of raw remote samples across survey
replacement, with explicit identity and age checks, so distant alternatives can
be screened too. Acquisition delay and future threat remain separate open
questions before these observations can influence live destination selection.

The [tracked results](data/capture-remote-arrival-v1.json) preserve every command,
source record, projected site, direction, unknown, publication tick, work count
and the 312 raw file hashes. The complete raw summary is
`target/capture-flag-survey/remote-arrival-v1/summary.json`, SHA-256
`a0570be53793ec6989c2e44971419f76f90d2d5b1437c501f5fae999f2b7f7ea`.
The previous raw archives were absent; regeneration succeeded without changing
their committed seeds, source ticks, durations, controls or trace digests.

Review corrected projected site velocity to the native vehicle-origin
convention and added a numeric bound to prevent overflowing solar samples from
appearing clear. Checks pass: 228 AI library tests, 18 profiled harness tests,
452 Python tests, formatting and strict all-target/all-feature AI Clippy.

The [independent audit](data/capture-remote-arrival-audit-v1.json) passes all
26 runs, 312 raw file hashes and 26 logs. It checks 783,035,360 decompressed
trace bytes, complete sensor/upstream parity, archived off-arm results, prior
measurement provenance, projections, destination ephemerides, all solar fields,
queue lifetimes, work totals and the tracked projection. No defects remain.
The record SHA-256 is
`1ea20eea726b3446e807ba9d6243b46275becd22cc77beef4326d05d0f9649ef`;
its separate script is retained at
`target/capture-flag-survey/remote-arrival-post-audit.py`, SHA-256
`a18023881366aa25e2638de94e7c5699ef51106f9c5c3445540bb0017f2ea6c1`.
It does not import the new production runner; the prior solar reconstruction
helper is identified by hash in its record.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/screen-remote-arrivals.py --out target/capture-flag-survey/remote-arrival-repeat
```

Use a clean checkout and a new output directory. The committed prior projection
contains the complete fixed command list, so earlier raw archives are optional.
