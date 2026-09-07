# Physics architecture

Status: implemented for Pizza, Rover Lab, Spaceling Lab, and Spacewars.

## Decision

Physical scenarios use one canonical Rapier-backed physics world. Rapier is
authoritative for every rigid body's pose and velocity and for contact,
constraint, and joint resolution. Scenario state owns gameplay data and refers
to physical objects with stable engine IDs; it never integrates a second copy
of a Rapier-owned body.

Rapier is an implementation detail of `engine-rapier`. Scenarios exchange
engine-native values, IDs, descriptions, motions, queries, and events with the
crate. The API is deliberately concrete: there is no runtime-selectable physics
backend and no trait that mirrors Rapier's full API.

The existing Classic Pizza implementation remains a benchmark and behavioral
reference. It is not a second implementation of the application physics API.

## Ownership

Rapier owns:

- rigid-body position, rotation, linear velocity, and angular velocity;
- collider geometry and material response;
- broad phase, narrow phase, contact solving, and sleeping;
- joints, limits, suspension, and motors;
- sensors, ray casts, shape casts, and selective CCD.

The engine or scenario owns:

- stable entity identity and gameplay metadata;
- control intent, scripted motion, and kinematic targets;
- external force fields, including radial and Barnes-Hut gravity;
- health, damage, ownership, docking, capture, and destruction rules;
- deterministic spawning and removal decisions;
- render-only or analytically integrated effects that do not participate in
  general rigid-body contact;
- render frames and agent observations.

A physical entity may be an assembly of several bodies, colliders, and joints.
Collider roles distinguish gameplay meaning within an assembly, for example a
rover wheel, ship hull, or docking sensor.

## State model

`PhysicsWorld` owns Rapier and maps stable `PhysicsId` values to assemblies.
Raw Rapier handles never leave the crate. IDs are never inferred from dense
array positions, and a removed ID is not reused within a scenario run.

Scenario metadata remains in scenario-owned dense storage. Physics motion is
read through allocation-free iterators or direct lookup. A scenario may cache a
read-only presentation snapshot, but that cache is derived state and is never
fed back into integration.

Structural changes use an explicit lifecycle boundary. Scenarios may decide to
spawn or remove entities while processing a tick, but physics changes are
applied outside contact iteration. This prevents stale-handle access and makes
event ordering reproducible.

## Tick order

Every fixed physics tick follows this order:

1. Decode human or agent actions into gameplay intent.
2. Apply queued removals and spawns from the preceding tick.
3. Set scripted and kinematic body targets.
4. Clear transient forces once.
5. Build external force fields from the current authoritative body positions.
6. Apply gravity, propulsion, motors, brakes, and queued impulses.
7. Advance Rapier exactly once.
8. Normalize and sort contact and sensor events by stable IDs and collider
   roles.
9. Apply gameplay results such as damage, capture, destruction, and new spawn
   requests.
10. Render and observe the authoritative post-step state.

Constant gravity may use Rapier's global gravity vector. Position-dependent or
mutual gravity is computed by `engine-gravity` as a per-tick velocity delta.
`PhysicsWorld::apply_velocity_delta` converts that into a center-of-mass impulse
using Rapier's inertial mass before the Rapier step. Collision broad-phase
remains Rapier's responsibility; the Barnes-Hut tree accelerates gravity, not
collision.

The gravity participant model is deliberately independent of rigid bodies. A
stable-ID participant may be:

- a normal source+target mass;
- a source-only scripted body;
- a target-only effect or gameplay body;
- a hierarchical source eligible for Barnes-Hut approximation; or
- a direct source evaluated exactly for every target.

The exact symmetric all-pairs backend is the correctness oracle. The
Barnes-Hut backend uses a deterministic point-region quadtree, f64 mass and
center-of-mass aggregation, Plummer softening, stable-ID ordering, and explicit
self-path exclusion. Dominant suns and planets can remain direct sources while
large populations use the hierarchy. The opening angle θ controls the
speed/accuracy tradeoff.

## Physical entity tiers

Not every visible object belongs in Rapier:

1. **Rigid mechanics**: ships, escape pods, rovers, wheels, asteroids,
   collision-relevant debris, and physical projectiles.
2. **Queries and sensors**: lasers, docking zones, capture regions, triggers,
   and line-of-sight checks. These use the physics query world without
   necessarily adding dynamic bodies.
