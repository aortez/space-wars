# Claim footing recovery

This follows [ground get-up recovery](ground-getup-recovery.md). The shared
capture/recovery ground task is now `ground_navigation_v7`.

## Recorded failure and behavior

The Pi seed-42/P1 mirrored duel with 3-second Mixed asteroid arrivals reaches
an enemy flag, lowers it, and starts raising a replacement. An asteroid edits
planet 0 at tick 9343 (155.72 seconds). After another interrupted raise, the
spaceling has balanced contact with retained material but its precise contact
point supplies no occupied anchor cell. The old bot interprets an absent flag
as arrival and remains on a neutral planet through the 180-second cutoff.

A supported bot now waits half a second to distinguish transient contact loss
from unusable claim footing. At the existing half-second ground survey cadence,
an additive sensor offers at most sixteen nearby standing positions, between
1.25 and 8 units away. They use the existing retained-material ray and standing
capsule clearance measurements, plus the same cell inset/occupancy check as a
real flag anchor. If a flag still exists, candidate anchors remain within its
interaction range. Detached debris supplies neither the floor nor eligibility.
The survey is read-only and unavailable while physics queries are dirty.

The AI chooses a measured walk/jump route of at most twelve units. Ordinary
inputs move the spaceling; reaching a proposed point only begins a settling
wait. Actual ready, raising, lowering, contested or secured claim status ends
relocation. The authoritative human support, balance, relative-speed, flag
range, ownership and three-second claim rules are unchanged.

A proposal loses eligibility on support loss or terrain revision changes. The
bot can try at most four distinct nearby proposals within twelve seconds;
it waits up to two seconds at each endpoint for real eligibility. The existing
ninety-second ground-task deadline also remains in force. Failure is reported
to the containing capture/recovery task, without teleportation, terrain edits,
free claims or a restarted mission deadline. Telemetry records relocation
attempts and the currently proposed local standing center.

## Reproduction and acceptance

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --seed 42 --seat 0 --mirror true --mode duel --asteroid-interval 3 \
  --seconds 180 --frames true --trace true --require-claim-recovery true \
  --out /tmp/claim-footing-recovery
```

The extra assertion requires the designated subject to attempt relocation and
subsequently finish a real claim. Reports retain the relocation and completed
claim ticks separately from ordinary capture-sortie departure counts. Physics
and gameplay success remain separate checks. This saved failure is Pi-specific;
its desktop trajectory is different and need not trigger that assertion.

The development Pi replay preserves all per-second motion hashes through second
157. It selects new footing at 157.52 seconds, resumes raising at 158.48, survives
another brief interruption and completes its flag at 162.95. The old run still
has no flag at 180 seconds. This proves recovery of the claim; the bot is still
returning to its ship at cutoff, so it does not establish a second completed
capture-and-departure sortie.

Artifacts, scripts, traces, frame JSON and build manifests:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/claim-footing-recovery-20260909/
```

## Testing as development proceeds

1. Save and replay a specific physical failure on the affected platform; compare
   the unchanged lead-in and the actual completed objective afterward.
2. Check decision contracts with synthetic observations: transient waits, fresh
   support, candidate identity/geometry, dirty queries, destruction, replay and
   clone behavior, actual claim progress, contests, retry limits and deadlines.
3. Exercise ordinary physics through the existing landing, mining, claim,
   jetpack, impact and ship-recovery fixtures, then run three-minute missions.
4. Keep physical audits (finite motion, bounded speeds, material accounting)
   separate from outcomes (claims, both departures, recovery and blocked tasks).
5. Verify the full release suite and frozen ordinary-game navigation/strategy
   baselines, then deploy one identified build and check live Pi behavior.

## Validation at `92c72fa`

The release workspace suite passed 1,074 tests, with zero failures and 24
existing ignored tests. The example-driver suites passed eight more: **1,082
passed tests** in total. Five new tests cover the claim-footing sensor and
relocation contracts. Formatting passed; Clippy completed with advisory warnings,
including two style suggestions in the new code. The frozen ordinary-game
baselines matched all six `navigation-v1` and twelve `strategy-v1` episodes.

