# Crow ground visits and water avoidance

Follow-up to the [wingbeat flight slice](crow-flight.md) of #152. The accepted
flight tuning is unchanged. The new behavior uses the same one-resident lifetime,
fixed simulation tick, seeded decisions, and native renderer.

## Visit behavior

- After half a second on a digit, consider one ground excursion with a 20%
  seeded chance, subject to a travel budget and ground clearance. The decision
  is made once; later perch stops do not reroll it. Ordinary hopping/perching
  continues otherwise.
- Two side lanes stay outside the complete digit face and inside the frame, with
  clearance for the crow silhouette. Pick the nearer eligible spot. Some narrow
  layouts have no lane or insufficient time and retain digit-only visits.
- Only the ordinary level floor or fully closed Rain panels permit a ground
  visit. Moving panels and custom duck courses are excluded; support changes
  abort an approach or end a pecking visit. They never create a phantom floor.
- Peck twice over 56 ticks (about 0.93 seconds), then climb and fly offscreen.
  Ground pecking is the last activity of the visit; losing support while pecking
  also makes the crow leave. There is no return to the digits after pecking.
  Admission reserves the full eight-second approach allowance plus pecking
  before the normal 17-second departure deadline. A climb/crossing estimate
  rejects floors too far below the face for the five-second exit window.
  The 22-second hard visit cap remains.
- A last-moment abandoned descent still respects the surviving floor, including
  the actual tilted panel surface and its open drain gap.
  Ground trips add no Rapier bodies, colliders, food actors, or physics world.

## Water response

The Clock **Crow** control cycles **Off → Varied → Shy → Hardy** in the
launcher and paused Clock controls. It combines the existing admission switch
with a saved `crow_water_tolerance` setting (`varied`, `shy`, or `hardy`).
Older saved settings default to Varied and retain their existing Crow on/off
choice. Varied selects Shy on 90% of admissions and Hardy on 10%. The draw uses a
separate stream derived from the visit seed, without advancing perch, water or
event randomness. Each visitor keeps its resolved temperament until departure;
live preference changes apply to the next admission.

The environment is a borrowed read-only view of the current Rain/Meltdown water.
It is sampled after the existing water step, with no extra water simulation.

Shy preserves the original behavior: three foot samples, 0.04 cell pitches above
support, detect occupied pooled water. Hardy raises that threshold to 0.85 pitches
and accepts ground destinations exposed to falling spray. It can stand in
the puddles on digit tops and hop between them, but still rejects deeper water.
Sampling at the feet distinguishes a wet digit above the crow from water under
it. Water above the visitor's
threshold rejects a destination or prompts immediate departure if already resting.
Neither temperament overrides geometry or support checks.

Falling parcels use a small body sensor and a one-tick velocity sweep so fast
drops can register between ticks. Exposure is weighted by parcel volume and
bounded to one unit per tick. Full local contact fills Shy's tolerance meter in
roughly 0.3 seconds and Hardy's in three seconds (ten times the exposure).
Either meter drains over three dry seconds. One brief splash does not
automatically cancel the visit. This is an approximate behavioral sensor, not
general fluid/body collision: it does not displace, consume, or redirect water.
An airborne stream still follows the existing solver, independent of the crow.

Zero-duration control/pause updates revalidate destinations but do not accumulate
wetness, peck, move the bird, or advance its age. Diagnostics add `pecking`,
`water_tolerance` (the resolved Shy or Hardy temperament),
`ground_target_milli`, `ground_visits`, `pecks`, `wetness_milli`, and
`wet_departures`; `wetness_milli` reports the fraction of this visitor's spray
tolerance used. Older state JSON defaults to Shy, matching its original behavior.

## Verification and preview

The 192 dry font/layout/seed visits still land and leave offscreen. In the initial
65%-chance prototype, 44 visits included ground pecking, 40 returned to a digit, and four
were redirected into the scheduled departure during their return. Those four
were Classic at 800×480 seed 0 and at aspect 4 seeds 1/2, plus Matrix at aspect 4
seed 6. The terminal-peck follow-up replaces that return with direct departure:
12 of the 192 fixtures visit the ground, and all 12 complete two pecks and
leave offscreen. A separate 1,000-seed decision test selects 194 ground trips
when geometry and time permit, verifies replay, and prevents repeated rolls.

Eight paired heavy-rain visits (two landscape ratios × four seeds) compare the
actual water statistics and every falling parcel at every tick, with and without
the crow. All eight register a wet departure and preserve identical water state.
Focused regressions cover puddle avoidance, interrupted ground descent, pooled
water interrupting pecking, brief versus sustained spray, pause, vertically
separated pools, removal of level support, and contact with an opening panel.
The existing paired replay also
compares the expanded crow diagnostics and flight state exactly.

