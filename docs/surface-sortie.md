# Surface Sortie

An opt-in first Spacewars integration for spacelings. Choose **surface-sortie**
in the launcher, or run:

```sh
cargo run -p engine-client -- --scenario surface-sortie
```

The fixture starts with your ship settling rear-first onto a slowly rotating planet. Press
**B** on a gamepad or **X** on the keyboard to disembark. Release the controls,
walk around, and return to the cyan access marker. Once standing and settled,
press B/X again to reboard the same ship. Boarding does not heal or replace it.

| Control | Aboard | On foot |
| --- | --- | --- |
| D-pad/left stick left/right; A/D or arrows | Turn | Walk |
| Gamepad A; Space | Thrust | Jump (fresh press) |
| D-pad down; Down/S | Brake | No action |
| Gamepad B; X | Exit landed ship | Board beside its cyan hatch |
| Start/Esc | Pause | Pause |
| R or pause menu | Restart fixture | Restart fixture |

After each successful transfer, all controls must return to neutral before the
new context accepts them. Holding A while exiting cannot become an unintended
jump, and holding it while boarding cannot immediately launch the ship. An
unsuccessful interaction does not repeat until B/X is released and pressed again.

## Flying and landing

Hold A/Space to take off; rotate with left/right. To return, point the nose
**away from the planet** and approach rear-first. Landing assist fades in below
25 world units of foot clearance and inside a 50° nose-outward cone; it reaches
full strength below 8 units and within 15°. It reduces sideways drift and spin
and targets a gentle 2 units/s descent. It does not turn the ship upright,
hover indefinitely, or stop an arbitrarily fast fall. Brake with Down/S before
a fast approach. Assistance releases on thrust so takeoff remains physical.

Both small rear feet must have real planet contact, with angle under 20°,
normal/lateral surface-relative speeds under 2 units/s, and relative spin under
0.2 rad/s, for 0.25 seconds. The HUD then says **LANDED**, the feet turn cyan,
and the hatch marker appears beside the actual ship. Only then can B/X enter
or exit it. Nose/wing impacts and passing through the old pad area do not count.
Hard impacts retain ordinary collision damage. There is no landing latch or
automatic pose correction; the unoccupied ship stays dynamic too.

These are initial feel-tuning values in `surface_sortie/landing.rs`, not changes
to ordinary Spacewars flight. The fixture uses bounded thrust, braking, and turn
rate control in the canonical Rapier world. The landing feet add two colliders
to the existing hull body, not extra bodies, joints, or mass.

The camera follows the active ship or spaceling. A translucent, fixed-scale,
north-up minimap stays visible at the right below the top HUD: cyan planet, red
ship with heading, orange spaceling, and a white camera footprint. Its footprint
uses the actual window aspect ratio, including portrait layouts. The main view
keeps the whole window; both vector and raster backends composite the circular
backing and map as one faded layer.

Orange is balanced, red is
knocked down, yellow is recovering; cyan identifies physical support and surface
access. The HUD shows landing phase, angle, descent and sideways speed, foot
contacts, transfer feedback, ship health, body count, and access distance.
Use raster rendering on software-only backends (including the kiosk).

## Model and boundaries

The pilot has a stable spaceling ID and owning player, separate from its vehicle
ID. While aboard, the pilot is an occupant without a separately simulated body.
Outside, the same creature has one dynamic capsule. The unoccupied ship remains
in the shared world, supported by its physical feet. This is an occupancy transition, not
ship/pod transformation or character death/respawn.

The fixture lives in `scenario-spacewars::surface_sortie` and shares Spacewars'
world, ship physics, gravity solve, and physics step. It adds one gravity target
and one body while on foot. It uses the reusable Spaceling controller, with
collision groups selecting the existing rough planet surface rather than also
hitting the coincident ship-only surface. Spawn clearance is an infrequent
capsule query against the world's spatial index; ordinary support queries stay
local to the character's contacts.

The former elevated berth/access connection is removed in this fixture. Its
ship opts out of the port sensor, compact dock collider, and kinematic hold,
and collides only with the planet's rough surface, not both coincident surfaces.
The cyan hatch follows the ship, on nearby ground beside its right side.
Exit requires a physically landed ship and a clear capsule placement.
Boarding requires that same available ship, proximity to its access point,
physical support, balance, and low support-relative speed. This is a short hatch
transition, not remote boarding or a physical door/ladder simulation.

At exit the capsule inherits the rotating surface's point velocity and spin.
Afterward Rapier and gravity own its motion; no radial snapping or per-tick
orbital transport is added. The center is stationary on purpose. Accelerating
scripted orbits need separate regression evidence before ordinary-world use.

This is a noncombat fixture: no weapons, asteroids, capture, healing, rover
construction, or win condition. Ordinary Spacewars and its bots are unchanged.
If you lose the ship while experimenting with flight, restart; damage, rescue,
pod, and elimination policy for an independent pilot are not settled here.
The next gameplay slice can add an intact surface outpost to capture and a
repair/rebuild reward. This is not yet that outpost game mode.

## Verification

```sh
cargo test -p scenario-spacewars surface_sortie
cargo test -p engine-client client_scenarios::surface_sortie
cargo test -p engine-rapier capsule_clearance
```

Headless regressions cover exit/walk/jump/return/reboard without pose shortcuts,
ship health and creature identity preservation, one-body lifecycle, velocity
inheritance, no airborne transport, blocked exits, unsafe/remote boarding,
held-input gating in both directions, and reproducible actions/restarts. Typed
`SurfaceSortieState::observation()` and version-2 JSON scenario observations
include landing phase, clearance, angle, relative speeds/spin, foot count,
assist strength, and settling duration for future runners; this does not add
a live IPC telemetry API.

Landing regressions fly gentle approaches at eight bearings, verify physical
takeoff/return, contact-only boarding, the settling interval, sideways damping,
no automatic pointing, and damage on fast approaches. Ordinary Spacewars' port
regressions and pinned AI suites remain the compatibility guard.

The client test checks both render paths and on-foot geometry. To retain images:

```sh
SPACEWARS_SORTIE_ARTIFACTS=target/surface-sortie-poses cargo test -p engine-client \
  registered_fixture_renders_and_restarts_in_both_backends
```

The real-window functional test covers launcher selection, both renderers,
pause, restart, return, and relaunch, with raster ship/HUD/minimap visibility checks:

```sh
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 cargo test -p engine-client \
  --test ui_control_functional surface_sortie -- --ignored --test-threads=1
```

Use an existing display or the [Xvfb workflow](functional-tests.md). Physical
gamepad feel and the complete round trip remain manual acceptance checks; the
initial sortie and refined assisted landing have both passed manual playtesting.
