# Spacewars Terrain

`spacewars-terrain` is a launcher fixture of the actual Spacewars scenario. It
uses the existing ships, cannon, laser, rover, gravity, and base services, with
one radius-60 material planet. Start it with:

```sh
cargo run -p engine-client -- --scenario spacewars-terrain --seed 42
```

The normal Spacewars launcher entry retains its circular planets. The fixture
is the first integration gate for [destructible planets (#16)](https://github.com/aortez/space-wars/issues/16).
`SpacewarsState::enable_planet_terrain(index)` also opts individual planets in
before play; the benchmark uses this to exercise a six-planet world.

## Playing

| Action | Keyboard | Switch Pro | Xbox |
| --- | --- | --- | --- |
| Turn | A / D | Left stick or d-pad | Same |
| Thrust | W | ZR or d-pad up | RT or d-pad up |
| Brake | S | ZL or d-pad down | LT or d-pad down |
| Reverse | X | A | B |
| Close wings | J | R | RB |
| Fire laser | Space | B | A |
| Fire cannon | K | Y | X |
| Open a test tunnel along the ship's aim | T | X | Y |
| Toggle planet overview / ship-follow view | V | L | LB |
| Pause / restart | Esc | + | Start |

The on-screen button labels use Switch Pro names. Tunnel and view controls
require a release before repeating. The tunnel is a diagnostic cut through the
planet center, aligned with the ship's current direction; it does not originate
at the ship. It is 29 cells wide. The ordinary cannon is the gameplay editing
tool. Its recoil can carry the ship away from the planet; switch to the follow
view to keep flying, and use overview to locate the planet again.

The HUD reports hull health, cannon hits, removed cells, fragment count, and
whether the base is supported. The ownership ring is strategic UI and can span
empty space after a cut. The filled material is the physical ground. The red
external berth belongs to Player 1 at fixture startup. An ordinary patrol rover
deploys through the existing construction lifecycle. The second Spacewars
ship is also present, without a rule bot. The fixture does not finish on victory.

## Terrain and weapons

Fields use one-world-unit cells, 32×32 processing chunks, rock hardness 100, and
ore hardness 180. Seeded coarse ore patches avoid the collider cost of per-cell
noise. The initial occupied radius follows Spacewars' existing 0.99 collision
radius convention. Rendering and collision use the same material rectangle
cover, including all interior holes. There is one terrain collision surface
shared by ships, rovers, shells, and fragments, with friction 0.9 and restitution
0.1. The rover-only circular traction collider is removed for an opted-in planet.

A cannon hit queues a radius-six-cell circle with 160 work per affected cell.
Intact rock breaks in one hit; intact ore retains 20 durability. Work affects
occupied cells in the circle, at most 113 cells per hit. Damage is committed at
the beginning of the next positive-duration tick, before services, motion,
queries, or the solver can consume a new terrain revision.

Contact surface points are captured in the hit body's local coordinates, so
rotation, orbital movement, and CCD do not retarget the edit. Multiple rectangle
contacts from one shell produce one edit. Up to four shells receive terrain
damage work per tick, ordered by shell identity. Every contacting shell is
consumed, including budget skips; there is no delayed damage backlog. These
limits bound cannon edit work, not connectivity scans or total simulation cost.

Lasers stop at the first surviving surface and pass through completed holes.
They also hit detached terrain. This slice gives terrain damage to the cannon;
weapon balance and laser ablation remain separate policy work. Destruction
provides no mineral credit. A later mining rig can connect extraction work to
local energy and mineral storage.

All queued edits commit against their original body IDs before any connectivity
split. The largest connected component retains the planet's kinematic identity.
Other components become cropped, dynamic terrain bodies. They preserve material,
partial durability, world placement, and the parent's point velocities. Further
cannon hits can split or completely remove those bodies. Their mass and inertia
come from remaining material geometry; CCD stays enabled. They respond to the
existing Spacewars gravity sources and do not become new gravity sources.

Each material planet supplies a finite-radius spherical gravity field. Outside its
nominal radius the existing inverse-square pull is unchanged. Inside, the pull
decreases linearly to zero at the center, meeting the exterior field continuously
at the surface. Ships, rovers, debris, particles, and detached terrain all use
this shared source. This removes the singular acceleration previously exposed
by excavation; it does not clamp body speeds. The sun and ordinary circular
planets retain their established point-source fields. Enabling material terrain
also enables the bounded field; removing its last material cell retains that field.

`engine-rapier::terrain::TerrainFragment` now supplies the common fragment
creation path to both Spacewars and Terrain Lab. Material fields, edit policy,
and lifecycle ownership remain in each scenario; rendering and Rapier shapes
are derived caches. Terrain Lab retains its mining tools and bounded fragment
impact-damage policy. Spacewars' new terrain edits currently come from cannon
hits and the fixture cut.

## Base support

Three material cells under the original planet-local berth define its footing.
All three must remain in the retained planet component. A cut that removes or
detaches any footing cell disables the base at the same lifecycle boundary:

- Remove the service sensor and release any retained ship hold.
- Clear capture, healing continuity, and pod-rebuilding progress.
- Reject further service contacts and stop new rover construction.
- Hide the berth and show `Base: OFFLINE` in the fixture HUD.

Existing rovers continue as physical actors and can fall into excavations.
Ownership and the ownership ring survive loss of the base. There is no automatic
base relocation or reconstruction in this slice. Natural ship landing, movable
infrastructure, and planet-death consequences need their own gameplay policies.

## Verification and limits

For repeatable runs up to three minutes, with visual timelines and per-second
health/performance samples, see the [terrain endurance test bed](terrain-endurance.md).

```sh
cargo test --locked -p engine-rapier -p scenario-spacewars \
  -p scenario-terrain-lab -p engine-client
cargo run --locked --release -p scenario-spacewars --example spacewars_terrain_benchmark
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test --locked -p engine-client --test ui_control_functional terrain \
  -- --ignored --test-threads=1
```

Integration tests exercise real cannon shots, bounded simultaneous hits, rotated
CCD contacts, laser clearance and ship traversal through a complete tunnel,
rover support loss, base hold/service invalidation, final-cell cleanup, empty
planets, pause/held controls, and clone continuation across queued damage.
The two terrain launcher scenarios share real-window lifecycle and renderer
checks. The shared fragment refactor also runs the existing Terrain Lab mining,
fragmentation, and impact regression suites.

The standalone benchmark warms up for 120 ticks and samples 1,200 subsequent
ticks. It runs ordinary Spacewars weapons and produces all four normal local-play
draw lists, even for the single-planet fixture. Each case asserts total remaining
cells plus destroyed cells equals the starting material count. Draw-list timings
exclude rasterization and presentation. The Pi runs alongside its active kiosk.

Measured 2026-09-06 with Rust 1.94.1 release builds on the Pi 5 and desktop
9800X3D. Every run also asserts that all 1,321 requested simulation ticks execute,
so a game-over pause cannot produce artificially low timings. Times are in ms.

| Workload | Pi peak fragments | Pi initial cut | Pi step P95 / max | Pi four-view draw-list P95 | Desktop step P95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| One planet, ordinary cannon fire | 2 | 0.101 | 0.390 / 0.898 | 0.140 | 0.137 |
| One planet, through-tunnel and cannon fire | 1 | 1.500 | 12.786 / 20.373 | 0.199 | 3.543 |
| One planet, grid cut and cannon fire | 90 | 2.527 | 11.855 / 37.232 | 0.182 | 4.334 |
| Six material planets and normal weapons | 2 | 2.078 | 5.172 / 8.233 | 1.115 | 1.305 |

The fixture begins with 11,069 occupied cells; the six-planet case has 192,346.
The ordinary fixture, grid cut, and six-planet runs end with matching material
removal counts on desktop and Pi: 364, 2,314, and 499. The tunnel encounter
diverges physically: desktop produces nine cannon hits and 3,781 removed cells;
Pi produces six hits and 3,758. Conservation passes on both. The stress-case
maxima exceed a 16.7 ms frame budget before rasterization, so those cuts can
visibly hitch even though the normal multi-planet run has substantial headroom.

Validation: 474 tests across the client, mechanics, Spacewars, and Terrain Lab,
plus two real-window lifecycle tests. Rust 1.89 checks and formatting pass.
Clippy completes with the existing Spacewars warnings (including older test
style and the existing particle helper's argument count).

Large connectivity scans and dense contact bursts can still hitch. There is no
automatic terrain debris deletion or fragment-count cap. Scripted orbits and
planet gravity mass and source radius remain independent of the surviving
material. The existing ship radar/AI model still describes planets with nominal radii and
berths; terrain-aware navigation and service observations are required before
enabling destructible planets throughout normal bot-controlled games. Same-build
clones are tested; cross-machine physics/cascade determinism is not established.
