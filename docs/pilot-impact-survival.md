# Pilot impact survival

This continues [match pacing](match-pacing.md) on `surface-terrain-integration`.
The baseline is `905fed3` (runtime `86a56df`); this runtime is `7650fa9`.
It addresses missile knockback and one demonstrated jetpack steering error.
Ordinary Spacewars promotion and
merging remain separate milestones.

## Collision response, not delayed braking

The new `surface_impact_probe` example runs the same ordinary combat or mission
controllers, recording each requested tick. `--pod-control brake` replaces only
the selected pod's controls after a specified tick with full brake, zero turn
and no thrust. The opponent continues using its ordinary policy. The probe
never injects hits, motion, damage, ownership or outcomes.

The read-only `impact_motion` diagnostic supplies actual centre-of-mass motion,
mass and world-boundary geometry. Landing observations instead report velocity
at the vehicle origin; that distinction matters during rapid spinning. The
probe also records the selected controls, pilot health, contact records,
recovery/ground decisions and jetpack supply.

For desktop seed 42/reflected/breaks 8, the diagnostic baseline reproduces the
old result at tick 2588. Its missile/wreckage sequence is:

| Completed tick | Pod speed | Spin | Ideal stopping distance | Distance to boundary along velocity |
| --- | --- | --- | --- | --- |
| 2485, before missile | 1.73 | 1.42 | 0.04 | 471.32 |
| 2486, missile hit | 216.96 | 125.24 | 588.42 | 440.90 |
| 2487, wreckage contact | 311.42 | -0.86 | 1212.27 | 405.29 |
| 2587, before fatal boundary contact | 207.70 | 2.32 | 539.22 | 2.12 |

Speed is units/s, spin radians/s and distance world units. Stopping distance is
`speed² / (2 × 40)`, an optimistic full-brake estimate excluding gravity and
turning. Boundary distance treats the actor as a point, so it is also optimistic.
The bot already holds full brake throughout the post-hit interval. Forced full
brake has the same finish tick and trajectory; coasting dies twelve ticks sooner.
The problem cannot be solved by noticing the hit one frame earlier.

## Selected change

The old radius-based missile mass was 12.57, versus 1 for a pod and 31.25 for a
full ship. An eight-unit recovery pod was tested first. It reduced some kicks,
but left five boundary deaths in eight desktop combat cases and did not address
external spacelings. That experiment is archived and is not the selected code.

Supplied missiles now have explicit mass **1**, about 3% of a full ship. Their
breakup pieces divide and retain that mass instead of becoming asteroid-mass
bodies again. The collider density and existing impact calculation both use the
same mass. The actual projectile, collision geometry, explosion motion and
physical wreckage remain present. Pod/spaceling mass, braking and turn limits,
asteroids, detached terrain and historical unsupplied shells keep their prior
settings. The controlled recovery hazard uses the new missile profile when it
runs in the material combat mode.

Pilot missiles still remove 40 health; the impact threshold remains 12 units/s.
There is no extra protection window, healing or velocity clamp. Altering the
physical encounter can change subsequent shot geometry and damage in a fight.
This is a gameplay calibration, not a guarantee of survival after every impact.

A regression forks the same stationary pod scene before a real 300-unit/s shot.
With ordinary braking, the legacy projectile kills it against the world boundary.
The supplied projectile leaves it at 60 health and able to stop, while still
imparting substantial momentum. Another test verifies mass through the actual
ammunition constructor and breakup path, including unchanged legacy shells.

## The on-foot landing finding

The Pi seed 7/reflected/quiet replay confirms that the injury at tick 5371 begins
with a failed powered crossing. The jetpack exhausts its charge at 5229; the
actor then falls for about 2.4 seconds and hits the planet at 16.78 units/s,
losing 19.11 health. Its ground task reports `settle`, but there is no remaining
fuel at that point. Merely requesting lift while settling would not fix this case.

The motor retains its takeoff velocity as the reference for airborne steering.
`jetpack_crossing_v2` instead requested speed relative to the planet's current
orbit/rotation. Those frames drift apart, causing the bot to keep chasing the
crossing while consuming its charge. The equipment observation now exposes the
actual motor reference; `jetpack_crossing_v3` converts its desired ground-relative
speed into that frame using the same bounded stick input.

