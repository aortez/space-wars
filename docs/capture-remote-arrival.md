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
