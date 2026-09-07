# Surface Sortie

An opt-in first Spacewars integration for spacelings. Choose **surface-sortie**
in the launcher, or run:

```sh
cargo run -p engine-client -- --scenario surface-sortie
```

The fixture starts with your ship at **75% health**, settling rear-first onto a
slowly rotating planet. Press **B** on a gamepad or **X** on the keyboard to
disembark. Release the controls, walk right to the amber terminal, and stand
beside it for three seconds to capture the outpost. It repairs your nearby
landed ship. Return to the cyan access marker and, once standing and settled,
press B/X again to reboard and depart. Boarding itself does not heal or replace
the ship.

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

## Capturing and repairing

Capture is automatic while standing within **3 world units** of the terminal's
base, balanced and supported by that planet's surface or terminal. Your
support-point-relative speed must be at most **1 unit/s**. Jumping, getting
knocked down, walking out of range, or moving too quickly resets partial
progress. Landing the ship cannot capture; neither can hovering nearby or
standing on an unrelated vehicle/platform. No additional interaction button is
needed.

After **3 continuous seconds**, the outpost raises your owner-colored flag and
its square minimap marker changes from amber to your color. The HUD shows
progress and why capture is waiting. Ownership persists when you leave; it
belongs to the outpost, **not the entire planet**.

Capturing a neutral outpost is one testbed interaction, not the universal rule
for claiming or developing a planet. Future scenarios must allow other paths
without requiring a pre-existing neutral terminal. Planet claims, infrastructure
ownership, and operational services remain separate concepts; this fixture
implements only outpost ownership and its simple repair service.

A friendly outpost restores **5% of maximum ship health per second**, capped at
full health, while the same live ship is physically landed and its center is
within **24 world units** of the terminal's base. The faint service-range circle
is a visual guide, not a landing constraint. Repairs work while you are outside
the ship too; the starting landing spot is already in range. Taking off or
leaving service range stops repair immediately. An enemy ship, destroyed ship,
or pod cannot use this repair service; this is not a rebuild or resurrection
mechanic. After landing elsewhere, you can still capture on foot, but must move
the ship into range to repair it.

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
ship with heading, orange spaceling, square outpost marker, and a white camera
footprint. A short leader connects the outpost marker to its surface location.
Its footprint
uses the actual window aspect ratio, including portrait layouts. The main view
keeps the whole window; both vector and raster backends composite the circular
backing and map as one faded layer.

Orange is balanced, red is
knocked down, yellow is recovering; cyan identifies physical support and surface
access. The HUD shows landing phase, angle, descent and sideways speed, foot
contacts, transfer feedback, ship health, body count, access distance, outpost
capture progress, and repair eligibility.
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

The intact terminal adds one solid collider to the existing rotating planet
body, not another simulated body. Capture and repair are small scenario-owned
policies consuming completed support/landing state; they do not manipulate
physics, transfer ownership of terrain, or reuse the ordinary game's port
capture/heal rules. This single-operator fixture does not define contested
capture, destructible infrastructure, or multiple-outpost service arbitration.

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

This is a noncombat fixture: no weapons, asteroids, pod rebuilding, rover
construction, resources, or win condition. Ordinary Spacewars and its bots are unchanged.
If you lose the ship while experimenting with flight, restart; damage, rescue,
pod, and elimination policy for an independent pilot are not settled here.
The intact outpost gives walking a useful capture-and-repair loop without
settling those larger game-mode policies.

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
`SurfaceSortieState::observation()` and version-3 JSON scenario observations
include landing phase, clearance, angle, relative speeds/spin, foot count,
assist strength, and settling duration for future runners. Outpost observations
add identity, position/normal, owner, active claimant, capture eligibility and
progress, capture count, repair eligibility/range, and cumulative health
restored. This does not add a live IPC telemetry API.

Landing regressions fly gentle approaches at eight bearings, verify physical
takeoff/return, contact-only boarding, the settling interval, sideways damping,
no automatic pointing, and damage on fast approaches. Ordinary Spacewars' port
regressions and pinned AI suites remain the compatibility guard.

Outpost regressions complete exit/walk/capture/repair/return/reboard/takeoff
using only player controls. They also cover interrupted capture, wrong support
identity, solid rotating terminal geometry without another body, ownership
separate from the planet, repair eligibility/rate/clamping, fresh capture time
for a changed claimant, deterministic replay, restart, and no progress without
simulation steps.

Client tests check both render paths, on-foot geometry, capture feedback, and
the actual owner-colored flag in landscape and portrait layouts. To retain images:

```sh
SPACEWARS_SORTIE_ARTIFACTS=target/surface-sortie-poses cargo test -p engine-client \
  registered_fixture_renders_and_restarts_in_both_backends
SPACEWARS_SORTIE_ARTIFACTS=target/surface-sortie-poses cargo test -p engine-client \
  outpost_capture_progress_and_owner_flag_render_in_both_backends
```

The real-window functional test covers launcher selection, both renderers,
pause, restart, return, and relaunch, with raster ship/outpost/HUD/minimap visibility checks:

```sh
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 cargo test -p engine-client \
  --test ui_control_functional surface_sortie -- --ignored --test-threads=1
```

Use an existing display or the [Xvfb workflow](functional-tests.md). Physical
gamepad feel and the complete round trip remain manual acceptance checks; the
initial sortie and refined assisted landing have both passed manual playtesting.
The outpost loop also passed user-reported controller playtesting on the
Raspberry Pi on 2026-09-06: gameplay worked well and the direction was accepted.
This validates the experimental loop, not a final planet-claiming mechanic.
