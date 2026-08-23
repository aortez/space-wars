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
- `telemetry()` reports the current goal, target, hazard, avoided celestial
  body, surface clearance, outward speed, predicted closest approach,
  avoidance age/stall state, range, and heading error without granting state
  access.

When a host changes between human, rule-bot, or benchmark control, it forwards
one neutral intent before the new source. This releases held thrust and weapons
and prevents transition inputs from leaking between controllers. Restart also
clears the encoder, brain context, and active source.

## Current rule bot

The first deterministic bot is selectable for Player 2 in the launcher. It:

- avoids nearby celestial bodies and predicted debris collisions, and guards
  briefly against a predicted re-entry into the planet it just departed;
- uses reusable shortest-heading and moving-target intercept guidance;
- approaches and brakes around a one-on-one target;
- derives a collision-free staging point and exact rigid-frame velocity from
  each moving spaceport observation;
- uses persistent rendezvous, port-approach, ingress, docked, and departure
  phases to capture uncaptured planets and then continue to another target;
- yields a neutral port only when the opposing full ship is already inside its
  immediate staging area and has closer claim, using planet-ID parity as a
  deterministic tie-break instead of letting identical brains repeatedly
  enter and eject one another;
- aligns with the moving spaceport before committing to a wings-closed launch
  burn, keeping that burn active until both port contact and the planetary
  surface are safely cleared;
- treats sensed docking contact as authoritative over its planned target,
  capturing an unexpected unowned port or relaunching from an owned one, and
  reacquires any capture/rebuild/repair port whose contact is lost before the
  operation completes;
- seeks an owned spaceport for rebuilding when reduced to an escape pod, and
  discards any full-ship departure state inherited when destruction happens
  during launch;
- preserves pod steering authority during avoidance, rebuild navigation, and
  fallback survival turns instead of combining those turns with angular
  braking;
- detects a deep planet avoidance maneuver outside the valid spaceport
  corridor that has made no clearance progress for five seconds and latches a
  partial-brake escape assist; a low-health ship that remains inside the
  surface and moving inward takes an emergency path after one second and
  immediately burns in reverse when its stern already points near the safe
  outward direction;
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

Docking now has an explicit physical contract. The observed target is a safe
anchor on the middle of the port's outer arc, rather than the polygon centroid
that can fold near a tiny planet's gravity center. Contact is damped in the
moving port frame and pulls linearly toward that anchor. An escape pod
automatically establishes the hold required for rebuilding; a full ship must
apply its brake, so an unbraked pass through the bay cannot become an invisible
persistent latch. Once established, the full ship uses a compact kinematic
body while its ordinary hull remains a sensor probe. It can release the brake
and turn in place, while nonzero thrust releases the hold and restores its
ordinary dynamic collider. Retained physical holds are included in gameplay
docking state across brief raw-sensor gaps.

Rule policy `rule_ship_v4` adds a persistent strategic layer above those
guidance maneuvers. Strategy evaluates at 1 Hz while collision avoidance and
guidance continue at 60 Hz. An ordinary valid objective is held for at least
three seconds, and a challenger must clear a utility margin before replacing
it. Rebuild, critical repair, and an active capture threat are urgent and may
interrupt that commitment. Temporary body or debris avoidance therefore owns
the controls without erasing the capture, repair, defense, combat, or rebuild
objective that should resume afterward.

Rule policy `rule_ship_v5` makes physical safety authoritative over that
strategy. An active planet target is exempt only while the craft is actually
inside its valid port corridor; a side impact or penetration can therefore
retain escape progress and engage emergency reverse even if critical damage
selects that same planet for repair. Capture, rebuild, and repair all return
from remembered `Docked` to `Ingress` when physical contact disappears before
completion. Full ships tolerate a bounded 30-tick contact gap while settling;
pods reacquire immediately because releasing their brake also applies forward
cruise.

After a safe departure, v5 remembers the origin planet for 30 ticks. During
that bounded half-second it projects the current body-relative trajectory up
to three seconds ahead. A projected clearance violation chooses and latches
one clearance-bearing tangent side, then steers toward the delta-velocity
needed to put the current momentum onto that tangent. Ordinary navigation,
target-planet rendezvous, and sun handling remain on their existing
controllers. A broader predictor was evaluated and rejected: applying
one-frame tangents to every body reduced captures and increased losses in
`strategy-v1`, so the production policy keeps only the measured departure
re-entry guard.

