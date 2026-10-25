# Quarter-turn native gravity scenes (#327)

`PhysicsWorld::step_material_flat_gravity` supports exact cardinal orientations
of the qualified pair and three-plate stack. The plate frame and gravity rotate
together through 0, 90, 180 or 270 degrees counterclockwise. Bodies remain
rotation-locked; this does not admit angular motion or moving/rotating supports.
The [translated-scene](material-translated-gravity.md) and
[unequal-mass](material-unequal-mass-gravity.md) bounds remain in force.

## Constructing an exact orientation

Use `world::QuarterTurn` and `PhysicsWorld::set_quarter_turn_pose` before enrolling
the scene. `QuarterTurn::rotate` maps a local vector into the chosen orientation
using only component permutation and sign changes:

```rust
use engine_core::Vec2;
use engine_rapier::world::{BodyId, PhysicsWorld, QuarterTurn};

fn place_floor(world: &mut PhysicsWorld, id: BodyId, local_position: Vec2) {
    let turn = QuarterTurn::Deg270;
    world.set_quarter_turn_pose(id, turn.rotate(local_position), turn, true);
    world.set_gravity(turn.rotate(Vec2::new(10.0, 0.0)));
}
```

Apply the same turn to every plate's pose and linear velocity. Local collider
shapes/offsets stay attached to their bodies. A world offset may be added after
rotation; its components must still satisfy the existing [-16, 16] bound.

The pose setter constructs an exact unit rotation. Ordinary radian-based
`BodySpec::angle` and `set_pose` are unchanged: a floating-point approximation of
pi/2 can have a nonzero cosine, so it is not an exact cardinal pose. Admission
checks the actual backend rotation and does not snap nearby angles or relax a
tangent-motion check. The new setter uses the same continuation invalidation
policy as `set_pose`: an actual change, even if later undone, invalidates the
history. Nonfinite positions refuse without editing; an identical pose is a
no-op for the saved witness.

## Frame and accounting

The fixed base supplies both the cardinal orientation and the origin. Subtract
its role distance (1 for a pair, 2 for a stack) along the signed normal axis in
f64. The resulting world offset must be exactly f32 representable and within
[-16, 16] on each axis. Stationary centers must have exact local role spacing;
all centers share the tangent coordinate. Gravity is positive along local +x,
and every velocity has zero tangent/angular component.

Flight, arrival, departure, potential energy, impact speed, support kicks,
recoil and projection accounting use signed local coordinates. Backend motion
and contacts remain at the actual world poses. Drift uncertainty uses the
world component along the moving axis, including its magnitude at a translated
origin. Local numerical budgets remain `1e-5` times the initial scales; no
energy tolerance, solve cap, contact threshold or departure threshold changes.

Every admitted normal velocity and position change is checked after the solve.
Tangent motion, changed orientation, moved stationary support, missing response
and unrepresentable work still refuse the entire call. A cardinal change cannot
rebase a ledger, create another impact owner or hide lost energy. Safe rest has
zero physical work/damage; bounded numerical settlement remains separate.

## Saved-state compatibility

No serialized field or lifecycle tag changes. Active endpoint angles already
carry the cardinal orientation, and endpoint positions retain the world frame.
History revision 4 identifies the nonzero quarter-turn coordinate interpretation:

| Orientation and offset | Revision | Pair | Stack |
| --- | ---: | ---: | ---: |
| Original +x, zero offset, mass 20 | 1 | 6 | 7 |
| Original +x, zero offset, other admitted masses | 2 | 8 | 9 |
| Original +x, nonzero admitted offset | 3 | 10 | 11 |
| Exact 90/180/270 degrees, any admitted offset/masses | 4 | 12 | 13 |

Versions 1–11 remain readable. Existing +x histories retain their versions and
bytes on restore and continuation; independent frozen serializers cover the
v6–v11 payload layouts. Older readers reject v12/v13. Outer version, revision
and pair/stack kind must agree. Active validation also checks all endpoint
orientations, tangent motion, the retained frame and exact reconstruction of
the initial f32 world coordinate from its f64 local reference.

Pause does not select a new orientation, policy or budget. Invalidation retains
the first cause, revision, initial scales, charged flight defects and stack
recoil corrections even after every body is removed. An invalidated history
cannot enroll again. Corrupt, truncated, cross-labeled and unknown versions
refuse; edits bypassing public invalidation still fail the live witness check.

## Qualification

The unrotated independent ballistic/stack schedulers remain separate from the
native frame implementation. Test setup rotates only the native physical scene;
independent component permutations convert its measured endpoints back for
comparison. Analytic arrival/work and ordinary zero-gravity contact references
provide additional physical checks.

- Pair and stack matrices each cover 264 configurations in each of four
  orientations: 11 translations, 1/4/8 solver iterations, four incoming speeds,
  unequal masses and solid/partitioned/duplicated geometry. Pair outcomes are
  188 successes and 76 explicit precision refusals per orientation; stack
  outcomes are 186 successes and 78 refusals. Refused calls preserve all state
  and previous outputs. Successful stack reports/scheduling match the
  independent fixture exactly; pair comparisons retain the existing measured
  fixture-tail bounds.
- Each topology has 96 three-impact histories across the four orientations,
  30/240 Hz, translated origins and unequal masses. They check real departures,
  distinct later owners, accumulated work, fixed numerical allowances, retained
  B-C support ownership and exact same-build snapshot replay.
- Another 1,728 nonzero-orientation stack rest/impact configurations exercise
  all six source orders, both insertion orders, two collider layouts, four
  mass pairs and zero/two translated origins. Ordinary contacts and
  mass-weighted support reactions remain the references.
- Public-only tests cover 384 orientation/translation/layout/mass/phase cases,
  exact local report equivalence and world endpoint permutations, replay,
  invalid input, geometry/gravity/tangent refusals, late subthreshold-arrival
  rollback and pose-edit invalidation.
- Persistence tests cover flight, impact, rest, settlement, pause, terminal
  histories, non-f32 local coordinates, frame/ledger corruption, stale raw
  endpoints, all version relabelings and truncated payloads.

These are bounded cardinal scenes. General oblique orientations, freely rotating
faces, moving supports, broader geometry/CCD and measured overloaded deformation
remain outside this operation. Gameplay defaults are unchanged; #327 is open.
