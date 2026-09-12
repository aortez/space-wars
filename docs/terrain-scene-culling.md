# Cull terrain before constructing the scene

This follows the native 1× baseline. The material-match player views now reject
whole offscreen chunks before allocating and transforming their terrain polygons.
Surviving polygons keep exactly the previous vertices, colors, strokes and order.

## Implementation

`ChunkGeometry` caches tight local bounds from the actual rectangle corners and
polygon vertices. The existing refresh replaces those bounds when mining changes
this chunk or any dependent neighbor. Empty chunks have no bounds. Using actual
geometry covers interpolated/concave patches that cross cell or chunk boundaries.

The frame builder expands the camera rectangle for pixel strokes and numerical
rounding, then transforms its four corners into each planet/fragment's local
coordinates. Nonintersecting chunks are skipped. A rotated camera rectangle's
local AABB is deliberately conservative. Rotation can reduce the savings, but
must not remove visible pieces.

The client knows pane dimensions but its scene API does not receive raster scale.
It supplies the smallest supported size (0.1×) for padding, for both raster and
vector backends. This retains up to roughly 60 logical pixels around the view;
we accept extra chunks to preserve all supported scales without propagating
raster settings through every scenario. Minimap construction is independent.
The comparison lab's diagnostic wireframe remains uncropped.

The cache adds bounds storage per chunk and a pass over newly built vertices
when a chunk refreshes. Visible polygons are still rebuilt each frame; this is
not a persistent transformed-mesh cache.

The uncropped scenario frame API remains available. The normal match adapter
also exposes an optional uncropped reference including its actual bot HUD. The
headless presentation probe can pair both implementations in one frozen world.
Physics, AI decisions and the fixed 60 Hz simulation are unchanged.

## Method

Run the normal Spacewars adapter with seed 42, default settings and two rule bots.
Advance to ticks 120, 1800 and 3600 outside all timers. Freeze each state, then
pair complete (culled) and uncropped scenes at 1× and 2× raster resolution, with
1024×768 output. Three repeats alternate variant/scale order, with ten warmup
draws and thirty measured draws per block. Compare every final UI pixel and text
row, not just image hashes. Save reference/new frames, layer counts, PNGs and CSV.

`scene_ms` includes constructing and disposing a new draw list. Raster time draws
from frozen lists; publishing and generic Slint drawing have separate timers.
The live host measures construction alone as `host_scene`; disposal contributes
to preparation. Their stage names must not be compared as identical definitions.
The generic RAM Slint probe is not the live LinuxKMS draw path.

The Pi kiosk is paused and temporarily STOPped under a resume trap while the
separate diagnostic executable runs. Settings are compared byte-for-byte. Paired
measurements hold simulation state fixed within each process; identical seeds do
not guarantee cross-architecture desktop/Pi physics trajectories.

## Paired Pi results, 2026-09-12

At native 1×, the same frozen state produces these per-frame wall times:

| Frozen match | Scene + disposal, before → after | Raster, before → after | Combined saving |
| --- | ---: | ---: | ---: |
| 2 seconds | 6.21 → 1.15 ms | 6.74 → 5.55 ms | 6.25 ms |
| 30 seconds | 6.55 → 1.78 ms | 7.00 → 5.98 ms | 5.79 ms |
| 60 seconds | 6.28 → 1.69 ms | 7.27 → 6.33 ms | 5.54 ms |

Scene construction/disposal falls by 73–82%. Rasterization also saves about
0.95–1.19 ms by avoiding per-polygon visibility work on the discarded chunks.
Together these stages save 41–48% of their previous native cost. This is not a
41–48% claim about the whole frame: live presentation and simulation remain.

| Frozen match | Terrain polygons, before → after | Visible polygons, both paths at 1× |
| --- | ---: | ---: |
| 2 seconds | 19,926 → 2,837 | 2,388 |
| 30 seconds | 19,926 → 5,545 | 2,447 |
| 60 seconds | 19,926 → 5,146 | 3,066 |

This avoids building 72–86% of terrain polygons in these fixtures. The larger
retained count at 30 seconds illustrates conservative whole-chunk culling:
we deliberately keep some invisible polygons within intersecting chunks.
Visible polygon/vertex counts and all final output pixels match the reference.

