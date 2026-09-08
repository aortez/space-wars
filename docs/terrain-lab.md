# Terrain Lab

Terrain Lab is the first implementation slice for [destructible planets,
issue #16](https://github.com/aortez/space-wars/issues/16). It hosts a seeded
material planet, a contact-driven spaceling, three aimed mining tools, and editing
tools. Terrain rotates and translates while a separate, softened point source
supplies gravity.

Start it from the launcher or directly:

```sh
cargo run -p engine-client -- --scenario terrain-lab --seed 42
```

[Spacewars Terrain](spacewars-terrain.md) is the subsequent integration fixture:
real Spacewars ships and weapons use material terrain and the shared fragment
creation path, including explicit base support and service invalidation.

| Action | Keyboard / pointer | Xbox layout | Switch Pro |
| --- | --- | --- | --- |
| Walk | A/D or left/right arrows | Left stick or d-pad left/right | Same |
| Jump / get up when prone | Space | A (bottom face) | B |
| Aim and mine continuously | Hold mouse/touch; drag to aim | Right stick aims; hold LB or RT | Right stick; hold L or ZR |
| Mine with retained aim | Hold E | Hold LB or RT | Hold L or ZR |
| Rotate retained aim | W/S or up/down arrows | D-pad up/down | Same |
| Cycle tool | T | Y (top face) | X |
| Cycle camera view | V | RB | R |
| Hold debug mode / show geometry | Hold J | Hold LT | Hold ZL |
| Debug crater beneath character | Hold J, press X | Hold LT, press B | Hold ZL, press A |
| Debug equatorial tunnel | Hold J, press K | Hold LT, press X | Hold ZL, press Y |
| Restart fixture | R | Pause menu | Pause menu |
| Pause | Esc | Start | + |

Jump, tool/view changes, and debug cuts require a release before repeating. A
debug cut requires the modifier when the cut button is pressed; adding the modifier
to an already-held cut button does nothing. Debug mode also shows the collision
probe and contact normal. Its ray across the planet stops at solid terrain in
orange and turns cyan when its entire path is clear.
Removing support makes the spaceling fall and settle through normal
Rapier contacts. The diagnostics show cell storage, shape counts, terrain
revision, removed cells, and timings for the last edit. Orange chunk boundaries
identify the chunks affected by that edit.

A fresh jump press while prone requests getting up. With stable footing and
room above, the spaceling pushes upward briefly and physically turns upright.
Holding jump does not repeat the push or queue a jump after standing. If a low
ceiling blocks standing, movement during recovery crawls at quarter speed so
the player can work toward open ground. The capsule still collides with rock.
Switching between terrain collision rectangles no longer resets recovery.

The suit is orange when balanced, red when knocked down, and yellow while
recovering. Normal play shows a get-up hint or the reason an attempt could not
start; debug mode shows balance, settle time, recovery progress, and the latest
get-up result and attempt count. The shared mechanics and thresholds are
documented in [Spaceling Lab](spaceling-lab.md#balance-and-recovery).

## Mining loop

The default tool is the original drill, aimed beneath the spaceling. Each tool's
beam stops at the first solid surface within its reach. Mouse/touch holds aim toward
the supplied world position; the right stick supplies a world direction.
Without either, the retained aim follows the character's orientation and W/S
rotates it. Releasing or cancelling the pointer stops pointer drilling.
Pads without a right stick can rotate aim with d-pad up/down; the mining buttons
still require a physical shoulder or trigger. Two-button NES pads cannot access
all lab tools.

| Tool | Reach | Cut half-width / rounded radius | Damage per pulse | Pulse interval |
| --- | --- | --- | --- | --- |
| Precision Laser | 5 units | 0 / 0: exactly one cell | 20 | 3 ticks (20 Hz) |
| Drill | 4 units | 1 / 0.5 units | 20 | 6 ticks (10 Hz) |
| Excavator | 6 units | 2.5 / 1 units | 30 | 12 ticks (5 Hz) |

`TerrainLabConfig::mining_tools` holds the three `MiningToolProfile` values, indexed
by `MiningTool`. Range, width, depth, damage, and pulse interval are independent
of terrain resolution. Initialization normalizes nonfinite/out-of-range dimensions;
reach is bounded to 0.5–12 units, cut half-width to 0–4, radius to 0–half-width,
damage to at least one, and interval to 1–120 ticks.

Rock starts at 100 durability and ore at 180. Damage persists when aiming
elsewhere or changing tools. The cooldown from the last pulse continues through
release and tool changes, so neither tapping nor switching accelerates it.
The HUD shows the selected tool, view, reach, target durability, and number of
cells in the preview. The beam turns orange while mining.

Each pulse queues a field-local capsule across the strike face, quantized to
cells. At default resolution, the precision laser affects one 0.5×0.5 cell;
the axis-aligned drill footprint spans 2.5×1.5 units, and the excavator 5.5×2.5.
Only occupied cells inside that footprint receive damage. Highlighted cells use
the exact same quantized brush and `Terrain::brush_cells` inclusion rules as the
next pulse; white outlines mark cells that would break on that pulse. Tool
switching changes the preview immediately. The preview follows the current hit
while waiting for the cooldown.

The two broad tools leave a floor wide enough for the spaceling. A precision
hole can be too narrow to enter; sweep the beam or switch tools to clear a
passage. Target acquisition, damage, geometry/collider updates, and the following
physics step use the same tick boundary as other terrain edits.

The lab collects 100% of cells removed by the drill into separate rock and ore
counters. The HUD converts these integer counters to area (`cells × cell_size²`,
shown as `u²`). Partial damage yields nothing. Debug craters, tunnels, and direct
edit actions yield nothing. Removed cells are counted once, including when a
debug edit and a drill pulse overlap in the same tick. Restart resets recovery.
This is a scenario recovery policy; prices, storage limits, hauling, and shared
Spacewars economic rules remain future work.

Mining controls use scenario action kind 3 with a version-1 payload; nonfinite
aim/turn values and malformed payloads are rejected. Tool/view button snapshots
use action kind 4, version 1. Observation version 6 includes selected tool and
view alongside the two material recovery counters, balance and get-up
diagnostics, the next fragment ID, and each fragment's ID, terrain revision/hash,
and motion. It also records impact totals, contact rearming state, and damage
queued for the next tick. Same-build state clones retain
all terrain bodies, tools, view, aim, held input, button edges, damage, inventory,
and pulse cooldown. `terrain_hash()` includes cached fragment content and IDs;
motion remains a separate part of the observation.

## Camera views

V or the right shoulder cycles Overview → Mining → Detail → Overview. Overview
frames the planet. Mining and Detail follow the character at world heights of
28 and 16 units, expanded when needed for the selected tool's reach and depth.
The camera keeps world-up and does not move in response to aiming or a target
cell disappearing. HUD positions follow the viewport at every zoom level.
Changing views cancels a held pointer: release and press again through the new
camera. Keyboard/gamepad mining can continue through a view change.

## Ownership and interfaces

`crates/engine-terrain` contains the reusable field and edit kernel. Fields are
dense, row-major, centered in their own local coordinates, and divided into
32×32 processing chunks. The default lab cell size is 0.5 world units. The crate
accepts rectangular fields; it contains no planet, gravity, actor, renderer, or
economy policy. The lab supplies the circular mask and seeded coarse ore regions.

Each cell is two bytes: a material ID and integer remaining durability. ID zero
is void. An immutable material table supplies initial hardness. The first lab
materials are rock and ore. Material counts returned by an edit are physical
cell counts; the consuming scenario converts them using cell size and its
recovery policy before crediting resources.

`TerrainEdit` holds a circle or capsule brush and a remove/damage operation.
Brush coordinates are integer cell centers. Capsule inclusion uses integer
distance comparisons with wide intermediates, including round endpoints.
World impacts must be transformed and quantized when their edit is queued,
before the planet moves again. Bounds and input limits are checked before any
mutation. Edits traverse cells in row-major order and report changed bounds,
sorted dirty chunks, removed material counts, and a monotonically increasing
revision. Empty edits do not advance revisions or credit material twice.

The field serializes through a validated, versioned serde representation.
`Terrain::hash` uses explicit little-endian configuration, material definitions,
and cells with FNV-1a. It includes damage but excludes revision history and
derived caches. This is a regression/content hash, not a cryptographic checksum.

`TerrainGeometry` caches a material-homogeneous greedy rectangle cover for each
chunk. Each occupied cell belongs to exactly one rectangle. Queries, rendering,
and physics share that cover, including tunnel interiors and partial edge
chunks. Construct a fresh cache when replacing a field or restoring its saved
state; revisions are scoped to an individual field instance. A normal clone may
retain its matching cache. Additional simulators can later maintain separate
chunk arrays for temperature or other state.

`engine-rapier::terrain::TerrainAssembly` binds the cached rectangles to one
body. Each chunk reserves one collider role, beginning at the configured
`first_chunk_role`; reserve infrastructure sensors outside that range.
`PhysicsWorld::replace_colliders` validates a role's complete replacement before
mutation and keeps the body, joints, and unrelated roles intact. Each chunk can
produce at most 1,024 rectangles; ordinary merged solid regions produce far
fewer. Shape complexity for fragmented/noisy fields still needs explicit
gameplay budgets before broad Spacewars integration.

## Tick and presentation

The lab decodes held controls, pointer aiming, and local edits, acquires a drill
target from the previous completed physics state, then commits queued edits,
checks connectivity on bodies that lost cells, updates the affected geometry/colliders,
creates detached bodies, sets kinematic targets, applies controls and
gravity, and steps Rapier once. Terrain contacts enqueue field-local impact damage
for the following tick. Rendering and ray queries consume the post-step state.
The initial terrain-only query world is primed before the first frame.

Rendering transforms cached local rectangles into the existing polygon draw
list. It never rescans the material field for a view. Both application renderers
work; the vector path adds a small same-color outline to cover antialiasing
seams. The prototype still allocates/transforms polygon points per frame. Shared
retained mesh handles and detailed/overview representations remain a later
rendering optimization, guided by measurement.

## Detached terrain

Cutting the last connection frees a section as a moving, collidable terrain body.
It retains all rock, ore, and partial damage. Separation awards no inventory;
only cells actually removed by mining are recovered. The same beam, exact cut
preview, damage profiles, and collision queries work on moving fragments. The
HUD reports the number of fragments. For a large demonstration, hold ZL and
press Y on Switch Pro (keyboard J + K) to cut the equatorial tunnel and release
half the planet. That half can fall back into the tunnel.

`Terrain::detach_disconnected` uses an iterative flood fill over edge-adjacent
solid cells. Corner contact alone does not connect pieces. The largest component
keeps its field coordinates and body identity; ties select the component with
the first row-major cell. Other components are returned in that same stable
order as cropped fields and parent-local offsets. Material and durability are
copied exactly, and the source cells are cleared with chunk revisions updated.
New fields start at revision zero. Processing chunk boundaries do not limit a
fragment's shape or size. Empty and already-connected fields are no-ops, and a
failed split leaves the field unchanged.

The lab commits all queued edits to their sampled body IDs before separating
any pieces. It scans connectivity once per body that lost cells in that tick;
durability-only edits skip the scan. The retained planet stays kinematic.
Fragments receive increasing IDs, uniform areal density 1, collider-derived mass
and inertia, CCD, and the parent's velocity at their center of mass plus its
angular velocity. Rebuilding a surviving body's colliders adjusts center-of-mass
velocity to preserve the motion of its remaining points. There is no artificial
separation impulse. Fragments collide with the planet, character, and each other;
they can support the character and split again. Mining away a fragment's last
cell removes its physics entity. IDs are not reused. Restart clears all fragments.

Gravity remains the existing softened point source at the prescribed planet
center, even if that region is hollow or the planet is fully mined. Fragments
respond at their center of mass but do not source gravity. They do not reattach
on contact. Structural strength, variable gravity mass, hauling,
and granular/fluid behavior remain subsequent slices. Pieces stay rigid until
their cell connectivity changes; thin connected bridges do not sag or break
under load. There is no fragment-count cap or automatic debris deletion in this
slice. Large fragmentation workloads still need performance measurements and
gameplay budgets before broad integration. Spacewars' existing planets and
service rules remain separate from this lab.

## Impact damage

Hard fragment impacts damage both contacting terrain bodies. This includes
fragment-to-planet and fragment-to-fragment collisions; spaceling contacts cannot
damage terrain. Damage uses the existing durability and connectivity path, so
an impact can sever a bridge and release a second falling piece. Rock starts at
100 durability and ore at 180: a 120-work hit removes intact rock while leaving
60 durability in intact ore. Impact destruction credits no mining inventory.

`TerrainLabConfig::impacts` controls this optional scenario policy. Defaults:

| Setting | Value |
| --- | --- |
| Minimum normal approach speed | Greater than 6 units/s |
| Damage work per unit of estimated impact energy | 1 |
| Maximum damage per affected cell | 220 |
| Maximum brush radius | 1 world unit |
| Maximum damaging body pairs per tick | 4 |
| Contact rearming gap | 8 ticks without contact |

The energy estimate is `0.5 * reduced_mass * (speed² - minimum_speed²)`, using
the dynamic fragment's mass against the kinematic planet and both masses for a
fragment pair. This is arcade tuning, not a full rotational energy calculation.
Normal approach speed includes each body's angular point velocity. Motion is
sampled before the current gravity step, so support acceleration by itself does
not become impact damage. The prescribed planet's next motion is included.

Rapier supplies body-local surface points and normals alongside its existing
world contact reports. Damage stays attached to those cells across translation,
rotation, and CCD substeps. A bounded neighboring-cell search handles exact
rectangle corners, including one-cell fragments. Hits are quantized immediately
and queued for the following tick. Pausing preserves pending damage; clone
checkpoints retain the queue and contact history. All queued edits commit before
any connectivity splits, preserving their original body/field coordinates.

Contacts are grouped by body pair, so multiple rectangles produce at most one
damaging hit per pair. The strongest eligible contact wins; global admission is
ordered by energy, then stable body IDs. Persistent contact and brief gaps do
not rearm damage. Children inherit recent contacts from their parent to avoid
counting a rebuild or split as another collision. Excess hits are counted and
discarded, without a delayed damage backlog.

Configuration normalization limits admission to 1–8 pairs per tick and brush
radius to 0–2 world units. Every brush is additionally capped at four cells in
radius (an 81-cell bounding square per body). These limits bound impact edit
work; they do not cap total fragment count or the cost of connectivity scans.
The default half-unit cells and one-unit radius cover at most 13 cells per body.

A brief amber spark marks an admitted hit. Hold the debug modifier to see
impact count, destroyed cells, last approach speed, and budget skips. For an
immediate controller demonstration, hold ZL and press Y on Switch Pro (J + K on
keyboard) to release the upper half of the planet and let it hit the lower half.

Tests cover hardness, damage to both sides, corner hits, complete body removal,
fast CCD contacts on moving/rotated terrain, gentle landings, resting weight,
remeshing, relative motion, character exclusion, rearming, simultaneous-hit
budgets, cascading collapse, material accounting, and checkpoint/pause behavior.

Use the 1,200-tick impact stress benchmark to compare intact collision against
damage and cascading breakage, including a grid cut that starts with many pieces:

```sh
cargo run --locked --release -p scenario-terrain-lab --example impact_benchmark
```

It asserts total-cell conservation and zero mining recovery, and reports impact
and destruction counts, skipped hits, peak fragments/colliders, simulation time,
and draw-list time. Rendering timings exclude rasterization and presentation.

Measured with Rust 1.94.1 release builds on the Pi 5 and desktop 9800X3D, after
120 warm-up ticks. The Pi kiosk continued running alongside the standalone
benchmark. Times below are milliseconds; fragment and impact counts are from
the Pi run.

| Workload | Peak fragments | Hits / destroyed cells | Pi initial cut | Pi step P95 / max | Pi draw-list P95 | Desktop step P95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Radius 20, equator, damage off | 1 | 0 / 0 | 0.486 | 0.246 / 0.285 | 0.018 | 0.082 |
| Radius 20, equator | 1 | 1 / 12 | 0.506 | 0.251 / 0.396 | 0.018 | 0.084 |
| Radius 20, blocks | 90 | 72 / 572 | 0.866 | 1.848 / 2.131 | 0.084 | 0.723 |
| Radius 40, blocks | 336 | 162 / 1,993 | 3.469 | 9.211 / 9.871 | 0.352 | 2.853 |
| Radius 150, equator | 1 | 1 / 12 | 24.841 | 9.213 / 15.305 | 0.452 | 2.772 |

The two grid-cut cases skipped 72 and 410 eligible hits under the four-pair
budget, with no delayed damage backlog. The radius-150 cut still exceeds a
16.7 ms frame budget, and its later maximum leaves little time for rendering.
These measurements bound the tested workloads, not arbitrary fragment counts.

Equatorial-cut content/ownership hashes matched across machines. The grid-cut
cascades diverged: the desktop radius-40 case reached 334 fragments and destroyed
1,996 cells, versus 336 and 1,993 on the Pi. Conservation and admission assertions
passed on both; cross-machine physics/cascade determinism remains unestablished.

## Verification

```sh
cargo test --locked -p engine-terrain -p engine-rapier -p scenario-terrain-lab
cargo test --locked -p engine-client terrain_lab
cargo clippy --locked -p engine-terrain -p engine-rapier \
  -p scenario-terrain-lab --all-targets --no-deps -- -D warnings
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test --locked -p engine-client --test ui_control_functional terrain_lab \
  -- --ignored --test-threads=1
```

Coverage includes exact brush accounting, cross-chunk rectangle coverage,
malformed input, serialization and edit replay, rotating/translating tunnels,
ray queries, CCD against a one-cell wall, preserved sensor/body/chunk handles,
empty collider cleanup, removal of support and landing on a crater floor,
same-build checkpoint continuation, and keyboard/gamepad input release. Client
tests check actual raster holes and the vector adapter; the functional test
checks material and text pixels in real window screenshots for both renderers.
Mining coverage includes hardness and pulse cadence, bounded reach, nearest
surface selection, rotated hits, exact material recovery, cancellation (including
the host's zero-duration pause/resume path), mid-damage checkpoint continuation,
and losing support then landing on a mined floor.
Tool-scale tests cover exact preview/damage agreement across chunk boundaries
and rotated planets, single-cell extraction without changing neighboring ore,
all three profiles, cooldown preservation while switching tools, button release
and disconnect behavior, debug modifier gating, camera/pointer cancellation,
and visible HUD/preview output for every tool and view in both renderers.

Set `SPACEWARS_TERRAIN_ARTIFACTS=/tmp/terrain-frames` for the client unit test to
write intact/tunnel/crater/overlay/mining raster images. These raw raster
artifacts omit the separate text overlay. Set `SPACEWARS_KEEP_FUNCTIONAL_ARTIFACTS=1` for the
functional test to retain complete window captures under
`target/functional-test-artifacts/`.

Fragment tests cover cell/durability conservation across chunk boundaries,
stable tie-breaking and IDs, translated/rotated separation, inherited point
velocities, changed mass and center of mass, fragment-to-fragment collisions,
falling onto terrain, character support, resumed physics, aiming/previewing on
rotated fragments, further splits, final-cell body removal, and recovery only
for actually mined cells. Real-window checks exercise the debug split, falling
pieces, and continued mining in both renderers.

## Fragment benchmark

```sh
cargo run --locked --release -p scenario-terrain-lab --example fragment_benchmark
```

Measured on the Pi 5 with a release build, after 120 warm-up ticks and over 600
post-cut physics ticks. The blocks case cuts a grid through the default planet,
creating 87 independently colliding fragments. The benchmark asserts total-cell
conservation and zero recovery for these debug edits. Times are milliseconds.
`fragment_benchmark` disables impact damage to preserve this separation-only
baseline; use `impact_benchmark` for subsequent destruction and chain reactions.

| Workload | Fragments | Initial cut step | Connectivity part | Later step P95 | Later step max | Draw-list P95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Radius 20, equator | 1 | 0.548 | 0.119 | 0.218 | 0.233 | 0.018 |
| Radius 20, blocks | 87 | 0.819 | 0.157 | 0.876 | 0.931 | 0.030 |
| Radius 150, equator | 1 | 24.040 | 6.187 | 7.398 | 7.943 | 0.283 |

The radius-150 cut exceeds one 60 Hz frame budget; large cuts can hitch even
when the subsequent simulation fits. Connectivity currently scans a body's
entire dense field when cells are removed. Draw-list timings include fragment
transforms and the HUD but exclude rasterization, GPU work, and presentation.
These measurements do not establish performance for arbitrary fragment counts.

The desktop 9800X3D measured initial cut steps of 0.195, 0.288, and 7.828 ms and
later step P95s of 0.066, 0.373, and 2.548 ms for the same three cases. Terrain
content/ownership hashes matched the Pi for all three; these hashes exclude
motion and do not establish cross-machine physics determinism. The previously
observed cross-machine mining divergence remains unresolved.

## Rebuild benchmark

The rebuild and mining measurements below predate detached terrain. The
`fragment_benchmark` example measures separation and subsequent fragment physics.

```sh
cargo run --locked --release -p scenario-terrain-lab \
  --example terrain_benchmark > terrain-benchmark.csv
```

The fixture generates a radius-150 planet at one-unit and half-unit resolution,
then applies 80 deterministic craters or cross-planet tunnels. It measures field
generation, initial geometry/physics binding, edits, dirty geometry rebuilding,
collider replacement, and Rapier stepping separately. It has one moving
kinematic body and no dynamic actors; it is a terrain rebuild workload, not a
full-game frame-rate result. Hash calculation and rendering are outside these
timed sections. Fragmentation and worst-case checkerboard material are not
covered by this first benchmark.

Desktop baseline, 2026-09-06: AMD Ryzen 7 9800X3D, Rust 1.94.1, release profile.
Times are milliseconds; P95 uses the nearest upper sample rank across 80 edits.

| Cell size | Workload | Cell storage | Initial quads | Edit P95 | Geometry P95 | Colliders P95 | Rapier step P95 | Maximum edit + geometry + colliders |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1.0 | Craters | 177.0 KiB | 775 | 0.001 | 0.009 | 0.034 | 0.335 | 0.053 |
| 1.0 | Tunnels | 177.0 KiB | 775 | 0.011 | 0.042 | 0.103 | 0.227 | 0.162 |
| 0.5 | Craters | 705.5 KiB | 2,491 | 0.002 | 0.017 | 0.081 | 0.967 | 0.129 |
| 0.5 | Tunnels | 705.5 KiB | 2,491 | 0.025 | 0.087 | 0.426 | 0.679 | 0.631 |

Pi baseline, 2026-09-06: Raspberry Pi 5 Model B Rev 1.1, AArch64, Rust 1.94.1,
release profile. This run uses the same fixture and timing definitions as the
desktop baseline.

| Cell size | Workload | Cell storage | Initial quads | Edit P95 | Geometry P95 | Colliders P95 | Rapier step P95 | Maximum edit + geometry + colliders |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1.0 | Craters | 177.0 KiB | 775 | 0.005 | 0.030 | 0.118 | 1.009 | 0.184 |
| 1.0 | Tunnels | 177.0 KiB | 775 | 0.061 | 0.106 | 0.353 | 0.683 | 0.539 |
| 0.5 | Craters | 705.5 KiB | 2,491 | 0.013 | 0.055 | 0.395 | 3.986 | 0.556 |
| 0.5 | Tunnels | 705.5 KiB | 2,491 | 0.127 | 0.250 | 1.506 | 2.668 | 1.946 |

All four final terrain hashes and rectangle counts match the desktop run. This
checks the material-edit results for these fixtures; physics-state hashes are
not part of this benchmark. The largest measured terrain commit took 1.946 ms
on the Pi, with collider replacement the largest P95 component in each workload.
Rapier stepping is measured separately, as shown above.

Cross-compile the standalone Pi benchmark with:

```sh
cargo build --locked --release --target aarch64-unknown-linux-gnu \
  -p scenario-terrain-lab --example terrain_benchmark
```

The measured binary ran from `/tmp/spacewars-terrain-benchmark-20260906` on
`spacewars@spacewars.local`.

## Mining simulation benchmark

```sh
cargo run --locked --release -p scenario-terrain-lab --example mining_benchmark
```

This fixture runs the complete Terrain Lab step, including a dynamic spaceling,
moving terrain, gravity, aiming, damage, recovery, terrain hashing, and collider
updates. For every tool at radii 20 and 150, it settles for 120 ticks and measures
600 ticks at half-unit resolution; the beam sweeps back and forth and is held for
the first 480 ticks. Draw-list construction and disposal are measured separately
in Detail view, including the cut preview, beam, and HUD.
It does not rasterize pixels, submit GPU work, or present a window, so it does
not establish the Pi's full rendered frame rate.

The original drill-only baseline below was measured 2026-09-06, before selectable
tools, cut previews, and camera views, with Rust 1.94.1 in release mode on the same
desktop and Pi 5 as above. Times are milliseconds. P95 uses the same sample-rank
rule as the rebuild fixture.

| Host | Radius | Ticks with edits | Simulation P95 | Simulation max | Draw-list P95 | Draw-list max | Peak primitives |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Desktop | 20 | 80 | 0.057 | 0.073 | 0.014 | 0.029 | 164 |
| Desktop | 150 | 37 | 1.146 | 1.330 | 0.279 | 0.305 | 2,520 |
| Pi 5 | 20 | 80 | 0.167 | 0.230 | 0.048 | 0.100 | 164 |
| Pi 5 | 150 | 37 | 4.158 | 5.149 | 0.749 | 0.820 | 2,520 |

Both hosts recovered 92 rock / 7 ore cells in the radius-20 fixture and 21 rock /
13 ore cells in the radius-150 fixture. The corresponding final terrain hashes
matched (`a9abed1358a53e43` and `a5e79b995c9a8541`). These are checks of the two
measured fixtures, not a guarantee of cross-platform physics lockstep.

Use libraries matching the Pi image when cross-compiling. The local GCC sysroot
selected `atan2f@GLIBC_2.43`, newer than the Pi image. The measured benchmark was
linked against the Pi's own `libm` using:

```sh
mkdir -p /tmp/spacewars-mining-pi-libs
ssh -F /dev/null spacewars@spacewars.local 'cat /usr/lib/libm.so.6' \
  > /tmp/spacewars-mining-pi-libs/libm.so
cargo rustc --locked --release --target aarch64-unknown-linux-gnu \
  -p scenario-terrain-lab --example mining_benchmark -- \
  -L native=/tmp/spacewars-mining-pi-libs
```

The resulting standalone binary ran beside the rebuild benchmark in the same
Pi temporary directory. A complete target-image SDK/sysroot is preferable for
building the full client.

## Tool-scale measurements

Measured 2026-09-06 on the Pi 5, Rust 1.94.1 release, with the three tool profiles,
cut previews, HUD panels, and Detail view. This is the same 600-tick workload
described above; simulation and draw-list times exclude rasterization/presentation.

| Tool | Radius | Simulation P95 / max (ms) | Draw-list P95 / max (ms) | Peak primitives | Rock / ore cells |
| --- | --- | --- | --- | --- | --- |
| Precision | 20 | 0.157 / 0.183 | 0.041 / 0.056 | 143 | 29 / 1 |
| Drill | 20 | 0.165 / 0.199 | 0.051 / 0.056 | 169 | 92 / 7 |
| Excavator | 20 | 0.150 / 0.234 | 0.061 / 0.067 | 194 | 194 / 19 |
| Precision | 150 | 4.300 / 4.743 | 0.703 / 0.742 | 2513 | 14 / 5 |
| Drill | 150 | 3.871 / 4.785 | 0.723 / 0.768 | 2525 | 21 / 13 |
| Excavator | 150 | 4.009 / 4.834 | 0.779 / 0.898 | 2556 | 80 / 73 |

All six workloads repeated their edit counts, material recovery, and final terrain
hashes exactly within each host. Five of six outcomes also matched between the
desktop and Pi. The radius-20 excavator workload recovered 180 rock / 19 ore cells
on desktop (`61f5f56c961c5683`) and 194 rock / 19 ore on Pi (`62f24f9843450aff`).
These workloads derive cuts from dynamic physics and raycasts; cross-platform
physics/mining lockstep is not established. Investigating that divergence remains
a follow-up before relying on cross-machine deterministic replay. The isolated
integer edit kernel and same-build checkpoint tests remain covered separately.
