# Rounder planet surface comparison

Issue [#67](https://github.com/aortez/space-wars/issues/67) now has an opt-in
physical prototype. Select either launcher entry, or run:

```sh
cargo run --locked -p engine-client -- --scenario spacewars-surface-contour --seed 42
cargo run --locked -p engine-client -- --scenario spacewars-surface-blocks --seed 42
```

The next refinement is available as **`spacewars-surface-round`**. It preserves
boundary samples and interpolates crossings within the grid, addressing the
remaining bump in the midpoint contour. See [Interpolated surfaces](rounder-planets-interpolated.md)
for its implementation, measured roundness and validation. The report below
preserves the first two-mode checkpoint.

Both scenes use the same radius-60 planet, material layout, ships, controls and
initial excavations. The planet is stationary and its grid is rotated 45 degrees
under the initial landing. A shallow crater, a roofed tunnel and a cap attached by
one cell occupy the opposite hemisphere. Settings supports one or two human seats.
The ordinary `spacewars` match still uses the existing stepped geometry.

Use the normal Expedition controls: land rear-first, B/X to exit or board,
left/right to walk, A/Space to jump or get up, and hold A while airborne for the
jetpack. Aim with the right stick or arrow keys, mine with RT/LB/E, and cycle
cut sizes with Y/T. Stand still to claim. Ship/pod loss and rebuilding use the
existing recovery loop. The comparison label identifies steps versus slopes;
dark outlines show the actual collision decomposition.

The cap's last connection is at planet-local `(-45, 0)`, cell `(15, 60)`.
Cutting that cell releases the cap. The crater is centered at `(0, -58)` with
radius 5; the tunnel runs from `(-18, -44)` to `(18, -44)` with radius 2.
These coordinates are reproducible fixture details, not additional player controls.

## What changed

Material remains a one-unit full/empty grid with two-byte rock/ore cells.
The optional contour joins exposed edge midpoints with slopes. Ambiguous
diagonal cells remain separate. Single cells become diamonds; holes and thin
bridges remain represented. Full interior cells still merge into rectangles;
only the boundary needs additional convex polygons.

Drawing and Rapier use the same rectangle/polygon cache. Queries at sloped
corners map back to a real source cell, including the small triangles extending
into concave corners. The triangle's material comes from that source. These
triangles are derived geometry, not added or deposited matter.

Physics retains the full cells' mass, center of mass and polar inertia even
when their collision silhouettes change. A clipped cell collider carries its
original square's mass properties; concave fill has no additional mass. New
fragments retain the selected surface style and the existing point-velocity
rules. The conservation ledger still counts material, not the area of the
approximated visible boundary.

Neighboring chunk revisions now invalidate dependent contour geometry. An edit
still commits through the existing shared lifecycle. Mining, impact targeting,
landing footing samples and claim surveys resolve the derived surface to
material. A flag on a surviving cell can re-anchor to its new exposed edge after
a neighboring edit; removal or detachment of its footing still neutralizes the
planet. Ordinary block-mode behavior is retained.

This is a coarse contour reconstruction, not continuous curvature or sediment
simulation. It can improve traversal while retaining visible facets at some
bearings. Deposition remains the separate [#51 investigation](design/rounder-planets-and-deposition.md).

## Initial measurements

Desktop debug build, seed 42. Eight walking cases start on untouched diagonal
ground at four quadrants, with both walking directions. Each measures six seconds
of ordinary held horizontal input after two seconds of settling. Ships are moved
out of the walking route for this fixture. Distance is angular travel around
the planet multiplied by its 59.4-unit nominal surface radius.

| Quadrant | Direction | Steps: distance | Slopes: distance |
| --- | --- | --- | --- |
| 0 | -1 | 6.65 | 27.05 |
| 0 | +1 | 9.03 | 27.18 |
| 1 | -1 | 9.03 | 27.30 |
| 1 | +1 | 6.65 | 26.94 |
| 2 | -1 | 11.61 | 27.08 |
| 2 | +1 | 9.03 | 27.60 |
| 3 | -1 | 6.65 | 27.37 |
| 3 | +1 | 11.78 | 27.40 |

The slope cases had zero knockdowns; three stepped cases each had one. This
does not establish continuously grounded motion: slope cases had a qualifying
support contact on 168–185 of the 360 walking ticks. Small contact gaps/bounces
remain a useful follow-up. Tests require meaningful travel and no knockdown,
not exact floating-point distances.

In the ten-second passive landing comparison, both ships reached `Landed` on
the contour. The stepped control left P1 in `Assisted` while P2 landed. That
existing stall is preserved as an observed control result, not hidden by changing
its landing criteria. These selected results do not prove every generated-world
landing or bot route is improved.

## Reproducing checks

```sh
cargo test --locked -p engine-terrain -p engine-rapier --lib
cargo test --locked -p scenario-spacewars surface_sortie::comparison -- --nocapture
cargo test --locked -p scenario-spacewars surface_sortie::material::tests
cargo test --locked -p engine-client --test ui_control_functional \
  surface_comparison_launch_pause_restart_and_both_renderers \
  -- --ignored --test-threads=1 --nocapture
```

The functional check needs an explicit display; use a private Xvfb for unattended
desktop runs. It launches both scenes through the launcher, changes the player
count, checks both renderers, pauses, restarts and returns to the launcher.

The existing flight-bot endurance runner now accepts `--surface original`,
`--surface blocks`, or `--surface contour`. `original` preserves its old fixture.
The two comparison options use the new prepared scene, including its initial
excavation accounting. For example:

```sh
cargo run --locked -p spacewars-ai --example surface_flight_soak -- \
  --surface contour --seconds 180 --seed 42 --players 2 --seat 0 --case 0 \
  --out /tmp/contour-flight-seat0-case0
```

The runner continues for all requested ticks after a completed sortie and records
goal changes, blocks, material accounting and finite motion. An incomplete sortie
still writes `report.json` and exits unsuccessfully. It uses the existing bot,
not a new surface-specific policy. Compare both directions (`--case 0/1`) and
both seats with otherwise identical arguments. Debug or concurrent run timings
are diagnostic data, not cabinet frame-rate measurements.

## Validation checkpoint — 2026-09-11

On `rounder-planet-surfaces`, based on main `611df45`:

- `cargo test --locked --workspace --all-targets`: **1,271 passed**, zero failed,
  41 display/manual tests ignored. A subsequent focused client test also passed
  after adding the comparison HUD label and collision-overlay regression check.
- The five comparison tests exercise diagonal landing and claiming, walking in
  eight directions/quadrants, aimed mining and deterministic clone continuation,
  flag re-anchoring, and cutting the last bridge into a physical fragment.
- The 19 material-scenario tests pass, with existing pod landing, rebuilding,
  harmless flag remeshing and detached flag-footing cases extended to both styles.
  The physics tunnel traversal test also passes with both styles.
- The display-dependent comparison lifecycle check passes under private Xvfb.
  Raster/vector screenshots show both labeled scenes and their collision shapes.
- Strict Clippy passes for `engine-terrain` and `engine-rapier` with all targets.
  Broader strict scenario Clippy remains blocked by pre-existing lint findings;
  this checkpoint does not claim a workspace-wide clean Clippy run.

Eight additional runs each executed all **180 simulated seconds** with seed 42,
two seats present, and one active flight bot per run. The other seat was idle.
They are flight/landing/claim checks, not bot-versus-bot combat or asteroid soaks.

| Surface | Bot seat | Direction (`--case`) | Sortie result |
| --- | --- | --- | --- |
| Steps | P1 (`--seat 0`) | +1 (`0`) | Initial landing did not settle; blocked at tick 901 |
| Steps | P1 | -1 (`1`) | Same initial landing stall |
| Steps | P2 (`--seat 1`) | +1 | Complete at tick 5,234 (87.2 s) |
| Steps | P2 | -1 | Complete at tick 5,183 (86.4 s) |
| Slopes | P1 | +1 | Three landing retries; raising a flag at the 180 s cutoff |
| Slopes | P1 | -1 | Complete at tick 5,286 (88.1 s) |
| Slopes | P2 | +1 | Complete at tick 5,296 (88.3 s) |
| Slopes | P2 | -1 | Complete at tick 5,327 (88.8 s) |

All eight retained material accounting and passed the sampled finite-motion/cache
audits; no audit issues were recorded. Three incomplete sorties intentionally
returned failure after writing their reports. All runs contain 180 one-second
samples, including the blocked controls. Two runs executed concurrently; their
timings are not comparative performance evidence.

The unfinished slope route is reproducible with `--surface contour --seed 42
--players 2 --seat 0 --case 0 --seconds 180`. It tried bearings 9, 10 and 11 before
landing at bearing 12 at tick 10,789 and disembarking. The final sample shows a
new flag raising, with no completed capture. Inspect `samples[].pilot.pilot.landing`
(feet, clearances, settling time) alongside `samples[].brain.landing` to distinguish
site selection from contact stabilization. Do not count this as a successful
sortie or assume the retry loop is solved by smoother ground.

Local logs, screenshots and per-run JSON reports are preserved in
`/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/`; its `README.md`
identifies the final captures and commands. These are local artifacts, not checked
into Git. Cabinet playtesting is the next gate: walk both directions, cross the
ship, mine around a flag, explore the tunnel, sever the bridge, and try pod recovery.
After that, address the remaining contact gaps and landing retries before enabling
contours in generated Spacewars matches. Deposition (#51) remains independent.