At 2× the combined saving is 5.66–5.99 ms; its raster cost remains much higher
(15.32–17.04 ms after culling). The uninstrumented 2-second control saves 6.22 ms
at 1× and 6.36 ms at 2×, consistent with the detailed run. Generic Slint draw
remains about 9.7–10.2 ms with either scene builder, as expected for equal images.

All 24 Pi pairs (18 detailed, six plain) match pixel-for-pixel. The detailed
run at all three desktop snapshots also passes, saving 0.35–0.44 ms in combined
scene/raster work. Desktop/Pi state trajectories differ after the initial
snapshot; compare variants within their own process. Pi temperatures are
68.2–70.1°C, with all 31 frequency samples at 1.5 GHz.

## Live deployment

Fast-deployed to `sw-picade.local` without a reboot. Installed and running client
SHA-256 match `97a8d36aee88109901a17676e1ba111bca8db95ce27bfd4c76eee81a7f82cb56`.
The service is active with zero crash restarts. Saved settings are byte-for-byte
unchanged; the ordinary automatic two-bot match resumes at native raster scale
1.0 and 1024×768 output. The CLI binary is unchanged.

Ten live samples spanning 47.90 seconds give **38.08 FPS / 59.56 updates/s** from
counter differences. Eight of ten rolling FPS samples are 35.1–44.7; the other
two are 23.3 and 29.3. The subsequent screenshot shows 41 FPS / 60 updates/s,
with P1 on foot rebuilding and P2 approaching a landing site.

The earlier native capture averaged 27.92 FPS. These are different match phases,
not a paired end-to-end comparison: use the frozen results above to attribute
the optimization's savings. This is not a sustained 38 FPS minimum or a 60 FPS
claim. The first slow window still has simulation/control p95 25.84 ms; another
has 21.98 ms KMS presentation.

| Native live stage | Earlier capture | After chunk culling |
| --- | ---: | ---: |
| Simulation/control per displayed frame | 2.68 ms | 3.28 ms |
| Scene construction | 5.52 ms | 1.06 ms |
| Host preparation, including raster/disposal | 9.82 ms | 5.71 ms |
| KMS/Slint presentation | 17.40 ms | 16.64 ms |
| Nested live text drawing | 8.02 ms | 7.80 ms |

Disjoint raster stages total 4.97 ms in the new sample; only 0.74 ms of host
preparation remains outside those timers. This supports moving attention to
live HUD text/presentation. Generic frozen text is only about 2.4 ms, so it
does not reproduce the changing live HUD's roughly 7.8 ms cost. Investigate
layout/invalidation and glyph drawing there before choosing the next change.
The periodic simulation/ground-map spikes remain separately tracked in
[the on-foot survey notes](on-foot-survey-fix.md).

## Validation

- 21 terrain library tests pass, including cached bounds after cross-chunk
  removal, damage, dependency refresh and emptying the field, for all surfaces.
- The targeted scenario test passes for visible geometry and drawing order
  on edited planets and detached fragments across rotation, translation and
  camera sizes. Its intersection check is independent of chunk bounds.
- 295 client tests pass, including 216 exact raster-image comparisons across
  Blocks/Contour/Interpolated, one/two players, multiple orientations, edits,
  landscape/portrait/odd-size views and raster scales 0.1, 1, 2 and 3. Minimap
  frames, text rows and raster/vector scene commands also agree. The existing
  explicit three-minute normal-entry/arena test remains ignored.
- All four headless presentation integration tests pass, including variant
  ordering, matching checksums, fewer primitives and untouched malformed
  settings. The probe runs with a deliberately unavailable display backend.
- Formatting and `git diff --check` pass. The Yocto application build succeeds.

## Reproduction

```sh
cargo +1.89.0 test --locked --release -p engine-terrain --lib
cargo +1.89.0 test --locked --release -p scenario-spacewars --lib chunk_culling
cargo +1.89.0 test --locked --release -p engine-client --bin engine-client
cargo +1.89.0 test --locked --release -p engine-client --test presentation_probe
SLINT_BACKEND=nonexistent-backend target/release/engine-client \
  --benchmark-presentation --presentation-raster --presentation-terrain-culling \
  --presentation-match-ticks 120,1800,3600 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 30 --presentation-repeats 3 --presentation-detail \
  --presentation-output /tmp/spacewars-culling-images > /tmp/spacewars-culling.csv
```

The output directory must not contain earlier output names: artifacts use
create-new semantics. Omit `--presentation-detail` to check instrumentation cost.

Evidence: `/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/scene-culling/`
