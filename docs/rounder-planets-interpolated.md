# Interpolated planet surfaces

The `spacewars-surface-round` preset refines the [first contour experiment](rounder-planets.md).
The initial contour only used solid/empty cells and fixed edge midpoints. It made
walking easier, but retained a visible crown beneath the initial ship: a cell
center barely inside the intended circle produced a disproportionately large
protrusion. There was no fit to the original circle.

```sh
cargo run --locked -p engine-client -- --scenario spacewars-surface-round --seed 42
```

All three comparison presets remain available. They use the same material,
prepared cuts and Expedition controls, including one or two human players.
`ROUND` identifies the refined mode; `SLOPES` is the midpoint contour and `STEPS`
the original blocks. Generated `spacewars` matches now use the refined surface;
see the [before/after match measurements](rounder-planets-matches.md).

## Shape and material are distinct

The new mode stores one signed shape sample per cell center. Initial values come
from the intended circle: radius minus distance to its center. Positive values
identify solid material; negative values identify void. On a sign-changing edge,
the relative magnitudes locate the crossing between the samples. Segments can
have varied angles and positions, rather than only grid axes and 45-degree slopes.

Filled marching-squares regions are partitioned into convex pieces by their
nearest solid sample. Each piece therefore has a material owner. Diagonal-only
material stays disconnected, consistent with the existing four-neighbor split
rule. Completely interior cells still merge into rectangles. Drawing and Rapier
consume these same pieces; contact queries use the same interpolation and ownership.

Samples are bounded to a one-cell distance band. A 0.001-cell minimum magnitude
keeps exact-zero samples from making degenerate colliders or erasing an owned
cell. This is a piecewise-linear approximation with a topology constraint, not an
analytic curved collider or a fixed number of sides around the planet.

The material ledger, durability and mining quantities remain full cells. One
convex patch per boundary cell carries that entire cell's mass, center and polar
inertia; its other patches have no additional mass. Collision area is therefore
an approximation, not the definition of material quantity. A small remnant can
still carry a full material cell's mass.

## Edits, contacts and fragments

When a brush actually removes material, samples in its removed cells and their
immediate neighborhood incorporate the circle/capsule cut distance. A radius-zero
brush uses a half-cell radius for its shape; positive brush radii retain their
current world size and cell-selection rule. Harder cells inside a damage brush
remain solid until their durability reaches zero. An edit that changes durability
without removing material leaves every shape sample untouched.

Affected sample chunks join the ordinary dirty set, including neighbors with no
material removal. Derived geometry checks its existing neighboring-chunk dependencies
before rebuilding. Shape changes commit through the shared physics lifecycle.

The refined contact lookup starts on the actual boundary and tries small inward
offsets if needed for solver separation. The old fixed 0.08-unit inset could step
entirely through a tiny remnant, making its material unmineable. Landing support,
flags, claim surveys, mining and impact targeting use the shared contact mapping.
Flags retain their source cell and re-anchor to a nearby surviving exposed edge;
destroyed or detached footing still neutralizes ownership.

Detached fields retain a one-sample void border around their material, preserving
the original crossings when cropped. Other components' material is excluded.
Fragments keep their samples through later cuts, splits, cloning and serialization.
The material transfer and point-velocity rules remain unchanged.

## Storage compatibility

Legacy fields retain their version-1 binary layout, hash and two-byte cells.
The reader still accepts their eight-field records, including when embedded in
a larger binary record. Fields with shape samples use version 2 and include the
sample bits in their content hash. Both binary and JSON loading validate dimensions,
finite sample values and agreement between sample signs and occupied cells.
Older clients cannot read version-2 fields.

Shape samples add four bytes per grid cell; the 121×121 comparison planet adds
58,564 bytes (about 57 KiB), before derived geometry. This does not increase
material resolution. Tiny mining cuts still have few samples and can remain faceted.

## Measurements and checks

