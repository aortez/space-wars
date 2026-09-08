# Material damage and reusable ship recovery

For the subsequent progress-based stabilization change, repeated asteroid and
missile trials, and current pod/flag limitations, see
[Pod recovery after severe impacts](pod-impact-recovery.md).

`spacewars-terrain-recovery` adds a controlled collision and recovery journey:
P2 flies the V2 swept-wing circuit, lands, claims and departs; the host sends one
heavy asteroid into the ship; `rule_pilot_v3` then lands the escape pod, exits,
rebuilds, boards and flies again. P1 remains human. Restart repeats the journey.

The ordinary two-human `spacewars-terrain` scene also has impact controls:

| Strike | Gamepad, either seat | P1 keyboard | P2 keyboard |
| --- | --- | --- | --- |
| Light | X / west face | K | Home |
| Heavy | RB + X | J + K | PageDown + Home |

One press spawns one asteroid, with a three-second cooldown. It targets the
assigned full ship, including an empty parked ship while its pilot is outside.
Release controls after transferring or losing/replacing a vehicle. The HUD
briefly shows damage or ship loss. A heavy hit normally destroys a healthy
ship; occupied ships eject their pilot in a pod, while empty ships leave the
existing spaceling alive. Pods and spacelings remain invulnerable in this lab.

## Shared mechanics and task contract

The strike fixture spawns a physical radius-two asteroid, 40 units from the
ship, with relative speed 25 (light) or 160 (heavy). Its damage scalar is 1.0;
ordinary Spacewars retains its existing 0.01 asteroid scalar. This explicit
lab tuning exercises damage and loss at surface-flight speeds. No direct
health write, forced pod spawn, position reset or extra physics/gravity solve
is involved. The angled endurance case rotates the incoming path by 0.35 rad.
Reported damage comes from the actual health/loss change after collision.

`RecoverShipTask` lives in `spacewars-ai`. Its caller supplies a versioned
observation and receives ordinary flight/spaceling controls plus
`Running`, `Blocked` or `Succeeded` telemetry. Success means the pilot is
aboard its assigned living full ship. The caller chooses the following mission.
The task also works when invoked on an already stranded spaceling, without
running the flight demo first.

The task stabilizes an ejected pod, surveys and revalidates actual material
footing, approaches at a bounded speed, aligns with the measured local normal,
and uses the ordinary landing and transfer gates. Pod surveys check its smaller
feet/hull/hatch and prefer flatter ground for the subsequent walk to a ship.
On foot it can get up, claim neutral ground, wait through rebuilding, move to
find build clearance, and walk/jump toward the replacement hatch. Flag or
support loss interrupts the shared rebuild timer; the task must earn that
progress again. It never writes ownership or creates a replacement itself.

Attempts have limits: four landing retries, four local build-space moves,
15 seconds without progress toward a replacement hatch, and a two-minute
overall task budget. A blocked task stays blocked until its caller explicitly
resets or replaces it. It does not silently restart its deadline every tick.

`RecoveryTaskObservationV1` adds pod-compatible sites around the unchanged
V1/V2 observations. Sensors do not flush dirty queries or advance physics.
Historical `rule_pilot_v1`, `rule_pilot_v2` and the ordinary match policies
retain their contracts. The interactive recovery host and the headless runner
use the same V3 policy. The host schedules the demonstration's one asteroid;
hazard creation is outside the AI task. P2 hardware is excluded, pause advances
neither the policy nor hazard schedule, and restart resets both.

## Reproduction and limits

```sh
cargo run --locked --release -p spacewars-ai --example surface_recovery_soak -- \
  --seconds 180 --seed 42 --seat 1 --oblique false --out /tmp/recovery-case
```

Use `--oblique true` for the angled strike. `--disruption flag`, `support` or
`site` removes the original flag footing during rebuilding, the spaceling's
current footing during rebuilding, or the pod's selected approach site.
These bounded queued terrain edits belong to the acceptance driver, not the
brain. The runner records the edit tick, interruption/neutralization effects,
actual damage and recovery counts, goals, sites, per-second material audits,
and separate sensor/policy and simulation timings.

