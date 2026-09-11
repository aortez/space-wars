# Combat breaks experiment

The material combat and duel presets can periodically disengage, fly a short
arc with weapons off, then return to combat. The pilot turns along the local
horizon away from its opponent and aims for 110 units above the planet's routing
bound. It uses ordinary turning, thrust, braking and open wings. Ground avoidance
still takes priority. The ship remains vulnerable, and rounds already in flight
can still hit during a break or after the opponent exits.

Launcher **Settings** exposes two choices for these presets:

| Setting | Choices |
| --- | --- |
| Bot combat breaks | Off, every ~8s, ~15s, or ~30s of engaged flight |
| Break duration | 2s, 4s, 6s, or 8s |

The initial settings default is 15s / 4s. Timings count engaged flight, not time
spent climbing, routing, patrolling or recovering. Each interval varies uniformly
by ±25%, using a private stream seeded by the episode and player. The duration is
fixed simulation time at the scenario's 60 Hz. Pause and repeated observations
cannot advance it. A restart reproduces the schedule. Ship loss interrupts the
break and starts the existing recovery task; losing the opponent resumes patrol.

Both bots use the same settings in the duel. In the human combat preset only P2
uses them. The bot HUD displays `flyby / weapons off` or `flyby / clearing ground`.
Ordinary Spacewars bots and the earlier flight/recovery presets are unchanged.
The original `RulePilotV4::new` policy also retains Off for frozen regressions;
client factories opt into `with_combat_breaks` using persisted settings.

`settings.toml` accepts custom whole-second values as well:

```toml
[combat_breaks]
interval_seconds = 8 # 0 disables breaks; maximum 120
duration_seconds = 4 # normalized to 1..15
```

Launcher selections are saved when starting a scenario and restored on returning
to the launcher.

## Reproducible comparisons

```sh
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --seconds 180 --break-interval 8 --break-seconds 4 --out /tmp/break-duel

# P1 uses the unchanged combat policy for 12 seconds, then attempts to land.
# Only P2's combat breaks vary across these pressure comparisons.
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --seconds 180 --land-after 12 \
  --break-interval 8 --break-seconds 4 --out /tmp/break-landing
```

The runner defaults to Off. Report version 3 includes the actual configuration,
start/end/interruption/re-engagement events, per-tick damage events with source,
and the active landing policy's telemetry in per-second samples. It audits
weapons-off intent throughout each break, plus the existing finite-motion,
material conservation and energy/ammunition bounds. An audit failure preserves
its partial report and returns nonzero.

The comparison holds the landing subject's policy fixed. An earlier exploratory
pass changed both pilots before the landing handoff; those results are archived
under `before-landing-subject-fix` and excluded from the final comparison.

## Desktop results, 2026-09-08

All 48 final runs completed 180 seconds with passing audits: each of four
intervals (Off, 8, 15, 30) covers eight duels (seeds 7/42, both mirrored starts,
separations 0.5/0.8) and four landing attempts (same seeds/mirrors, separation
0.5). All breaks last four seconds. This is a small controlled sample, not an
estimate of general player success.

| Interval | Median first ship loss, eight duels | Landing and exit, four attempts |
| --- | ---: | ---: |
| Off | 21s | 0/4 |
| 8s | 39s | 2/4 |
| 15s | 22s | 0/4 |
| 30s | 21s | 0/4 |

At 8s, seed 7 mirrored lands/exits at approximately 42.8s and completes capture, reboarding and departure. Seed 42 unmirrored lands/exits at
50.9s, claims at 54.4s and reboards at 54.5s. P2 then correctly reacquires the
occupied ship and fires two more rounds; a cannon hit destroys it during
departure at 56.1s. The landing script cannot recover from that loss.
The remaining two attempts lose their ships at 50.9s and 91.9s. Neither 15s nor
30s creates a break before the first loss in three of four landing attempts.