In the isolated steering comparison the crossing completes at tick 5229,
retains about 0.6% charge and avoids the old landing injury. It later recharges
while walking. The small arrival reserve is still worth improving. A policy
contract covers reference drift and common inertial translation; the existing
physical tests still cross both directions, claim and return through the hatch.

A separate current-baseline seed 2/reflected Pi injury at tick 6905 involves an
already knocked-down actor spinning at about 20 rad/s, with a full unused pack.
It is not the exhausted-crossing case. Airborne recovery from that posture and
safe interruption of a powered route remain follow-up investigations; this
change does not claim to resolve every hard touchdown.

## Paired bounded trials

The candidate matrix uses the same seeds, reflections and 180-second caps as the
previous report: sixteen generated worlds per architecture (seeds 0/2/7/42,
quiet or Mixed asteroids every three seconds), and eight close combats per
architecture (seeds 7/42, combat breaks Off/8 seconds).

| Measure | Previous desktop | Current desktop | Previous Pi | Current Pi |
| --- | --- | --- | --- | --- |
| Combat boundary deaths | 7/8 | 1/8 | 6/8 | 0/8 |
| Combat runs with actual rebuilds | 1/8 | 3/8 | 2/8 | 4/8 |
| Combat round finishes | 8/8 | 6/8 | 8/8 | 6/8 |
| Generated-world round finishes | 4/16 | 4/16 | 3/16 | 8/16 |
| Generated worlds with a claim | 16/16 | 16/16 | 16/16 | 16/16 |
| Generated worlds with weapon contact | 13/16 | 13/16 | 13/16 | 13/16 |
| Physical/material audits | 24/24 | 24/24 | 24/24 | 24/24 |

The twelve current combat defeats are six laser deaths, three missiles, two
planet impacts and one boundary impact. Rebuilding occurs nine times across
seven runs; four combat rounds remain active at the cap. The twelve generated
finishes end in six planet impacts, four boundary impacts and two sun impacts.
Each architecture also has one generated-world run with an actual rebuild.
Twenty generated rounds are unfinished. All unfinished cases remain explicitly
`budget_exhausted`; there is no forced winner or extended simulation clock.

The 48 candidate runs total 6,570 simulated seconds (about 109.5 minutes).
Desktop generated finishes take 77.17–136.63 seconds; Pi finishes take
63.72–178.02 seconds. These small, architecture-sensitive samples demonstrate
more opportunity for recovery, not final balance or universal reliability.
The Pi headless runs share the device with the paused kiosk; they are not
isolated performance benchmarks. Median per-run generated physics p95 is
0.29ms desktop and 0.99ms Pi. Rendering is checked separately on the device.

## Regression and artifact record

There is a retained **landing regression in the original combat acceptance
fixture**: desktop seed 42/unreflected/breaks Off, with invulnerable lab pilots,
used to rebuild. Both ships now die near eight seconds and the pods exhaust
four landing attempts, becoming blocked at ticks 6173 and 6751. Neither rebuilds
by 180 seconds. This is not fixed by the current change. Its full report,
decisions and frames are in `parked-landing-regression/s42-mirrorfalse-breaks0`;
reproduce with the combat runner using `--seed 42 --mirror false
--break-interval 0 --match false --seconds 180 --frames true`.

The physical rebuild acceptance test uses the reflected encounter, which still
loses a ship, rebuilds through normal controls and resumes combat. This refresh
retains the end-to-end contract while explicitly recording the old case's
regression; the larger matrix is not an all-landings success claim. Resolving
the retained case belongs with the parked pod-landing investigation, rather
than restoring excessive projectile inertia to reproduce its former trajectory.

All 1,151 workspace tests and ten example tests pass. The ten explicit rendered
terrain UI workflows pass, together with formatting, Clippy (existing warnings),
and the six navigation/twelve strategy baseline episodes. A valid off-planet
combat screenshot initially failed the generic filled-terrain pixel threshold.
The two free-flight combat workflows now accept their minimap/body scene pixels
while still requiring the HUD; both affected workflows passed after that test
correction. The original screenshot and failure are retained. Production
rendering is unchanged by the test correction.

The generated-world regression now checks capture, pursuit, actual weapon
contact and ship-loss survival without requiring that particular encounter to
finish in three minutes. The terminal client test uses a recorded lethal laser
encounter; its previous fixture depended on the excessive boundary knockback.
It still checks the real result message, frozen physics/brains and healthy restart.

