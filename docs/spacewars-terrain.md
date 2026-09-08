# Spacewars Terrain

`spacewars-terrain` now runs the shared Expedition gameplay loop on one
radius-60 destructible material planet. It replaces the old launcher fixture's
external landing pad, berth capture and repair service. Select it in the launcher
or run:

```sh
cargo run -p engine-client -- --scenario spacewars-terrain --seed 42
```

Choose **1 or 2 players** in Settings. Each seat has its own ship, spaceling,
controls, camera and minimap. The first integration uses a controlled spinning
planet with the accepted surface gravity tuning; generated-world expansion and
ordinary-game bot adaptation remain separate work.

## Playing

Land rear-first on both feet, exit, then stand still for three seconds to raise
a flag at the spaceling's ground contact and claim the planet. To replace an
opponent's flag, approach within three world units, lower it for three seconds,
then raise yours for another three. A distant defender cannot contest an
existing flag. The shared [Expedition rules](surface-expedition.md) govern
movement, contests, loss and recovery.

| Action | Modern gamepad (Xbox labels) | Player 1 keyboard | Player 2 keyboard |
| --- | --- | --- | --- |
| Turn aboard / walk on foot | Left stick or d-pad left/right | A / D | Numpad 4 / 6 |
| Thrust aboard / jump or get up on foot | A (bottom face) | Space | Numpad 8 |
| Exit / board the landed assigned vehicle | B (east face) | X | Numpad 2 |
| Brake | D-pad down | S | Numpad 5 |
| Swept-wing cruise (release to open) | Hold RB | J | PageDown aboard |
| Aim mining beam on foot | Right stick | Arrows (left/right also walk) | Facing direction |
| Mine | RT or LB | E | End |
| Cycle cut size | Y (top face) | T | PageDown |
| Deliberate ship-loss drill | Hold A+B+Down for 3s | Space+X+S | Numpad 8+2+5 |
| Light asteroid strike | X (west face) | K | Home |
| Heavy asteroid strike | RB + X | J + K | PageDown + Home |
| Pause / restart menu | Start | Esc | Esc |

Mining has three cut sizes: one cell, radius one (up to five cells), and radius
three (up to 29 cells). Cells are one world unit wide. Each pulse applies 100
work, at most once every eight ticks, over an eight-unit beam range. Rock breaks
in one pulse; intact ore takes two. The first physical obstruction stops the
beam, including ships, other pilots and detached material. Mining removes
material without adding inventory or economic credit in this slice. Ship
weapons remain disabled in this Expedition mode.

Release controls after transfers. Mining has its own release gate, so holding
its trigger while exiting cannot immediately excavate under the spaceling.
The small two-button controller can still fly, walk, board and recover; mining
uses the additional controls of a modern pad or keyboard.

Impact controls spawn a real asteroid toward the assigned full ship, including
an empty parked ship. One press calls one rock, with a three-second cooldown.
See [damage and recovery AI](material-recovery-ai.md) for the controlled tuning
and the `spacewars-terrain-recovery` demonstration, where P2 handles ship loss,
pod landing, rebuilding and departure autonomously.

Choose `spacewars-terrain-combat` for P1 versus the combat bot, or
`spacewars-terrain-duel` to watch two bots. These enable forward laser and cannon
weapons aboard and reuse the same on-foot mining and recovery loop. See
[Material combat V4](material-combat-ai.md) for controls and validation. Launcher
Settings also offers **Bot mission: Capture** for a
[cover-aware landing, claim and departure attempt](tactical-surface-sorties.md).

## Destruction, flags and recovery

The flag is currently the only owned object on a planet. Removing or detaching
its supporting material removes the flag and neutralizes the retained planet.
It never awards ownership to the attacker. An owner can also lose their claim
by mining beneath their own flag. Claiming again requires the ordinary on-foot
hold, and loss of ownership interrupts unfinished rebuilding. A detached flag
footing cannot carry ownership onto a fragment.

Flags follow their actual planet-local contact point and normal. Unchanged
material footing survives harmless collider remeshing. Shape-identical,
durability-only edits preserve collider handles and physical contacts. After a
structural edit, hatch/placement queries wait until the normal physics step has
updated spatial indexing. Previously earned landing time survives only when
both sampled foot contacts still have material support; fresh solver contacts
must then pass the normal landing gates. There is no extra physics step or
gravity solve.

A ship or pod must genuinely settle before transfer. The hatch queries surviving
ground beside the real hull, checking one cell either side of a missed central
probe, and checks capsule clearance; a mined-out floor or
blocked hatch can make exit unavailable. Jump can request the existing physical
get-up maneuver when prone. A fresh get-up request during a structural edit waits
for valid clearance queries while the button remains held; releasing cancels it.
Already-held buttons never become new presses because terrain changes.

Occupied ship loss leaves the same pilot in an escape pod. Empty ship loss
preserves the on-foot pilot. Stand supported and still on owned material for
eight seconds to rebuild. Nearby placement uses bounded ground rays and hull
clearance, and the replacement must settle before boarding. The combined mode
has no repair terminal, rover construction or legacy pad services.

## Combined verification

The dedicated action-driven runner performs land → exit → claim → excavate the
flag footing → reclaim → lose ship → rebuild → board → depart, then continues
sampling material conservation, finite motion and body/collider health for up
to three minutes. It writes phase transitions, per-second audits and full-step
wall timings in `report.json`; timings exclude report generation and rendering.

```sh
cargo test --locked -p scenario-spacewars surface_sortie::material
cargo run --locked --release -p scenario-spacewars --example surface_material_soak -- \
  --seconds 180 --players 2 --seat 1 --seed 42 --out /tmp/surface-material-soak
```

`--seat` selects the scripted pilot; the other ship remains independently
simulated. This runner exercises gameplay through actions, without teleporting
actors or directly setting ownership. Separate regression tests cover same-tick
support removal, detached footing, contested players, paused edits, held input,
recovery interruption, and cloned continuation through queued damage.

## Historical terrain stress fixtures

The following cannon/rover/base descriptions and measurements refer to the
headless `SpacewarsScenario::init_terrain_fixture` and multi-planet workloads,
which retain their established configuration for regression comparisons. They
are not the current `spacewars-terrain` launcher controls or gameplay rules.
The shared material, fragmentation and bounded-gravity implementation continues
to serve both modes.

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
