# Responsiveness before extra raster resolution

The user prefers higher frame rate over smoothing small jagged edges. Use
native-resolution rendering as the performance baseline for the material match;
do not make 120 FPS a claim based on a small raster-only benchmark.

## Native Picade baseline, 2026-09-12

`sw-picade.local` now has saved raster scale **1.0**. Its ordinary automatic
two-bot Spacewars match reports 1024×768 internally and on the display. No binary
or physics changes were made in this follow-up. During this capture the previous diagnostic build
was installed (`6dd603c1ff8727b148e81eae0634c77878bd2b55f4c161df5708b294bca609af`).

Ten live status samples spanning 48.14 seconds give **27.92 FPS / 59.50 updates/s**
from counter differences. Rolling FPS readings range from 21.8 to 32.5, with
most at 29–33. These are different match phases from the earlier 2× capture,
so the total FPS change is descriptive, not a controlled comparison. The
[paired raster experiment](material-match-raster-profile.md) already establishes
the scale benefit with identical frozen input frames.

| Live stage | Earlier 2× sample | Native 1× sample |
| --- | ---: | ---: |
| Simulation/control per displayed frame | 2.58 ms | 2.68 ms |
| Scene construction | 5.30 ms | 5.52 ms |
| Host preparation | 20.05 ms | 9.82 ms |
| KMS/Slint presentation | 19.75 ms | 17.40 ms |
| Nested text drawing within KMS | 8.04 ms | 8.02 ms |
| Nested image clear within preparation | 2.85 ms | 0.71 ms |
| Nested HUD backing/bars within preparation | 5.24 ms | 1.38 ms |

One low-FPS window has simulation/control averaging 8.07 ms with p95 25.47 ms;
another has more visible terrain and higher presentation cost. Native raster
resolution improves the baseline without eliminating these other sources of
variation. Text is drawn at output resolution, so changing raster scale does
not remove its cost.

The later gameplay screenshot, with a pod attempting to land, shows 19 FPS /
54 updates/s. A following status reads 26.5 FPS / 59.9 updates/s, with simulation
cost averaging 6.87 ms and p95 27.78 ms. These observations are outside the
48-second table above; 1× is an improved baseline, not a sustained 28 FPS floor.

The sole saved-setting difference is `launch.raster_scale = 1.0`. Scenario
settings are committed on **Start Game**, not when returning from the settings
page. The selected Round lab was briefly started to save the change, then the
launcher resumed automatic ordinary Spacewars at 1×. Automatic launches read
saved settings directly, including after restart. The installed kiosk service
still passes `--raster-scale 2.0`, which affects the initial manual-launcher
selection; remove that forced argument in a future image/configuration update
so manual startup also respects the saved choice. Do not silently change CLI
argument precedence to work around the service configuration.

