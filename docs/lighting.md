# Shared direct lighting

The `engine-core::lighting` service is a generic simulation foundation for the
plant roadmap's environment inputs. It does not admit Clock lights or connect
living tissue. Hosts publish after their canonical physics/event update, once
per completed tick; paused rendering retains the existing `Arc` snapshot.
`LightingPublisher` rejects repeated/backward ticks and backward geometry
revisions. A restarted simulation needs a new publisher.

## Source contract and scalar units

A local source has a stable `LightId`, active flag, display RGBA, scalar strength,
finite range and one to eight world-space emission points. The simulation owner
applies the host's current translation/rotation to these points each publication;
render transforms must never enter physiological inputs. Points may each name
an emitting surface. A bounded footprint retains actual lit locations instead
of collapsing a digit into the empty space at its center. Equal-energy points
can be grouped in batches of eight: sum their strengths, retain their positions,
and use their average contribution. Do not group unequal point energies using
this representation. Repeated identical source identities deduplicate; different
values with the same identity reject the whole publication. Removing a point
or source requires a new publication. Reusing an identity for another lifetime
requires a new generation. Source IDs and lightweight surface IDs are assigned
by their simulation owners; reserve a disjoint namespace from physical IDs.

Strength uses the plant adapter's 0..1000 scale: zero is no usable energy,
1000 is full sun. Dim-room illumination is a caller-specified value on that same
scale, not a display-gamma conversion. Each point receives `strength / count`.
For distance `d` and range `R`, contribution is
`strength / count * max(0, 1 - d/R)^2`. This compact quadratic is finite at
contact, decreases with distance, and has zero value and slope at cutoff.
All visible local and directional contributions add; usable exposure clamps to
1000. Diagnostics retain separate unclamped local/directional sums. Hue and
alpha do not change scalar energy. Reflection, refraction, spectral biology,
water attenuation and multiple bounces are outside this model.

The overhead directional source supplies a unit vector toward the source,
strength and display color. Its visibility segment is bounded by the named
`DIRECTIONAL_DISTANCE` (100000 world units). Hosts must fit their optical world
inside that horizon. Source/receiver coordinates are limited to +/-100000 per
axis, and local ranges to 100000. Nonfinite, negative or out-of-range input is
an error, never an implicit no-light or unblocked result.

## Canonical opaque visibility

`QuerySnapshot::lighting_visibility` borrows collision geometry copied from the
one canonical world after its normal step. It creates no bodies, solver or
physics world and never steps. Enabled ordinary colliders block regardless of
collision groups; sensors never block. Hosts explicitly classify water and
physical decorative geometry in the transparent collider list. UI and halos
must not be supplied as opaque footprints. Non-collider solids and declared
plant tissue use `OpaqueFootprint` world-space disks, updated by the host from
its authoritative transforms. This is a query-only representation.

`light_collider_id` encodes the scenario-stable physics entity ID plus collider
role/part. Physics IDs cannot be reused within a session. A geometry revision
must change on movement, rotation, shape replacement, insertion/removal,
sensor/enabled/optical-classification changes, or footprint changes. Pass that
same revision to exposure queries. The publisher enforces monotonic revisions;
the host is responsible for binding the revision to its matching immutable
physics snapshot. No cache carries exposure across revisions.

Only matching local receiver/emitter surface identities permit trimming the
ray's endpoint. `ENDPOINT_TOLERANCE` is 0.001 world units; each trim is at most
one quarter of the segment. Nonzero segments retain endpoints and trim limits
in `f64` parameter space, transform into each canonical shape's local frame in
double precision, and clip against the admitted canonical geometry. No trimmed
point is added to world-space `f32` coordinates, and no tolerance is subtracted
from a long `f32` range. This retains the same 0.001-unit tolerance at +/-100000,
for both receiver and source endpoints, including long segments and large local
shapes. Only the final blocker diagnostic distance is converted back to `f32`.
Geometry extending inward farther still blocks.
Other colliders on the same entity and other parts of a compound collider still
block. There is no entity/organism/digit exclusion. Attached receivers sample
at the actual surface, not at cell/body centers. Deeply embedded receivers
remain blocked. For coincident endpoints, matching either endpoint's local
surface permits contact only within `ENDPOINT_TOLERANCE` of that geometry's
boundary. Deeper interiors and overlapping geometry with another identity
remain opaque. Compound children are checked separately, so a local child
boundary cannot exempt an overlapping child's interior. The zero-length check
uses bounded point queries over the same admitted shape nodes, with no new
allocation or world step. Blocker diagnostics select the nearest hit with identity ties.

