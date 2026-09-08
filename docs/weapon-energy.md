# Ship energy and mounted rounds

Enabled in `spacewars-terrain-combat` (P1 human versus P2 bot) and
`spacewars-terrain-duel` (two bots). Ordinary matches and the earlier flight,
mining and recovery presets retain their existing weapon rules.

| Resource | Initial tuning |
| --- | --- |
| Ship energy | 100%, regenerates 10 percentage points per second |
| Laser | Draws 12 points per second while emitting; net drain 2/s |
| Loaded ammunition | Two rounds, one on each fixed rail beside the wing roots |
| Launch interval | 0.5 seconds between already loaded rounds |
| Automatic reload | 25 points paid at the start, two seconds per round, one at a time |
| Depleted laser | Waits until 10% charge; releasing the trigger cannot skip this wait |

Launches spend loaded ammunition, so an empty battery does not disable a loaded
round. Reloading starts when an empty slot and 25% energy are available. Once
paid, the two-second loading operation finishes even if the laser drains the
remaining charge. The battery recharges while parked as well as while flying.
Thrust, mining and ship recovery do not consume this weapon supply.

Loaded rounds have pale bodies and amber noses. Each launches from its actual
rail, leaving that rail empty. A dim replacement feeds forward along the rail
and becomes bright when ready. Rails stay aligned with the hull while the wings
sweep. Launched rounds use the shared unguided shell collision, speed and damage;
the new missile silhouette follows velocity. Attached rounds are ship rendering,
not additional rigid bodies or independently damageable components.

The HUD shows energy, marks each quarter of the battery, and reports reload time
or laser charging. RT/LB (keyboard E) fires the laser; X/west (keyboard K) launches
a round. Holding both allows laser fire between actual launches. A launch
suppresses the beam for that tick and does not charge for an un-emitted laser.

## Implementation boundary

`ShipState` owns an optional `weapons::ShipArmament`. Both human and AI actions
pass through the shared projectile/laser step, which advances the supply once
per simulation tick. Rendering and observations cannot advance recharge or
reloads. Pause and cloning retain in-progress operations. Escape pods discard
the lost armament; reconstructed ships receive a full battery and two loaded
rounds, behind the existing transfer/control-release gate.

`CombatObservationV2` exposes the supply and actual weapon readiness. V4 uses
that readiness with its existing aiming, pursuit and recovery policy. This
change does not add reaction delay, aim error, evasive maneuvers or a new AI
resource-management strategy. Ordinary V5 navigation/strategy remain frozen.

The landing-pressure run also exposed a legacy ejection kick being added after
the fatal contact impulse had already been resolved. Combat pods now inherit
that resolved velocity without the additional damage-derived kick. The failing
run had launched its pod at 1,096 units/s; all four repeated pressure runs stay
below the existing 500-unit/s audit threshold. This correction is opt-in with
combat. The earlier controlled asteroid/recovery fixtures retain their ejection
behavior pending a coordinated landing-policy migration.

## Test bed and results, 2026-09-08

```sh
# Two combat policies, up to three minutes.
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --seconds 180 --mirror false --separation 0.5 --frames true --out /tmp/duel

# After 12 seconds of combat, P1 releases weapons/swept wings and uses the
# existing landing policy; P2 continues fighting with its ordinary policy.
cargo run --locked --release -p spacewars-ai --example surface_combat_soak -- \
  --seed 42 --seconds 180 --land-after 12 --frames true --out /tmp/landing-pressure
```

Reports include supply samples, physical hits and recovery milestones. The
landing variant records the state at handoff, touchdown, exit and loss. It is a
pressure measurement, not an evasive landing policy. Selected full-view and
close-up draw lists can be exported as JSON. A failed audit saves the partial
report and a final draw list, reports `completed_seconds` and `failure`, and
returns a nonzero exit status; an aborted run is not a completed three-minute run.

The initial workspace all-target run passed 1,005 tests. Focused coverage includes
time-based drain, serialized paid reloads, both rails at multiple headings and
wing sweeps, zero-energy launches, depletion/recharge, seat isolation, pause and
clone/replay, actual shared laser/projectile hits, and three minutes of held fire
bounded by the battery budget. A subsequent regression checks that combat pods
retain the resolved impact velocity and discard the destroyed weapon supply.
Both ordinary AI baselines match (six navigation
episodes and twelve strategy episodes).

All eight desktop duels (seeds 7/42, mirrored seats, separations 0.5/0.8) completed
180 seconds with passing material, finite-motion and energy audits. Seven
completed at least one recovery and returned to firing (nine completed recoveries,
eight returns in total). Seed 7, mirrored 0.8, could not stabilize its pod. Later
losses still encounter enemy flag routes and incomplete recoveries. These remain
recorded limitations. The physical seed-42 loss/rebuild/fire regression retains
its original assertions and checks that the replacement has the new supply.

All four landing-pressure runs (seeds 7/42 and both seat arrangements) now finish
180 seconds with passing audits. P1 is destroyed before touchdown in all four,
at approximately 18.4–31.5 seconds, after beginning its landing attempt at 12s.
The opponents remain active; no safe-landing or survival result is implied.
Weapon pacing alone does not establish that landing under fire is balanced.

Artifacts: `/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/weapon-energy-20260908/`.
