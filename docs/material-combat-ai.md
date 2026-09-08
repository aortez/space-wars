# Material combat V4

Select `spacewars-terrain-combat` to fly P1 against a combat bot, or
`spacewars-terrain-duel` to watch two bots. Both start with healthy ships above
the same destructible planet. These are the next controlled integration slice;
ordinary Spacewars and the earlier flight/recovery presets remain available.

| Action | Controller | P1 keyboard |
| --- | --- | --- |
| Turn / thrust / brake | Left stick or D-pad / A / Down | A/D or arrows / Space / S |
| Swept-wing cruise | Hold RB; release to open | J |
| Forward laser aboard | RT or LB | E |
| Cannon aboard | X, the west face button | K |
| Exit / board | B | X |
| Mine on foot / change size | Right stick aims; RT or LB / Y | Arrows aim; E / T |

The combat presets disable the controlled asteroid-spawn input. Transfer and
ship replacement still require releasing controls, now including weapon buttons.
Pods and spacelings retain the laboratory's invulnerability. The aerial bot
targets only living, occupied full ships and patrols while its opponent recovers.
Humans can still damage empty ships with weapons.

## Shared implementation

`SurfaceWeaponAction` is an opt-in, seat-scoped action. Laser tracing, shell
spawning/cooldown, contact damage, breakup and pod ejection run in the existing
Spacewars pipeline, with one shared physics step and gravity solve. Cannon shells
damage material through the existing queued terrain-edit boundary. Lasers stop
at the first solid, including material and detached fragments; they damage ships
and debris but do not excavate terrain. The on-foot mining beam remains separate.

The material control envelope uses 70/140-unit cruise speeds, so this preset uses
8 units/s of recoil instead of the ordinary game's 200. Projectile speed, damage,
health and cooldown keep their shared values. No damage, kill, claim or successful
recovery is scripted in the duel.

`rule_pilot_v4` reuses the ordinary `rule_ship_v5` combat solution: projectile lead,
pursuit decision and firing windows. Its tuned ranges and physical steering fit
material flight. It climbs with braking/turning clearance before aiming inward,
and routes around ground that blocks its target. Passing projectiles can inhibit
fire without forcing a new route. The existing V5 caller retains its original
configuration and arithmetic for the frozen historical suites.

`CombatObservationV1` adds actual body motion, weapon readiness, first-solid
visibility, and hit counters to the recovery observation. Queries are read-only
and fail closed while material geometry is dirty. Contact counters are sampled
before shared debris cleanup compacts its indices.

Ship loss delegates to `RecoverShipTask`, as in V3. A pod knocked onto a slope
now lifts clear before beginning its approach, so ground friction cannot pin
its stabilization turn. Recovery ends after ordinary boarding; V4 then climbs
and resumes its combat mission. `combat_returns` counts the first firing command
after each completed recovery. Subsequent losses retain that milestone.

## Test bed

```sh
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --seconds 180 --mirror false --separation 0.5 --out /tmp/combat
```

The two actual policies run against each other for up to three minutes. Initial
bearings can be mirrored and separated by 0.3–1.5 radians to vary geometry. The
runner saves per-second observations, material audits, goal transitions, actual
weapon contacts, recovery milestones, and separate simulation/AI timing.
Seeds vary deterministic simulation details, not the controlled planet layout.

Treat finite/material audits, combat contacts, and completed recovery/return as
separate results. A run can finish during a later recovery. A blocked task is
reported explicitly and must not be counted as a successful recovery. The
regression test requires a physical duel to lose a ship, rebuild and fire again
within 180 seconds, without directly editing health or ownership.

Enemy flag navigation and escape from arbitrary craters remain limited. Only one
planet can be owned here; a pilot stranded on enemy-owned ground still needs a
future flag-route task. This slice does not yet add random asteroid pressure,
generated multi-planet matches, or tactical evasive maneuvers.

## Validation, 2026-09-08

