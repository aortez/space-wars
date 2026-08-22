# Spacewars ship-controller architecture

This document records the first implementation slice for
[issue #14](https://github.com/aortez/space-wars/issues/14). It is deliberately
small enough to play-test while establishing the boundary that later rule,
training, and autonomous logistics controllers share.

## Authority and data flow

The Spacewars scenario remains authoritative for state, physics, damage,
capture, and scoring:

```text
SpacewarsState
    -> SpacewarsScenario::observe_ship(actor, sensor profile)
    -> ShipObservationV1
    -> ShipBrain::intent()
    -> ShipIntent
    -> ShipIntentEncoder
    -> ordinary Spacewars actions
    -> SpacewarsScenario::step()
```

A brain cannot receive or mutate `SpacewarsState`. Human keyboard/gamepad
sources are even narrower: they produce `ShipIntent` from device state and no
longer receive scenario state at all. This keeps AI policy out of simulation
authority and makes replay behavior depend only on observations, reset context,
and emitted actions.

`spacewars-ai` is a separate library so the interactive client and a future
headless `engine-agent` runner can use the same controller and guidance code.
The client owns brain instances and their reset/handoff lifecycle.

## Observation V1

`ShipObservationV1` is serializable and uses typed `PlayerId`, `PlanetId`, and
`DebrisId` identities. It includes:

- own craft capability, health, form, velocity, weapon readiness, and docking;
- the opposing craft's relative pose, velocity, form, and health;
- universe center/radius, sun, and planets including moving spaceports and
  capture state;
- nearby tactical debris with stable scenario IDs.

All vectors use an actor-local coordinate frame: positive x is right and
positive y is forward. This removes irrelevant world translation and rotation
from policy inputs.

The initial `FullMapRadar` profile exposes the global ship and strategic-body
awareness already available to local players. Debris requires tactical detail,
so it is bounded to the nearest 64 contacts within 600 world units. Selection
uses a fixed-size heap and final deterministic ordering; dense asteroid fields
therefore cannot cause an unbounded controller allocation.

Adding fog of war or specialized sensors should add named sensor profiles and
preserve V1 field meaning. Breaking schema changes require a new observation
version.

## Brain lifecycle

`ShipBrain` has three operations:

- `reset(BrainReset)` installs an actor and episode seed;
- `intent(&ShipObservationV1)` produces one normalized controller intent;
- `telemetry()` reports the current goal, target, hazard, range, and heading
  error without granting state access.

When a host changes between human, rule-bot, or benchmark control, it forwards
one neutral intent before the new source. This releases held thrust and weapons
and prevents transition inputs from leaking between controllers. Restart also
clears the encoder, brain context, and active source.

## Current rule bot

The first deterministic bot is selectable for Player 2 in the launcher. It:

- avoids nearby celestial bodies and predicted debris collisions;
- uses reusable shortest-heading and moving-target intercept guidance;
- approaches and brakes around a one-on-one target;
- derives a collision-free staging point and exact rigid-frame velocity from
  each moving spaceport observation;
- uses persistent rendezvous, port-approach, ingress, docked, and departure
  phases to capture uncaptured planets and then continue to another target;
- aligns with the moving spaceport before committing to a wings-closed launch
  burn, keeping that burn active until both port contact and the planetary
  surface are safely cleared;
- treats a sensed docking contact as authoritative over its planned target,
  capturing an unexpected unowned port or relaunching from an owned one;
- seeks an owned spaceport for rebuilding when reduced to an escape pod;
- uses fast wings only for long, aligned pursuit;
- fires laser/cannon only inside configured alignment and range windows;
- falls back to simple escape behavior when a pod has no accessible port.

The port arrival controller first reaches a safe moving ring around the target
planet, then tracks the port around that ring. At each target it chooses a
bounded desired closing velocity and steers toward the error between that
velocity and the craft's current target-relative velocity. The ordinary brake
is used only when braking would reduce that error. Entry and departure are
deliberately different maneuvers: entry matches the rotating port, while
departure commits to an outward burn until the hull has safe surface
clearance.

This is still a narrow deterministic strategy. It does not yet score capture,
repair, defend, and attack goals against each other or use personality weights.
Small Duel remains the clearest combat test; a normal planet world exercises
the docking path.

## Next slices

1. Add a slower strategic state machine for capture, repair, defend, and combat
   goals, including commitment and hysteresis.
2. Add deterministic repair and full escape-pod rebuild episodes around the
   moving-spaceport fixture.
3. Surface brain telemetry in developer HUD/IPC tooling.
4. Let `engine-agent` run the same observation/brain/action loop headlessly.
5. Add policy adapters and batched observation storage only after the rule path
   gives us stable semantics and measurable workloads.
