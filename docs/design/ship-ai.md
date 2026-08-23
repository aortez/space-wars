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
count; debris and laser hits are already discrete events. Docking contact
transitions count port sessions, while each capture or rebuild opens its own
pending departure window. A window completes only after the craft reaches 90
world units of surface clearance beyond its collision hull. Keeping these
windows independent preserves attribution even if a fast craft reaches another
port before clearing the previous window. This does not widen the controller
boundary: each rule brain still receives only `ShipObservationV1`, and every
intent still passes through `ShipIntentEncoder` and ordinary scenario actions
before `Scenario::step()`.

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
Player 2 captured planet 3 at tick 2,714 and remained docked through tick
36,000; seed 2 left Player 1 similarly docked after its fourth capture. Both
brains remained in `Depart`, continuously requesting a turn plus brake without
ever starting thrust, roughly 45–50 units inside the surface. Two effects made
that state self-sustaining: the general brake canceled the requested angular
motion, and the broad asymmetric ship hull could bridge the inner planet and a
spaceport wall while trying to turn.

The departure repair is intentionally split at the controller/physics
boundary. Rule policy `rule_ship_v2` does not apply the omnidirectional brake
while aligning for launch; spaceport contact already damps and centers linear
motion. A docked full ship that is actively maneuvering to leave keeps its
ordinary collision groups and mass but temporarily uses a body-sized circular
solver collider, so it remains contained while being free to rotate. A
zero-mass copy of the normal hull preserves the established spaceport sensor
footprint during that maneuver. The full physical hull remains in use while
landing and capturing, and is restored when departure/ejection maneuvering
ends. Escape pods retain their ordinary compact hull.

The former seed-4 characterization is now a bounded regression test. Within
4,000 ticks Player 2 captures planet 3, reaches the evaluator's 90-unit safe
clearance within one 300-tick trace heartbeat, and has no unfinished capture
departure at episode end. The complete six-seed `navigation-v1` run records 36
captures and 36 safe capture departures; neither previously observed deadlock
remains.

## Next slices

1. Add a slower strategic state machine for capture, repair, defend, and combat
   goals, including commitment and hysteresis.
2. Compare each strategy revision against the baseline outcomes and traces.
3. Promote additional scenario transitions to typed evaluator events when
   state-difference metrics are no longer expressive enough.
4. Add policy adapters and batched observation storage only after the rule path
   gives us stable semantics and measurable workloads.