Both platforms ran the same 52 missions and 36 impact/recovery trials, each for
three simulated minutes: **176 runs / 8.8 simulated hours**. All physical audits
passed, and all 72 dedicated impact trials completed recovery and departure.

| Mission outcome | Desktop before → after | Pi before → after |
| --- | ---: | ---: |
| Quiet: capture and depart both planets | 12/12 → 12/12 | 12/12 → 12/12 |
| Deliberate ship loss: recover a replacement | 8/8 → 8/8 | 8/8 → 8/8 |
| Deliberate loss: also finish both capture sorties | 5/8 → 5/8 | 6/8 → 6/8 |
| Combat/asteroids: finish both capture sorties | 17/32 → 17/32 | 15/32 → 15/32 |
| Completed recovery events during combat/asteroids | 9 → 9 | 7 → 7 |
| Final blocked mission subjects | 1 → 1 | 0 → 0 |

Reports designate one subject seat; duel worlds appear once per subject and
retain both brains. Four report trajectories change on each platform (two duel
worlds per platform). The additional desktop diagnostic replays show small
ground-steering differences around flag interaction, with no changed completion
counts. The existing desktop block remains seed 7/P1, mirrored duel with
3-second arrivals: the assigned ship has no grounded hatch.

The dedicated final Pi regression uses the immutable final runner and enables
`--require-claim-recovery true`. It matches all 180 motion hashes from the final
matrix run. It preserves the old lead-in through second 157, selects footing at
tick 9450 (157.50 seconds), and completes the real claim at tick 9777 (162.95).
The old world ends neutral with no flag; the new world ends owned by P1 with a
fully raised flag. The coarse both-departures metric does not increase because
returning to the ship still extends beyond the cutoff. This is why reports now
retain completed claim-footing recovery independently of departure counts.

On the Pi, the largest per-case p95 measurements were 0.305 ms for sensors,
0.020 ms for policy and 0.424 ms for physics. Recorded maxima were 17.264,
2.116 and 11.416 ms respectively. These headless timings are separate from live
rendering; desktop jobs also overlapped the build and are not a controlled
performance comparison. The Pi gameplay host was paused during the final batch.

The source was committed before the final runner and Yocto builds. The image
and runners are archived with source identity and hashes. Further work remains
on long ground routes and finding a recoverable ship/hatch after displacement.
This slice does not establish general cave navigation or generated-world AI.

## Deployment and live playtest

Yocto completed all 6,608 tasks (21 rerun) from source
`92c72fafe363ab64562a0859c4c9e3fd54578343`. The archived image was installed on
`spacewars.local`, which booted slot B (`/dev/sda3`). The installed client hash
matches the extracted image binary:

```text
image SHA256:  918e16345ad740025465f801b273997ae23d4a6aa53ac3313779bb106621fcf2
client SHA256: cd2d8d0f405082c5fccc550ec10c6f402e50b86c438b2d5285d9a89033c333cc
Pi runner:    e43e070cdec919559f4bf573fe0aad58ebfc225bbfe84941e8bf6c60fe5c14ee
```

A live two-planet duel ran for three wall-clock minutes with 3-second Mixed
arrivals and raster scale 2. The 30/75/120/180-second captures recorded
58.6/59.0/57.5/59.1 FPS, with zero kiosk restarts. Both bots captured their first
planets and travelled onward. At 180 seconds, P1 was landing from a jetpack
crossing to resume its ground route; P2 patrolled with both planets owned.
This live round verifies the installed combined loop and rendering. The exact
mirrored flag-footing regression is established separately by the Pi runner.

The Pi was then returned to a fresh, paused `spacewars-terrain-travel` round:
P1 human, P2 mission bot, 8-second Mixed arrivals. Start or B resumes. Screenshots,
UI state/history, actual capture times and service diagnostics are archived
beside the headless reports. This results update is documentation only; the
installed implementation remains `92c72fa`.
