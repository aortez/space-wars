# Spacelings and natural surface support

Status: first Spaceling Lab slice implemented and manually playtested. Tracks
[issue #41](https://github.com/aortez/space-wars/issues/41).

## Naming

A **spaceling** is the small humanoid creature, whether controlled by a player
or a bot. This names the organism, not its role: a spaceling can walk, pilot a
ship, or operate a rover. **Entity** remains the generic engine term for an
object or assembly. The reusable mechanics live in `engine-rapier::spaceling`;
the dedicated testbed is **Spaceling Lab** (`spaceling-lab`).

## First slice

Build a playable `spaceling-lab` scenario, using one dynamic capsule per spaceling in
the canonical Rapier world. Arms and legs are animated render geometry, not
separate physical bodies. This keeps the baseline small enough to measure
before considering articulated crowds or ragdolls.

Gravity comes from `engine-gravity`, independently of terrain geometry.
Opposite gravity defines up; with no gravity, retain the last valid up direction.
Walkable contact normals define support. A nearby planet, wall contact, or
sensor overlap alone cannot make a spaceling grounded.

Walking applies bounded acceleration toward a tangential target velocity,
relative to the supporting body's velocity at the contact point (including
rotation). Air control is weaker. Upright control is bounded and acts on angular
velocity, not pose. A fresh jump press while supported adds outward velocity;
holding the button through a landing does not repeat the jump. Ordinary gravity
and Rapier contacts remain responsible for flight and landing.

The lab starts on a small rotating planet with a shallow ramp/obstacle. Use
keyboard left/right (or A/D) and Space, or NES d-pad left/right and A. Diagnostics
show grounded state, support identity, relative speed, gravity, and jump count.
The usual host controls pause, restart, and return to the launcher.

Run instructions and diagnostic definitions are in [Spaceling Lab](../spaceling-lab.md).

Acceptance tests cover both directions, jump/land, slopes, moving/rotating
support, wall rejection, removal of support, zero gravity, and deterministic
action sequences. Visual verification must also check both renderers and
readability at the normal client viewport.

The first slice passes workspace tests, the six real-client functional
workflows, and Rust 1.89 checks. Desktop vector and software-raster screenshots
were visually checked. The full-lap test covers both obstacles in both walking
directions. Physical gamepad feel remains a manual acceptance check.

## Boundaries

`engine-rapier::spaceling` owns the reusable body/controller and contact queries;
the scenario owns input decoding, gravity participants, fixture construction,
and presentation. Rapier poses/velocities remain authoritative. No new Rapier
pipeline, per-spaceling terrain scan, or second position integrator is introduced.

This is a contact-based arcade controller, not a biomechanical simulation.
Automatic stair climbing, coyote time, jump buffering, ragdolls, NPC policy,
inventory, damage, ship entry/exit, and Spacewars integration are follow-ups.
Do not claim crowd performance from a single-spaceling demo; benchmark populations
before choosing more expensive mechanics.

## Planet and ship direction

The continuous surface and external berth from PR #40 fix the interior-bay
problems, but the special landing area is an interim compatibility mechanism.
Long term, ships should land naturally on suitable physical surfaces. Surface
normal, support-relative motion, clearance, and pilot intent should establish
landing, independently of ownership or services.

Capture, healing, and pod rebuilding remain explicit scenario policies.
Natural contact anywhere must not automatically provide those services.
Replace the ship berth in a later slice, with launch/landing, damage, bot, and
moving-planet regressions; Spaceling Lab does not change those rules.

[Cell-based terrain (#16)](https://github.com/aortez/space-wars/issues/16)
should start from the continuous exterior, not recreate a cavity or ownership
gate. Bases and mining machinery belong on supported exterior geometry.
Terrain removal must invalidate physical support and explicitly reconcile
infrastructure and service eligibility. Spacelings and rovers provide small testbeds
for that contact contract before it is used by ships and planetary bases (#13).
