# Rounder planets and material deposition

Investigation, 2026-09-11, against `main` at `611df45`.
Related issues: [#67, rounder planets](https://github.com/aortez/space-wars/issues/67)
and [#51, particles filling planetary surfaces](https://github.com/aortez/space-wars/issues/51).
This preserves the initial source audit and proposed experiment. The subsequent
opt-in implementation and measured results are documented in
[Rounder planet surface comparison](../rounder-planets.md). Deposition remains
a separate future experiment.

## Findings

Spacewars generates a circular occupancy mask in a square grid with **1.0 world
unit cells**. The circle's occupied-cell centers lie within `0.99 * planet.radius`.
The generator does not add surface noise: the untouched planet's stairs come
from full square cells. Processing chunks are 32 by 32 cells, independent of the
size of a mined cut or a detached piece.

`TerrainGeometry` merges solid cells into material-homogeneous rectangles.
Spacewars draws those rectangles, and `TerrainAssembly` turns the same rectangles
into Rapier cuboids. Thus the stepped boundary is physical as well as visual.
The same-color outline used to hide rendering seams does not smooth collisions.

The Spacewars spaceling uses the default capsule: radius 0.3, half-segment 0.6,
total width **0.6**, total height **1.8**. One cell is approximately 1.67 character
widths and 56% of character height. Increasing the planet radius alone does not
shrink these steps. The older Terrain Lab defaults to 0.5-unit cells; that is not
the current Spacewars resolution.

Source entry points:

- `scenarios/spacewars/src/terrain.rs`: `generate_field`, `render_body`, `commit`.
- `crates/engine-terrain/src/lib.rs`: `Cell`, `CHUNK_SIZE`, `chunk_rectangles`,
  `TerrainGeometry`.
- `crates/engine-rapier/src/terrain.rs`: `TerrainAssembly::synchronize`.
- `crates/engine-rapier/src/spaceling.rs`: default `SpacelingSpec`.
- `scenarios/spacewars/src/surface_sortie.rs`: `SurfaceSortieState::spec`.

## What currently happens to broken matter

There are three different systems which can look like particles or rubble:

| System | Current behavior | Can become planet terrain? |
| --- | --- | --- |
| Visual impact particles | Small triangles receive gravity, bounce using nominal circular body bounds, fade, and are removed. They do not query excavated terrain. | No |
| Asteroids and ordinary breakup debris | Mechanical bodies collide and take damage. Asteroids chip and break into polygon debris; small breakup fragments retire below radius 0.1 after shrinking. | No |
| Detached material terrain | Disconnected rock/ore cells become rigid, collidable, mineable bodies with their material and partial damage preserved. | No |

After cell removal, `Terrain::detach_disconnected` finds components connected
through cell edges. Diagonal touching alone does not connect them. The largest
component retains the original body's identity, and smaller components transfer
to cropped material fields in deterministic order. Detached bodies inherit the
parent's point velocity and angular velocity, with no artificial breakup impulse.
Processing chunk boundaries do not define fragment sizes.

Mining and terrain damage remove cells outright. They do not convert each removed
cell into grains or conserved visual particles. Detached material can physically
fall into a crater and rest there, but remains a separate body; it cannot provide
planet ownership or reattach itself. Connected material is rigid, so a thin bridge
does not sag or crumble simply under its own weight.

The ordinary match and Terrain Lab also have different damage policies. Terrain
Lab explicitly damages terrain on sufficiently hard fragment-to-planet and
fragment-to-fragment impacts. That general terrain-fragment impact policy is not
wired into ordinary Spacewars. The match does support mining, cannon damage and
bounded terrain edits from its environmental asteroid arrivals. A source review
must not mistake the lab's extra policy for match behavior.

The environmental arrival controller caps its tracked live asteroids at 24 and
expires those originals after 30 seconds. These limits do not bound all their
breakup children or detached terrain bodies.

Source entry points:

- `scenarios/spacewars/src/lib.rs`: `update_particles`, `body_physics`,
  `ParticleState`, `damage_debris`, `DebrisState::shrink_to`,
  `resolve_physics_collisions` (mechanical contact damage policy).
- `crates/engine-terrain/src/connectivity.rs`: `detach_disconnected`.
- `crates/engine-rapier/src/terrain.rs`: `TerrainFragment::insert`.
- `scenarios/spacewars/src/surface_sortie/asteroids.rs`: `begin_step`,
  `record_contacts`.
- `scenarios/terrain-lab/src/impacts.rs`: the separate lab impact policy.

## Options for #67

| Approach | What it improves | Principal limitation |
| --- | --- | --- |
| Smaller cells, e.g. 0.5 or 0.25 | Both drawing and physical step size, with the existing geometry representation | Still stair-shaped; roughly 4x or 16x dense cell storage for the same world area |
| Smooth drawing only | Silhouette | Feet, lasers and landing still encounter square steps; visible ground can disagree with physical ground |
| Shared smoother boundary derived from material | Appearance, contact normals and traversal together | Requires careful topology, edit mapping, chunk seams and fragment mass handling |
| Loose material settling into terrain | Actual crater filling and changing ground over time | New material transport/gameplay system; does not itself remove square-cell boundaries |

The recommendation is to prototype a **shared smoother boundary**. The existing
material field remains the authoritative record of material, durability, removal
and connectivity. Derived geometry can approximate the outside edges with short
slopes, for example through a marching-squares-style boundary reconstruction.
This is a candidate to evaluate, not a selected algorithm or a promise of perfect
circles. A binary full/empty grid contains limited information about curvature.

Keep the 0.5-unit grid as a useful comparison. Existing historical measurements
in [Terrain Lab](../terrain-lab.md#rebuild-benchmark) show the resolution
tradeoff, but are not a current Picade performance result. One radius-150 lab
fixture increased from 775 initial rectangles at 1.0 to 2,491 at 0.5, with about
four times the cell storage. Current match or cabinet timings must be measured
separately if needed; performance tuning need not drive this first investigation.

## Proposed first implementation chunk

Build a controlled, opt-in terrain comparison with the current cell boundary and
the candidate smoothed boundary on the same radius-60 material planet. Include
an untouched slope, a shallow crater, a tunnel, and a removable bridge. Reuse the
real spaceling, mining and landing mechanics. Capture identical close views with
the collision boundary visible, and run the same walking inputs both ways.

The experiment should establish these contracts before broad match integration:

1. **One physical boundary.** Rendering, collision and ray queries consume the
   same derived surface. Tunnel interiors remain open and collidable. Do not
   build one convex hull around an entire concave planet or fragment.
2. **Material identity at contacts.** Surface hits must resolve to a surviving
   source cell or support footprint. Mining and flag anchoring currently step
   0.08 units inside a hit and call `local_to_cell`; impact targeting also has a
   hardcoded 0.502 half-cell tolerance. Audit these assumptions instead of
   assuming that new polygon edges preserve them.
3. **Local rebuilds with neighbors.** Boundary construction needs neighboring
   samples. A chunk-edge edit must invalidate every affected derived chunk, with
   consistent seams and no accidental bridges across diagonal cells. Current
   chunk revisions only mark chunks containing changed cells.
4. **Stable topology and thin features.** Explicitly define how single cells,
   one-cell bridges, holes and diagonal ambiguity are represented. Smoothing
   must not silently close a mined passage or erase material that still exists.
   Durability is remaining mining work, not fractional occupancy; do not reuse
   it as a density value.
5. **Deliberate mass handling.** Dynamic terrain currently derives mass, center
   of mass and inertia from full-area cuboids at density 1. Shaving corners into
   slopes changes those values. Keep material quantity accounting exact, measure
   the geometric area difference, and establish mass/inertia behavior before
   using the candidate for moving fragments. A zero-density outline alone is
   insufficient. The wrapper already exposes convex polygons and polylines;
   the decomposition and mass policy are still work to do.
6. **Support after edits.** Revalidate ship feet, spacelings, flags and rebuild
   eligibility at the existing edit boundary. Preserve a flag through harmless
   remeshing of its surviving footing; destroying/detaching that footing still
   removes the flag and neutralizes the planet. Nearby contour changes must not
   leave a flag floating above its supporting surface.

Acceptance is a tangible improvement in walking and landing on diagonal ground,
with matching visible contacts and no loss of mining, tunnels or flags. A
successful static contour screenshot alone is not sufficient. Only extend to
the ordinary generated match after the combined physical tests pass.

## How #51 can build on that foundation

Treat deposition as material transfer through the same edit boundary, not as
permanent visual effects. `EditMode` currently supports only removal and damage;
there is no addition, fill fraction or granular state. A future deposit operation
needs a defined material quantity and destination and must atomically debit the
source and credit the terrain, without overlapping an actor or double-counting
material.

A manageable first deposition experiment would use small, explicitly tracked
mineral pieces in a prepared crater. Require sustained contact and low speed
relative to the translating/rotating planet before transferring material. Map
through the planet's local frame and rebuild the affected surface once per edit
batch. Keep larger pieces as rocks. Quantify retained, loose, removed and deposited
material separately; record momentum absorbed by the prescribed planet rather
than accidentally accelerating attached deposits. Arbitrarily rotated pieces
will require an explicit rasterization/quantity policy.

That experiment proves settling and attachment, not full natural smoothing.
Loose soil that flows toward low spots also needs transport under local gravity
and a rule for stable slopes. Depositing whole rock cells without redistribution
can make piles and new steps. Finer occupancy or a separate material-amount field
may later support finer sediment, but should be a distinct state-format decision.

Further decisions include finite deposition capacity in the current fixed-size
fields, handling full destinations, impact erosion versus deposition, and
whether deposited material can support claims. Existing flying fragments must
continue to be ineligible to claim their former planet. Visual sparks have no
conserved quantity and should not be promoted to terrain implicitly.

## Verification plan

- Geometry cases: all local corner patterns, chunk edges, field boundaries,
  one-cell fragments/bridges, holes, and mixed materials. Check deterministic
  results, preserved connectivity and agreement between draw and query geometry.
- Physical cases: walk both ways around cardinal and diagonal ground; approach
  the same slope with ship and pod feet; mine under a supported actor; shoot and
  walk through a tunnel; detach and continue mining a moving fragment.
- Gameplay cases: claim, harmless remesh, destroy/detach the flag footing,
  recover, rebuild and depart. Retain the existing material-support, point
  velocity, clone/replay and final-step ownership tests.
- Longer comparison once integrated: selected existing bot navigation/recovery
  seeds for up to three minutes, with and without asteroid pressure. Compare
  stalls, falls, support loss and mission completions, alongside finite motion
  and material accounting. Record terrain revision and case seed for failures.
- Cabinet check after desktop acceptance: close-up playtesting on sw-picade,
  including left/right walking, landing and all mining sizes. Existing reports
  are historical baselines, not evidence that a new contour works on the Pi.

No new simulation suite was run for this source-only investigation.
