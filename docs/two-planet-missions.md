# Controlled two-planet missions

The next world-expansion slice is the [generated material arena](generated-material-arena.md).

This extends the [damaged-ground recovery checkpoint](damaged-ground-recovery.md)
with `material_mission_v1`: a coordinator around the existing capture, ground,
recovery and combat policies. It is a controlled experiment on two fixed
destructible planets, preceding generated-world match integration.

Follow-up: [pod landing retries](pod-landing-retries.md) addresses the failed
approach and permanently rejected footing found in this checkpoint's reports.

## Playtest

- `spacewars-terrain-travel`: P1 human, P2 mission bot.
- `spacewars-terrain-travel-duel`: two independent mission bots.

Both pilots start with full ships beside separate neutral planets. The bot
prefers another unowned planet, travels there, lands, exits, claims, boards and
departs. It then chooses another unowned planet. When both are owned it patrols
and fights using the existing combat policy; losing ownership creates another
capture objective. The HUD shows the destination and current task. Planet IDs
are 0 and 1, consistent with the existing surface HUD.

The human has the existing flight, weapons, mining and jetpack controls. Launcher
Settings expose asteroid arrivals and strength; the settings persist across
restart. Both planets can receive random arrivals. The shared combat-break
settings continue to apply during patrol. Start pauses/resumes; R restarts the
world and both brains. This scene does not yet have a victory screen or generated
worlds, and retains the laboratory's invulnerable pods/spacelings.

## Boundaries

`MissionObservationV1` adds cheap planet motion, conservative radius, terrain
revision and ownership data around the existing local observation. The target
planet is AI intent. It never changes the physical approach frame, support,
landing eligibility, hatch access or claim/rebuild authority. Detailed surveys
remain local; transit requests no landing survey. Losing a full ship triggers a
fresh pod survey, and a site from another approach frame cannot leak into the
current local observation.

Travel climbs clear, guides toward the destination's outer approach area and
routes around intervening conservative body bounds. It compensates for the
observed shared gravity solve and applies ordinary held controls. This short
transfer uses open-wing guidance capped at a desired 55 units/s; longer-distance
cruise tuning remains a later experiment. Local capture receives a neutral
handoff only after the destination is the actual approach planet and the ship
is slow enough. It retains all material landing and ground-travel gates.

The coordinator records departure after the existing task observes actual claim
and boarding milestones and the ship clears the target's radius by 70 units, or
the existing capture task completes. This lets the next objective take over
before a change of nearest planet could confuse the previous local task.

Ownership changes can cancel a pending destination; site damage is handled by
the local task. A failed transfer has a sixty-second total budget and a
twenty-second progress budget. Failed destinations are deferred thirty seconds
while other objectives are reconsidered. Capture/recovery keep their existing
bounded retries and explicit failure reasons. Ship loss interrupts travel or
capture, delegates to `RecoverShipTask`, and resumes objective selection after
the replacement is boarded and flown clear. A surviving but unreachable ship
can still strand its pilot; this does not introduce a remote rescue shortcut.

There is one shared physics step and gravity solve for both planets and pilots.
The asteroid source samples both retained material bodies independently of
actors; one-planet fixtures retain their original random stream.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 1 --mirror false --seconds 180 \
  --mode quiet --bearing 0 --require-route true --frames true \
  --out /tmp/two-planet-mission
