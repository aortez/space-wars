# Rounded Spacewars match: Pi performance investigation

Measured 2026-09-12 on `sw-picade.local`, a Raspberry Pi 4 Model B Rev 1.4,
at 1.5 GHz. Start/end temperatures during the diagnostic runs were 63–66 °C.
The first optimization target is **unrequested full landing surveys while a bot
is on foot**. The measurements below describe the original diagnostic experiment.
The subsequent [production fix and cabinet validation](on-foot-survey-fix.md)
apply that finding to the default mission observation path.

## Live symptom and scope

Ordinary Spacewars already uses rounded material terrain and chunk compounds.
The separate Round lab also uses rounded terrain, but lacks the same two-bot
mission workload. The installed and running kiosk executable matched the
`2610d9095f3a` deployment (SHA256
`327393e7fce87e431d5c898a927846a462ee7f2d7ad9db1abd2b54bcc99331dd`).
The later `8e1b357` commit only changes tests and CI notes.

A slow automatic match measured **4.63 FPS / 23.17 simulation updates per second**
over 30 seconds, using frame/update counter differences. Rolling timing samples
averaged 157.2 ms in simulation/control per frame, with five simulation updates
per callback. Scene generation cost another 9.3 ms, preparation 21.7 ms and KMS
rendering 25.1 ms. Output was 1024×768, raster scale 2, with about 26,200 scene
primitives. A fresh match later ran around 15 FPS / 60 updates per second: this is
a state-dependent slowdown, not a constant five-FPS cost of rounded terrain.

The two-planet Round lab measured about 29 FPS / 60 updates per second, with only
0.56 ms simulation time per frame and about 6,227 primitives. That comparison
separates workloads; it is not a paired terrain algorithm benchmark.

## The expensive query

`MaterialMissionClientScenario::step` requests an observation for each bot before
advancing the shared simulation. These sensors perform real material queries;
their time is outside the underlying physics-step timer.

`RecoverShipTask::site_request()` returns `None` when recovery has no selected
landing site. After a rebuild, this can persist while the spaceling walks back
to board its full ship. In `pilot_observation`, `None` means survey **all 64
landing bearings on every update**. Each candidate measures footing, hull and
hatch clearance; further tactical queries consume the resulting candidates.
The recovery policy's on-foot branch returns before consulting landing sites.

In simulation seconds 180–240 of the Pi baseline, player 2 was on foot with a
rebuilt ship and no selected landing site. The observation path made **227,187
landing-site checks in 3,600 updates**. They consumed 40.33 seconds of the 44.37
seconds spent in its tactical sensor path. During seconds 210–240, that bot's
sensors averaged 12.40 ms per update, while the whole shared simulation step
averaged 0.73 ms and its nested raw physics timer averaged 0.49 ms.

## Controlled diagnostic experiment

The archived experimental runner can replace a `None` site request with the existing no-site
sentinel when the player is on foot (`--omit-on-foot-sites true`). Explicit
selected-site requests, ground navigation, hatch/boarding checks, rebuilding and
support sensors remain active. The mission API already normalizes the sentinel
back to a full request for escape pods, so this route does not suppress their
surveys. This switch defaults off and is only in the diagnostic example.

Both Pi runs used generated seed `12448715435361755911`, two mission bots,
default rounded/compound terrain, match rules, combat breaks 15/4 seconds and
asteroids off. Each requested 600 simulated seconds and ended at **18,544 ticks
(309.067 seconds)** with the same match result: player 1 wins after player 2's
pilot dies from a sun impact.

| Pi measurement | Original queries | Omit unused on-foot surveys |
| --- | ---: | ---: |
| Player 2 sensor mean, seconds 210–240 | 12.403 ms/update | 0.776 ms/update |
| All sensor observations, whole-run mean | 2.064 ms/actor | 0.615 ms/actor |
| Shared simulation step, whole-run mean | 1.316 ms/update | 1.332 ms/update |
| Landing-site query calls | 361,410 | 76,146 |
| Landing-site query time, accumulated | 59.14 s | 7.20 s |
| Ground-map calls | 693 | 693 |
| Ground-map time, accumulated | 13.31 s | 13.21 s |
| Sensor observations over 16.67 ms | 693 | 693 |
| Sensor P99 | 20.60 ms | 20.35 ms |