Departure itself remains an atomic port maneuver until the launch planet has
90 units of hull clearance. An experiment that allowed another celestial body
to preempt an undocked departure looked safer locally, but deterministically
left the seed-4 navigation ship alternating between sun avoidance and
`Depart` for 24,856 ticks. The production ordering therefore completes the
known port corridor first, then enables ordinary avoidance and the bounded
origin re-entry guard. If the full ship is destroyed before that transition,
its pod invalidates `Depart` and immediately reselects an owned rebuild port.

The initial deterministic priorities are:

- an escape pod must rebuild at an owned port, or evade if none exists;
- a ship below 50% health selects an owned port and remains committed until it
  reaches 90%;
- an actively contested owned planet triggers defense;
- capture remains the ordinary expansion objective;
- combat can displace capture after commitment when a nearby full ship is
  sufficiently vulnerable;
- an enemy escape pod does not look like an easy low-health ship—the bot takes
  its remaining territory instead of entering an endless pod chase.

Attack, capture, repair, and defense weights live in `RuleStrategyConfig`, so
later personalities can tune the same declared decision model. Telemetry keeps
the persistent strategic goal, target, selected utility, all best goal-class
scores, selection tick, age, and reason separate from the active per-tick
`BrainGoal` maneuver.

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

The named `strategy-v1` suite uses ordinary health and no random asteroids. For
each of four seeds it runs the rule policy in both sides against an idle seat
and then in self-play. Episode and aggregate reports count objective selections
and ticks spent idle, surviving, attacking, capturing, repairing, defending,
and rebuilding. The first run exposed a policy that chased an enemy escape pod
for the remainder of a 300-second episode; the resulting focused regression
now requires territorial capture to outrank that chase.

The evaluator is allowed to read authoritative state to measure captures,
ship losses, rebuilds, eliminations, collision incidents, docking, departures,
and outcomes. A ship loss that occurs on the same simulation step as a
mechanical planet or sun impact is counted separately, and batch reports also
aggregate these metrics by versioned controller policy so side-swapped suites
remain interpretable. Body and ship collision incidents re-arm after 30
contact-free ticks, preventing a sustained or briefly flickering scrape from
dominating the count; debris and laser hits are already discrete events.
Docking contact transitions count port sessions, while each capture or rebuild
opens its own pending departure window. A window completes only after the
craft reaches 90 world units of surface clearance beyond its collision hull. Keeping these
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
boundary. Rule policy `rule_ship_v2` introduced a departure maneuver that does
not apply the omnidirectional brake
while aligning for launch; spaceport contact already damps and centers linear
motion. Later physical regression work made that settling state explicit: a
braking full ship or any accepted pod establishes a moving-frame kinematic
hold at the safe docking anchor. A held full ship uses a body-sized circular
solver collider throughout capture, repair, and launch alignment, while a
sensor copy of its normal hull preserves the established port footprint. Ship
thrust releases the constraint and restores the full dynamic collider; turning
or a transient sensor gap does not. Escape pods retain their ordinary compact
hull, and an unbraked full ship can cross the sensor without latching.

Rule policy `rule_ship_v3` accounts for the pod actuator's different meaning:
releasing its brake provides automatic forward cruise, while applying the
brake damps angular as well as linear velocity. A pod therefore suppresses a
requested brake while it has a meaningful heading correction during body or
hazard avoidance, rebuild-port navigation, and fallback survival. Unlike a
ship, it cannot hover while matching a moving staging ring's velocity. Pod
rendezvous and approach transitions are therefore position-driven geometric
waypoints; ingress still has to establish a real, ownership-approved spaceport
contact. If that contact is lost before the eight-second rebuild completes,
the remembered `Docked` phase returns to `Ingress` and actively reacquires the
port. V5 extends that contact-authority rule to incomplete capture and repair.
Full-ship staging and capture behavior retain their position-and-velocity
requirements. Body-avoidance telemetry records whether the active obstacle is
the sun or a stable planet ID together with signed hull-to-surface clearance,
outward speed, predictive closest time/clearance, maneuver age, stalled ticks,
and escape-assist state. The
interactive control-socket status also includes the episode seed and current
life fraction so a live failure can be identified and reconstructed. Escape
assist is intentionally narrower than ordinary avoidance: only a deep scrape
against a planet outside a valid port corridor can arm it. Its normal timeout
is five seconds without meaningful clearance progress. A ship at or below 50%
life instead arms after one second when its collision circle is inside the
surface and its relative motion is still inward. If the stern is
within 0.65 radians of the outward heading, that emergency releases the brake
and applies reverse thrust while continuing to steer; otherwise it retains the
partial-brake turn until either end of the ship is usefully aligned. Planned
spaceport ingress, healthy transient grazes, and sun avoidance retain their
established controllers.