The ordinary and flag-loss cases require departure within 180 seconds. Site
and support disruption require either completion or an explicit terminal
failure; reports retain `complete: false` for blocked outcomes. Removing the
spaceling's ground can leave it in a crater below the rebuilt ship. The current
walking/jump task can report an inaccessible hatch; it cannot plan a mining
escape or arbitrary route out of a cavern. That is a known remaining AI task,
not counted as successful recovery. Enemy flag routing, combat decisions,
random hazard pressure and generated multi-planet missions remain later work.

## Validation

Workspace tests passed (982), followed by two additional focused sensor/query
tests. Tests cover light/heavy physical contacts against flying and parked
ships; occupied/empty loss; edge/cooldown/release gates; standalone recovery;
flag/support interruption; both seats and strike angles; clone/replay/reset;
dirty-query/site invalidation; and the interactive host's P2 ownership and
single-strike schedule.

Detailed duration, UI, baseline and Pi results are recorded alongside this
checkpoint in:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/material-recovery-v3-20260908/`.

Desktop: eight core 180-second runs (both seats, straight/angled strikes, seeds
7/42) all completed in 136.47–176.10 seconds, with no landing retries or blocked
ticks. The additional flag and landing-site removals recovered in 141.90 and
147.40 seconds; removing current spaceling support produced a terminal
inaccessible-hatch result after interruption and rebuilding. All 1,980
per-second audits passed. Core simulation step P95 was 0.0504–0.0548 ms and
sensor/policy P95 was 0.0027–0.0039 ms. This controlled one-planet load includes
ship breakup but does not represent heavy terrain fragmentation or a match.

The Pi 5 repeated all eleven 180-second cases with the kiosk running. All eight
core recoveries completed; flag and selected-site removal recovered, and the
current-support crater produced the same explicit inaccessible-hatch result.
Together the desktop/Pi matrices cover 66 simulated minutes and 3,960 passing
per-second material/finite-motion audits. Seeds 7/42 vary the deterministic
simulation, not the controlled planet layout.

All 19 real-window UI workflows passed. Both frozen ordinary-game suites
(`navigation-v1`, six episodes; `strategy-v1`, twelve episodes) match exactly.
Rust 1.89 desktop and Rust 1.94.1 ARM builds pass; workspace Clippy completes
with pre-existing warnings, and the new AI code is clean. Formatting passes.

Pi core step P95 was 0.1439–0.1623 ms and sensor/policy P95 was
0.0099–0.0140 ms; the worst measured step across its eleven runs was 0.6142 ms.

## Deployed checkpoint

Gameplay source `7d9f0289426832dce44778baa3c145dfc9d7cf7e` was built with the
accepted Yocto layer pins and installed on 2026-09-08 UTC through the A/B
updater. All 6,608 build tasks succeeded (21 rerun). The Pi booted slot B
(`/dev/sda3`), and its installed client hash matches the archived image:
`e594101bc4f87792c55dd5cdcd8190beaaa1e20c0c4897fa4fe99fb489cfc043`.
The compressed image SHA-256 is
`d4d31c53e83fef0c8ab3bc81d99f212ef48ab10b87f2a44c0b69c4f118d2ab7a`.

The live 800×480 kiosk completed the journey: circuit and claim, asteroid
ship loss, pod stabilization and landing, exit, rebuilding, boarding and
continued flight. Captures at 87, 100, 116 and 145 seconds show those phases;
the last reads `Planet 0: P2` and `AI: recovered / flying again`. Ownership was
won before the strike and retained because the flag survived. The live run
held about 60 FPS with zero kiosk restarts.

Screenshots, guarded UI history, installed-image verification and the complete
headless reports are in the artifact directory above. The final playtest setup
is a fresh, paused two-human `spacewars-terrain` session. Select
`spacewars-terrain-recovery` to repeat the bot demonstration. Merging remains
deferred; crater escape/mining routes and combat are still future integration
work.
