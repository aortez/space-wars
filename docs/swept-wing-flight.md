# Material flight and the first swept-wing circuit

`spacewars-terrain` now has physical swept-wing flight for either human pilot.
`spacewars-terrain-ai` runs the same controls through `rule_pilot_v2` in P2:
settle on the ground → take off → fly one fast circuit → open and brake →
return → land → exit → claim → board → depart and hold. The controlled material
planet is still the integration fixture. Ordinary Spacewars and its historical
V5/V6/V7 controllers retain their existing behavior.

## Controls and tuning

Hold the right shoulder (RB/R) to sweep the wings and apply cruise thrust.
Release it to reopen; hold Down/S to brake. Braking suppresses automatic cruise
thrust, while A/Space remains an independent thrust button. P1 keyboard wings
are J; P2 uses PageDown aboard, with numpad 5 for brake and 8 for thrust.
PageDown is also P2's existing mining-size key on foot. Neutral input, including
wing release, is required after a transfer or vehicle replacement.

| Property | Open | Fully swept |
| --- | ---: | ---: |
| Thrust acceleration | 45 units/s² | 90 units/s² |
| Forward cruise target | 70 units/s | 140 units/s |
| Maximum commanded turn rate | 1.8 rad/s | 0.7 rad/s |
| Turn acceleration | 6 rad/s² | 3 rad/s² |
| Brake acceleration limit | 40 units/s² | 40 units/s² |

A transition takes 0.45 seconds and interpolates the controls. Engines taper
within 10 units/s of the forward cruise target, relative to the planet frame.
This is not a hard velocity cap: opening retains momentum, as do impacts and
gravity. At 140 units/s the ideal full-brake distance is 245 units, before
allowing for gravity or steering. Reopen and brake early. The HUD reports
actual wing position, surface-relative speed, health and landing phase.
Landing assist fades with the sweep, and a settled landing requires open wings.

Wing animation replaces only the hull collider on the existing physical body.
Rear-foot colliders, body identity, pose, spin and origin-point velocity survive
changes to the hull's center of mass. The body retains its mass and continuous
collision detection. Shape changes mark queries dirty until the normal shared
physics step completes. No additional physics or gravity steps are introduced.

## Policy and observations

`PilotObservationV2` adds measured wing state, control limits, relative speed
and ideal braking distance around the unchanged V1 observation. `RulePilotV2`
is separate from `RulePilotV1`; the original approach tests and
`surface_pilot_soak` runner remain available. V2 guides the high flight with
ordinary steering, brakes and binary thrust. Its target circuit is about 200
units above the nominal surface at 90 units/s. It integrates actual angular
travel in the planet's frame and requires a full revolution plus sustained
fast swept flight. The direction can be reversed in the evaluator.

After opening and braking, the bot descends upright under gravity and hands
the local approach to V1 at 65 units of radial altitude. V1 surveys surviving
material, uses the shared landing/transfer gates and performs the claim.
The host does not move bodies or grant AI-specific contacts or ownership.
P2 hardware is excluded while the bot owns that seat; pause does not advance
the policy and restart resets the mission. The HUD shows each flight and
landing goal, and bounded flight failures are reported explicitly.

This is a controlled circuit, not general terrain obstacle avoidance or
combat navigation. Enemy flag routes, useful mining, damage response and
vehicle recovery remain future AI work. V2 stops with a blocked reason if its
vehicle is lost or the local landing policy blocks; restart begins a new run.
Generated/multiple-planet missions, weapons and random asteroid strikes are
subsequent integration slices. Merging remains deferred.

## Verification

Run the interactive policy without rendering, up to 180 simulated seconds:

```sh
cargo run --locked --release -p spacewars-ai --example surface_flight_soak -- \
  --seconds 180 --case 0 --seat 1 --players 2 --seed 42 --out /tmp/flight-case0
```

Case 0 flies counterclockwise and case 1 clockwise. The eight-run matrix uses
both directions, both seats and seeds 7/42. Seeds vary the deterministic
simulation; this fixture does not generate different planetary layouts.
Reports include per-second terrain conservation/finite-motion audits, actual
wing state, speed and circuit progress, flight and landing milestones, and
separate sensor/policy and simulation-step timings. `max_goal_ticks` measures
the longest phase residence, not time without movement.

Desktop: all eight 180-second runs completed in 84.95–94.97 seconds, with no
landing retries or blocked ticks. Step P95 was 0.0482–0.0508 ms; sensor/policy
P95 was 0.0021–0.0029 ms. These are one-planet flights without fragmentation
load, not a replacement for the terrain endurance stress cases.

Tests exercise actual circumnavigation, both seats/directions, landing and
capture ordering, continued departure, clone/replay/reset, physical turn and
speed differences, momentum through reopening, braking, in-place shape/mass
updates, cruise-speed material collisions, and wing-held transfer/loss gates.
The existing V1 and recovery regressions also pass.

Raspberry Pi 5: all eight 180-second cases also completed in 84.95–94.97
seconds, with zero landing retries and blocked ticks. With the kiosk running,
step P95 was 0.1375–0.1492 ms (worst step 0.4419 ms); sensor/policy P95 was
0.0062–0.0086 ms. The sixteen desktop/Pi runs cover 48 simulated minutes and
2,880 successful per-second audits. The ARM runner was compiled with Rust
1.94.1 against the Pi-compatible libc; desktop evaluation used Rust 1.89.

The workspace/all-target run passed 981 tests, followed by two additional
cruise-impact and transfer/loss tests. Final focused flight and host tests
cover the subsequent exhaust/HUD changes. All 18 real-window UI workflows
passed. Both frozen navigation-v1 (6 episodes) and strategy-v1 (12 episodes)
match exactly. Formatting passes; Clippy completes with existing warnings
outside the new flight code.

Raw reports, runners, logs and deployment artifacts:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/swept-wing-flight-20260908/`.
