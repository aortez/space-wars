# Pod recovery after severe impacts

`RecoverShipTask`, shared by the material recovery demo and combat bots, now
keeps stabilizing while observed motion is improving. It can also interrupt
a settled landing approach when another hit creates excessive speed or spin.
The task still uses ordinary turning, braking and thrust. Physical control
limits, collision impulses, damage, transfer gates and gravity are unchanged.

## The reproduced failure

The previous Pi Capture trials had two identical P1 pod failures with seed 7,
mirror off, capture subject P2, subject health 100 or 50, and interceptor
weapons disabled. P2 captured successfully and subsequently fought P1. At tick
4600 P1 lost its ship to laser damage; missiles already in flight then struck
the newly ejected pod. Its spin reached about 113 radians/second. Pod turning
can remove only 6 radians/second each second, so the old 15-second stabilization
deadline expired while the pod was still slowing down.

The preserved pre-change ARM binary reproduces that failure at tick 5502.
Both final updated Pi reruns settle at tick 5814, about 20.2 seconds after ship
loss, land at tick 9251 and exit at tick 9253. They then explicitly stop at
`enemy flag route required`: P2 owns the planet. This is recovery through pod
landing and exit, not a completed rebuild or combat return.

The original 77-second speed alarm remains: the fastest body's center-of-mass
speed reaches 567.14, above the existing 500-unit audit limit. The reports
remain failures and retain the full 180-second continuation. That brief
physical impulse was not removed by a velocity cap, altered damage or pod
immunity. All later per-second speed samples are below the limit.

## Controller contract

Once a second the task compares estimated remaining braking, spin arrest,
heading alignment and clearance work. The estimates use the observation's
control limits and gravity; they are progress measures, not promised arrival
times. A meaningful reduction refreshes the progress timer. Comparing
successive windows allows progress after another impact without requiring
the pod to beat its pre-impact best immediately.

Fifteen seconds without progress still blocks the task. The original
two-minute overall recovery budget stays fixed through renewed stabilization.
Landing retries and build/boarding limits are unchanged. Terminal failures
still require an explicit caller reset.

Settling requires low relative speed, low relative spin, upright alignment
and ground clearance continuously for 0.2 seconds. Briefly rotating through
upright no longer counts as settled. During approach, relative speed over 20
or spin over twice the available turn speed restarts stabilization and clears
the old site. The pod surveys again after settling. Telemetry records attempt
count, start/settled/progress ticks, motion and estimated braking/spin work.

## Extended physical test bed

```sh
cargo build --locked --release -p spacewars-ai --example surface_recovery_soak
python3 tools/run-pod-impact-trials.py \
  --binary target/release/examples/surface_recovery_soak --out /tmp/pod-trials
```

The matrix runs 36 cases for 180 simulated seconds each:

- Eight original circuit → claim → asteroid loss → recover → depart journeys,
  with seeds 7/42, both seats and straight/oblique strikes.
- Four airborne starts at altitude 100, with both seats and strike angles.
  The first heavy asteroid arrives immediately; neutral-ground claiming and
  rebuilding must be earned. This leaves the full recovery budget in the run.
- Twenty-four matching airborne cases with a light asteroid, heavy asteroid
  or missile follow-up, either during initial stabilization or after the pod
  has settled and selected a landing site.

Individual cases accept `--start airborne`, `--followup light|heavy|missile`
and `--followup-phase ejection|approach`. Defaults retain the original sortie.
The matrix driver also supports `--ssh`, repeated `--ssh-option`, and
`--remote-out`; `--binary` then names an already uploaded executable on the Pi.

Follow-ups are diagnostic hazard spawning, outside the AI policy. They use
ordinary radius-two asteroids at relative speed 25/160, or the shared missile
constructor at speed 300 with its normal damage and collision properties.
Pod follow-ups approach from the side, 6 units away for light asteroids and
12 for the faster hazards, to reduce interception by the initial wreckage
trail. Oblique cases rotate that path by 0.35 radians. Full-ship diagnostic
strikes start radially, 40 units away. These are controlled contact tests,
not an estimate of random-hazard frequency or natural encounter difficulty.

Contact telemetry is captured at the shared collision boundary before debris
cleanup changes indices. It records source and projectile spawn tick, including
contacts that impart momentum to a pod without health damage. Every requested
follow-up must actually contact the pod to pass. Interactive impact controls
retain their existing full-ship, release and cooldown restrictions.

