# Pod recovery after failed landings

This follows the [controlled two-planet missions](two-planet-missions.md).
The shared recovery task is now `recover_ship_v6`. This slice addresses failed
pod approaches; ground navigation, equipment and material interaction rules
retain their existing behavior.

## Failure and correction

The saved desktop seed-42/P1, unmirrored, interceptor plus 3-second Mixed asteroid
case loses its ship around 100 seconds. Its first approach becomes wedged and
exhausts the ten-second progress timer at about 131 seconds. The retry then uses
a separate climb routine, which cannot trigger the pod's physical righting lift.
It remains tipped against the ground until another asteroid moves it around
171 seconds.

After stabilizing again, the only surveyed landing candidate is one the task
previously rejected. That rejection lasts for the whole task, so the bot reports
"no suitable pod landing site" around 174 seconds. By 180 seconds the actual pod
is landed with a ready exit, but the task is already permanently blocked.

Retries now return through the shared stabilization/righting routine. They do
not reset the overall recovery clock. A rejected site is deferred for fifteen
seconds, or reconsidered sooner when its measured local footing changes by at
least half a unit. It must still pass the ordinary pod-foot, hull and hatch
survey before selection.

An empty or fully deferred survey allows up to fifteen seconds for new footing,
an expiring deferral or an actual landing. The bot brakes and aligns with local
up during that wait. Actual landed state and ready hatch access take precedence
over the pending survey or retry count. A blocked hatch still uses the existing
two-second exit wait; four failed attempts remain terminal. The two-minute
recovery deadline and existing one-time hostile-ground extension are unchanged.

Telemetry records each rejected site, local footing, terrain revision, rejection
tick and eligibility time, together with the start of an active site search.
The HUD distinguishes checking another pod landing site from active descent.

## Reproduction

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror false --mode intercept --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --out /tmp/pod-landing-retries
```

`--trace true` writes optional `trace.jsonl` observations, encoded ordinary
actions and mission telemetry once per second and when a bot's label changes.
Entries precede the host's physics step. Sensor and policy timings exclude this
diagnostic serialization. The normal compact report and physical audit remain
available with tracing disabled.

The saved case now lands at about 143 seconds, claims, rebuilds and boards its
replacement, resuming the mission around 155 seconds. It has not completed two
capture-sortie departures by the three-minute cutoff. Further validation and
device evidence are recorded below.

Artifacts are retained at:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/landing-route-reliability-20260909/
```

## Validation at `22b224d`

The workspace and example-driver suites passed **1,070 tests**, with zero
failures: 1,062 workspace tests plus eight example tests. Twenty-four tests were
ignored. Four new contracts cover righting after retry, actual exit eligibility,
site cooldown/changed footing, finite survey/deadline behavior and the four-retry
cap. They also check repeated observations and cloned task behavior. Clippy
passed with existing warnings in unchanged code. Both frozen ordinary-game
baselines matched: six `navigation-v1` and twelve `strategy-v1` episodes.

Each platform ran the same 52 two-planet mission cases as the preceding
checkpoint, plus 36 existing single-planet impact-recovery trials. Every run
lasted 180 simulated seconds: **176 runs and 8.8 simulated hours** across the two
platforms. All 176 physics audits passed, and all 72 dedicated impact trials
completed their recovery/departure objective.

| Mission outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: capture and depart both planets | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate ship loss: recover a replacement | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also finish both capture sorties | 5/8 → 5/8 | 6/8 → 6/8 |
| Combat/asteroids: finish both capture sorties | 17/32 → 17/32 | 15/32 → 15/32 |
| Completed recovery events during combat/asteroids | 8 → 9 | 7 → 7 |
| Final blocked mission subjects | 1 → 0 | 0 → 0 |

