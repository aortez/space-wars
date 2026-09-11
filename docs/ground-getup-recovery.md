# Ground get-up recovery

This follows [pod landing retries](pod-landing-retries.md). The shared ground
navigation task is now `ground_navigation_v6`.

## Recorded failure

Two saved Pi seed-42/P1, duel, 3-second Mixed asteroid runs end with repeated
get-up attempts. Both the mirrored and unmirrored layouts have steady contact
with retained material. The character is recovering from a knockdown and its
get-up result is `Blocked`; there is no swept clearance along gravity-relative
up. Even larger proposed upright lifts intersect the terrain. These are actual
stair/overhang contacts, rather than gravity oscillation or an unsettled body.

Diagnostic baseline replays matched all 180 original per-second motion hashes
in each case. The original controller requests a fresh get-up once per second
but never moves sideways during this state, so the obstruction cannot clear.

## Behavior

The additive recovery observation now includes balance, get-up result, attempt
count, contact stability and at most two short crawl corridors. Each corridor
sweeps the real capsule shape for 0.6 units along the actual contact tangent and
checks three retained-floor samples. The query has a 0.025-unit contact-slop
offset. It permits separation from an existing contact; a static overlap test
would reject the very escape being measured. Sensors do not move bodies, flush
dirty queries, change terrain or authorize claims/boarding. Detached debris
cannot supply crawl footing.

After a blocked get-up, a settled recovering bot chooses an available corridor,
preferring its existing direction or the closer destination. It uses ordinary
horizontal input: the same reduced-speed movement a human already has while
recovering. It finishes at most one second of the measured short step while its
body turns against contacts. Support loss, dirty queries and a changed terrain
revision invalidate the retained step. Missing clearance cannot renew it.
Getting up still uses the normal fresh jump press. Active lifting pauses crawl
movement. The whole crawl episode is bounded by eight seconds and six units of
displacement, within the existing ninety-second ground task deadline.

Once balanced, the bot surveys a new route from its actual position. A nearby
waypoint above the actor can also trigger an ordinary jump after walking stops
making progress, even when its horizontal error is inside the usual dead zone.
The HUD distinguishes crawling clear from trying to stand; telemetry retains
the direction, short target and reposition count.

There is no change to spaceling strength, get-up lift, jetpack charge, shared
physics/gravity stepping, or the human capture and transfer rules. Historical
V1/V2 flight observations keep their existing semantics.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror false --mode duel --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --out /tmp/ground-getup-recovery
```

Repeat with `--mirror true`. The optional trace now includes physical posture
and writes when the balance or get-up result/count changes, in addition to its
previous once-per-second and label-change samples. Entries precede physics;
sensor/policy timings exclude trace serialization.

In the development Pi replays, the unmirrored character crawls at 147.82 seconds
and is balanced at 148.10; the mirrored character crawls at 125.42 and is balanced
at 125.82. The mirrored run reaches the enemy flag at about 150.52 seconds and
lowers it. An asteroid edits planet 0 at 155.72 seconds, interrupting the replacement
flag. The bot subsequently has physical contact but no valid claim anchor and
needs to find new footing. The unmirrored run resumes walking, jumping
and jetpack travel, reaching about 13 units from its flag by 180 seconds. Neither
run has completed its second capture-sortie departure by cutoff. These outcomes
establish recovery and resumed traversal, not a completed two-planet mission.

Artifacts, including the unchanged baseline traces and development replays:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ground-getup-recovery-20260909/
```


## Validation at `6f1f5a1`

The release workspace suite passed **1,069 tests**, with zero failures and 24
ignored tests. The two example-driver suites passed another eight, for **1,077**
passed tests in total. Seven new contracts cover measured capsule sweeps,
read-only/dirty-query posture sensing, ordinary physical crawling beneath a
roof, stable get-up gating, replay/clone/reset behavior, bounded crawl steps,
support/terrain invalidation, malformed observations, and the overhead-waypoint
jump case. Formatting, Clippy, and both frozen ordinary-game suites passed:
six `navigation-v1` and twelve `strategy-v1` episodes matched their baselines.
Clippy retains warnings in unchanged code.

