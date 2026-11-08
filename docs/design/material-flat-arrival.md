# Flat-face arrival qualification (#327)

The fixture in `response/tests/flat_arrival.rs` schedules initially separated,
parallel rectangular plates without speculative braking. It exercises the
existing material response at the physical arrival, including transmission
through an already resting third plate. It now independently qualifies the
[native flat-face operation](material-native-flat-arrival.md), whose complete
scene admission verifies the geometry needed for internal-seam filtering. Its
findings also led to the physical-patch impulse application fix in the opt-in
[material response](material-native-response.md), described below.

## Geometry and scheduling

Construction supplies two or three solid plates with half-extents `(0.5, 1)`.
All plates have the same fixed orientation and locked rotation. They have no
gravity, forces, friction, damping, joints or CCD. Each plate has mass 20 when
dynamic and one registered material region. The last plate is wood; the others
are armor. The last body may instead be fixed or translate kinematically.

Each plate can be one, two or four contiguous slices along its tangent, with
one or two coincident copies of every slice. Dividing mass among the colliders
preserves total mass. The geometry check proves that each body's collider union
is exactly the solid rectangle `[-0.5, 0.5] x [-1, 1]` in body coordinates.
Gaps, shifted face planes, non-cuboids or local rotations fail that certificate.
This is a narrow fixture certificate, not general compound-shape admission.

The normalized common face normal `n` and tangent `t` give, for each ordered pair:

```text
gap = dot(center_j - center_i, n) - 1
approach = dot(velocity_i - velocity_j, n)
overlap_width = 2 - abs(dot(center_j - center_i, t))
arrival_time = gap / approach
```

Positions and rounded response velocities are promoted to f64 for these
calculations. Relative motion must be normal to the faces, allowing only
rounding-scale tangential residue (`16*EPS*(abs(v.x)+abs(v.y)+1)`). Width must
exceed `1e-5`; corner entry and sliding along an edge are outside this experiment.
Approach remaining at a gap at most `1e-6` after response preview is an explicit
`Unaccounted` refusal, even when that velocity is tiny.

Each frame stages a clone and allows at most eight previews and eight actual
advances. Speculative solver points are removed. The zero-duration observation
in each preview computes response velocities without consuming capture time.
The actual advance must reproduce those velocities and material outcomes,
and ordinary solving must leave the velocities unchanged. Each actual endpoint
must match constant drift within the existing per-coordinate allowance
`8*EPS*(abs(start)+abs(velocity*dt)+1)`; penetration beyond `1e-5` is refused.
All advances retain their real duration in capture output. Any refusal restores
the whole original frame, including earlier material work and previous outputs.
An arrival exactly at frame end responds on the next positive call.

The base fixture uses ordinary episode separation grace. An optional,
independently tested [departure tracker](material-flat-departure.md) now
qualifies rapid departure/reimpact and independently checks the native path's
persisted departure evidence and event ordering.

## Internal collider seams are not physical boundaries

A rotated, translating plate represented by multiple slices can produce solver
contacts against the slices' internal faces. In the retained reproduction,
rotation `0.37`, common velocity `(17, -11)` and tangent offset `0.5` give both
the physical face normal and its perpendicular seam normal. Two plates split
`[4, 2]`, or three split `[4, 2, 4]`, with duplicate colliders and reversed
insertion expose this case.

The unfiltered response refuses `AmbiguousPatch`. These are different face
normals, not small floating-point variations of one normal. Approximate normal
merging would conflate distinct geometry.

After proving the filled rectangle union, the fixture removes only contacts
whose outward tangent face lies strictly inside that rectangle. A face at
either outer tangent boundary remains visible. With that certificate and
filter, the same reproductions succeed with widths 1.5 at A-B and 2 at B-C.
Separate checks preserve outer faces and refuse incomplete or misaligned unions.
The native operation now supplies its own complete-scene geometry certificate
before filtering. Ordinary stepping cannot assume that a compound collider has
a filled interior.

## Positive evidence and physical-patch impulse application

Seven regression tests include:

- **Two plates: 1,280 successful combinations.** Five matching or mismatched
  slice layouts, 30/60/120/240 Hz, one/two copies, both insertion orders,
  orientations 0/0.37, common velocities `(0, 0)`/`(17, -11)`, tangent offsets
  0/0.5, and dynamic or prescribed last bodies.
- **Supported three plates: 1,280 successful combinations.** Five layouts and
  the same rates/copies/orders/orientations/common velocities/tangent offsets,
  with dynamic, fixed and kinematic final supports.
- The internal-seam reproductions above, two former supported closing-residual
  failures, unrepresentable rebound, exact frame-end arrival and pause, invalid
  time, incomplete geometry, tangential motion, corner contact, below-threshold
  touching approach, capture overflow and a work limit reached after a staged
  material response.

Every positive sweep case runs two frames and checks exact snapshot/output
replay, one incoming root, physical patch widths, material charges and balances
against an unsplit already-touching reference. Actual kinetic loss plus signed
prescribed work must close against the ledger within `1e-5` of initial relative
kinetic energy. Fully dynamic cases also check linear momentum in both axes.
The initial normal gap is 0.02 and relative speed 40, so response occurs around
0.5 ms across the tested subdivisions.

The broader supported sweep initially exposed partition-dependent closing
residuals after a successful material callback. Two explicit regressions retain
the configurations that exposed the problem:

| Three-plate scene | B-C approach before the fix |
| --- | --- |
| Unsplit plates, two copies each, rotation 0.37, fixed C | about `9.6e-16` |
| Two slices per plate, one copy, horizontal, tangent offset 0.5, all dynamic | `9.536743e-7` |

The adapter previously divided each physical patch impulse equally among its
solver points. Different collider representations change the number of points.
Rounding those shares to f32 and accumulating them separately left a small
closing velocity, even though the equivalent one-collider-per-body scene
completed. The scheduler correctly refused before committing any drift or work.

The adapter now applies the total patch impulse once at the first canonical
solver point. Ball duplicates have identical point/normal/body identity; flat
faces have rotation locked. Their active normal Jacobians are therefore identical,
so choosing one point preserves the modeled response. Other points retain zero
requests: the backend still validates all targets and clears every target's
restitution and warmstarts. Material geometry, budget, energy tolerance and
velocity rounding are unchanged. No backend correction impulse or velocity
clamping is introduced.

Both former failures now produce exactly equal B-C velocities and match the
unsplit reference's response records and material balances. The complete
two/three-plate sweeps cover 2,560 successful combinations, including the offset
and rotated prescribed cases previously excluded. This establishes the tested
cases, not general representability of every possible shared response.

An additional low-speed impact against a translating support retains the
numerical-energy boundary. At relative speed 0.25, common x velocities 0 and 17
produce accounted harmless damping with unchanged material capacity. Common
velocities of +/-1,000,000 cannot represent the required rebound at f32 precision:
the backend energy validator still refuses atomically. Neither the energy
tolerance nor the scheduler's strict approaching-contact check has been relaxed.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib flat_arrival_
```

The native qualification compares exact snapshots, batches and durations with
this independent fixture across the entire sweep. The solid rectangle
certificate must not become an assumption about arbitrary compound shapes.
General shapes, rotation, other loading classes, pulse calibration and #328
failure consequences remain unfinished.
This does not complete #327.