```

Modes are `quiet` (idle opponent and weapon output suppressed), `intercept`
(existing combat bot opposes the mission), and `duel` (two mission bots).
`--asteroid-interval 3` adds Mixed arrivals, distributed across both planets.
`--strike-after-departure true` schedules one physical heavy asteroid after the
first departure to test interruption/recovery; this belongs to the runner and
is not an ability used by the bot. `--bearing` changes only initial launch faces.

Reports separate physical audits, distinct planet departures, later recovery,
blocked/unfinished work and sensor/policy/step timings. A pressure physics pass
does not establish a completed mission. Frames and per-second observations are
retained, with bounded mission event history and explicit reasons for replans.

## Validation at `b508b76`

The release workspace suite passed **1,066 tests**, with zero failures and 24
ignored tests. All **eight material UI workflows** were then run explicitly
under Xvfb, including both new presets, persisted asteroid settings, both
renderers, pause and restart. Clippy passed for the changed scenario, AI and
client packages with existing warnings in untouched code. Ordinary
`navigation-v1` and `strategy-v1` baselines matched.

New contracts cover read-only world observations, separation of destination and
physical support, identity/version checks, repeated observations, clone/reset,
ownership replanning, transfer timeout, ship loss and host ownership of bot
inputs. The physical integration test exercises both seats, mirrored layouts
and three launch bearings through capture/boarding/departure on both planets.
The asteroid test verifies that both bodies receive seeded, replayable arrivals.

Each platform then ran 52 three-minute episodes from immutable binaries built
from this commit: 104 runs and 5.2 simulated hours altogether.

| Outcome | Desktop | Pi |
| --- | ---: | ---: |
| Clean physical audits across every run | 52/52 | 52/52 |
| Quiet runs: capture and depart from both planets | 12/12 | 12/12 |
| Deliberate-loss runs: recover a replacement ship | 8/8 | 8/8 |
| Deliberate-loss runs: also finish both planet sorties | 5/8 | 6/8 |
| Combat/pressure runs: subject finishes both planet sorties | 17/32 | 15/32 |

Quiet runs use both seats, mirrored layouts and launch bearings -π/2, 0 and π/2.
Loss runs use seeds 7/42, both seats and both layouts. Combat runs combine those
seed/seat/layout cases with an interceptor or two mission bots, with asteroids
Off or 3-second Mixed. Two-mission runs repeat the same world setup for each
reported subject seat; these are not 52 distinct generated worlds. Their reports
retain both brains, while the table counts the designated subject.

No run reported an exhausted interplanetary transfer budget. Two exact side
starts required a later landing retry: they deferred around 92 seconds and
completed around 154 seconds. This exposed an overly short two-minute cutoff in
the initial integration test; it now matches the three-minute acceptance window.

All deliberate losses recovered, but some subsequent capture attempts remained
in progress or in their thirty-second retry wait at cutoff. Under combat and
asteroids, desktop subjects ended in patrol (10), recovery (8), capture (11),
transfer (2), or blocked (1). The blocked case was seed 42/P1, unmirrored,
interceptor plus 3-second arrivals: no suitable pod landing site. Pi subjects
ended in patrol (9), recovery (12), or capture (11), with no final blocked
subject. Earlier attempts still exhausted landing, ground or jetpack budgets and
were interrupted/replanned. Running and retrying tasks are not counted as success.

The useful result is that world travel and the existing local lifecycle compose
successfully. Remaining failures in this matrix center on local landing or
damaged-ground recovery, rather than transfer guidance. This does not yet prove
generated-world navigation, recovery by choosing a different planet, rescue of
surviving unreachable ships, or complete match AI.

## Timing and provenance

The Pi kiosk remained active during its sequential headless trials. The largest
per-case sensor p95 was 0.305 ms, policy p95 0.0194 ms, and physics-step p95
0.422 ms. Individual maxima were 17.13 ms, 2.04 ms and 11.32 ms respectively.
These are separate per-call/step measurements, not one combined frame budget.
Sensor spikes can still exceed a 60 Hz frame. Desktop ran four independent
episodes concurrently; its timing tails also include that contention.

Artifacts, reports, retained UI screenshots and scripts are in:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/two-planet-missions-20260909/
```

`validation-summary.json` records outcomes, replan reasons and timing maxima.
`final-desktop/` and `final-pi/` hold the authoritative matrix reports;
`prototype-*` and `exact-side-*` are earlier diagnostic evidence. The final
binary manifests identify source commit `b508b76a6e866d6f2fb7a058fd7a422c4ad09788`:

- Desktop runner SHA-256: `de616b0a62282f7a25547ec5edf7c0ab54c2d5849b8c42c00b83a02874784d94`
- Pi runner SHA-256: `e51c805c29de308bfe62be16f2c2b3e5e3550f2a31278625de45ec97f7411803`
- Image SHA-256: `08e18f73c57f53ba0557bc9985cbad7d6a5dafcd133e02fee7e92423afe46078`
- Packaged client SHA-256: `6742608908aabbb82f029d1b43f3fc0c4b2d9f716a381fe3f52e5e5e5f4e6a7d`

The image is archived as `spacewars-image-b508b76.ext4.gz`. OTA deployment to
`spacewars.local` completed, booting slot A (`/dev/sda2`). The installed client
hash matches the packaged client above; the kiosk remained active with zero
service restarts.

## Live Pi check

A three-minute `spacewars-terrain-travel-duel` round used 3-second Mixed asteroid
arrivals and the raster renderer at scale 2. Screenshots and status samples were
requested at 30, 75, 120 and 180 seconds. Sampled frame rates were 60.2, 59.6,
59.3 and 59.5 FPS, with zero service restarts throughout. These are point samples,
not a full frame-time distribution.

At 30 seconds both bots had crossed to their opposite destination and were
descending. At 75 seconds each owned its first destination and was approaching
the other planet. At 120 seconds P2 owned both planets and patrolled while P1
recharged for a ground route; at 180 seconds P1 was still traversing and P2 still
held both. This visually confirms travel, capture, boarding, retargeting and
contested ownership in the client. It also reproduces the local traversal delay
seen in headless trials; it does not establish completion for both bots.

`live-summary.json`, `pi-live-*.png` and their status/time records retain this
evidence. `installed-verification.log` records the boot slot, installed hash and
service state. A fresh `spacewars-terrain-travel` round was then restarted and
left paused for P1 human versus P2 mission playtesting, with 8-second Mixed
arrivals. `pi-ready-state.json` and `pi-ready.png` verify that state. Press Start
to resume.