Artifacts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/impact-survival-20260910/
```

`comparison-summary.json` retains per-case hashes, outcomes, actual rebuild
counts and timing. `baseline-*-probes` retains the unchanged runtime diagnoses;
`mass8-*` and `frame-*` identify the intermediate experiments. `missile1-*` is
the candidate matrix. Binary manifests retain source hashes and build recipes.
`braking-comparison.json` and `braking-distance.png` show the tick-level brake
comparison. The initial failing terminal-UI regression log is retained separately.
All thirty remote Pi reports were copied and checksum-verified. The complete
manifest contains 81 reports including the diagnostic/control experiments and
retained landing regression. Four final-runner replays reproduce the candidate
round records and final physical audits exactly (two desktop and two Pi).

## Reproduce and investigate further

Run the current physical combat probe:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_impact_probe -- \
  --seed 42 --mirror true --break-interval 8 --seconds 180 \
  --seat 1 --from-tick 2296 --until-tick 2846 --out /tmp/pod-impact-probe
```

Add `--pod-control brake --control-from-tick 2486` for the ordinary full-brake
comparison. For the original excessive-impact trace, use the archived
`baseline-desktop-surface_impact_probe` binary; the current physics changes the
encounter and need not reproduce that old collision tick.

For the Pi on-foot cases use the architecture-matched probe with
`--world generated --seed 7 --mirror true --break-interval 15 --seconds 180
--from-tick 5000 --until-tick 10800`; seed 2 records the separate severe knockdown.
The mission runner retains the broader context and frame JSON:

```sh
cargo +1.89.0 run --locked --release -p spacewars-ai --example surface_mission_soak -- \
  --world generated --seed 7 --seat 0 --mirror true --mode duel --match true \
  --asteroid-interval 0 --seconds 180 --frames true --trace true \
  --out /tmp/pilot-survival-mission
```

Future work should distinguish a fuel-starved flight, a severe airborne
knockdown, inadequate stopping room after a real heavy impact, and a stalled
landing/ground route. Preserve the before-hit state, identify the actual contact
and compare ordinary controls before changing damage thresholds. Keep the
broader [landing investigation guide](landing-investigation-guide.md) parked
independently. The next integration milestone is still promoting the combined
loop into ordinary Spacewars, followed by representative match playtesting.

## Verified Pi installation and display check

Runtime `7650fa9` was installed on `spacewars.local` through the A/B updater.
The Pi booted slot B (`/dev/sda3`); the installed client checksum matches the
client extracted from the archived image. The kiosk remained active with zero
service restarts.

```text
image  2fa4688d43dd4c7e0f17c457e445a69311362dadcd13282c10ffd5054e97d1d1
client dc1c8c572216d7699cfa852de4858c218dd3065e2abd24e9486c7eb6a8462f86
```

The rendered arena duel used Mixed asteroids every three seconds and raster
scale 2 on the actual 800×480 display. All four screenshots were inspected.
Capture completion times include screenshot/transfer overhead and any delay
before requesting the capture; they are not relabeled as exact simulation times.

| Nominal capture | Actual completion | FPS | UPS | Updates |
| --- | --- | --- | --- | --- |
| 45s | 45.87s | 49.2 | 60.1 | 2725 |
| 90s | 90.90s | 57.1 | 60.0 | 5389 |
| 135s | 151.05s | 53.7 | 59.7 | 9025 |
| 180s | 180.88s | 47.4 | 60.2 | 10839 |

The third capture shows P2 alive on foot at 100 health after losing its ship
and landing its pod. At the final capture P2 remains alive on foot at 82 health;
P1 is still attempting a landing. The round remains active. This is evidence
of continued survival and recovery, not a completed match or proof that the
remaining landing stalls are solved. Display FPS remains below the 60-Hz target
in these views, while simulation updates stay around 60/s.

The device was then reset to a fresh paused human-P1/bot-P2 arena, with Mixed
asteroids every eight seconds for controller playtesting. `pi-ready.png` and
`pi-ready-state.json` retain the final handoff. The final screenshot was inspected:
both pilots and ships have full health, and Start resumes the match. The later
UI-test/report checkpoint does not change the deployed runtime.
