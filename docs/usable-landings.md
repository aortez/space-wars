# Usable landings on moving material planets

This implements the first chunk from the
[continuation investigation](continuation-investigation.md): make touchdown
lead to an actual exit, claim, boarding and departure. Flag-aware landing
selection and a finished ordinary Spacewars match remain subsequent work.
The confirmed rule that a surviving pod or spaceling may reclaim and rebuild
still applies; this pass adds no elimination rule.

## Evidence and changes

The soak trace now includes bounded read-only landing diagnostics: actual hull
and foot contacts, retained-material identity, contact normal/separation/speed,
individual foot clearances, and actual hatch capsule clearance. Contact output
is capped at sixteen records per hull/foot part. These queries run outside
policy timing and never advance physics. Eight focused desktop/Pi replays
exactly matched the previous audits and mission metrics before behavior changed.

The reflected seed 0/P2 delay was a genuine one-foot stop: the other foot was
about 0.8–1.0 units above the ground, with no supporting hull contact. Holding
the surveyed heading indefinitely could not finish that landing.
`material_landing_v3` waits for half a second of slow one-foot contact, then
uses ordinary rotation to close half the measured height difference across
the six-unit foot span. Each correction is capped at 0.1 radians, remains
within eighteen degrees of outward orientation, and is stored in the moving
planet's frame. At most three corrections are allowed in that controller
attempt. Existing progress deadlines and fresh-site retries remain active.
Ray clearance guides rotation; only solver contacts can grant landing.

Desktop seed 2/P2 exposed a different failure: the hull rested on a step while
both feet were about 0.13 units above the ground. Landing surveys now query the
whole hull with 0.75 units of lateral tolerance and 0.2 units of settling depth.
Foot rays and an empty central belly ray alone could admit this obstruction.

The hatch survey now checks both the actual normal-oriented exit capsule and
room to stand radially upright, including small lateral offsets. It also
reports whether three lateral offsets combined with three headings, including
±0.1 radians of tilt, preserve clearance. The bot only rotates a one-foot stop
when this additional margin exists; otherwise it retries at a fresh site.
Checking sideways drift and tilt independently missed a Pi seed 1/P2 case:
after landing, the ship slid and tilted together, lost a usable hatch during
the claim, and required recovery. Requiring the combined tilt margin at every
site also excluded otherwise usable two-foot landings and delayed another
route. Applying it to the proposed correction keeps that distinction explicit.

The shared hatch search still looks at its central ray and one cell on either
side. It now prefers a nearby floor whose actual exit capsule is clear, while
excluding the pilot's own body from that clearance query. If none is clear,
the first nearby floor remains observable and the transfer gate rejects the
exit. Other actors, missing material and occupied exit reservations retain
their existing authority. Humans and bots use this same hatch selection.

Changed landing positions also exposed a departure regression between nearby
planets. `material_mission_v6` keeps world guidance after the nearest approach
frame switches back to the departure planet. While between two nearby bodies,
it commands velocity outward from both; for nearly opposing outward directions
it preserves tangential travel. Completion still requires the real claim and
boarding followed by physical clearance of the original planet, within the
existing thirty-second departure budget. Ship loss still enters recovery.
The composed capture identity is `tactical_sortie_v9`.

Initial two-foot settling, parked corner-contact recognition, claim rules,
boarding requirements and recovery permissions are unchanged. Experiments
with a larger single rotation and different parked-contact aggregation were
rejected after paired regressions. There are no pose snaps, automatic ownership
or extra physics/gravity steps.

## Validation

Focused tests cover bounded one-foot rotation, repeated observations, clone
and reset, unsuitable contact/motion/query states, and an obstructed central
hatch with a usable neighboring exit. Observation tests also check diagnostic
queries do not change world observations or terrain audits. The generated
mission regression now requires all three departures for seed 0/P1 and also
exercises seed 1/P2. Existing removed-ground, fragment, blocked-exit, solar,
parked-return and physical recovery cases remain required.

The final validation uses the existing twenty-five three-minute cases per
platform: thirteen capture-then-hunt routes, eight two-bot asteroid duels and
four quiet routes. Thirteen additional pursuit cases per platform separate
physical capture preparation from the chase, each with its own three-minute
cap. Outcomes come from real claim, board, departure and hit ticks; timing out
is not counted as success. Desktop and Pi results are evaluated separately.

## Results at `7726c09`

All 1,117 workspace tests and nine example tests passed: **1,126 tests**.
The workspace invocation ignored twenty-six display-dependent tests; all ten
terrain UI workflows were run explicitly under Xvfb and passed, with their
eighty-two artifact files archived. Formatting and Clippy passed (existing
advisories remain outside the changed code). The six frozen navigation and
twelve strategy episodes matched their baselines.

