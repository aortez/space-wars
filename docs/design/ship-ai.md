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

## Headless evaluation

`engine-agent` now embeds the Spacewars scenario and runs the same
observation/brain/intent/action loop without rendering, audio, or realtime
pacing. An episode explicitly records its world preset, seed, tick limit, and
versioned controller identity. Batches walk a configurable seed sequence and
can install an idle or rule controller independently in each player seat.
The `standard-no-asteroids` preset changes only the standard configuration's
asteroid spawn rate, providing a controlled navigation baseline without
redefining normal gameplay.

The named `navigation-v1` suite turns that baseline into a reproducible
contract: seeds 0 through 5, rule-brain self-play, a 36,000-tick ceiling, no
random asteroids, and deliberately high ship health. Collisions and docking
physics remain active. This keeps accidental destruction from truncating the
experiment while preserving the contacts that navigation guidance must avoid.

The evaluator is allowed to read authoritative state to measure captures,
ship losses, rebuilds, eliminations, collision incidents, docking, departures,
and outcomes. Body and ship collision incidents re-arm after 30 contact-free
ticks, preventing a sustained or briefly flickering scrape from dominating the
count; debris and laser hits are already discrete events. A docking session
records whether capture or rebuilding happened there, and counts successful
departure only after the craft reaches 90 world units of surface clearance
beyond its collision hull. This does not widen the controller boundary: each
rule brain still receives only `ShipObservationV1`, and every intent still
passes through `ShipIntentEncoder` and ordinary scenario actions before
`Scenario::step()`.

Each deterministic episode summary includes its action stream and selected
outcome-relevant terminal state in a SHA-256 trace fingerprint. Wall-clock
throughput is reported only at the batch layer and is intentionally excluded
from deterministic comparisons. Versioned JSON output provides the first
artifact format for regression suites and later training experiments.

An opt-in per-player navigation trace sits on the evaluator side of the same
boundary. It samples semantic brain and docking transitions immediately, then
adds a heartbeat every 300 ticks while a post-capture departure is unfinished.
The sample combines `BrainTelemetry`, the emitted `ShipIntent`, dock/contact
state, planet surface clearance, and outward velocity. Collecting it does not
alter controller actions or the deterministic episode fingerprint, and normal
untraced batches do not pay its extra observation cost.

The first traced baseline isolated two unfinished departures. In seed 4,
Player 2 captures planet 3 at tick 2,714 and remains docked through tick 36,000;
seed 2 leaves Player 1 similarly docked after its fourth capture. Both brains
remain in `Depart`, continuously request a turn plus brake, never request
thrust, and stay roughly 45–50 units inside the surface. Their measured hull
rotation repeatedly reverses while the turn command does not. The evidence
points to a dock-bay rotation deadlock: solid contact opposes the attempted
alignment, while the general brake removes angular authority along with linear
motion.

A bounded characterization test now reproduces the seed-4 failure at tick
4,000. It pins the capture at tick 2,714 and verifies that Player 2 remains
docked on planet 3 for the next 1,286 ticks with negative surface clearance,
full turn, full brake, and no thrust. This assertion is intentionally a
temporary bug contract; the departure repair should invert it to require safe
clearance within the same budget.

## Next slices

1. Turn the seed-4/Player-2 departure into a regression test, then test removing
   the pre-launch brake while retaining the spaceport's existing positional
   damping. Confirm the seed-2 failure and the complete `navigation-v1` suite.
2. Add a slower strategic state machine for capture, repair, defend, and combat
   goals, including commitment and hysteresis.
3. Compare each strategy revision against the baseline outcomes and traces.
4. Promote additional scenario transitions to typed evaluator events when
   state-difference metrics are no longer expressive enough.
5. Add policy adapters and batched observation storage only after the rule path
   gives us stable semantics and measurable workloads.