The experiment changes 4,528 requests. Independent copies of both original bot
policies consume the original observations and produce **identical encoded
controls on all 37,088 player updates**. The paired reports match exactly after
removing timing values and the experiment's own metadata, including events,
telemetry, world snapshots, material audits, populations and collision-pair
counters. Every non-timing column in the per-tick CSV also matches. Both physical
and material audits pass. This validates this replay, not every possible world.

Measurements exclude JSON/CSV writes and the extra reference-policy work. That
extra work can affect CPU caches, so the single pair is exploratory evidence,
not a statistical speedup estimate. Both runs suspend the kiosk process during
headless measurement and resume it afterward. These are not rendered FPS tests.
The current kiosk executable remains installed and its automatic match resumed.

The seed was recovered from the original slow match, but the replay is not a
bit-identical reconstruction of its late-game state: diagnostic and deployed
binaries/toolchains differ, and desktop/Pi trajectories diverge. The reproduced
on-foot query bottleneck is a strong optimization lead; a live post-change test
is still needed to establish how much of the original five-FPS symptom it fixes.

## Recommended order

1. Avoid a full landing-candidate survey while on foot without a selected site.
   Express the request deliberately in the production mission/sensor interface;
   preserve explicit candidate validation and all current-ground, transfer and
   pod-landing safety checks. Test recovery, ordinary walking, mining away support,
   boarding and flight transitions, then repeat the paired replay and live FPS
   measurement. Do not reduce simulation frequency to mask query cost.
2. Re-profile ground surveys after that change. The remaining 693 spikes coincide
   with ground-map work, which samples 512 positions and connections out to six
   neighbors. Investigate bounded/local work or reuse, with explicit invalidation
   for terrain edits, moving obstacles and gravity changes. Bot policy execution
   itself is much cheaper than producing its observations.
3. Measure rendering independently. Scene/preparation/KMS rendering together
   already cost around 50 ms per live frame in the recorded workloads. Faster
   sensors can restore simulation headroom but do not imply a 60-FPS display.
   Keep physics profiling for contact-heavy cases; this replay does not justify
   another broad-phase or solver rewrite as the first step.

## Reproducing the original experiment

The exact original executables are retained in the artifact directory. For the
historical paired desktop experiment, run `runner-desktop-profile` for the first
command and `runner-desktop-final` for the second, using these arguments:

```sh
./runner-desktop-profile \
  --world generated --seed 12448715435361755911 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --profile-physics true --timing-csv true --out /tmp/match-profile-original

./runner-desktop-final \
  --world generated --seed 12448715435361755911 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --profile-physics true --timing-csv true \
  --omit-on-foot-sites true --verify-ablation true --out /tmp/match-profile-omit
```

Use fresh output directories. `--seconds` now allows a full ten-minute manual
profile; the default and existing explicit three-minute fixtures are unchanged.
`--timing-csv` writes per-player observation/policy costs and shared step metrics.
Its locations are post-step, while sensor/node data describe pre-step observations.
The `sensor-profile` Cargo feature adds nested inclusive/exclusive query timings
in `sensors.jsonl`. These scopes are absent from ordinary builds. In the command
above, `--verify-ablation` asserts action equality each tick and is intentionally
too expensive to use as a total-wall-time performance comparison. The current
runner replaces those experimental switches with `--verify-on-foot-surveys true`,
which checks production bot controls against full-survey reference policies.

Validation: 357 scenario library tests pass with profiling enabled. A desktop
180-second run matches the archived baseline's non-timing report, and the
28,787-tick desktop run matches with and without detailed profiling. Nested
timings partition correctly on desktop and both Pi runs. Release examples build
on Rust 1.89 for desktop and AArch64; the Pi build uses Zig's
`aarch64-linux-gnu.2.31` target to retain compatible glibc linkage.

Raw reports, CSVs, nested profiles, exact diagnostic executables, source patch,
analysis script, checksums and build/test logs are archived outside Git at
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/match-profile/`.
Earlier live status samples are in its sibling `pi-live-fps/` directory.