The interactive seed-0 failure that motivated the emergency path is preserved
as both an observation-level and physical regression. Its captured state has
32.3% life, -3.096 surface clearance, -0.195 outward speed, a -2.55-radian
heading error, +0.441 angular velocity, and 158 stalled ticks while repairing
at one planet and avoiding another. Replaying the former partial-brake,
zero-thrust action from that fixture destroys the ship before it clears the
surface; the rear-aligned reverse action must clear it without changing form.
An observation-level companion regression makes that same obstacle the active
repair target and requires the emergency state to keep aging rather than reset
to zero. This preserves the safety hierarchy across the exact strategy switch
seen in the interactive recorder.

Interactive rule bots also maintain a bounded, typed flight recorder. It
samples at 10 Hz and immediately on avoidance, physical contact,
assist, form, or docking transitions. Each typed sample contains strategy and
guidance telemetry, issued intent, form/life/wings, docking and collision
state, world position/velocity/angular velocity, and a linear five-second
closest-approach prediction for the most threatening sun or planet. A rolling
18-second window describes current behavior. Mechanical body contact or an
escape-assist transition starts a separate bounded encounter capture with
about six seconds of pre-trigger history and twelve seconds after it; ordinary
accepted spaceport contact does not count as a crash trigger. A mechanical
contact incident rearms only after 30 contact-free ticks, so a flickering
Rapier manifold remains one encounter and cannot overwrite its true lead-in.
The episode
seed, world radius, asteroid rate, planet setting, tick rate, and starting
health are recorded alongside it.

Collection is deliberately data-only: the recorder uses bounded buffers,
reuses its rolling allocation during normal sampling, and performs prediction
only on sampled ticks. It emits neither per-frame text nor files, and it cannot
alter the brain observation or intent. When the host enters pause or game over,
it formats the frozen capture into the existing control-socket `status`
response. This gives live failures a useful timeline without adding logging
noise or perturbing the normal render/simulation path.

The pod rebuild regression runs seeds 0 through 5 in 10,000-radius generated
worlds, targets outer `PlanetId(9)`, and starts with the tangential fixed-speed
flyby that previously produced a permanent orbit. Every run must observe
approach, physical docking, at least 480 docked ticks, restoration to a full
ship, departure, and 90 units of safe surface clearance. A small port may
establish real contact directly from `Approach`, so the test does not require a
purely logical `Ingress` sample. Focused contact-loss tests separately verify
that incomplete capture, rebuild, and repair return to ingress instead of
remaining logically docked in empty space.

The former seed-4 characterization is now a bounded regression test. Within
4,000 ticks Player 2 captures planet 3, reaches the evaluator's 90-unit safe
clearance within one 300-tick trace heartbeat, and has no unfinished capture
departure at episode end. The `rule_ship_v3` six-seed baseline recorded 36
captures and 36 safe capture departures. `rule_ship_v4` is compared against
that result rather than silently replacing it. The initial strategy slice
recorded 39 captures and 39 safe departures; with deep non-target scrape
recovery, the current run records 52 captures and 52 safe departures. Neither
previously observed navigation deadlock returned.

With the final v5 contact authority, physical docking hold, and bounded
departure re-entry guard, the same six-seed suite records 77 captures, all 77
followed by safe capture departure. It also records one planet-impact ship
loss and one successful rebuild. That is substantially more throughput than
the v3 baseline and no unfinished capture departure, but it is not a zero-loss
claim. The exact policy ID is part of every result so this comparison does not
silently replace the v4 baseline.

The ordinary-health `strategy-v1` result is deliberately less flattering. The
final v5 run records 40 rule-policy captures, nine ship losses—three coincident
with planet impacts and six with sun impacts—and six rebuilds. The targeted
interactive planet trap and stale post-destruction departure are now preserved
as regressions, while this suite makes clear that total body survival and sun
avoidance remain unsolved. Capture throughput and recovery improved during the
slice, but those are not substitutes for reducing the measured loss count.

## Next slices

1. Give a repeatedly obstructed rendezvous/ingress a bounded failure outcome
   and target cooldown, then measure the seed-4 sun/port overlap rather than
   letting tactical avoidance and strategic capture alternate indefinitely.
2. Tune strategy weights and thresholds through `strategy-v1` plus interactive
   play-testing, preserving explicit policy versions and comparison artifacts.
3. Add declared personality configurations only after the default policy has
   understandable capture/repair/defend/combat tradeoffs.
4. Promote additional scenario transitions to typed evaluator events when
   state-difference metrics are no longer expressive enough.
5. Add policy adapters and batched observation storage only after the rule path
   gives us stable semantics and measurable workloads.