The mission matrix varies both seats, mirrored layouts and three launch faces
for quiet travel; pressure combines seeds 7/42, both seats/layouts, interceptor
or mission opposition, and arrivals Off or 3-second Mixed. Duel cases repeat a
world for each reported subject seat. Counts refer to that subject, while reports
retain both mission brains. Recovery events can repeat within a run. Capturing
ground during recovery is recorded separately from a capture-sortie departure.

Four desktop pressure trajectories change, starting at samples 132, 153, 174 or
175 seconds. The saved seed-42/P1 case supplies the additional completed recovery
and removes its terminal block. All 52 Pi mission trajectories retain their
previous per-second physical hashes. The measured improvement is in the saved
desktop case; the Pi matrix supplies regression evidence. Diagnostic baseline
replays also match all 180 original motion hashes for each of two traced cases.
At 150 seconds, `baseline-pod/frame-150-p1.png` shows the tipped pod;
`prototype-1-pod/frame-150-p1.png` shows the spaceling at 49% rebuild progress.

Overall two-planet sortie completion is unchanged. At cutoff, desktop pressure
subjects are patrolling (10), recovering (8), capturing (12) or transferring (2).
Pi subjects are patrolling (9), recovering (12) or capturing (11). Long ground
routes, repeated get-up attempts, and ships that move beyond their spacelings'
reach remain useful follow-up targets. Ongoing tasks are retained as unfinished.

## Timing and provenance

The largest per-case Pi sensor p95 is 0.305 ms, policy p95 0.0194 ms and step p95
0.424 ms. Individual maxima are 16.99 ms, 2.07 ms and 11.14 ms respectively.
These are separate measurements; sensor spikes can still exceed a 60 Hz frame.
The kiosk was paused during these sequential Pi runs. Desktop mission episodes
ran concurrently with the impact suite and build work, so its timing tails
include contention.

`validation-summary.json` records the before/after results, changed trajectories,
remaining replans and timings. `final-desktop/` and `final-pi/` hold mission
reports; `final-{desktop,pi}-impact/` hold impact trials. Binary manifests identify
source commit `22b224d859c2ba56ce417744819fa0ce8c3c1d27`. The immutable desktop
runner matches the diagnostic prototype used for the saved failure.

- Desktop mission runner SHA-256: `872d04371e88433b6c98f5243a34e96126e605a38eda0453d400dd67ba170dc4`
- Pi mission runner SHA-256: `1cce87b3acb6be60f0a29143222652ed97289565384834cf3395503749551cf5`
- Image SHA-256: `3e735349768851b8d2a0a9a243cd375251ac287cd8eb602937befea6c3e01229`
- Packaged client SHA-256: `cfe34f9f5f2ab4b01c8bdd5ffdd44e7d88350fc90dbdfa36129c4327e96af969`

The image is archived as `spacewars-image-22b224d.ext4.gz`. OTA deployment to
`spacewars.local` completed, booting slot B (`/dev/sda3`). The installed client
hash matches the packaged client above; the kiosk is active with zero restarts.
`installed-verification.log` retains those checks.

## Live Pi check

A three-minute `spacewars-terrain-travel-duel` round used 3-second Mixed asteroid
arrivals and raster rendering at scale 2. Screenshots/status were requested at
30, 75, 120 and 180 seconds. Sampled frame rates were 60.0, 59.6, 58.0 and 59.3
FPS, with zero service restarts. These are point samples, not a full frame-time
distribution.

Both bots captured their first destination and approached the other planet.
By 120 seconds P2 owned both planets and patrolled while P1 recharged on its
ground route. At 180 seconds P1 was still trying to get up on that route. This
preserves the remaining traversal problem as a separate case; the live check
does not establish a completed mission for both bots or exercise the saved
desktop pod failure. The headless traces and impact trials supply that evidence.

The device was then left in a fresh paused `spacewars-terrain-travel` round:
P1 human versus P2 mission, with 8-second Mixed arrivals. Press Start to resume.
`live-summary.json`, `pi-live-*.png`, `pi-ready-state.json` and `pi-ready.png`
retain the live run and ready state.
