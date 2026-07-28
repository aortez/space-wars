# Physics architecture

Status: implemented for Pizza, Rover Lab, and Spacewars.

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
mutual gravity is computed by an engine force system and applied as
`mass * acceleration` before the Rapier step. Collision broad-phase remains
Rapier's responsibility; a Barnes-Hut tree accelerates gravity, not collision.

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
keeps broad-phase, narrow-phase, island, solver, lifecycle, projection, and
presentation costs separate.

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

Spacewars' implemented mapping is:

- planets and orbiting spaceports: fixed or kinematic bodies;
- ships, escape pods, asteroids, and collision-relevant debris: dynamic bodies;
- spaceport and capture volumes: sensors with explicit collision roles;
- thrusters, braking, gravity, and ejection: forces or impulses;
- cannon shells: dynamic bodies with selective CCD;
- lasers: ray casts;
- damage: a deterministic rule over normalized contact impulse events;
- visual particles and trails: lightweight scenario storage.

Planets are kinematic assemblies rather than hollow circle outlines. A solid
inner disk and convex annular sectors leave one physical spaceport cavity. The
port is a sensor, the inner bay wall remains solid, and an ownership-aware gate
prevents an unauthorized escape pod from crossing the opening. This lets a
normal ship dock through geometry while also resolving objects that spawn or
teleport wholly inside planet material.

Spacewars keeps motion fields in its public scenario state as a post-step
presentation snapshot and as an explicit command staging surface for controls,
scripted ejection, and tests. Reconciliation only writes deliberate changes
back to Rapier; no scenario code advances a registered body's position or
rotation. Rapier advances every rigid body once and its normalized, stable-ID
contact impulses drive gameplay damage.

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
spawn/removal mapping cleanup, a solid planet interior, a real port cavity, and
the ownership-aware pod gate.