For an untouched radius-59.4 field, sampling exposed boundary vertices and segment
interiors gives a maximum radial error of approximately **0.483 units** for the
midpoint contour and **0.00416 units** for the interpolated contour. Internal
decomposition edges are excluded from this measurement. A mined radius-6 circle
has approximately **0.028 units** of sampled boundary error.

The physics test separately casts rays at 72 bearings around the outside and
through the mined interior, checking hit positions and material ownership. The
outside normals align with the radial direction. An additional tiny-remnant test
reproduces the fixed-inset miss and verifies the corrected contact lookup.

In the same eight six-second walking trials as the initial experiment, the new
mode travels **29.41–29.44 units**, versus **26.94–27.60** for the midpoint contour.
Both have zero knockdowns. Qualifying support is present on 199–222 of 360 walking
ticks in the new mode, so continuously grounded motion is still not established.
Both ships reach `Landed` in the ten-second passive comparison.

```sh
cargo test --locked -p engine-terrain -p engine-rapier --lib
cargo test --locked -p scenario-spacewars surface_sortie::comparison -- --nocapture
cargo test --locked -p scenario-spacewars surface_sortie::material::tests
cargo test --locked -p engine-client --test ui_control_functional \
  surface_comparison_launch_pause_restart_and_both_renderers \
  -- --ignored --test-threads=1 --nocapture
cargo run --locked -p spacewars-ai --example surface_flight_soak -- \
  --surface round --seconds 180 --seed 42 --players 2 --seat 0 --case 0 \
  --out /tmp/round-flight
```

The display-dependent check covers all three presets and both renderers. The
flight runner accepts `--surface original|blocks|contour|round`; use both seats
and both `--case 0|1` directions. It reports an incomplete sortie as a failure
after saving its full three-minute report. These runs do not exercise combat or
random asteroid pressure. Deposition (#51) remains a separate feature.

## Three-minute flight comparison — 2026-09-11

The full workspace suite passes **1,280 tests**, with zero failures and 41
display/manual tests ignored. The separate three-preset display lifecycle test
also passes under Xvfb; both refined-mode renderer captures were visually checked.
Strict all-target Clippy passes for `engine-terrain` and `engine-rapier`, and
formatting/diff checks pass. Broader pre-existing scenario lint debt recorded in
the initial report remains outside this change.

Eight runs used seed 42, two seats present and one active `RulePilotV2` per run.
The other seat remained idle. Each ran all 10,800 ticks, including after a completed
sortie, with 180 one-second material/cache/motion audits. Every audit passed and
material totals remained constant. Runs used the same desktop debug build, two
at a time; elapsed timings are not a performance comparison.

| Active seat | Direction (`--case`) | Midpoint contour | Interpolated |
| --- | --- | --- | --- |
| P1 (`--seat 0`) | +1 (`0`) | Incomplete at 180 s; three landing retries | Complete at 87.9 s; zero retries |
| P1 | -1 (`1`) | Complete at 88.1 s | Complete at 87.5 s; zero retries |
| P2 (`--seat 1`) | +1 | Complete at 88.3 s | Complete at 87.9 s; zero retries |
| P2 | -1 | Complete at 88.8 s | Complete at 88.5 s; zero retries |

The earlier P1 +1 failure reproduces unchanged on the midpoint contour. The
interpolated run completes at its first selected bearing, 9, with landing at tick
4,982, capture at 5,168, boarding at 5,170 and departure completion at 5,274. This
is evidence for the controlled scene, not proof that all generated landing stalls
are resolved. Preserve the midpoint failure as a comparison when investigating
other site-selection or contact-stabilization problems.

Local logs, reports and screenshots are under
`/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/interpolated/`.
The next playtest should compare the initial crown, walk both ways, mine beside
a flag, sever the bridge, and recover from ship loss. Small-cut faceting and
intermittent grounded contacts remain explicit follow-ups before generated-match
adoption; no material deposition or terrain addition is included.