The workspace all-target run passed 997 tests. The subsequently added physical
duel regression also passed, and the final client passed all 233 unit tests.
All 21 real-window workflows passed with both rendering backends. The frozen
navigation-v1 (six episodes) and strategy-v1 (twelve episodes) suites match
exactly. Formatting and Clippy complete; existing workspace warnings remain,
with no warnings in the new combat code. Desktop uses Rust 1.89.0; the ARM
headless runner uses Rust 1.94.1.

The matrix uses seeds 7/42, both mirrored seat placements, and initial bearing
separations 0.5/0.8 on desktop and Pi 5: sixteen 180-second duels, 48 simulated
minutes, and 2,880 passing per-second material/finite-motion audits. All runs
produced actual weapon hits. Seven of eight runs on each platform completed at
least one recovery followed by firing again. Across both platforms there were
17 completed recoveries and 14 returns to firing; three recoveries completed
without a subsequent firing opportunity before the run ended or another loss.

Two runs never completed their first recovery: desktop seed 42, mirrored 0.8
timed out on difficult ground; Pi seed 42, mirrored 0.5 could not stabilize its
pod. Later recovery attempts also encountered pod stabilization and enemy-owned
ground. These are recorded blocked tasks, not successful recoveries. The other
unfinished tasks at the three-minute boundary remain marked running. Collision
cascades differ across architectures; milestone coverage and physical audits
are compared instead of demanding identical duel traces.

| Platform | Step P95 range | Worst step | Sensors + both policies P95 range |
| --- | --- | --- | --- |
| Desktop | 0.0696–0.1142 ms | 0.3175 ms | 0.0071–0.0488 ms |
| Pi 5, kiosk running | 0.1794–0.3113 ms | 0.8852 ms | 0.0344–0.1682 ms |

These are controlled one-planet dogfights with ship breakup and occasional
cannon craters, not heavy fragmentation capacity or generated-match results.

The eight prior V3 core asteroid/recovery cases were also rerun on the Pi after
the pod clearance change. All eight completed within 180 seconds with zero
blocked ticks. Together with the combat matrix, the explicit duration runs
cover 72 simulated minutes and 4,320 passing per-second audits.

Validation artifacts are stored in
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/material-combat-v4-20260908/`.

## Pi deployment and live check

Gameplay checkpoint `6da67e0cf33a009cc5b8cc839fdda893d8c199fe` was built
with the accepted Yocto layer pins and deployed through the A/B updater.
All 6,608 build tasks succeeded (21 rerun). The Pi boots slot A (`/dev/sda2`).
The installed client SHA-256 matches the binary extracted from the archived
image: `8fc3285a90f108b3b499a36d9ef47973efdf32633ce1b9c9e6015026402557db`.
The compressed image SHA-256 is
`9466ccf6d6e479184537ae7e4b342c058beb2298fe9eb8f271a557c1e8c22b19`.

An additional headless Pi run with its formerly saved seed 0 reached a blocked
replacement hatch after landing, claiming and rebuilding. That failure is
retained in `pi-live-seed0.json`; it is additional to the matrix above. The Pi's
playtest seed is now 42. The seed affects collision cascades and this is not a
claim that recovery succeeds from every fight or landing position.

The live 800×480 raster kiosk on seed 42 completed a weapon-loss recovery:
P1 landed its pod, captured the planet, rebuilt, boarded and resumed firing.
The 60-second capture shows rebuilding at 60%; the 90-second capture shows
P1 firing again with its surface flag and ownership retained. These are saved
as `pi-p1-rebuilding-60s.png` and `pi-p1-firing-again-90s.png`. The live run held
about 60 FPS with no kiosk restarts. Its collision sequence differs from the
headless compiler build, while exercising the same gameplay milestones.

The final playtest setup is a fresh, paused `spacewars-terrain-combat` session:
P1 human, P2 AI, seed 42. Resume with Start. Select `spacewars-terrain-duel` to
watch both bots. The guarded UI state, settings backup, screenshots and device
hash verification are retained with the report. The branch remains unmerged.
