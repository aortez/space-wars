# Bot jetpack crossing trial

`spacewars-terrain-jetpack` is the first bot-driven test of powered spaceling
movement. P2 exits its parked ship, crosses over the hull to the left, lands and
claims, recharges, crosses back to the right, and boards through the usual hatch.
Directions are relative to local gravity. P1 is human; keep P1 aboard to watch
the neutral-planet trial without contesting its claim. Restart repeats the trial.

Both pilots have the same equipment and controls in this preset. Tap A/Space to
jump or get up; hold while airborne for lift, with ordinary left/right steering.
The two jet nozzles and flames show when lift is active, and the HUD shows charge.
Other material presets and historical movement fixtures do not enable this
equipment yet. The existing combat bot's blocked flag route is still a separate
integration task; this trial proves the crossing maneuver before adding it to
general ground navigation.

## Shared movement and measured flight

The optional jetpack belongs to the existing spaceling assembly. It applies
bounded velocity impulses before the existing shared physics/gravity step; it
never moves a body by setting its pose or advances an extra simulation. Lift
consumes charge in proportion to the applied impulse. There are three seconds
of full-thrust-equivalent charge at 40 units/s²; lift tapers at 9 units/s of
ascent relative to the launch support velocity. External impacts are not clipped.
Equipped airborne steering targets up to 10 units/s with bounded acceleration.

A tapped jump keeps its normal height. Holding a get-up request cannot turn
into unintended jetpack thrust. Empty charge cannot generate lift; holding jump
through landing does not recharge and relaunch. Four stationary, balanced seconds
on physical support with jump released recharge an empty pack. Boarding preserves
the remaining charge for the next exit. Planet capture and boarding retain their
own stricter material-ground requirements.

The scenario measures the real parked ship's hull outline, retained takeoff and
landing footing, and an inflated capsule corridor above the hull. Surveys are
bounded and staggered at 2 Hz. They query the completed world, excluding only the
observing spaceling; the ship, other actors and debris remain solid obstacles.
A roof or blocked landing spot can reject the corridor even with a full pack.
The first planner handles the assigned full ship and a short outer-surface
crossing; it is not a cave or arbitrary-obstacle flight planner.

`jetpack_crossing_v1` consumes those observations and emits normal movement,
jump/hold and transfer actions. It controls lift, crossing and descent from
measured position and velocity. Ground contact ends landing. Terrain revisions,
ship movement, missing corridors and a ninety-second mission deadline bound
failure. Ship orientation comparisons account for angle wrapping. The controller
has no world-mutation access, free refills or AI-only boarding permission.

## Validation

The final matrix runs twelve starts for 180 simulated seconds on each platform:
seeds 7/42, both seats, and parked or airborne starts at ±0.4 radians from each
seat's initial bearing. Every run continues through the full three minutes,
including after success or a recorded failure. Initial airborne altitude is
15 units; these starts also exercise settling and exit eligibility.

| Result | Desktop | Pi |
| --- | ---: | ---: |
| Complete both crossings, claim and board | 10/12 | 10/12 |
| Blocked before exiting the ship | 2/12 | 2/12 |
| Physics/material audits pass | 12/12 | 12/12 |

All starts that successfully exit complete the round trip. The four blocked
runs are P2 at +0.4 radians, for both seeds on both platforms: the ship settles,
but the existing hatch clearance gate stays `exit_blocked`. The trial expires
at ninety seconds without ever launching a spaceling. Those reports retain
failing exit codes and are not counted as crossing successes.

Successful missions finish in 20.9–32.2 simulated seconds; the initial parked
demo takes about 21 seconds. The lowest observed remaining charge is 7.2%.
The largest sensor/policy sample is 0.070 ms desktop / 0.212 ms Pi, and the
largest shared physics step is 0.266 ms / 0.475 ms. These are controlled,
headless single-planet measurements, not full-client frame-time bounds.
One successful start differs by six ticks across architectures; the tests do
not assert cross-platform trajectory identity.

Earlier exploration exposed a hovering descent target and an angle-wrap false
invalidation. Those reports and binaries remain under `*-initial` in the archive.
Focused checks cover fuel depletion/recharge, normal tapped jumps, clone/replay,
observation identity, wrapped headings, terrain invalidation, read-only sensors,
solid roof rejection, physical crossings in both seats and charge persistence
through boarding. The real launcher workflow passes pause/restart and both
renderers under Xvfb.

The final workspace/all-target run passes 1,046 tests with 22 display/hardware
tests ignored by default; the jetpack launcher test was also run explicitly and
passes. Formatting and Clippy complete; Clippy reports existing warnings, with
none in the added jetpack modules. Historical `navigation_v1` (six episodes) and
`strategy_v1` (twelve episodes) both match their checked-in baselines.

Run an individual case:

```sh
cargo build --locked --release -p spacewars-ai --example jetpack_crossing_soak
target/release/examples/jetpack_crossing_soak \
  --seed 42 --seat 0 --seconds 180 --out /tmp/jetpack-trial
```

Use `$CARGO_TARGET_DIR/release/examples/jetpack_crossing_soak` with a shared target
directory. `--bearing` selects an airborne start; omit it for the parked demo.
Each report retains initial settings, per-second observations/audits, phase
transitions, completion/failure, remaining charge and separate timings.

The local archive, including the complete matrix driver and frozen binaries, is:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/jetpack-crossing-20260909/`.