Evidence and collection script:
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/native-scale/`

## Implemented: cull before constructing terrain polygons

[Chunk culling and its measurements](terrain-scene-culling.md) now cover the
ordinary material match, with an uncropped reference path and repeatable pixel
comparisons. Bounds are derived from each chunk's actual cached geometry and
refresh alongside it after edits. Rotated planets and detached fragments use a
conservative inverse-transformed camera rectangle. Minimap construction stays
independent. The shared physics/gravity step remains unchanged.

Paired native Pi snapshots save **5.54–6.25 ms per frame** in scene construction,
disposal and raster work combined. Scene construction/disposal falls from
6.21–6.55 ms to 1.15–1.78 ms, and the raster stage saves a further 0.95–1.19 ms.
72–86% of terrain polygons no longer need building; visible geometry and complete
output pixels match. The first implementation deliberately retains whole
intersecting chunks and padding for every supported raster scale.

These savings do not establish complete-frame FPS. Presentation still paints
the same image and text, while gameplay can still have expensive simulation
windows. Per-cell culling or persistent terrain meshes remain possible follow-ups
if measurements justify them; whole chunks already provide a useful saving.

## Implemented: reduce text-profiler overhead

The [text-drawing investigation](text-drawing-profile.md) found that repeated
thread-CPU clock reads add **1.5–1.8 ms per draw** in paired Pi RAM fixtures.
Nested renderer hooks now use monotonic time; the larger presentation stages
retain CPU measurements. The deployed profiler also separates font matching,
glyph work, text textures and fallback painting.

That follow-up's live capture averages 14.49 ms in KMS/Slint, including 6.41 ms of text.
Only 0.10 ms is font matching; glyph lookup/placement/painting takes 4.70 ms, of
which 4.39 ms is texture fallback. The previous live captures include the more
expensive clock policy and different gameplay phases; do not treat their total
FPS as a controlled comparison.

## Implemented: avoid unnecessary glyph destination reads

The [matched RAM/DRM experiment](text-memory-profile.md) confirms that the same
336 glyph operations take 0.77 ms in RAM and 3.79 ms in mapped display memory.
The LinuxKMS XRGB/ARGB pixel target now skips transparent pixels and directly
writes opaque pixels. Partial-alpha blending is unchanged. Mapped glyph fallback
falls to 2.53 ms; complete draws save **1.36–1.40 ms** with identical output.

The deployed change's live sample averages 13.27 ms in KMS/Slint, with 5.04 ms
of text and 3.31 ms of glyph work. The fresh match runs at 42.16 FPS / 59.31
updates/s over 48.12 seconds; this is not evidence that the later-game
control/physics stalls are resolved.

The probe also measures RAM staging and includes its 1.45–1.47 ms full-frame
copy. That helps its generic texture path, but the kiosk already uses a different
opaque RGB image fast path. Keep direct buffers until an equivalent comparison
through that production path justifies changing the strategy.

## Remaining work in this optimization pass

Two measured problems now deserve separate, bounded investigations:

- **Bot sensor stalls:** [live per-update attribution](landing-query-profile.md)
  catches a nine-FPS window with P2 sensors averaging 14.94 ms while the shared
  simulation step averages 0.63 ms. P2 is surveying all 64 landing candidates
  every update. Reusing identical hatch checks cuts hatch work by 41% in a paired
  Pi replay, with unchanged gameplay report and per-tick CSV fields. It reduces
  the cost of a full survey; repeated surveys and ground graph connections
  remain separate follow-ups. Actual match seeds and per-seat timing are now
  available in live status, rather than only the combined host-step bucket.
- **Remaining glyph display-memory cost:** compare RAM staging through the
  actual LinuxKMS fast image path, including copy cost and clock/launcher
  workloads. Alternatively, measure small glyph/HUD regions in RAM. Keep fonts,
  layout and pixels equivalent; do not assume the generic probe's full-frame
  staging result establishes the best live buffer mode.

Scene construction is about 1.19 ms in the post-deployment live sample. Finer terrain
culling and native HUD/minimap blending remain candidates when measurements
justify them, but neither addresses the large simulation stalls.

Keep improvements, reference pixel checks, Pi playtesting and reproduction
notes together. A remaining architectural performance project should not
become an unbounded requirement for finishing the terrain work.

## Later: sustained 60 Hz, then higher-refresh support

The Pi's boot configuration explicitly requests
`video=HDMI-A-1:1024x768M@60`. Its advertised mode names are 1024×768, 800×600,
640×480 and 256×160; this investigation does not establish 120 Hz panel support.
The game uses a 60 Hz fixed simulation step, and the host's render callback has
a fixed 16 ms interval. Merely reducing raster scale does not change either.

60 FPS permits 16.67 ms for a complete frame; 120 FPS permits 8.33 ms. The latest
native KMS presentation alone averages 13.27 ms, leaving little of a 60 Hz budget
for simulation and scene preparation. First pursue steady 60 FPS on this cabinet. Higher-refresh
work then includes confirming a suitable display mode, refresh-aware scheduling,
input latency, and whether to interpolate rendering between existing simulation
steps. Do not double physics frequency by default: it changes CPU demand and
can change gameplay/replay behaviour.
