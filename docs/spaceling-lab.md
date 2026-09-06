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
- Gamepad A: jump on press; release before jumping again.
- Keyboard A/D or left/right arrows: walk; Space: jump.
- Start or Esc: pause. R or the pause menu: restart.

The camera follows the spaceling without rotating. Walking right goes clockwise
around the planet; at the bottom, the spaceling is upside down on screen. The two
orange slopes are real colliders. Short surface marks rotate with the planet,
making moving-ground behavior visible. The spaceling is 1.8 world units tall.

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

## Model and scope

Each spaceling has one dynamic capsule body and collider in `engine-rapier`.
Legs, arms, and helmet are presentation. A bounded controller changes angular
velocity toward gravity-relative upright and tangential velocity toward walking
speed. Rapier alone integrates the body and resolves collisions.

Grounded movement targets speed relative to actual supporting motion. Air
control is weaker; releasing movement in flight does not brake. This is an
arcade controller, including its limited air control, not simulated limb forces.
With no gravity it retains its last up direction. A jump requires a fresh press
and walkable support, so walls, ceilings, and speculative distant contacts do
not allow jumps.

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

Tests cover both walking directions and stopping, jump/release/landing,
translating and rotating support, slopes, walls/ceilings, removal of support,
zero gravity, bounded controls, malformed input, reproducible actions/restart,
and complete laps over the lab obstacles.

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