An initial debug workspace run hit the default Rust test-thread stack limit
while constructing the unmodified Slint `MainWindow` in the minimap test. That
test passed separately with an 8 MiB stack. The reported full-suite totals use
the same release profile as the previous checkpoint; the minimap test also
passes there with the default stack. The debug failure is retained in
`initial-debug-workspace-tests.log` rather than counted as a passed full run.

The same 52 two-planet missions and 36 single-planet impact trials ran on each
platform: **176 three-minute runs, or 8.8 simulated hours**. All 176 physical
audits passed. All 72 dedicated impact trials completed recovery and departure.

| Mission outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: capture and depart both planets | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate ship loss: recover a replacement | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also finish both capture sorties | 5/8 → 5/8 | 6/8 → 6/8 |
| Combat/asteroids: finish both capture sorties | 17/32 → 17/32 | 15/32 → 15/32 |
| Completed recovery events during combat/asteroids | 9 → 9 | 7 → 7 |
| Final blocked mission subjects | 0 → 1 | 0 → 0 |
| Once-per-second subject samples labeled get-up | 6 → 6 | 97 → 9 |

The comparison uses designated subject seats. Duel worlds occur once for each
reported subject; reports retain both brains. Label sample counts are coarse
aggregate observations, not precise timings of the subsecond crawl. Two desktop
and four Pi trajectories change; the latter are the two saved duel worlds
reported from each seat. Both final Pi cases match every one of the development
trace's 180 physical hashes, including the recorded stand-up and route behavior.

The additional desktop block is seed 7/P1, mirrored, duel with 3-second Mixed
arrivals. It now reaches the existing "assigned ship has no grounded hatch"
timeout; previously it was still in an unfinished capture task. Neither version
completed both capture-sortie departures. Missing/moving ships, slow extended
routes, and selecting valid claim footing after destruction remain follow-up
work. The matrix does not show improved complete-mission counts.

## Timing and build provenance

The largest per-case Pi sensor p95 was 0.305 ms, policy p95 0.0194 ms and physics
step p95 0.420 ms. Individual maxima were 16.90 ms, 2.10 ms and 11.22 ms. These
are separate distributions; sensor spikes can still exceed a 60 Hz frame.
The kiosk remained paused during sequential headless Pi tests. Desktop timings
include concurrent validation/build work.

`validation-summary.json` records the comparison, changed trajectories,
remaining blocked case and timing measurements. Immutable runner manifests and
`image-manifest.json` identify source commit
`6f1f5a1fc44f094bf1c6b8de2725981a83182baa`.

- Desktop mission runner SHA-256: `daa18895d90dc3ae62a6afc1bf4830951806a584e6b8829f11071ce221d6ce6e`
- Pi mission runner SHA-256: `9f75d0e9468b9798ae43a4d934f1211baae67579326f9c035d90f4d446b51265`
- Image SHA-256: `620294ce0198e1d702a3ec8977a6df9f5bee8a188d83aafdc398c5463c5bcebd`
- Packaged client SHA-256: `364a6c241637401e38b5a7b43ef9aa48a8d751c417e831cd83248a76cc21aaf8`

The image is archived as `spacewars-image-6f1f5a1.ext4.gz`. Yocto completed all
6,608 tasks, with 21 rerun; its existing host-distribution warning remains.


## Deployment and live Pi check

OTA deployment to `spacewars.local` completed, booting slot A (`/dev/sda2`). The
installed client hash matches the packaged client above. The kiosk is active
with zero restarts; `installed-verification.log` retains the checks.

A three-minute `spacewars-terrain-travel-duel` round used 3-second Mixed arrivals
and raster rendering at scale 2. Screenshots/status requested at 30, 75, 120 and
180 seconds sampled 58.8, 59.4, 58.3 and 59.3 FPS, with zero service restarts.
These are point samples, not a frame-time distribution.

Both bots captured their first destination and approached the other planet.
At 120 seconds P2 patrolled with both planets secured while P1 recharged on its
ground route. At the final capture P1 was landing from a jetpack crossing to
continue that route, rather than remaining in the previous repeated get-up
state. This live round does not establish completed missions for both bots.

The device is left in a fresh paused `spacewars-terrain-travel` round: P1 human
versus P2 mission, with 8-second Mixed arrivals. Press Start to resume.
`live-summary.json`, the four `pi-live-*.png` images, `pi-ready-state.json` and
`pi-ready.png` retain the live run and controller-ready state.
