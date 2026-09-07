# Spaceling Lab

Run the small contact-based character experiment:

```sh
cargo run -p engine-client -- --scenario spaceling-lab
```

Or choose **spaceling-lab** in the ordinary launcher. It supports both vector and
raster rendering and does not require any assets.

The creature is a **spaceling**, independent of whether a player or bot controls
it. Saved launcher selections using the old `human-lab` name migrate to
`spaceling-lab`; use the new name in command-line launches.

Use raster on software-only backends (including the Pi kiosk). Slint's software
backend does not draw vector paths; desktop vector mode needs the graphics
backend. Text diagnostics are a Slint overlay in either mode.

## Controls

- D-pad or left stick left/right: walk relative to local gravity.
- Gamepad bottom face (Xbox A / Switch B): jump or get up on press; release before repeating.
- Gamepad B: apply a repeatable off-center test shove; release before repeating.
- Keyboard A/D or left/right arrows: walk; Space: jump or get up when prone.
- Keyboard X: the same test shove. It pushes outward and in the facing direction.
- Start or Esc: pause. R or the pause menu: restart.

The camera follows the spaceling without rotating. Walking right goes clockwise
around the planet; at the bottom, the spaceling is upside down on screen. The two
orange slopes are real colliders. Short surface marks rotate with the planet,
making moving-ground behavior visible. The spaceling is 1.8 world units tall.

Press B/X after settling, then watch the spaceling tumble, land, and stand up.
Try it facing either direction, on a slope, while jumping, or again during
recovery. The red mark/line briefly shows the shove's application point and
direction. This is a lab stimulus, not an attack or damage mechanic.

## Diagnostics

The cyan foot marker and line show the selected physical support and its normal.
The orange indicator beside the spaceling shows gravity direction.

- **Grounded/airborne:** a walkable contact beneath the body, not proximity to
  the planet or a landing sensor.
- **Support:** stable entity ID and collider part. Planet `1:0` is the circular
  surface; `1:1` and `1:2` are the two slopes.
- **Contacts:** candidate solver contacts, including ones rejected as support.
- **Relative speed:** tangential contact-point speed relative to the supporting
  body, including its rotation. In flight, this is world-frame tangential speed.
- **Gravity:** current field acceleration, which decreases with altitude.
- **Jumps/landings:** actual supported jumps and airborne-to-grounded
  transitions. The initial drop counts as a landing.
- **Air:** current airtime when airborne; previous airtime when grounded.
- **Balance:** orange is balanced, red is knocked down, yellow is recovering.
  Grounded/airborne remains independent: lying on the ground is supported, not
  necessarily balanced.
- **Recovery:** progress through the minimum supported recovery duration. At
  100%, the controller still waits for a sufficiently upright, settled pose.
- **Knockdowns/recoveries/shoves:** actual transitions and applied test impulses,
  not frames spent in a state or repeated held inputs.

## Model and scope

Each spaceling has one dynamic capsule body and collider in `engine-rapier`.
Legs, arms, and helmet are presentation. A bounded controller changes angular
velocity toward gravity-relative upright and tangential velocity toward walking
speed. Rapier alone integrates the body and resolves collisions.

Grounded movement targets speed relative to actual supporting motion. Air
control is weaker; releasing movement in flight does not brake. This is an
arcade controller, including its limited air control, not simulated limb forces.
With no gravity it retains its last up direction, but applies no upright or
movement correction; linear and angular momentum remain physical. A jump requires a fresh press
and walkable support, so walls, ceilings, and speculative distant contacts do
not allow jumps.

## Balance and recovery

The controller compares actual velocity with the previous commanded velocity
plus the supplied gravity step. An unexpected change of at least 12 units/s,
or angular speed of at least 8 rad/s relative to support (world-frame in air),
causes knockdown. These are arcade disturbance thresholds, not measured damage
or a reconstruction of collision energy. Solver impacts are detected on the
next controller tick; a shove applied before controls is detected immediately.
Ordinary walking and jump/land cycles are regression-tested not to knock down.

While knocked down, walking, ordinary jumping, air control, and automatic upright
correction are disabled. Jump instead requests an explicit get-up attempt. The
same capsule gains ordinary contact friction (0.6, combined using
the minimum material friction) so it can physically settle. In free space,
there is no artificial angular damping or timed auto-recovery.