## Bounds and failure behavior

| Resource | Hard limit / behavior |
| --- | --- |
| Unique local sources | 256; reject overflow atomically |
| Footprint samples/source | 1..8; reject overflow |
| Publication input copies | 2048; bound sorting/dedup work before allocating |
| Receivers/query batch | 64; reject overflow |
| Visibility rays/batch | At most 64 * (1 + 256 * 8) = 131136 |
| Canonical colliders | 2048; reject larger optical view |
| Shape complexity | 2048 total shape nodes plus polygon/polyline vertices |
| Lightweight opaque disks | 2048 |
| Transparent physical colliders | 256 |
| Exposure cache entries | 0; each query recomputes from the immutable inputs |
| Source update cadence | Once per completed simulation tick; no render updates |
| Maximum age | One simulation tick; future or older snapshots return `Stale` |

Ball, cuboid, capsule, convex polygon, polyline and bounded compound geometry
come directly from the canonical Rapier query snapshot. Nonzero lighting
segments use double-precision interval clipping of those same shapes (slabs,
disk intervals, capsule rectangle/end disks, convex planes and polyline edges).
Compound children transform the endpoints locally without constructing rounded
world-space child poses. Untrimmed fixtures compare this path with the native
query oracle. Coincident samples retain the native boundary-distance queries.
There is no duplicate geometry store or additional physics world.
Unsupported/over-complex geometry rejects
the view; hosts must partition it or handle the error explicitly. Canonical
snapshot creation has the existing world's copying cost; the optical adapter
borrows it and bounds its own admitted shape/query work. The adapter's linear
scan touches at most 2048 collider records and 2048 disk records per ray;
compound/vertex dispatch has the separate total complexity limit. Each admitted
node uses constant work (a capsule has at most three interval tests), and polygon
and polyline work is linear in the admitted vertex count. Interval scratch is
fixed-size and stack-only. These are
work bounds, not measured performance claims.

`Capacity`, `InvalidInput`, `ConflictingIdentity`, `Stale` and
`GeometryGeneration` are observable errors. Failed publication retains the old
snapshot; age checks still apply. The publisher counts successful publications,
rejections and capacity rejections. Snapshots report deduplicated copies;
exposure reports ray count, blocked-ray count and first blocker. No overflow
silently discards light or multiplies brightness by draw copies. Retaining old
`Arc` snapshots is the caller's responsibility; the publisher retains only one.

## Acceptance fixtures

- Core `lighting_contract`: contact/distance/cutoff, addition and directional
  baseline, hue/active state, footprint energy and draw-copy identity,
  invalid inputs, immutable lifecycle/age/revisions, publication counters,
  maximum source/footprint/receiver work and overflow.
- Rapier `lighting_contract`: canonical solids, sensor/water exclusions,
  attached receiver and emitter endpoints, intervening same-host part,
  lightweight tissue, removal and immutable copy, moving/rotating support,
  world/step inactivity and footprint overflow.
- Client `lighting_presentation_contract`: actual vector/raster consumers,
  camera changes, no/repeated presentation, fixed paused ticks and software
  scales; source-derived unculled Clock digits and maximum admitted text,
  including transformed text effects, pack positions without collapsing holes.
  Calling-thread physics activity counters assert no new world or step.

The client currently has no global quality enum or display gamma/bloom controls.
Software output scales and both frame consumers exercise the existing controls;
future presentation controls must preserve this read-only dependency. These
fixtures do not claim GPU pixel equivalence or performance on a device.