All **76 final trials** passed physical/material audits, covering **4.53
simulated hours**, including pursuit preparation. The two preparation timeouts
below are reported as failures to prepare, not successful pursuits.

| Three-minute result | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Complete capture/departure routes | 12/13 → 12/13 | 12/13 → 12/13 |
| Actual weapon contact | 9/13 → 11/13 | 9/13 → 11/13 |
| Required contact regressions | 7/7 → 7/7 | 7/7 → 7/7 |
| Additional quiet routes completed | 4/4 → 4/4 | 2/4 → 4/4 |

The separated pursuit trials prepare 12/13 worlds on each platform, and all
twenty-four prepared trials reach actual weapon contact. Chase times are
10.68–62.20s desktop and 10.70–66.15s Pi. Reflected seed 42/P1 still needs
slightly more than the whole-mission three-minute window to make contact;
its isolated chase succeeds on both platforms.

The diagnostic reproductions show where time was saved:

| Visit | Desktop landing time, before → after | Pi landing time, before → after |
| --- | ---: | ---: |
| Seed 0/P2, reflected, planet 2 | 126.97s → 77.72s | 126.82s → 77.80s |
| Seed 2/P2, planet 0 | 170.83s → 142.53s | 143.38s → 143.58s |
| Seed 7/P2, unreflected, planet 0 | 94.62s → 66.37s | 95.30s → 66.50s |

These are times since mission start. Arrival times for each compared visit
are unchanged, so these differences measure landing time rather than a shorter
journey. The reflected seed 0/P2 visit proceeds through claim, boarding and
departure at 84.70s desktop and 84.78s Pi, about forty-nine seconds earlier.
Its complete Pi quiet route now finishes at 136.87s, including the previously
blocked final exit. The other reflected seed 0 quiet route also completes on Pi.

Some landings take longer after a fresh-site retry. Seed 1/P2's planet 2 visit,
for example, lands at 95.92s desktop and 96.10s Pi; the earlier baseline landed
at 76.65s and 82.92s. The final policy preserves the original ship and completes
all three departures on both platforms. The paired results support an overall
improvement, not a claim that every touchdown is faster.

Seed 7/P2 without reflection remains the preparation/departure timeout. Its
inner-planet landing improves by about twenty-eight seconds and retains zero
sampled solar exposure, but the later itinerary still exceeds the limit.
Desktop captures the third planet at 177.87s and boards at 177.90s, without
departing by 180s. Pi is still approaching its third landing. Thus desktop
reaches ownership of all planets in 13/13 cases, while complete departures and
prepared pursuits remain 12/13. The limits were not extended to count this as
completion.

Across the final diagnostic workloads, policy p95 ranges from 0.000250–0.006402ms
desktop and 0.001148–0.017166ms Pi; sensor p95 ranges from 0.021531–0.970764ms
and 0.059407–2.095451ms respectively. These are concurrent headless trials,
not rendered frame-rate measurements. The additional clearance queries belong
to sensor time, not policy time.

## Deployment verification

The image uses the same five accepted Yocto layer revisions as the previous
deployment, pinned in the archived build configuration. The Pi soak runner
uses a frozen copy of the installed Pi's `libm`, with its hash recorded. An
initial runner linked through a mutable build-directory symlink failed to
start; that rejected binary and launch logs are archived separately from the
final trials. No failed image or incompatible runner was installed as the game.

The archived image contains the expected `material_mission_v6`,
`tactical_sortie_v9` and `material_landing_v3` identities. OTA deployment to
`spacewars.local` completed on slot B (`/dev/sda3`). The installed client hash
matches the extracted image binary:

```text
source: 7726c0907d2aecaad9d87a88e19a27a8dc328d0f
client: 20d6f02af1b691d7963d18872e1cb1b11faa50ec9d0206fb6aea2c91e9e545ac
image:  e0d492d039162065afead31b4dd6383e61277f9cfeadc6091879070f69aead67
```

A three-minute live two-bot run used Mixed asteroid arrivals every three
seconds, the raster renderer and 2× raster scale. Captures at 30, 75, 120 and
180 seconds recorded 40.6–51.4 FPS and 59.3–60.1 updates per second, with zero
service restarts. The screenshots show real landing, exit, claiming, boarding
and travel to the next planet. They also show a later ground-route block after
capture at 120s and 180s, so this run is evidence of stable operation rather
than complete bot navigation under pressure. Those later route/return failures
remain work alongside objective-aware landing selection.

After the live run, a fresh human-P1 versus mission-bot-P2 material arena was
restarted and paused, with Mixed asteroid arrivals every eight seconds.
Independent installed-binary, service and UI-state checks confirmed this
playtest-ready state. All Pi trial reports, traces, frame recordings and
runners were archived before reboot; the final archive was checked against
the local reports, traces and binary hashes.

Artifacts, including diagnostic baseline replays and rejected candidates:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/usable-landings-20260910/
```
