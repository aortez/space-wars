# Surface Sortie

An opt-in first Spacewars integration for spacelings. Choose **surface-sortie**
in the launcher, or run:

```sh
cargo run -p engine-client -- --scenario surface-sortie
```

For the same loop on a moving planet, choose **surface-sortie-orbit** in the
launcher (including gamepad navigation), or run:

```sh
cargo run -p engine-client -- --scenario surface-sortie-orbit
```

These are presets of the same scenario, not separate gameplay implementations.
Restart repeats the selected preset; returning to the launcher preserves its
selection. Controls and the capture/repair rules are identical.

The additional **surface-sortie-generated** preset is an explicitly untuned
[compatibility diagnostic](surface-compatibility.md), not another accepted
gameplay preset. It tests ordinary generated masses/motion with these controls.

Choose **surface-sortie-world** for the experimental **Surface V1** profile on
generated planets. It retains their sizes and relative layout, with gentler
gravity, bounded spin, sun-matched orbits, and more room before the world wall:

```sh
cargo run -p engine-client -- --scenario surface-sortie-world --seed 0
```

The controllers and outpost loop are unchanged. This is fixture-only tuning,
not a new ordinary-game default. Both generated presets start on planet 0,
initially away from the sun. See the [profile comparison](surface-compatibility.md#surface-v1-experiment)
for parameters, results, and remaining approach/long-idle limits.

For travel between those planets and flag-based planet claiming, choose
**surface-expedition**. It adds dynamic approach/support selection to the same
scenario; the four original presets remain pinned compatibility fixtures.
See [Surface Expedition](surface-expedition.md) for that opt-in loop and scope.

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

The camera follows the active ship or spaceling, framing the nearby landed ship
and pilot between the HUD strips even on the sides or underside of the planet.
Farther from the ship it follows the pilot alone. A translucent, fixed-scale,
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

At exit the capsule inherits the moving surface's point velocity and spin.
Afterward Rapier and gravity own its motion; no radial snapping or per-tick
orbital transport is added. The original preset retains its stationary center;
the orbital preset and its compatibility boundary are described below.

This is a noncombat fixture: no weapons, asteroids, pod rebuilding, rover
construction, resources, or win condition. Ordinary Spacewars and its bots are unchanged.
Only the pilot's ship is simulated; the legacy world's unused second ship slot
has no physics body, controls, or gravity target to interfere with the experiment.
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
`SurfaceSortieState::observation(player)` and JSON scenario observations
include landing phase, clearance, angle, relative speeds/spin, foot count,
assist strength, and settling duration for future runners. Outpost observations
add identity, position/normal, owner, active claimant, capture eligibility and
progress, capture count, repair eligibility/range, and cumulative health
restored. The current JSON envelope is version 9 with a `players` array;
these pinned presets contain only seat 0. Version 9 makes `outpost` optional
and adds `planet_claim`/`planet_claims` for Expedition; pinned outpost fixtures
retain their existing behavior. This does not add a live IPC
telemetry API.

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

The real-window functional tests cover both presets' launcher selection, both renderers,
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

## Moving-planet evidence

The orbital preset uses a radius-60 planet on a radius-220 circular path at
0.065 radians/s (14.3 units/s, about 97 seconds per orbit), with the same
0.015 radians/s surface spin as the original fixture. The yellow central source
and orbital guide appear on the minimap. A `Translating` Rust configuration
provides a short, straight-line diagnostic at 3 units/s; it is not another
launcher entry because an indefinite straight path eventually leaves the map.

Terrain remains position-driven kinematic geometry in the canonical world.
The fixture schedules its path once per step. Before physics, gravity and
controls read the completed terrain pose, not its next target; after physics,
landing and services consume the new completed contacts. Point velocities use
Rapier's center-of-mass-aware query. A body's origin and center of mass need
not coincide, so adding angular velocity around the origin to its raw linear
velocity is not generally correct.

For this controlled orbit, the sun's mass is selected so its gravity at the
planet center matches the scripted centripetal acceleration. Ships and the
spaceling receive the ordinary shared field, including its spatial variation,
once per tick. There is **no** rover-style frame transport, common-gravity
subtraction, invisible attachment, or extra gravity solve. Jumping and thrusting
inherit surface momentum and then leave support normally.

This does not establish compatibility with every generated Spacewars planet.
The ordinary game's scripted orbital rates and gravity masses are selected
independently; its planets need not follow the acceleration experienced by
nearby actors. That mismatch and the supported spin/acceleration envelope need
an explicit policy before replacing ordinary-game docking. This fixture does
not retune those orbits, gravity, rovers, ships, or bots.

### Motion diagnostics

Typed/JSON observations include `motion` and `motion_metrics` (introduced in
version 4; version 5 adds the optional `generated_case` identifier; version 6
adds its explicit gravity/motion `profile`; version 7 adds travel, explicit
support/frame planet IDs and the complete outpost inventory; version 8 wraps
independent seat views in a `players` array; version 9 adds planet claims and
an optional focused outpost):

- completed planet position, origin velocity, angle/spin, active-body
  surface-relative velocity, and actual support-point velocity/relative speed;
- scripted orbital acceleration and external gravity at the planet center,
  making their mismatch visible rather than silently compensating for it;
- on-foot/supported/landed tick counts, unexpected pilot/ship support losses,
  jumps, knockdowns, landings, deliberate departures, and cumulative ship damage;
- current and maximum planet-local idle drift of the active actor. Movement,
  flight, loss of balance, and controller-context changes reset the current
  interval; the lifetime maximum remains.

Counters advance only in simulation steps. Boarding and commanded jumps are
not unexpected support losses; service healing does not erase recorded damage.
The HUD shows the preset, speed/spin, relative speed, support losses, current
idle drift, and damage. These are scenario observations and HUD diagnostics,
not a new live IPC gameplay-state API.

### Reproduce the motion tests

```sh
cargo test -p scenario-spacewars surface_sortie::tests::motion_tests -- --nocapture
cargo test -p scenario-spacewars outpost_round_trip -- --nocapture

# Isolated headless timing, including assertions/diagnostic reads, not rendered FPS:
cargo test --release -p scenario-spacewars \
  orbiting_and_translating_sorties_stay_supported -- --nocapture --test-threads=1
```

Coverage includes a 200-second orbital idle period, faster/opposite orbit and
spin, both actors' independent flight, eight-bearing orbital approaches, the full capture/repair/reboard/
departure loop, deliberately excessive support acceleration, stable body/collider
counts, and deterministic actions/observations/restarts. Timing is printed for
comparison but never used as a flaky pass/fail threshold.

One local x86_64 Rust 1.89 release run on 2026-09-06 completed the 12,000-tick
orbital idle window with pilot support and ship landing on every tick, zero
support losses/knockdowns/damage, five bodies while on foot, and maximum recorded
idle drift of 0.128 units. It ran at about 98,000 headless ticks/s including
assertions and diagnostic reads. This is a tiny-fixture measurement, not rendered
FPS, Pi performance, or evidence about crowded worlds.

The orbital preset was deployed to `spacewars.local` on 2026-09-06. Runtime
checks reported about 60 FPS / 60 UPS with the Switch Pro controller connected;
the user subsequently reported that playtesting seemed good. This adds orbital
Pi/controller acceptance alongside the previously accepted stationary outpost
loop, without extending the claim to arbitrary generated planets or combat.