Recovery starts after 0.25 seconds of continuous suitable support, with
support-relative contact-point speed at most 2 units/s and relative angular
speed at most 2 rad/s. It ramps bounded upright control and ground traction over
at least 0.8 seconds. Completion also requires upright alignment within 0.15
radians and relative angular speed below 0.8 rad/s. Supported movement during
recovery crawls at 25% of walking speed, allowing escape from a low roof or ledge.
Ordinary jumps remain disabled until completion; holding jump during recovery
cannot produce a buffered jump afterward.

A 0.1-second contact grace period tolerates tiny gaps while physically standing
up; progress does not advance during a gap. A valid contact handoff preserves
progress, including when mining replaces collision rectangles on the same body.
Longer gaps, removed support, loss of gravity, or another severe disturbance
interrupt automatic recovery. Removed support and loss of gravity cancel
immediately.

A fresh jump press while prone or unbalanced requests a physical get-up. It
requires gravity and real support, no severe impact, contact-relative speed at
most 4 units/s, and relative spin at most 2 rad/s. A shape sweep checks upward
clearance and a full-capsule overlap query checks the proposed upright space.
Sensors and the character's own entity are excluded; collision groups apply.
With room, a bounded lift follows the supporting body's translation and rotation
for at most 0.8 seconds while full upright correction turns the capsule. The
lift briefly counters gravity and permits the contact gap needed to stand;
upright alignment and low spin end it early. Gravity loss, support removal, or
a severe new impact cancels it. Holding the button cannot repeat it, and a
press in free flight cannot provide an extra jump. Rapier owns the entire motion
and collision response: no pose is snapped and no new collider appears.

The snapshot reports get-up attempts and the latest result (started, succeeded,
blocked, unsupported, unsettled, or no gravity). Terrain Lab displays these in
its debug HUD and shows a normal-play hint when getting up is needed.

Balanced airborne self-righting uses 15% of normal angular acceleration. The
knockdown/recovery thresholds live in `SpacelingBalanceSpec`; Rapier still owns
mass, inertia, integration, and contact response. The lab shove is a physical
impulse applied 0.6 units above the body center, sized for a 4-unit/s sideways
and 6-unit/s outward velocity change with the default body. Rapier computes its
rotational effect from the actual lever arm and inertia.

## Fixture

The lab uses the shared gravity solver with one source-only planet and one
target-only spaceling. The character controller accepts any resulting gravity
vector and does not know about planets. The fixed fixture is intentionally
unrandomized. Default tuning is walk speed 5, jump speed 8, surface gravity 18,
planet radius 20, and rotation 0.025 radians/second.

This slice does not change Spacewars ships, bots, landing, ownership, or services.
The external ship berth remains an interim mechanism; natural ship landing and
destructible terrain support are follow-ups in the
[design plan](design/spacelings.md).

## Verification

```sh
cargo test -p engine-rapier spaceling
cargo test -p scenario-spaceling-lab
cargo test -p engine-client client_scenarios::spaceling_lab
```

To retain raster geometry images for the knocked-down, recovering, and balanced
poses (HUD text is checked separately as the Slint overlay):

```sh
SPACEWARS_SPACELING_ARTIFACTS=target/spaceling-poses cargo test -p engine-client \
  knockback_frames_reach_both_renderers
```

Tests cover both walking directions and stopping, jump/release/landing,
translating and rotating support, slopes, walls/ceilings, removal of support,
zero gravity, bounded controls, malformed input, reproducible actions/restart,
and complete laps over the lab obstacles. Balance tests include mild/severe
shoves, real high-speed landings, physical tumbling and recovery, moving support,
interrupted recovery, support removal/gaps, zero-gravity momentum, and held-input
gating. The rotating-planet lab regresses shoves in both directions at multiple
locations. Get-up regressions cover both prone directions and all balance states,
strong gravity on translating/rotating support, collider replacement, low-roof
collision and crawling, held input, unsupported/zero-gravity requests, and
interruption of an active lift. Version-1 walking/jumping actions remain readable;
version-2 actions add held shove input. Observation version 3 includes explicit
get-up diagnostics alongside balance and lab-shove state.

The real-client functional workflow covers launcher selection, both renderers,
pause, restart, return, and relaunch. It captures screenshots; use an existing
display or the [Xvfb workflow](functional-tests.md):

```sh
SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1 cargo test -p engine-client \
  --test ui_control_functional spaceling_lab -- --ignored --test-threads=1
```

No crowd-scale performance claim is made from this one-spaceling fixture.
Population scaling, dynamic-support reactions, stair handling, jump buffering,
coyote time, physical limbs, NPCs, and ship entry/exit need separate work.