Schema-2 reports contain contact events with before/after observations,
per-second observations, task events, terrain audits and separate simulation
and sensor/policy timing. They are written even when the physical audit or
recovery fails. The runner finishes the diagnostic interval, then returns a
nonzero status for missed impacts, audit failures or incomplete recovery.
The matrix retains every report and continues through gameplay failures.

## Results and remaining limits

Desktop and Pi 5 each completed all 36 runs: 216 simulated minutes combined.
All 48 requested follow-ups contacted their pods. All 12,960 per-second terrain,
conservation and motion audits passed in this controlled matrix.

| Cases per platform | Desktop departures | Pi departures |
| --- | ---: | ---: |
| Original full journey | 8/8 | 8/8 |
| Airborne, no follow-up | 4/4 | 4/4 |
| Light asteroid follow-up | 8/8 | 8/8 |
| Heavy asteroid follow-up | 7/8 | 7/8 |
| Missile follow-up | 6/8 | 7/8 |
| Total | 33/36 | 34/36 |

All 24 combined follow-ups during a settled approach re-entered stabilization
and ultimately departed. The five failures occur with early oblique impacts:
P1 heavy asteroid on both platforms, P2 missile on both, and P1 missile on
desktop. Those pods come to rest sideways or nose-down against material.
Their relative speed and spin approach zero while their upright error remains
large (about 88–126 degrees in the desktop traces). The current actuators and
upright-only lift policy do not free them; the landing/exit gate remains
`ship_not_settled`. They stop with an explicit no-progress reason, and are
counted as failures. Waiting longer is not the same remedy as for free spin.
Grounded pod self-righting or an intentional emergency-exit rule is a separate
remaining design/implementation task, alongside enemy-flag navigation.

Original full journeys departed in 136.63–176.22 seconds on both platforms.
Across this matrix Pi simulation-step P95 was 0.1420–0.1690 ms; sensor/policy
P95 was 0.0094–0.1477 ms. These measurements cover a controlled single planet
with breakup debris, not heavily fragmented multi-planet matches.

All 1,024 workspace tests passed. Focused contracts cover extended progressing
spin, actual stalls, settling dwell, renewed impacts without deadline reset,
clone/replay/reset and physical projectile contacts without target writes.
Recovery, combat and duel UI launch/pause/restart workflows passed with both
renderers. Frozen navigation-v1 (6 episodes) and strategy-v1 (12 episodes)
baselines match. Rust 1.89 desktop and Rust 1.94.1 ARM builds pass; Clippy
completes with existing warnings and no warnings in the changed recovery code.

Reports, saved binaries/hashes, original/final Pi reproductions and validation
logs are archived in:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/impact-recovery-20260908/`.
Only `desktop/` and `pi/` are the final matrix; `preliminary/` is exploratory
fixture development and is excluded from these totals.

## Pi deployment

Gameplay checkpoint `20368039193ad81c3ce67159417cf94b09d04b09` was built with
the accepted Yocto layer pins and installed through the A/B updater on
2026-09-08 UTC. All 6,608 tasks succeeded (21 rerun), with the existing host
distribution warning. The Pi booted slot A (`/dev/sda2`). The installed client
matches the binary extracted from the archived image:
`3df791f244b1f8d9948fc7b2cf27b814b9e1b5328813ef9b9eb1f271bb88ccd6`.
The compressed image SHA-256 is
`95531623b0a4c01cb5a5bc48794cd888c2923b3ec4e0222c898a4e3ee4bfcd88`.

The live 800×480 recovery demo completed the circuit, capture, physical ship
loss, pod landing, rebuild, boarding and departure. The 87-second capture
shows pod stabilization; at 130 seconds it is departing in the replacement
ship; the final 178-second capture reads `AI: recovered / flying again` with
P2 still owning the planet. The kiosk stayed around 60 FPS with zero restarts.
Screenshots, actual capture timestamps and status logs are in the archive.

The playtest setup is a fresh, paused `spacewars-terrain-combat` session:
P1 human, P2 Capture, combat-break interval 8 seconds and duration 4 seconds.
Resume with B or Start. The source remains on the integration branch; merging
is still deferred.