3. **Lightweight effects**: exhaust, sparks, smoke, stars, and cosmetic
   fragments. These remain in scenario-owned dense arrays.

An object moves between tiers only through an explicit gameplay operation. It
is never simultaneously integrated by Rapier and a lightweight system.

## Determinism, replay, and persistence

- Physics scenarios use a fixed timestep.
- Rapier's `enhanced-determinism` feature remains enabled.
- Entities are inserted and removed in stable order.
- Gameplay-visible physics events are sorted before rules consume them.
- Random decisions use scenario-owned seeded generators.
- Replays store the scenario seed, versioned actions, and configuration.
- Short-lived rollback or local checkpoints may store a versioned opaque
  Rapier snapshot.
- Durable saves and network protocols do not expose raw Rapier serialization.
  Network play should use an authoritative simulation with state correction
  instead of assuming cross-build floating-point lockstep.

## Scale and performance

Each physical scenario declares a consistent world-unit scale used to tune
Rapier's tolerances. Scenarios should keep ordinary dynamic collider sizes near
that scale rather than mixing astronomical and microscopic coordinates in one
world.

The hot API supports reserved capacity, batch lifecycle changes, allocation-free
motion access, collision filtering, sleeping, and selective CCD. Profiling
keeps gravity validation/build/aggregation/traversal, broad-phase, narrow-phase,
island, solver, lifecycle, projection, and presentation costs separate.

The portable baseline is single-threaded, non-SIMD Rapier. Stable SIMD and
parallel features are benchmark variants, introduced independently and only
after behavior and determinism tests pass. The dense-ball benchmark is not a
proxy for articulated workloads; joint-heavy populations get a separate
performance fixture.

## Application model

Pizza exercises bulk bodies, deterministic churn, pointer manipulation, and
external gravity. Rover Lab exercises multi-body assemblies, kinematic terrain,
joints, suspension, motors, and snapshots. Spacewars exercises collision roles,
sensors, projectiles, contact impulses, damage, ownership, and destruction.

These scenarios share the canonical world rather than maintaining specialized
Rapier owners. Domain builders may assemble common objects, but assembly
construction and gameplay policy remain separate from the physics kernel.

Spaceling Lab adds a single-body capsule character with bounded upright and
support-relative movement control. Its support query traverses only contacts
adjacent to that collider, without allocating or scanning all world bodies.
Gravity supplies orientation and acceleration independently of terrain; limbs
are visual geometry. This is an arcade controller, not a ragdoll or a claim
that articulated crowds have been benchmarked.

