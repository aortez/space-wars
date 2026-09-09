# Jetpack routes in ordinary material gameplay

This records the initial full-ship integration milestone. The subsequent
[pod and damaged-ground recovery slice](jetpack-recovery-navigation.md) extends
these routes and records the next validation results.

The material combat and duel presets now equip both pilots with the shared
spaceling jetpack. `GroundNavigationTask` (`ground_navigation_v3`) can combine
walking/jumping with one measured crossing of its assigned parked full ship.
Capture and recovery already use this task, so the same route option serves
flags, returning to a hatch, and reachable rebuild destinations.

Tap A/Space to jump or get up, hold in the air for lift, and use left/right to
steer. Stand still on support with jump released to recharge. Boarding preserves
charge. The nozzles, flames and charge HUD use the same equipment for humans and
bots. The dedicated `spacewars-terrain-jetpack` trial remains available.

## Route choice and control

The existing retained-material ground graph still supplies walk/jump routes.
At its staggered 2 Hz survey, an equipped pilot also measures one bidirectional
corridor over the real ship hull. Both endpoints require retained planet footing,
suitable slope and capsule clearance. The corridor retains solid vehicles,
debris and other spacelings; only the observing actor is excluded.

The navigator compares the direct ground route with two alternatives: walk to
one takeoff endpoint, cross, then follow measured ground to the original target.
The complete alternative must be reachable. Flight has a cost for climb, descent
and recharging, so a short walk wins. Planning waits for a completed joint survey
instead of selecting a long walk while the flight survey is between refreshes.

The selected maneuver reuses the trial controller in a single-crossing mode.
It waits for at least 98% charge, aligns at takeoff, climbs, crosses and descends
through ordinary controls. Within 1.5 units of the measured floor and aligned
with the destination, it releases lift to establish contact instead of hovering
away the landing reserve. Real supported, balanced contact completes the leg.
The navigator then plans toward the original destination. Claiming and boarding
retain their normal scenario rules; the maneuver cannot write either state.

## Destruction and interruption

Each refresh rechecks the corridor and ship pose. A material revision temporarily
holds powered travel until a fresh survey arrives. A still-clear corridor with
essentially unchanged endpoints can be revalidated; stale geometry cannot grant
permission to fly. Larger geometry changes, a missing corridor or exhausted fuel
interrupt the leg. Gravity and collision contacts resolve the motion while the
navigator waits for retained planet support before replanning. It never teleports
or refills a pilot to rescue a failed route.

A changed or destroyed flag updates the objective while preserving an active
landing. Capture and recovery callers wait for that landing before transferring
control to claiming, rebuilding or boarding. The original ninety-second ground
budget remains fixed; three interrupted flight attempts also stop explicitly.
A failed landing that cannot regain planet support remains a bounded failure.

## Scope and validation

This handles the assigned parked full ship on a controlled planet. It does not
supply cave flight, arbitrary crater escape, escape-pod crossings, jetpack combat
or generated multi-planet strategy. Blocked hatches still prevent exit. Some
landing positions have no measured route even after exit: the retained desktop
exploration at seed 42, P1 offsets -0.4/-0.5/-0.6 reproduces that failure with
jetpacks enabled and disabled.

Focused checks exercise route choice, actual charge waiting, repeated ticks,
clone/reset, changed objectives, interrupted corridors, fixed deadlines, and
physical crossings in both directions from both seats. The physical round trip
also revises terrain during flight and verifies normal claims and hatch entry.
Read-only sensor tests cover both directions, blocked overhead space, dirty
queries and the on-foot survey gate.

The existing three-minute ground runner accepts `--jetpacks true`; its matrix
accepts `--jetpacks`. An opposite-side capture start forces the regular mission
to cross the ship on the way to the enemy flag and again on its return:

```sh
cargo build --locked --release -p spacewars-ai --example surface_flag_soak
cargo run --locked --release -p spacewars-ai --example surface_flag_soak -- \
  --seed 42 --seat 0 --offset -0.2 --mode capture --jetpacks true \
  --edit flight-flag --out /tmp/jetpack-capture
```

