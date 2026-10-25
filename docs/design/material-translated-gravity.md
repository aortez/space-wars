# Translated native gravity scenes (#327)

`PhysicsWorld::step_material_flat_gravity` supports a bounded common translation
of the qualified pair and three-plate stack. It retains the
[unequal-mass contract](material-unequal-mass-gravity.md), physical geometry,
positive +x gravity, locked rotation, safe support and transactional refusal.
The subsequent [quarter-turn extension](material-quarter-turn-gravity.md) admits
exact cardinal orientations with the same bounds. Moving supports remain separate.

## Frame and admission

The fixed base determines the frame. In f64, subtract 1 from its x coordinate
for a pair, or 2 for a stack; its y coordinate is the common tangent offset.
Each offset component must be exactly representable in f32 and in **[-16, 16]**.
All body centers must have exactly that y coordinate. The stationary role
centers are exactly 1 and 2 units from the origin in f64. A's relative x is at
most `1e-6`; its gap is `-x`. Independent rounding of nominal placements must
still satisfy this actual geometry. The offset bound is on the frame, not every
body center: a stack's base can therefore have world x=18.

This is an admission envelope, not a guarantee of success at every point or for
every trajectory. In particular, small motion at a large x offset may be
unrepresentable. Refusal preserves the original physics, material capacities,
owner state, saved numerical history, diagnostics and previous output.

Refreshed dynamic masses remain in [5, 80], with maximum stack ratio 4:1. The
[pair](material-native-gravity.md) and [stack](material-native-stack-gravity.md)
restrictions on topology, material bindings, force drivers, friction, rotation,
CCD and safe supporting loads remain in force. Gameplay defaults are unchanged.

## Physical and numerical accounting

Arrival times, potential energy, initial numerical scales, retained excursion
energy, gap bounds, support/settlement eligibility and departure evidence all
use x relative to the touching center. Subtract f32 world coordinates in f64;
do not first round the local coordinate to f32. Backend integration still runs
at the actual world positions. No scene recentering or hidden pose correction
is performed.

The existing drift uncertainty formula uses the backend's world x coordinate.
That uncertainty can grow with translation and can prevent a departure or
contained settlement from being certified. The physical work, impulse and
position-defect budgets remain `1e-5` times their original **local** scales.
Translation cannot turn absolute potential energy into collision work, enlarge
an allowance, or reset an owner's history.

A flight that exceeds those bounds refuses. An analytically timed arrival whose
rounded endpoint remains outside the contact band also refuses rather than
committing an unaccounted approach. Safe stationary support still has zero
physical work and zero damage; its temporary kick/projection energies cancel.

## Saved-state compatibility

No serialized fields or lifecycle tags change. Active endpoints already store
the exact world positions needed to infer the fixed frame. The contract revision
and outer version identify the coordinate interpretation:

| Frame and masses | Revision | Pair version | Stack version |
| --- | ---: | ---: | ---: |
| Zero offset, all dynamic masses 20 | 1 | 6 | 7 |
| Zero offset, other admitted masses | 2 | 8 | 9 |
| Nonzero admitted offset, any admitted masses | 3 | 10 | 11 |

Versions 1–9 remain readable. Zero-offset histories keep their original version
and byte layout when restored and continued. Independent frozen serializers
verify both the original mass-20 tails and the bounded-mass v8/v9 tails.
Older readers reject the new versions.

Revision 3 stores the initial relative x in f64; subtracting two world f32 values
need not produce a value representable in f32. Active validation reconstructs
the initial world coordinate from the retained frame and requires an exact
round trip. Other reference values retain their f32 provenance checks, and
scales/errors keep their original validation. Outer version must match both
revision and pair/stack kind; relabeled, unknown and truncated payloads refuse.

Continuation compares exact world endpoints with the saved witness on every
positive call. An edit, even if undone, permanently invalidates the history.
Invalidation drops live endpoint claims but preserves revision, initial scales,
all charged flight defects, stack recoil corrections and the first invalidation
cause. Removing every body cannot erase those ledgers or permit reenrollment.
Pause neither changes policy nor renews the frame or allowances.

## Qualification

- A pair matrix covers 264 configurations at 11 x offsets from -16 to 16 and
  y=3.25, with mass 13, speeds 0/0.01/0.5/4, one or partitioned/duplicated
  rectangles, reversed insertion and 1/4/8 solver iterations. 188 succeed and
  76 explicitly refuse. Independent ballistic work, actual rounded starting
  gap, ordinary zero-gravity impact response, capacities, native exact time,
  same-build replay and whole-call rollback are checked.
- The corresponding 264-case stack matrix uses masses 13/31. 186 succeed and
  78 explicitly refuse. Every successful native schedule/report matches the
  independently scheduled fixture, including support ownership, corrections
  and final state. Analytic arrival and ordinary contacts additionally check
  the physical response.
- 24 pair and 24 stack three-impact histories cover 30/240 Hz, positive and
  negative x translations, y offsets through both bounds, unequal masses and
  partitioned/duplicated geometry. They preserve distinct impact owners,
  ordered departures, the original B-C owner, accumulated work and replay.
- 384 translated resting/impact configurations cover all six stack source
  orders, both insertion orders and two collider layouts, comparing ordinary
  contact work and mass-weighted support reactions.
- Persistence tests cover flight, impact, rest and settlement, zero-time pause,
  terminal invalidation after all sources disappear, non-f32 local reference
  coordinates, unknown/cross-labeled versions, truncated data, frame/owner/
  budget corruption and stale raw endpoints. Public-only integration covers
  masses 5/13/20/80, stack ratios through 4:1, edits restored to their old pose,
  new and old versions, and late subthreshold-arrival rollback.

Some slow approaches and tiny hops refuse at larger offsets while equivalent
small-offset scenes succeed. The matrix keeps those refusals as precision
regressions; no work tolerance, departure threshold or solve cap was relaxed.
Samples establish bounded evidence, not universal translation invariance in f32.
General oblique orientations, moving/rotating supports, broader geometry and measured
sustained deformation remain separate work under #327.