Export native dry/rain captures with:

```sh
SPACEWARS_CROW_GROUND_ARTIFACTS="$PWD/target/crow-flight/peck" \
  cargo +1.89.0 test --locked --profile ci -p engine-client \
  crow_ground_and_rain_render_and_export_playback
python3 -m http.server 8766 --directory target/crow-flight
```

Open `http://localhost:8766/peck/`. Matrix font, seed 13, reading 12:34; Heavy
Rain starts at three seconds in the right-hand scene. All three display sizes
run for 22 seconds. The narrow portrait fixture intentionally skips ground
visits. The earlier return-to-perch prototype remains at `/ground/`.
Captures use the actual Rust simulation and both native render adapters;
the HTML only replays frames. Full exports are opt-in, and normal tests include
peck-pose and periodic raster/vector smoke samples.

For Shy and Hardy in the same rain, use the same server with:

```sh
SPACEWARS_CROW_TOLERANCE_ARTIFACTS="$PWD/target/crow-flight/puddles" \
  cargo +1.89.0 test --locked --profile ci -p engine-client \
  crow_water_tolerance_renders_and_exports_playback
```

Open `http://localhost:8766/puddles/`. Both sides use seed 5 and Heavy
Rain at three seconds; this seed visits adjacent digit tops so the comparison
can show wet hops. The forced profiles make the occasional visitor easy to
compare without waiting for random admissions. Tests compare identical dry flight
and perch choices across all settings, reproducible 90/10 sampling, live setting
changes, shallow versus deep puddles, steady-spray soak times, and identical water
states in paired real-rain runs.

The tolerance sample produced 101 Hardy profiles from 1,000 seeds. A steady
full-contact spray triggered departure after 18 Shy ticks versus 180 Hardy ticks.
The initial Hardy setting stayed until the ordinary 17-second departure in the
seed-1 landscape fixtures, but repeatedly abandoned wet approaches and never
returned to hopping. Staying longer did not establish that it could use puddled
perches. The previous preview remains at `/tolerance/`.

The wet-hop follow-up measures actual rain pools on exposed digit tops. Across
eight seeds, their 95th-percentile depths are about 0.76–0.84 cell pitches on
landscape layouts and 0.65–0.67 on portrait. The old 0.35-cell limit was too low.
The new 0.85-cell limit permits wading and hopping in those puddles; a deeper
wave can still prompt departure. Spray tolerance and Shy behavior are unchanged.
Regressions require an actual completed hop onto pooled water in all three
layout sweeps and cover a 0.65-cell puddle across every candidate perch. A crow
must still reject deeper pools and leave if support disappears.

Initial settings checkpoint (2026-10-02): 311 Clock tests passed (four existing opt-in tests
skipped), along with 24 selected client checks, the crow control-protocol test,
CLI compilation, strict Clock Clippy (`--no-deps`), formatting and diff checks.
The dependency-inclusive strict lint run stops on an existing
`collapsible_else_if` in `engine-rapier/src/spaceling.rs`; it is unchanged here.
Each six-case native gallery contains 3,966 playback PNGs. Native peck/rain and
Shy/Hardy stills were inspected; all frame files, served manifests/viewer, and
JavaScript syntax were checked.
Interactive browser/device testing and the display-driven functional workflow
were not run for this slice. Flight-only PR #157 passed all six CI jobs and was
merged. The accepted water/ground prototype is committed on
`crow-behavior-followup` in PR #159. Occasional peck-and-depart behavior is the
next slice on `crow-peck-departure`.

Puddle follow-up (2026-10-03): 313 Clock tests passed (four opt-in tests skipped),
including 24 real-rain visits and the controlled wet-hop comparison. The six
native preview cases passed; Hardy makes one hop on each landscape layout and
two on portrait, starting around eight seconds. All 3,966 frames and the viewer
were checked, and hop/landing stills were inspected. Scoped strict Clock Clippy,
formatting and diff checks passed. This follow-up has not been deployed.

Occasional-peck follow-up (2026-10-03): the final two-peck tuning passed all 314
Clock tests and the native dry/rain capture test, along with scoped strict Clock
Clippy, formatting and diff checks. The preceding three-peck pass also covered
eight selected client checks.
The native preview has 3,966 frames across six cases. In the dry landscape scenes,
the two pecks occur around 9.4–10.7 seconds, followed directly by climbing and
departure; portrait retains its ordinary digit visit. Peck/exit stills, the
served viewer, frame completeness and JavaScript syntax were checked. The
previous flight, ground-return and puddle previews remain available separately.