`--edit none`, `flight-flag`, and `flight-crater` select no interruption, removing
the enemy flag during flight, or revising terrain away from the active corridor.
All advance 180 simulated seconds, including after completion. Reports retain
failed outcomes, route/flight telemetry, actual edits, claims, boarding/departure,
physics/material audits and separate sensor/step timings.

The local evidence archive is
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ground-jetpack-20260909/`.

## Final desktop and Pi matrix

The final gameplay checkpoint is `b4a554d83df1349703427a6872547747f175398b`.
Eighty three-minute runs supply four hours of simulated play. Every run reaches
180 simulated seconds and passes its physics/material audits.

| Mission suite | Desktop completions | Pi completions |
| --- | ---: | ---: |
| Existing ground/capture/rebuild matrix, jetpacks equipped | 18/20 | 18/20 |
| Opposite-side capture, including edits during flight | 12/12 | 12/12 |
| Capture under armed opposition | 8/8 | 7/8 |

The two incomplete ground cases on each platform are intentional disconnected
routes. The remaining incomplete mission is Pi seed 7, subject P2, unmirrored:
its approach exhausts the existing time/retry budget before exiting. It keeps a
failed capture result despite clean physical audits. Overall, 75 missions finish;
these results do not mean 80 successful sorties.

All 24 opposite-side missions complete both flights: 48 crossings, with no flight
interruptions and at least 15.5% charge remaining at landing. They cover both seats,
seeds 7/42, ordinary countercapture, destroying the enemy flag during flight, and
changing terrain away from the flight corridor. The regular capture coordinator
resumes the appropriate objective after landing, then returns, boards and departs.
The earlier near-ground fuel-exhaustion reports and binaries remain archived as
`*-before-touchdown`; final touchdown behavior resolves all of those cases.

The largest observed survey-refresh sample across ground and crossing matrices
is 6.77 ms desktop / 17.41 ms Pi. The Pi peak slightly exceeds a 16.67 ms frame
budget, before rendering. The bounded, staggered survey avoids doing this every
frame, but reducing survey spikes remains a performance follow-up. Largest shared
step samples are 3.00 ms / 7.84 ms in the broader ground matrix; the crossing
matrix's maxima are 0.24 ms / 0.66 ms. Desktop background compilation overlapped
these runs, so these are sampled costs, not isolated performance benchmarks.

All 1,049 workspace/all-target tests pass; 22 display/hardware workflows remain
ignored by default. Six material launcher workflows were then run explicitly
under Xvfb and pass, including combat, duel and the dedicated jetpack trial,
pause/restart and both renderers. Formatting and Clippy complete, with inherited
Clippy warnings only. Navigation V1's six episodes and Strategy V1's twelve
episodes still match their checked-in baselines.

## Deployed Pi verification

The Yocto build completed all 6,608 tasks (21 rerun) with the inherited host
warning. The archived `spacewars-image-b4a554d.ext4.gz` was installed on
`spacewars.local`, which booted slot A, `/dev/sda2`. The installed client matches
the image's extracted SHA-256:
`364c1cd0c3b34b12328cbf7f5db7a5fb98837bceb311b7eff2fad76e6ae5d667`.

A live 800×480 raster duel used P1 Capture versus P2 interceptor, with combat
breaks 8s/4s. At 30 seconds P1 owns the planet and is departing behind cover.
At 120 seconds P2 is landing its escape pod. At 180 seconds P2 is on foot,
equipped with a full jetpack, and reports no measured walk/jump/jetpack route to
the flag; the HUD shows 58 removed cells. This post-combat pod recovery remains
incomplete. The controlled full-ship crossing proof does not establish a route
through that damaged ground or over an escape pod.

The five samples from 30–180 requested seconds report 59.7–60.1 FPS/UPS, zero
service restarts and no pause. The last sample records 10,832 updates. These
sampled client rates do not remove the measured ground-survey peak concern.
Actual capture timestamps, screenshots and CLI logs are retained in the archive.

The final state is a fresh paused `spacewars-terrain-combat` round: P1 human,
P2 Capture, breaks 8s/4s. Start or B resumes. Land and exit normally, then hold
A and steer to use the shared jetpack. `pi-ready-paused.png` and the final UI
settings/state verify the playtest setup. No merge or push was performed.