Eight-second duels recorded 46 breaks, including interrupted ones, and 42
completed three-second observation windows after re-engagement. All 42 returns
had both rounds loaded. Mean damage recorded as laser/cannon hits to the opponent in those windows was
3.3 percentage points, maximum 28.9; four windows included a ship loss. At 15s,
16 such windows averaged 8.3 points, maximum 40.2, with four losses. These windows
begin when pursuit resumes, before the ship necessarily aims or fires; opponents
also start them at different health levels. They document the recharge tradeoff
and do not establish a controlled comparison of burst damage. Damage source
labels can also include simultaneous contact damage on the same tick.

Off reproduces all eight previous desktop duel observation and weapon traces
exactly. Unit and client coverage checks timing, seed/seat independence, repeated
observations, reset, recovery interruption, ground avoidance, Off, configuration
bounds, old settings migration, persistence and application to both duel seats.


## Pi comparison and validation

The same 48 cases also finish 180 seconds on the Pi 5 with passing audits.

| Interval | Median first ship loss, eight duels | Landing and exit, four attempts |
| --- | ---: | ---: |
| Off | 23.5s | 0/4 |
| 8s | 37s | 0/4 |
| 15s | 23.5s | 0/4 |
| 30s | 23.5s | 0/4 |

Eight-second duels complete a recovery and return to combat in 5/8 desktop and
6/8 Pi runs, versus 7/8 and 4/8 respectively with Off. Changing trajectories and
fight durations also changes which recovery problems a three-minute run reaches;
this is not a monotonic recovery improvement. Pod stabilization, enemy flag routes
and incomplete recoveries remain recorded in the reports. All eight Pi Off duel
observation and weapon traces also match the preceding energy checkpoint exactly.

The opening fights last longer on both machines at 8s. The desktop landing gain
does not reproduce on the Pi. Use 8s / 4s for the first hands-on comparison, while
keeping Off and the other rates available; the experiment creates readable
openings but does not establish that landing under fire is balanced.

| Platform | Step P95 range | Worst step | Sensors + policy P95 range |
| --- | ---: | ---: | ---: |
| Desktop | 0.0653–0.1159 ms | 1.1135 ms | 0.0045–0.0503 ms |
| Pi 5, kiosk paused | 0.1836–0.3577 ms | 0.8711 ms | 0.0136–0.1692 ms |

The 96 final cases cover 288 simulated minutes and 17,280 passing per-second
audits. The complete workspace all-target suite passes 1,011 tests. Both combat
and duel UI lifecycle tests pass with raster and vector rendering, including
changing the new controls, Off, persistence, pause and restart. Formatting and
Clippy pass; existing workspace Clippy warnings remain, with none in the new
combat policy or endurance runner. Ordinary navigation (six episodes) and
strategy (twelve episodes) retain their exact frozen baselines.

Artifacts and comparison scripts:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/combat-breaks-20260908/`.

## Deployment and live check

Gameplay checkpoint `c2d1ac46e7bec7afe81331e4b9e3b314dfcf9a4d` was built with
the accepted Yocto layer pins. All 6,608 tasks succeeded (21 rerun). The A/B
updater verified slot A (`/dev/sda2`), an active kiosk and zero restarts.
The installed client hash matches the archived image's extracted client:
`78ed511f85da11688a6deb0e34b8e88e358d02e7aa3cb6fa43f6fc164e5b5af9`.
The compressed image hash is
`42b35d24971009a01d6b7cf094aab1ca6d8901e589e422f4e9d7b7ef50fa3027`.

Live 800×480 raster captures show the flyby HUD and readable Settings controls.
Active duel samples report 60.0–60.1 FPS with zero kiosk restarts. The paused
ready-screen sample is excluded from that range. The saved setup is a fresh,
paused `spacewars-terrain-combat` round, seed 42, P1 human versus P2 bot, with
**8s interval / 4s duration**. Press Start to resume; return to Launcher → Settings
to compare Off, 8s, 15s or 30s and change the duration.