Spaceling balance is separate from contact support. Strong velocity disturbances
or spin disable upright/locomotion control; the capsule settles with contact
friction before gradually recovering. Off-center impulses use the canonical
world's Rapier mass/inertia binding. Recovery does not introduce another body,
pose snapping, free-space damping, or a terrain scan. See the
[lab's balance model](../spaceling-lab.md#balance-and-recovery).

Surface Sortie's stationary and orbital presets use that same controller and
canonical world alongside a dynamic ship with physical landing feet. Terrain
targets are scheduled once; pre-step gravity/controllers read the completed
physics frame, and post-step landing/services consume completed contacts.
`BodyMotion.position` is the body origin but `linear_velocity` is the center-of-mass
velocity. Use the allocation-free `PhysicsWorld::velocity_at_point` when comparing
motion at a hatch, foot, or frame origin; an asymmetric collider set can offset
the center of mass. Never substitute the next kinematic target or an analytic
endpoint derivative for the contacts' completed motion.

The orbital sortie matches its sun's gravity to the prescribed planet path at
the center; it adds no actor transport or compensating field. This controlled
fixture does not establish support on every ordinary scripted orbit, whose
rate and mass may be inconsistent. See the [motion boundary](../surface-sortie.md#moving-planet-evidence).

Spacewars' implemented mapping is:

- planets and orbiting spaceports: fixed or kinematic bodies;
- ships, escape pods, asteroids, and collision-relevant debris: dynamic bodies;
- rovers: three-body dynamic assemblies whose chassis and independent wheels
  are joined by motorized suspension constraints;
- spaceport and capture volumes: sensors with explicit collision roles;
- thrusters, braking, gravity, and ejection: forces or impulses;
- cannon shells: dynamic bodies with selective CCD;
- lasers: ray casts;
- damage: a deterministic rule over normalized contact-onset events and
  pre-solver closing speed;
- visual particles and trails: lightweight scenario storage.

Rover construction, ownership, health, patrol intent, and death are Spacewars
state. Eligible owned planets advance a normalized six-second construction
state before inserting one rover at the lifecycle boundary. Laser ray hits and
armed debris contacts are mapped back to the stable rover ID; Spacewars turns
those typed events into damage. On death or an ownership change, the assembly
is removed at the next reconciliation and six bounded ordinary debris pieces
inherit the final Rapier motion of the chassis and wheels. No rover pose or
contact response is duplicated in gameplay code.

Pizza balls opt into both sourcing and responding to gravity. Spacewars keeps
its established gameplay policy: suns and planets are direct source-only
bodies, while ships, debris, and visual particles are target-only. Rover Lab
owns a mutable list of direct or hierarchical sources and sends its chassis and
two wheels through the same field. The rover assembly can also be inserted at
an arbitrary pose with no planet or gravity. These are scenario policies rather
than solver limitations; a game mode can opt selected dynamic entities into
hierarchical source mass without changing Rapier ownership.

Planets are kinematic assemblies with one continuous circular ship surface and
a separate smooth traction surface for rovers. A rotating rectangular sensor
pad begins at the surface and extends outward to a low-orbit berth. Full ships
may touch the pad, but only a braking ship establishes the compact kinematic
landing hold used by capture and healing. Owned escape pods establish the same
hold automatically for rebuilding; access is a scenario rule, so an
unauthorized pod simply meets the ordinary continuous planet surface. No ship
needs to cross into planet material to dock.

Spaceport contact has two gameplay phases. `Touchdown` is raw accepted sensor
overlap and supplies moving-frame damping/pull without starting planet
services. `Landed` means Rapier has established the retained hold. Thrust drops
that hold and restores the full dynamic hull at a berth proven clear for every
ship and planet rotation. Contested or weapon-triggered ejection applies
outward velocity until the craft clears the complete external pad corridor.
While the hold is active, the ship is excluded from external gravity targets;
the moving-frame constraint is the sole authority over its motion.

The external berth is an interim compatibility mechanism, not the long-term
landing model. Ships should eventually land naturally on suitable physical
surfaces, with support and relative motion distinct from capture, healing, and
rebuild eligibility. Spaceling Lab will exercise a small contact-based character
before that ship transition. See [spacelings and surface support](spacelings.md).
Future cell-based terrain must not recreate the retired interior cavity or
ownership gate merely to preserve docking assumptions.

Body contact and body impact are separate gameplay events. A contact remains
visible for as long as Rapier reports the pair. An impact occurs only when a
pair starts (or has been absent long enough to rearm), and its speed is the
pre-solver relative closing speed at the contact point, including angular
motion. Spacewars applies planet damage only at impact onset and only above the
30-world-unit-per-second safe-contact threshold. This prevents ordinary
gravity support impulses from repeatedly draining a resting ship while
preserving damage from genuine high-speed crashes.

Spacewars keeps motion fields in its public scenario state as a post-step
presentation snapshot and as an explicit command staging surface for controls,
scripted ejection, and tests. Reconciliation only writes deliberate changes
back to Rapier; no scenario code advances a registered body's position or
rotation. Rapier advances every rigid body once and its normalized, stable-ID
contact events drive gameplay damage. The client retains current landing and
body-contact state plus the last body impact for both human and bot players so
a paused live session exposes the event that led to its current state.

## Acceptance criteria

The architecture is established:

- Pizza and Rover Lab use the same `PhysicsWorld` and no specialized world
  owns a second Rapier pipeline;
- interactive and benchmark Pizza use Rapier without duplicated mechanical
  state;
- removing and recreating large populations does not leave stale mappings;
- snapshot restore resumes equivalent same-build behavior;
- identical seeds and actions produce identical observations in determinism
  tests;
- the existing dense and churn benchmark counters remain available;
- Spacewars can express ships, planets, ports, and their contact roles without
  exposing Rapier handles to scenario code.

The acceptance suite additionally verifies deterministic Spacewars continuation
for identical actions, same-build physics snapshot equality, debris
spawn/removal mapping cleanup, one continuous planet surface, an external
surface berth, explicit touchdown/landed transitions, safe full-hull release,
owner-only pod landing, non-damaging sustained surface support, and damaging
high-speed impact onset.
