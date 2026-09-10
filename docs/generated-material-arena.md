# Generated material arena

This extends [controlled two-planet missions](two-planet-missions.md) and
[ship return recovery](ship-return-recovery.md) into a bounded generated world.
The launcher offers `spacewars-terrain-arena` (P1 human, P2 mission bot) and
`spacewars-terrain-arena-duel` (two mission bots). Restart reproduces the seed.
The ordinary `spacewars` match and its frozen AI policies remain separate.

## World and shared gameplay

The existing generator creates the initial layout. The arena keeps its first
three planets with their generated radii, orbital spacing and angular positions,
then trims the world boundary and applies the established `SurfaceV1` profile.
This gives the outer planet 200 units of flight margin and preserves the
profile's surface gravity, bounded spin and orbital motion. P1 starts above
planet 0 and P2 above planet 2, on their outward faces. Mirrored headless trials
reflect the layout and reverse orbital and surface spin. Only construction
places actors; all later motion and actions use the shared simulation.

All three planets use material terrain. Natural landing, on-foot capture,
mining, weapon energy and visible missile rails, jetpacks, damage, pods and
replacement ships use the existing mechanics. Destroying or detaching a flag's
footing removes the flag and neutralizes the planet. Asteroid settings apply
across all three planets. There remains one shared physics step and gravity
solve. This playtest still has invulnerable pods and spacelings and no match
victory or elimination screen.

The mission bot is `material_mission_v2`. It keeps the existing destination
selection, local capture and recovery tasks. Its read-only world observation
now includes the sun as a flight obstacle; it uses the existing arc waypoints
for both the sun and intervening planets. Neither bounds nor waypoints grant
landing or claim eligibility. Telemetry identifies the obstacle and waypoint.
This is local obstacle avoidance, not a global route planner.

## Failures exposed by the first seeds

The first twelve quiet desktop trials (six seeds, both seats) completed all
three capture-and-departure trips in three cases. Four made no completed trip.
All physical audits passed. Traces identified two assumptions that the moving,
varied-size planets made visible:

- A boarded ship could remain grounded while departure guidance waited for it
  to rotate toward a lateral route. Foot contacts resisted that rotation. The
  shared committed-descent policy now uses ordinary radial lift until its feet
  clear the ground. It also accepts a physically valid landed hatch even when
  that landing differs from its proposed cover site.
- Claim anchoring converted an earlier solver world contact into the planet's
  newly integrated pose. On an orbiting planet, this could place the anchor
  outside surviving material and repeatedly reject a real standing contact.
  Physics now exposes the supporting-body-local point and normal from the
  actual manifold. Claims use that local anchor, retaining the occupied-cell,
  balance and relative-speed checks. Existing solver world contact fields
  remain unchanged for movement and landing consumers.

The second twelve-case quiet batch completed at least one trip in every case,
with five completing all three. Some remaining approaches consume their retry
budget, and claims are not equivalent to boarded departures. The final matrix
below records these outcomes separately. The shared capture policy version is
`tactical_sortie_v5`; ground navigation remains `ground_navigation_v8` and
recovery remains `recover_ship_v7`.

## Microscopic breakup debris

The first full generated desktop matrix exposed a collider-lifecycle assertion
in eight duel subjects (four seeded worlds) with 3-second Mixed asteroid
arrivals. A diagnostic replay located a live, non-damaging breakup triangle
with radius 0.0000142 and 81% health. Repeated grazing damage compounded its
existing shrink rule until convex-hull construction rejected the replacement;
gravity then attempted to address a body that had not been inserted.

Breakup triangles now retire as dust below radius 0.1, before losing usable
collider geometry. The ordinary cleanup path removes their bodies and mappings,
and suppresses further breakup. This is distinct from excavated material
fragments, whose retained-plus-removed accounting and physical lifecycle remain
unchanged. A lifecycle regression repeatedly applies tiny positive damage and
checks live body access followed by complete removal. Two generated asteroid
duels also run for three simulated minutes in the regular test suite.

An initial attempt to make breakup size proportional to original health kept
larger wreckage in play and failed an established pod-recovery regression. The
bounded retirement rule preserves the existing breakup behavior at useful
sizes. Debris insertion now asserts at the actual failing lifecycle operation
with the offending state, rather than first failing during the gravity solve.

## Validation design

Decision checks cover sun and owned-planet avoidance, read-only observations
and repeated-tick behavior. A manifold regression checks both collider orders
and a supporting body that translates and rotates during integration. Physical
regressions exercise seeded orbiting-ground claims and departures. Client tests
cover both new scene registrations, seat ownership, pause/reset and both renderers.

The generated matrix runs 48 cases per platform for 180 simulated seconds:
24 quiet (six seeds, both seats, both reflections), 12 against an interceptor,
and 12 with two mission bots and 3-second Mixed asteroid arrivals. Seeds are
0, 1, 2, 3, 7 and 42. All cases must retain finite physics, bounded speed and
conserved retained-plus-removed material. Reports distinguish any completed
trip, three distinct completed trips, planets ever owned, final ownership,
recoveries and blocked outcomes. They retain layouts, per-second observations,
obstacle avoidance, mission events and renderer frames.

The fixed-world regression matrix remains: 52 missions, 36 impact recoveries
and 24 assigned-ship return trials per platform. Quiet fixed-world routes must
finish both planets, impact fixtures must recover and depart, and return
fixtures must board and depart. Full workspace/example checks, real client
lifecycle workflows, and frozen navigation/strategy baselines complete the
regression gate. Headless simulation timing is separate from live Pi rendering.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 2 --seat 1 --mirror false --mode quiet \
  --seconds 180 --frames true --out /tmp/generated-arena
```

Use `--require-route true` to require three distinct completed sorties in a
particular generated replay. It is deliberately separate from the physical
audit gate across the learning matrix. Optional `--trace true` adds decision
and contact diagnostics.

Artifacts and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/generated-material-arena-20260909/
```
