# Responsiveness before extra raster resolution

Current endpoint: the [GPU-default milestone](gpu-renderer-prototype.md#gpu-default-milestone)
promotes the tested Pi 4/5 geometry/text/buffer profile, retains explicit
software recovery, and bounds release validation. The measurements below are
chronological research notes; further micro-optimizations and long-frame
investigations are follow-ups rather than an open-ended release prerequisite.

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
selection. The kiosk service now omits that forced argument, so the next normal
image update makes manual startup respect the saved choice too. Explicit CLI
arguments retain their existing precedence; a fast binary update does not
install the service change.

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
  The follow-up [survey cadence experiment](landing-survey-cadence.md) removes
  full scans between usable planning updates while keeping selected-site checks
  current. Paired measurements on identical physical trajectories save 28–42%
  of all sensor CPU and 85–87% in the busiest two-second survey windows. Ground
  graph construction still causes individual roughly 51 ms calls; it is the
  next sensor lead. The [ground-route breakdown](ground-route-profile.md)
  reproduces both seeds and separates about 19 ms of ground connections,
  14–16 ms of graph searches and 6 ms of proposed-ship filtering in the slowest
  calls. The [indexed-route implementation](indexed-ground-routes.md) preserves
  the gameplay records and reduces that routing contribution by 55–56%, including
  index construction. Worst complete sensor calls fall to 43–44 ms, but calls
  above 16.67 ms remain equally frequent. Ground connection queries dominate
  accumulated sensor time and remain a separate follow-up. The
  [planning design](design/budgeted-bot-planning.md) connects further query reuse
  and bounded work with improved sortie and mission decisions.
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

## Direction: GPU acceleration, with bounded software improvements now

Keep native 1× rendering as the baseline. The [Clock follow-up](clock-performance-lab.md#native-resolution-follow-up-short-blends-and-covered-clears-2026-09-29)
adds smaller exact ARM blend batches and omits output-background clears hidden
by opaque RGB images. It deliberately does not add a rectangle raster fast
path or change scene geometry, lighting or gameplay.

GPU acceleration is the longer-term architectural direction, not an unlimited
series of CPU shape-specific optimizations. A future experiment should measure
two distinct steps: GPU presentation of the existing CPU-generated image, then
GPU rendering of the scenario's geometry/layers as well. The former cannot
remove the CPU scene-rasterization cost. Preserve the shared renderer inputs
and software fallback, and verify clipping, alpha/layer order, text, screenshots,
input and rotated HyperPixel output alongside the Pi 4 Picades. Benchmark CPU,
frame pacing and visual quality before selecting a default; enabling the
desktop GPU renderer does not establish support in the current Yocto kiosk.

### Next isolated GPU experiment

Use a separate branch and an opt-in build/runtime choice; keep software as the
default until the device checks pass. The current pinned LinuxKMS backend has
a FemtoVG/OpenGL option, while `engine-client`'s kiosk feature and renderer policy
are configured for software. First audit that path and the image's EGL/GBM/Mesa
support before choosing a backend or requiring an OS update.

The [opt-in GPU prototype](gpu-renderer-prototype.md) now provides the explicit
backend selection, separate Mesa-enabled image, and CPU presentation timings.
It runs on the Pi 4 Picade's V3D driver. Live Clock comparisons show that aligned
RGBA conversion makes GPU image presentation modestly cheaper than the software
reference, while retaining CPU scene rasterization. Texture-lifetime experiments
are documented separately from that format/conversion result. The first typed
Clock geometry adapter bypassed CPU rasterization and image uploads, but
per-path FemtoVG submission was slower (40 FPS idle, about 19 FPS rain) and added
seams between adjacent water polygons. The subsequent ordered triangle batch
holds 60 FPS in both fixtures and reduces application CPU from software's
80.5%/92.5% to 32%/55.5%, idle/rain, at native 1× resolution. One scene draw plus
a multisample resolve also removes the internal water-cell fringes. This is a
positive result for the scene-renderer boundary; the live adapter remains
Clock-only and opt-in. The new
[uncapped geometry lab](gpu-renderer-prototype.md#uncapped-geometry-capacity-lab)
also replays frozen Pizza frames and synthetic transparent layers. On Pi 4,
10000 balls take 61.14 ms per completed geometry frame, including 53.44 ms of CPU
tessellation; 128 transparent layers take 70.74 ms, falling to 17.78 ms with
one-quarter the pixels. This identifies separate geometry-processing and
pixel-work limits beyond the display's 60 Hz cap. The subsequent selectable
shared-circle experiment reduces the Pi's 10000-ball geometry frame from 61.03
to 16.62 ms, but regresses Clock rain (4.55 to 4.85 ms) and a mixed interleaved
synthetic animation (3.31 to 8.13 ms, with 384 draws instead of one). Both modes
remain available for paired comparisons; shared circles are not the default.
The subsequent diagnostic controls show that draw count alone is not the main
mixed-scene cost: splitting identical triangles into 384 draws takes 3.77 ms
versus 3.32 ms baseline, while the instanced path takes 8.23 ms. Expanding the
same templates into one triangle batch brings it to 3.73 ms but increases upload
work, so it still does not beat baseline. A separate single-batch slowdown
remains for smaller circle populations on Pi. The
[submission-state controls](gpu-renderer-prototype.md#submission-state-controls)
now isolate a large program-switching cost: alternating two identically linked
triangle programs takes 8.69 ms versus 3.71 ms for the same split geometry and
vertex layout with one program. Uniform/buffer-binding changes save only about
0.29 ms of the circle path. A further shared circle/triangle program reduces
mixed instancing from 8.18 to 5.30 ms with the same 384 draws, uploads and pixels,
but still trails the 3.33 ms streamed triangle baseline and does not improve
Clock rain or the smaller single-batch circle case. The subsequent
[vertex-input controls](gpu-renderer-prototype.md#vertex-input-controls) reduce
mixed instancing further to about 3.7 ms by using a common vertex format and
attribute divisors, confirmed over six 512-frame blocks. Changing only the draw
API or keeping one VAO bound provides little additional benefit. The 3.30 ms
streamed baseline still leads, and neither Clock rain nor smaller circle scenes
benefit. The subsequent
[bounded polygon experiment](gpu-renderer-prototype.md#reusable-polygon-meshes)
extends reuse beyond circles without changing the scenario API. Explicit
immutable definitions plus transforms/colors cut grouped-scene Pi thread CPU
from 4.88 to 2.36 ms, but completed time improves only from 4.97 to 4.68 ms.
Interleaving reusable and deforming polygons increases submission to 768 ordered
draws and regresses completed time from 5.05 to 5.94 ms. Retention adds little
over sharing definitions within each frame; changing definitions also exposes
unnecessary uploads of inactive entries in the combined template buffer. Clock
rain has no reuse hints and does not improve. The subsequent
[expanded polygon batch comparison](gpu-renderer-prototype.md#expanded-polygon-batches)
separates tessellation reuse from submission: CPU expansion into one ordered
triangle batch improves Pi completed time to 4.01 ms grouped, 4.01 ms interleaved
and 3.91 ms with replaced definitions, versus 4.94 / 5.06 / 4.61 ms for ordinary
triangles in six 512-frame confirmation blocks. Grouped instancing still uses
less CPU, and wins completed throughput on the workstation; dynamic scenes with
no reuse hints do not benefit. The subsequent
[production ship artwork trial](gpu-renderer-prototype.md#production-ship-artwork-trial)
calls the actual ship renderer inside timing and includes the cost of identifying
seven immutable local parts, still producing complete ordinary world polygons.
For 72 ships and 24 pods on Pi 4, cached expansion reduces completed time from
7.21 to 6.01 ms and thread CPU from 7.13 to 5.94 ms; instancing regresses to
9.06 ms with 745 ordered draws. Camera zoom changes preserve the result. A
single ship/pod pair has no completed-throughput gain, and Clock rain still does
not benefit. These are prescribed artwork workloads, not full matches.
The subsequent [local geometry trial](gpu-renderer-prototype.md#local-geometry-through-projection)
keeps definitions and transforms through projection, materializing ordinary
polygons only for software or uncached/cache-budget fallback. In the same-binary
Pi comparison it improves the cached fleet batch from 5.98 to 5.49 ms (another
8.3%), versus 7.20 ms for uncached world triangles. Producer plus projection CPU
falls from 0.97 to 0.64 ms; draw counts, uploads and captured cached images do not
change. The small pair still has no throughput gain. This remains a private
renderer-lab input, not a changed scenario serialization contract or default.
Keep all three submission paths and both input representations selectable;
the subsequent [live viewport integration](gpu-renderer-prototype.md#live-viewports-and-grouped-minimaps)
adds ordered split-screen cameras, grouped translucent circular minimaps and
texture-retirement checks without changing scenario output. Pi 4 passes the
24 offscreen layout comparisons and live Clock/Spacewars/menu/fallback checks.
Separate retained minimap targets fix a same-frame scratch-reuse failure caught
by the Pi's targeted pixel probes but missed on the workstation. The standard
image and rendering default remain software.

This is a correctness milestone, not a general speedup: two frozen Spacewars
snapshots take 21.80/30.02 ms per synchronously completed triangle frame on
Pi 4, with 13.97/20.95 ms spent building triangles. These latency measurements
exclude simulation and UI/presentation and are not directly comparable with
the earlier in-flight throughput trials. The subsequent
[retained terrain trial](gpu-renderer-prototype.md#retained-terrain-chunk-experiment)
tests explicit chunk replacement, including neighboring contour edits and new
worlds with reset revision counters. Four prescribed contour planets on Pi 4
improve from 19.61 to 10.79 ms completed and 14.98 to 5.61 ms thread CPU, with
unchanged draws/uploads. An edited/replaced pair improves from 12.68 to 9.10 ms;
the smaller scene mostly saves CPU rather than completed time. Cold caches
regress, so retention is useful where geometry really survives between frames.
The [live terrain integration](gpu-renderer-prototype.md#live-terrain-reuse)
now carries optional neutral definitions/poses beside ordinary frame data.
Two moving Pi replay windows improve 22.61 → 12.50 ms and 32.19 → 18.62 ms,
including source generation, validation and completed GPU work, but excluding
simulation/UI/scanout. Each device passes 144 paired image checks. However,
continuous-view cache churn appears live: a paused scene improves 28 → 45 FPS,
while an unpaused window has no cache hits. Diagnostics confirm 99.5% of its
6,036 misses are existing geometry at a different scale. The close-up player's
hints also fail projected-point validation because of floating-point rounding.

The [adaptive follow-up](gpu-renderer-prototype.md#adaptive-terrain-retention)
reproduces that rejection and validates exact world-space source points instead.
It adds bounded probation before retaining an exact-scale mesh, with eager
and cold controls preserved. The new zoom-heavy Pi replay takes 36.41 ms
ordinary, 38.33 ms eager and 34.82 ms adaptive, cutting 7,681 measured admissions
to one and ending with 0.44 MB instead of 7.13 MB of accounted cache payload.
All 432 paired image checks pass on each device, and live close-up hints are
accepted. This removes a cache regression, not the underlying zoom cost:
tessellation still takes 20.91 ms in that fixture.

The subsequent [scale-independent trial](gpu-renderer-prototype.md#scale-independent-terrain-tessellation)
separates local positions from fixed-pixel outline offsets. Guarded shapes or
scales retain ordinary tessellation, independently of their reusable fills.
Across five matched Pi repeats, the zoom-heavy replay improves from 35.08 ms
adaptive to 27.88 ms, and CPU from 30.23 to 22.99 ms. All hinted fills reuse,
but only 22.4% of outlines do. Conversely, the two steadier windows regress
12.64 → 14.91 ms and 18.87 → 20.09 ms against adaptive exact-scale caching.
Both devices pass 858 paired image checks. The new mode is opt-in; no defaults
change.

The [bounded hybrid](gpu-renderer-prototype.md#hybrid-geometry-reuse) now shares
one cache budget between both representations. New zooms use templates;
exact scales qualify for promotion only after reuse in a later frame, not
merely in another camera pass. Six matched Pi repeats preserve the early
steady view at 12.55 → 12.50 ms, improve the later window 19.08 → 16.47 ms,
and improve changing zoom 36.01 → 27.69 ms versus adaptive exact caching.
Zoom CPU falls 31.33 → 23.01 ms. Both devices pass 1,284 paired image checks.
First-use and tail times are now explicit: the zoom fixture's first hybrid
frame costs 52.18 ms versus adaptive's 39.37 ms, although the eight-frame
total improves 293.44 → 245.45 ms. About 3,872 small outlines per zoom frame
still require tessellation. The [refined outline guard](gpu-renderer-prototype.md#refined-outline-reuse)
now follows Lyon's sharp-corner and both-neighbor folding conditions more closely,
while preserving the 2× margin and tiny-edge fallback. Seven paired Pi repeats
reduce zoom render time 27.42 → 21.57 ms versus the previous hybrid (21.3%),
and CPU 22.77 → 16.85 ms. The two steadier windows are essentially unchanged;
cache size and GPU uploads do not grow. Guarded outlines fall to about 939 per
frame, with all 2,322 image comparisons passing on each device. First use improves
51.71 → 46.20 ms in the zoom fixture, but a real construction spike remains.
The subsequent [paced admission experiment](gpu-renderer-prototype.md#paced-geometry-admission-and-terrain-churn)
adds cache-build timing and a work allowance shared by all camera passes. Eight
matched Pi repeats reduce first-frame cost by 15–18%, but increase eight-frame
startup totals by 27–38%; zoom's later maximum rises 22.72 → 26.07 ms. Pacing is
not a general speedup and remains an optional control. The new terrain-history
probe exercises visibility, edits and world replacement, with no hidden cache
warmup. Both devices pass 3,432 image comparisons and per-frame budget checks.
Keep refined outlines preferred for investigation; next reduce construction
work rather than merely spreading it across frames. Preserve unchanged, edited,
cold and uncached controls so caching cannot hide stale geometry or move work
outside timing. Defaults remain unchanged.

The [construction follow-up](gpu-renderer-prototype.md#exact-repeated-polygon-construction-reuse)
finds roughly 75% of instrumented Pi template-build time in fill/stroke
tessellation. Optional `retained-memoized` reuses exact repeated polygons within
each definition build, with a bounded temporary dictionary and unchanged guards.
Uninstrumented construction improves 13–14% on Pi. Nine matched replay repeats
reduce first-frame time about 6%, with 1–3% better startup totals and essentially
unchanged warm rendering. Five-repeat terrain stress lowers median block maxima
27.60 → 24.08 ms during reveal/zoom and 20.45 → 18.77 ms with edits/replacement.
Average terrain gains are under 1%. Both devices pass 4,492 image comparisons
and source/counter/memory checks. This remains optional: all-unique definitions
can pay extra lookup/finalization cost without reuse. Warm zoom still costs
about 21.5 ms, including 7.33 ms of mesh encoding.

The [warm-encoding breakdown](gpu-renderer-prototype.md#warm-encoding-breakdown-and-bulk-validity-scan)
finds about 2.36 ms of guarded outline tessellation, 1.93 ms of scalable-vertex
expansion and 0.79 ms of final validity scanning in the instrumented Pi zoom
case. The CPU-only total is 6.16 ms without detailed timers; it excludes GPU
traffic and is not directly interchangeable with the full-render encoding
interval. Optional `retained-scan` replaces per-number early exit with a
vectorizable finite-number reduction, retaining every check. Six matched Pi
repeats change full-render averages 12.840 → 12.776 ms, 16.512 → 16.282 ms and
21.487 → 21.295 ms. It is a small 0.5–1.4% win, not a solution to zoom cost;
terrain stress is essentially unchanged and not every tail improves. Both
devices pass 3,240 exact mesh and 5,336 image comparisons, with identical saved
baseline/scan images. An integer-maximum scan was rejected after a workstation
regression. Next investigate the guarded outlines or repeated vertex expansion;
do not weaken geometry guards to turn fallback into a nominal cache hit.

The [contiguous-expansion experiment](gpu-renderer-prototype.md#contiguous-cached-geometry-expansion)
first rejects a frame-local outline dictionary: all 908–964 guarded outlines
in the sampled bot view have distinct exact ordered shape keys. Optional
`retained-ranges` instead combines adjacent safe cached pieces for CPU
expansion, flushing before every guarded tessellation and keeping painter order.
Pi zoom encoding improves 6.090 → 5.854 ms without detailed timers, and completed
rendering improves 21.282 → 20.991 ms (1.4%). About 7,946 expansion calls become
1,002, but vertex counts, upload volume, seven GPU draws and geometry guards
do not change. Both devices pass 5,400 exact mesh and 6,180 image checks.
Terrain edit-history results are mixed, including a worse Pi block maximum;
keep the comparison modes and do not claim a general latency improvement.
The remaining opportunity is substantive tessellation/data-movement work, not
an assumption that every fallback shape can be memoized or that fewer CPU
calls automatically mean less GPU work.

The [GPU-resident reference experiment](gpu-renderer-prototype.md#gpu-resident-ordered-references)
keeps cached vertices in a bounded GPU atlas and streams ordered indices,
references and transforms, retaining the same guards and seven replay draws.
Pi zoom uploads fall 29%, and encoding improves 6.784 → 6.024 ms, but completed
rendering regresses 21.175 → 25.053 ms. A separate texture per camera pass
recovers a large early-replay submission stall (23.121 → 15.591 ms), still
slower than the 12.894 ms CPU-expanded control. Workstation warm replays improve
2–6%; neither result generalizes across hardware. Both devices pass 6,480 exact
mesh and 7,868 image comparisons plus GL-state checks; live Pi session
transitions also pass.
Keep both resident variants opt-in and preserve `retained-ranges` on Pi.
Do not recover it by reverting to hundreds of per-piece draws.

The [buffer-access follow-up](gpu-renderer-prototype.md#buffer-backed-resident-geometry)
adds full read-only storage buffers and a separate uniform-transform control,
preserving ordered references and draw counts. Full storage buffers improve
workstation warm replays 8–12%, but the tested Pi driver exposes **zero**
vertex-stage storage blocks and no texture-buffer extension. Explicitly
requesting that mode fails; probe validation reports it as unsupported, not
passed. The Pi-compatible 16 KiB uniform table handles 512 poses per pass with
texture fallback above that. It does not fix the slowdown: native changing-zoom
means are 21.639 ms for CPU-expanded ranges, 25.478 ms for textures and 25.614 ms
for uniform transforms. The uniform variant keeps identical geometry and seven
draws, adding only 2026 transform-upload bytes/frame. Both devices pass 6480
exact mesh checks; image validation passes 9562 host and 8718 Pi cases,
including overflow/shrink and foreign-binding checks. Preserve the CPU-expanded
Pi control. Further work should address remaining geometry costs or a cheaper
draw representation rather than assuming less upload or a different storage
API automatically improves completed rendering.

The [cached-triangle visibility experiment](gpu-renderer-prototype.md#cached-triangle-visibility)
adds optional `retained-culled`, using actual cached vertex bounds and each
camera's clip. It keeps guarded tessellation, ordering, batching and the same
cache limit; metadata counts toward that limit. Pi zoom uploads fall 35%, from
3.137 to 2.039 MB/frame, and CPU-only encoding improves 5.898 → 5.213 ms.
Completed rendering improves only 21.977 → 21.281 ms because submission rises
2.676 → 3.196 ms. Later gameplay improves 16.682 → 14.948 ms; early gameplay
is effectively unchanged. Terrain reveal is about 2% slower, and some zoom
and early-replay tails worsen. Host replay means improve 4–14%, but its terrain
controls regress 2–4%. Keep both controls rather than assuming fewer vertices
guarantee smoother frames. All 216 replay and 160 terrain-history culling
comparisons per device are byte-identical to the ranges path. Next isolate
Pi buffer-submission costs and consider coarser bounds for already-visible
geometry; do not weaken outline guards to inflate culling/reuse statistics.

The [submission breakdown](gpu-renderer-prototype.md#submission-timing-and-per-pass-streaming)
locates the Pi zoom penalty inside vertex-upload calls: profiled ranges versus
culled costs about 1.20 versus 1.88–1.93 ms there, with similar index/draw costs.
An offscreen-only per-pass buffer control does not fix it: all Pi uninstrumented
completed-frame means change by less than 0.9%, and host means by less than
0.7%. Exact image/count parity passes on both devices. Keep defaults unchanged.
Do not confuse GL-call wall time with GPU-active
time or treat the faster profiled early-game runs as an optimization.

The [stable-capacity follow-up](gpu-renderer-prototype.md#stable-streaming-capacity-experiment)
adds bounded grow-only power-of-two stores, discarded before uploading only
used bytes, and counterbalances positions and within-row predecessors over
eight repeats. Pi culled completed frames improve 3.6–5.0%; changing zoom is
20.704 → 19.661 ms, with pooled warm p95 24.319 → 21.121 ms. Vertex API wall
time falls 1.849 → 0.903 ms including both reserve and upload. Geometry, actual
uploaded bytes and seven draws are identical. Logical zoom capacity grows
2.08 → 3.57 MB; it is not a measurement of peak driver memory. Non-culling Pi
means change less than 1%, and workstation means less than 0.8%. Exact image,
geometry and foreign-binding checks pass on both devices.

The [exact-size split-upload control](gpu-renderer-prototype.md#separating-the-upload-api-from-capacity)
now separates the API sequence from the capacity policy, using six
counterbalanced uninstrumented modes while still validating all twelve
plain/profiled variants. Exact combined and exact split have identical storage
metrics on every frame; their Pi means differ by less than 0.9%. Bucketed stores
retain a 3.8–4.4% improvement for culled replays. Culled zoom is
20.579 / 20.620 / 19.671 ms for exact combined / exact split / bucketed split,
with pooled warm p95 24.098 / 24.263 / 21.007 ms. Geometry and actual uploaded
bytes are unchanged; logical zoom capacity is 2.08 / 2.08 / 3.57 MB. The
workstation remains effectively tied. First-frame latency is not uniformly
better. Both devices pass 2,592 exact RGBA/count checks and twelve PNG-pair
comparisons. The benefit depends on the capacity policy, not just splitting
the upload calls; this still does not identify internal driver allocation
behavior. Keep defaults unchanged. The queued-history follow-up below covers
offscreen overlap and terrain edits/reveals; full live validation remains.

The [queued-history experiment](gpu-renderer-prototype.md#queued-uploads-across-moving-and-edited-scenes)
compares exact/bucketed uploads at queue depths one and three, using prepared
moving Spacewars and terrain histories. It times encoding through the final
fence drain, excluding simulation, source/projection and UI/presentation. At
depth three, bucketed culled Pi rendering costs 15.0% less early, 18.1% less
later and 10.0% less during zoom; render-thread CPU falls 7.9–10.4%. Zoom is
9.012 → 8.114 ms per completed frame. Terrain depth-three changes stay below
0.4%, and workstation policy changes below 1.8%. These are not complete live
frame times. Overlap improves throughput but increases CPU-observed completion
age: bucketed culled zoom's warm p95 is 14.640 ms at depth one versus 23.771 ms
at depth three, not a GPU or input-latency measurement. Reserved capacity is
higher, and prepared inputs keep definitions alive rather than testing live
producer garbage collection. Both devices pass 3,200 queued image/count checks
and 120 PNG-pair comparisons. Do not deepen the gameplay queue based on
throughput alone; the live follow-up below preserves normal application pacing.

The [live stream selector](gpu-renderer-prototype.md#live-stream-policy-selection)
now compares shared/exact/bucketed uploads independently of geometry mode.
Six counterbalanced blocks per policy and fixture on Pi 4 show effectively
unchanged Clock CPU: about 32% of one core idle and 56% in heavy rain. Spacewars
draw CPU falls 12.922 → 10.968 ms with bucketed versus shared stores, while
sampled rendering increases 45.12 → 48.38 FPS at roughly 60 UPS. Whole-process
CPU stays near one core, 96.35% → 95.25%, with more frames rendered.
Submission wall time falls 3.618 → 1.806 ms; the largest sampled host interval
falls 75.735 → 26.763 ms. These are short live windows with slightly different
render histories, not identical-frame timings or guaranteed latency bounds.
Logical retained stores rise from 0.285 to 4.512 MB including dormant shared
buffers; these are not peak driver/GPU memory measurements. All nine paused
RGBA screenshot comparisons match exactly, including UI and split views;
mode/session transitions, pool retirement and host/Pi layout checks pass.
The [longer live validation and controlled camera sweep](gpu-renderer-prototype.md#longer-live-match-validation)
extend that evidence. Across 608 seconds and two rematches, targets stay at three,
stream slots at five, and logical stream capacity stops growing during the first
match at 6.35 MB. A first-game-over host interval reaches 122.58 ms, accompanied
by 100.65 ms draw CPU; other 40–44 ms intervals coincide with long simulation
steps. These observations identify follow-up work, not isolated causes or a
global latency bound.

The new session-only `host gpu-zoom sweep` varies both player cameras in opposite
phases without changing physics, saved settings or minimap consistency. Four
paired 32-second blocks per policy show 45.86 → 51.22 FPS and 13.370 → 10.820 ms
draw CPU with bucketed uploads, at roughly 60 UPS. Nine more paused full-image
RGBA comparisons match exactly. Resource limits and session retirement pass;
the remaining sampled host maximum is 59.06 ms. Keep defaults unchanged. This
validates the upload change in the app without solving all its work or
establishing Pi 5 compatibility.

A [bounded rendering trace](gpu-renderer-prototype.md#first-and-repeat-game-over-transitions)
now aligns loop stages, GL calls and the visible UI on each captured iteration.
Two natural match endings in one Pi process show 2,653 versus 125 GL draw calls
and 109.302 versus 16.267 ms draw CPU on the first/repeat result-screen frame.
Settled results use 93 calls. Uploads and presentation waits account for little
of the initial burst. Pinned FemtoVG source renders uncached glyph outlines
eight times into a reusable atlas; this and the warm repeat strongly implicate
glyph generation. Zero explicit
shader compile/link calls does not rule out internal driver compilation. These
intrusively timed captures diagnose a hitch, not an optimization speedup.
The [isolated text-atlas probe](gpu-renderer-prototype.md#isolating-the-text-atlas)
now counts 3,560 atlas draws when fresh versus zero when cached, with 14 output
draws in either case. On Pi, a fresh atlas after process warmup costs 92.024 ms
complete render-thread CPU versus about 2 ms for repeated cached text.
Same-renderer row preparation makes reveal cheap but costs 182.638 ms in total
and has a 23.456 ms largest step; a separate renderer cannot warm this atlas.
All compared outputs match exactly. Half-pixel item movement reuses glyphs,
but centered-layout and font-size changes can cause another large burst.
Fresh/warm/fresh disk-cache controls also expose additional first-process
work without changing the recurring 92 ms population cost.

The [CPU glyph-mask comparison](gpu-renderer-prototype.md#cpu-glyph-mask-comparison)
now reduces fresh-atlas Pi CPU from 91.934 to 3.275 ms and completed wall time
from 94.737 to 5.135 ms. It replaces 3,560 vector-atlas draws with one batched
upload, retaining the same shaping, glyph placement, RGBA atlas and output
quad API. Cached rendering stays cheap; layout/font-size bursts also shrink.
Its GPU reference matches the earlier Slint fixture exactly, while CPU masks
have small measured antialiasing differences with unchanged alignment.
A second process and odd-sized viewport confirm the result. This is still
an offscreen, fill-only prototype with explicit fonts and a hard resource cap,
not a production text backend or a whole-app startup/FPS result.

The [switchable live integration](gpu-renderer-prototype.md#live-ui-cpu-glyph-cache)
now runs CPU masks inside Slint's pinned FemtoVG dependency. Cold launcher,
settings and result-screen draw CPU falls from 106.500/90.165/162.398 ms to
21.119/18.994/27.242 ms in three alternating-policy reset comparisons. Sixteen
menu/device-info cycles stabilize at one CPU page and 355 entries; a full
natural match also completes without CPU-cache errors or fallback. Bounds,
deferred resets, upload failure and destruction have regression tests.
Vector generation remains the default/reference; unsupported text falls back.
The [per-run face-reuse follow-up](gpu-renderer-prototype.md#per-run-font-face-reuse)
now reduces integrated Slint CPU from 13.607 to 5.507 ms in opposite-order
comparisons, with identical pixels, masks, uploads and draw counts. It removes
repeated font parsing within a run (237 → 14 calls in the cold fixture), without
a persistent font cache. Live cold result draws fall from 26.836 to 18.508 ms.
The full match passes, but its first natural result draw still costs 26.904 ms
CPU, plus 12.180 ms in timers; this is not a hitch-free transition. Uploads
account for only about 2 ms. The
[bounded text diagnostics](gpu-renderer-prototype.md#separating-text-preparation-from-mask-generation)
now distinguish mask construction from the uncached shaping path. With detailed
clocks enabled, the first natural result loop spends 8.760 ms in 51 shaping-run
misses, plus 2.374 ms constructing masks and 2.215 ms uploading them. Atlas-reset
controls have zero shaping-run misses: they cannot represent every first-use
cost. The [fresh-context shaping experiment](gpu-renderer-prototype.md#reusing-font-faces-during-uncached-shaping)
now isolates repeated font-face construction. Bounded per-run reuse reduces
cold Pi shaping CPU from 6.796 to 3.606 ms, with the same word-shaping work and
glyph metrics. Width/font-size changes improve similarly; cached text does not.
All 360 paired host/Pi images match exactly. A paired natural result draw improves
28.895 → 26.068 ms CPU, while its whole loop still costs 38.177 ms. This is one
live transition per mode, with slightly different texture-upload bytes, not a
statistical whole-app result. The cleaner fresh-context comparison is repeated
and counterbalanced. Diagnostic overhead is measurable; use ordinary captures
for performance comparisons. The
[application-attribution capture](gpu-renderer-prototype.md#frame-aligned-application-attribution)
now adds opt-in host and rendering-callback spans via `render-trace start-work`.
The measured first result loop costs 37.853 ms CPU; scoreboard publication and
menu properties account for just 0.029 ms. On the frozen result, scene generation,
staging and direct drawing account for 14.741 of 21.195 ms instrumented loop CPU.
Physics is stopped and shaping has no further misses. This attributes work; it
is not another optimization result. Alternating text-only/application captures
differ by just 0.036 ms on average, while the basic control without optional
text/application clocks costs 19.888 ms. Pixels and steady GL workloads agree.
The [preparation follow-up](gpu-renderer-prototype.md#scene-preparation-and-temporary-storage)
separates roughly 4 ms of projection from 2.4 ms freeing temporary source/plan
data. Its session-only `gpu-plan reference|reserved|scratch` comparison preserves exact
geometry while reserving known sizes and skipping unused hint indices. On the
same frozen Pi result, projection falls 4.135 → 3.516 ms and basic whole-loop CPU
20.427 → 19.744 ms (3.3%), with identical pixels and GL work. Cleanup remains
about 2.5 ms. The [bounded scratch follow-up](gpu-renderer-prototype.md#bounded-scratch-plan-storage)
now halves plan construction/disposal in matched moving-camera replays, with
roughly 10% lower completed geometry wall time. In the full application,
frozen whole-loop CPU falls 20.819 → 18.137 ms (12.9%), with identical pixels
and GPU work. One completed CPU plan can be retained under an 8 MiB cap;
the natural match peaks at 1.74 MiB with zero discards. Every frame still
validates and rewrites its geometry. Keep the reference default and compare
broader workloads before promotion. The [source-point pool follow-up](gpu-renderer-prototype.md#source-allocation-measurements-on-pi-4)
now cuts source creation/disposal CPU 44–47% in moving Spacewars fixtures and
same-build basic frozen whole-loop CPU 19.375 → 17.108 ms (11.7%). Pixels and
GL work remain identical. The renderer-independent pool retains only empty
point allocations, peaking at 476 KiB under its 2 MiB host limit; it does not
cache source contents or change the owned frame contract. Keep source pooling
opt-in and separately selectable. Projection and direct drawing now outweigh
source cleanup; the cold result transition still costs 35.611 ms CPU.
The [prepared-transform comparison](gpu-renderer-prototype.md#prepared-transform-measurements-on-pi-4)
adds a separate `gpu-plan prepared` option over scratch storage. Once-per-camera
bounds and once-per-object rotations preserve exact point arithmetic and hint
validation. Pi plan CPU falls another 11–12%, but completed geometry wall time
falls only 1.4–1.5%; this is a small saving, with identical pixels and memory
bounds, not removal of the larger encoding/submission costs. The same-build
full-application comparison averages 17.562 → 17.320 ms loop CPU (1.4%), with
unchanged screenshots and GL work. The first natural result loop still costs
38.250 ms CPU, so this does not resolve cold UI latency.
The [CPU expansion experiment](gpu-renderer-prototype.md#cpu-mesh-expansion-experiment)
rejects a portable four-vertex loop: real Pi encoding CPU regresses 3–19%,
and completed drawing does not improve. Its initial microbenchmark used
different compiler settings; matching the application's settings reverses
that kernel result too. The candidate is archived rather than retained as
another live policy. The scalar renderer remains unchanged. Guarded stroke
construction remains a larger cost. The
[stroke attribution follow-up](gpu-renderer-prototype.md#guarded-stroke-attribution)
places 0.174 ms in input preparation and 2.177 ms in Lyon tessellation/output
per changing-zoom frame. Detailed clocks add 15.7%, so use the 5.209 ms ordinary
encoding total as the performance baseline rather than summing nested spans.
About 41% of guarded strokes generate wholly offscreen geometry; this identifies
an opportunity, not a proven safe early-culling rule. The
[bulk output writer comparison](gpu-renderer-prototype.md#stroke-output-writer-in-moving-scenes)
now reduces real moving zoom encoding CPU 5.047 → 4.923 ms (2.47%) and completed
geometry wall time 13.461 → 13.349 ms (0.83%), with exact geometry/pixels and all
eight paired blocks improving. Ordinary gameplay is effectively unchanged.
Keep the writer benchmark-only until live validation; this small saving does
not establish a whole-application FPS gain. The
[early-rejection comparison](gpu-renderer-prototype.md#early-rejection-of-guarded-strokes)
now skips 40.6% of guarded zoom strokes while preserving the safety guard.
Encoding CPU falls 5.110 → 4.207 ms (17.7%), completed drawing wall time falls
13.459 → 12.396 ms (7.9%), and upload payload falls 9.0%. All eight paired
blocks improve, with exact pixels across 84 view states and independent
tessellation checks of every proposed rejection during validation. The bound
is intentionally loose and restricted to the pinned Lyon polygon-stroke path;
unknown cases keep ordinary rendering. Early/later gameplay is effectively
unchanged. The [live rejection control](gpu-renderer-prototype.md#live-guarded-stroke-control)
now adds session-only `gpu-strokes off|conservative|checked` and measured
tested/unknown/rejected counters. Four paired Pi zoom blocks reduce drawing CPU
9.668 → 9.089 ms (6.0%) and whole-loop CPU 14.939 → 14.345 ms (4.0%); sampled
FPS improves 55.69 → 56.80, with simulation near 60 updates/s. These are aligned
simulation intervals at natural rendering rates, not identical per-frame
workloads. Both full matches, rematches, and session transitions pass with
bounded resources. Paused captures match exactly but use warmed cached geometry;
the separate moving checked run exercises real rejections without mismatches.
Keep rejection off by default pending broader scene/device coverage, and retain
the original path. The bulk output writer remains benchmark-only. Neither
experiment changes a rendering default.
Unchanged-world reuse for paused/results is a separate opportunity, not
evidence of better active-game throughput. UI work outside the scene callback still is
not layout alone. Keep the CPU controls and the vector reference available.
The [UI correctness follow-up](gpu-renderer-prototype.md#rounded-borders-and-fresh-kms-screenshots)
now fixes Pi 4 rounded borders with a version-gated V3D workaround and captures
KMS/FemtoVG screenshots before swap. The old-binary menu regression fails and
the new one passes; 46 isolated border, clipping and overlap cases pass too.
This does not establish arbitrary-font coverage, extend the screenshot
guarantee to desktop GPU backends, or remove geometry and simulation costs.
The [Pi 5 follow-up](gpu-renderer-prototype.md#pi-5-rotation-fix-and-matched-clock-comparison)
now fixes Slint's physical/logical canvas-size mismatch with a notifier on the
rotated HyperPixel, and normalizes GPU screenshots to logical coordinates.
All 66 pixel fixture cases pass, as do real menu transitions before and after
enabling/disabling direct rendering. A same-build counterbalanced native-1×
comparison measures software / aligned RGBA / triangles at 27.2% / 22.0% / 7.2%
of one core in idle Clock and 34.0% / 28.1% / 17.4% in heavy rain, all at 60 Hz.
Direct geometry therefore saves 74% / 49% CPU in these fixtures. The initial
Pi 5 comparison covered original RGB uploads, not the direct path responsible
for the larger Pi 4 win. Cropped samples from that initial run remain invalid.
The [complex-scene comparison](gpu-renderer-prototype.md#pi-5-spacewars-and-pizza-comparison)
then shows why the rendering paths must remain separately selectable. At native
800×480, the live Spacewars landing/approach fixture uses 57.2% / 34.6% / 81.9%
of one core with software / aligned RGBA / direct triangles. Retained/culled
geometry with the allocation controls reduces the direct path to 57.0%, still
above RGBA. Live dense Pizza with 1000 balls instead benefits from instancing:
48.3% CPU versus software's 71.1% and RGBA's 62.7%. All hold about 60 Hz, while
the uncapped 10000-ball render-only fixture falls from 23.25 to 4.85 ms with
circles. Mixed interleaved geometry regresses 0.93 to 2.51 ms under that same
circle policy. Moving Spacewars reuse improves early/later replay rendering
8.15/12.96 to 3.57/4.71 ms, but close-up zoom still costs 8.90 ms, including
4.69 ms encoding. Keep the faster RGBA reference alongside the direct controls
while investigating that remaining zoom cost; no universal GPU default follows
from Clock or dense circles alone.
The Pi 5 encoding-only control further attributes the zoom cost to guarded
outlines, not cache lookup: 4.43 ms uninstrumented versus 6.34 ms with detailed
clocks, including 4.52 ms in outlines. About 1123 of 3690 rebuilt outlines per
frame prove fully offscreen after tessellation. The subsequent
[Pi 5 guarded-stroke comparison](gpu-renderer-prototype.md#pi-5-guarded-stroke-comparison)
tests the existing conservative rejection control: identical-scene zoom
encoding improves 23.6% and completed rendering 18.2%, with exact pixels in all
84 validation states. Live normal-camera process CPU falls 57.0% → 51.0%, still
above aligned RGBA's 33.7%. A separate forced sweep improves 47.7% → 42.6%, with
both paths near 60 Hz; moving checked runs exercise real rejection without
renderer errors. Keep this opt-in. The subsequent
[matched Pi 5 zoom comparison](gpu-renderer-prototype.md#pi-5-matched-zoom-comparison)
moves the sweep into renderer-independent presentation state and compares the
same camera trajectory: software / aligned RGBA / cached geometry / cached
geometry with rejection use **53.8% / 32.2% / 48.2% / 42.2%** of one core.
Reverse-order repeats agree, with all paths near 60 Hz. Direct rendering saves
host-side work but still spends 3.067 ms encoding; its whole-loop CPU is
1.670 ms/frame above RGBA. Investigate the remaining visible-outline work while
retaining this matched reference. Neither larger caches nor relaxed outline
safety guards are justified by these measurements. `host camera-zoom` works
across rendering paths, retains the `gpu-zoom` alias, and changes no saved
settings or simulation state.

The [Pi 5 outline census](gpu-renderer-prototype.md#pi-5-small-outline-attribution)
narrows the close-up replay hot spot further: after rejection, all 2575 rebuilt
outlines/frame are at most two pixels across with 0.75-pixel strokes. Shapes at
most one pixel across account for 88% of nested tessellation time; wholly
offscreen output accounts for only 4%. These are diagnostic replay timings,
not a new whole-application gain. The census is bounded and runs separately
from timed blocks; it is never enabled in normal gameplay.

The subsequent [tiny-outline control](gpu-renderer-prototype.md#pi-5-tiny-outline-approximation)
uses bevel joins only on uncached opaque same-color polygons at most one pixel
across. Close-up encoding falls **3.183 → 2.785 ms**, with completed drawing
**5.420 → 4.987 ms**; early/later cached views gain nothing meaningful. Across
443 quality comparisons, production differences stay within four pixels/frame
and the checked terrain/history transitions remain intact, but isolated tiny
shapes expose greater coverage changes. Keep this explicitly lossy control
benchmark-only. Cached templates remain round, so it is not yet a general
history-independent detail policy. The subsequent
[exact round-join shortcut](gpu-renderer-prototype.md#pi-5-exact-round-join-shortcut)
skips calculations only when they would emit no arc. Its same-build close-up
encoding falls **3.361 → 3.003 ms**, with completed drawing
**5.657 → 5.292 ms**; all 439 cross-mode image checks and independent
reference/exact/checked vertex comparisons are exact. Geometry, upload sizes,
scale guards and budgets stay unchanged. However, a four-pair comparison with
the preceding binary finds **5.1% higher close-up encoding CPU in the new
disabled/reference path**; that within-build saving is not a net gain over
stock Lyon. The subsequent
[compile-time policy comparison](gpu-renderer-prototype.md#pi-5-compile-time-round-join-policies)
removes the per-join mode/counter machinery from the reference specialization
without duplicating the stroke algorithm. In its final three-binary run,
close-up reference encoding remains **1.3%** above the original, while exact
encoding improves **3.204 → 2.929 ms (8.6%)** and completed drawing improves
**5.481 → 5.127 ms (6.4%)**, measured against that original executable. Output,
geometry, uploads and cache decisions remain exact. An explicit inline hint
has only small, mixed effects; it does not explain the main gain. Ordinary
views are still about 1.2–1.3% slower in completed drawing, so keep the shortcut
experimental. Preserve the original comparison path, and do not silently
discard small geometry or add a rectangle-specific path. The subsequent
[live round-join comparison](gpu-renderer-prototype.md#pi-5-live-round-join-comparison)
adds `host gpu-joins reference|exact|checked`, with reference still the default
and no saved setting. In 16 counterbalanced live runs, exact reduces process
CPU against the original from **35.0% → 34.4%** for ordinary cameras and
**42.4% → 41.4%** for zoom. CPU/frame improves 1.2% and 2.4%; the new reference
still carries a small baseline penalty. Aligned RGBA remains cheaper at
**30.1% / 32.0%** of one core. All paths remain near 60 Hz, so FPS alone would
hide these differences. Twelve paused transition comparisons are exact, moving
checked runs pass, and the Pi's 443 offscreen quality checks are pixel-exact.
The matched zoom still leaves direct geometry about **1.58 ms/frame** higher
in KMS-loop CPU than RGBA. Keep the controls and investigate remaining geometry
construction/submission; do not promote a universal direct-rendering default
or subtract replay savings from older live timings.

The subsequent
[combined exact stroke comparison](gpu-renderer-prototype.md#pi-5-combined-exact-stroke-controls)
adds an independent session-only `host gpu-stroke-writer stock|bulk` control,
with stock still the default. Combined with exact joins, bulk reduces close-up
encoding **2.953 → 2.856 ms (3.3%)** within the new build, but completed drawing
is effectively flat at **5.151 → 5.143 ms**, with mixed repeat directions.
The previous executable remains a separate baseline: its exact encoding is
2.925 ms and completed drawing 5.169 ms. All six writer/join combinations
preserve vertex bits and triangle order, and all 5,316 Pi image checks are
exact. The following 16-run live comparison finds no meaningful net gain over
the previous exact executable: ordinary process CPU is **34.46% → 34.48%**,
and zoom **41.66% → 41.69%**. Aligned RGBA is **29.16% / 30.93%** in the same
matrix; combined direct zoom still costs about **1.78 ms/frame** more KMS-loop
CPU. All 20 paused image comparisons and moving checked runs pass. Keep the
control experimental and move attention from output writing to remaining
stroke construction; isolated encoding savings have not established useful
application headroom here.

The [concrete-output follow-up](gpu-renderer-prototype.md#pi-5-concrete-stroke-output)
keeps the writer's type through the shared polygon/line stroker, allowing
compiler specialization without a second algorithm or changed geometry.
The session-only `bulk-static` control reduces same-build close-up encoding
**2.814 → 2.509 ms (10.9%)** and completed drawing **5.091 → 4.774 ms (6.2%)**
with exact joins. All four groups improve, all nine writer/join combinations
preserve geometry, and all 7,088 Pi image checks are exact. The following
16-run live comparison now finds a modest real gain: process CPU drops
**34.05% → 32.99%** ordinarily and **41.14% → 39.86%** during zoom, about
**3.1%** relative to the dynamic bulk path. The previous bulk/exact executable
uses 34.57% / 41.81%; aligned RGBA uses 30.06% / 31.57% in the same matrix.
All 32 paused screenshot comparisons and moving checked runs pass. Direct
zoom still costs about **1.36 ms/frame** more KMS-loop CPU than RGBA. Keep the
controls experimental and continue measuring the remaining stroke/whole-frame
cost before changing defaults.

The [Pi 4 repeat](gpu-renderer-prototype.md#pi-4-concrete-stroke-output) finds
a smaller isolated close-up gain: **4.2%** encoding CPU and **1.6%** completed
drawing. Its live CPU/frame changes are mixed—**0.9% better** ordinarily and
**0.7% worse** during zoom—so the Pi 5 application gain does not generalize.
Direct zoom still uses less process CPU/frame than aligned RGBA
(**14.370 versus 14.945 ms**), but submits fewer frames
(**56.95 versus 59.54 FPS**). Investigate frame scheduling, submission and
waiting, not CPU work alone.

A reusable [broader exact stroke probe](gpu-renderer-prototype.md#broader-exact-stroke-probe)
now covers frozen Clock idle/rain, Pizza 300/5000, and all 32 moving
mixed-shape poses on both devices. All nine writer/join combinations preserve
geometry; each device passes 432 image checks and 60 exported pairs exactly.
Completed Clock/Pizza differences stay below 1%; mixed-shape encoding improves
about 4–5%, with completed drawing improving 2.8% on Pi 5 and 1.0% on Pi 4.
These checks broaden confidence without establishing a universal speedup.
They exclude live physics, text/UI and scanout; preserve the full-app and
aligned RGBA controls alongside them. No defaults change.

The [Pi 4 frame-pacing traces](gpu-renderer-prototype.md#pi-4-frame-pacing-attribution)
show why average CPU is insufficient: direct zoom uses **13.997 ms CPU/draw**
versus RGBA's **14.504 ms**, but its loop wall p95 is **21.793 ms** versus
**16.706 ms**. Of 154 direct loops longer than 20 ms, 148 already exceed a
60 Hz budget in CPU work alone. The larger cost is inside the scene-drawing
callback; ordinary uploads, EGL swap and short no-draw wakeups do not explain
the gap. Detailed clocks add measurable overhead, so use the basic trace as
the attribution control and preserve untraced performance comparisons.
The bounded trace now also exposes kernel page-flip sequences/timestamps,
separate from CPU submission/observation offsets. A complete four-process zoom
repeat measures **60.004 kernel flips/s for RGBA versus 57.697 for direct**.
Of 75 direct intervals skipping a refresh, 74 follow frames that exceed the
refresh budget in CPU work alone; their previous-flip wait averages only
**0.017 ms**. These are real presentation gaps accompanying expensive frames,
not merely extra event-loop iterations. The complete repeat uses RAM-backed
capture files after two USB-backed collection attempts stall; partial matrices
are retained separately. Keep rendering and pacing defaults unchanged. Next,
align cache, tessellation and vertex-output costs to the worst zoom frames.

The [live geometry attribution](gpu-renderer-prototype.md#live-geometry-attribution-during-zoom)
now separates two costs. Basic captures reproduce **72** skipped-refresh
intervals, **62 with no cache miss**. The largest frame nevertheless spends
**21.427 ms** constructing cache entries. Detailed captures put the recurring
cost in guarded stroke tessellation/output, not ordinary drawing or cached
vertex expansion. Extra encoding clocks increase measured CPU and misses, so
keep the basic control.

The [settled admission comparison](gpu-renderer-prototype.md#settled-scale-admission-experiment)
now requires an exact scale in three consecutive presentations before promoting
it. Across 48 Pi 4 captures in eight fresh processes, moving-zoom CPU falls **1.69%** with all
four paired blocks improving. Sampled moving cache misses fall **2,246 → 0**;
the largest construction burst falls **21.590 → 0 ms**, and largest total CPU
frame **42.270 → 26.547 ms**. The stop still needs roughly **16 ms** of cache
construction, while steady-view cost is unchanged. The
[moving-image and Pi 5 follow-up](gpu-renderer-prototype.md#admission-image-coverage-and-pi-5-timing)
adds independent cold/moving/stopped cache histories against ordinary triangles.
All **9,990 image comparisons** pass the unchanged gate across the workstation,
Pi 4 and Pi 5, including resize, terrain edits and resource recreation.
The Pi 5 repeats the elimination of moving cache builds, but average CPU falls
only **0.89%** (6.567 → 6.509 ms/draw), with one of four pairs slightly worse.
Both policies stay at about 60 Hz, and stopping still requires about **9 ms**
of construction. Keep `repeated` as the default; the available evidence supports
`settled` as a targeted opt-in spike control, not a broad throughput solution.
The remaining 85 Pi 4 moving refresh gaps have no cache miss; reduce guarded-stroke
work separately. Approximately 42 ms spikes after zoom stops are dominated by
timer callbacks, not geometry encoding, and need separate attribution.

The [direct polygon input experiment](gpu-renderer-prototype.md#direct-polygon-input-experiment)
finds no useful net improvement from removing path-event dispatch. Same-build
close-up encoding gains are only **0.15% on Pi 4 / 0.53% on Pi 5**; completed
rendering is 0.20% slower / 0.04% faster. The old-executable comparison exposes
larger encoding differences than the proposed saving, including regressions.
All **2,575** new-path image comparisons and exact geometry checks pass, but
correctness alone does not justify adoption. Keep this comparison-only and
leave defaults and installed apps unchanged. Next investigate geometric
calculations and vertex emission inside guarded strokes, not more input-wrapper
optimization; preserve the independent old-binary timing control.

The [adjacent-edge calculation experiment](gpu-renderer-prototype.md#adjacent-edge-calculation-reuse)
then reuses exact edge lengths/directions within each stroke. High reuse counts
and **3,461** pixel-identical new-path comparisons establish correctness, but
not a useful cross-device gain. Close-up encoding improves **0.64% on Pi 4**,
while worsening **0.41% on Pi 5** and **1.10% on the workstation** against
same-build references. Pi 4 completed drawing improves 0.42% in that comparison
but only **0.02% against the previous executable**; Pi 5 completed drawing is
effectively unchanged. Keep the switch comparison-only and defaults unchanged.
Before another local optimization, separate the remaining join-construction
and vertex-emission costs to identify a larger target. The independent
old-executable baseline and uncapped CPU/completed-draw scopes remain required.

The [stroke construction attribution](gpu-renderer-prototype.md#stroke-construction-attribution)
now captures and replays real guarded outlines with plain, no-op, counter-only,
individual-region and all-region controls. Both devices point toward join
geometry and mixed round-join work, rather than triangle writes. Fine clocks
increase isolated replay CPU by **103% on Pi 4 / 168% on Pi 5**; use the spans
as qualitative evidence, not precise production CPU percentages. The independent
uninstrumented close-up reference is also **2.79% / 0.69% slower** than the prior
executable, while completed rendering is nearly unchanged. Keep the previous
binary and do not claim zero observer/build cost. All 1,772 new-build image
comparisons and 720 saved old/new PNG comparisons pass; installed apps and
defaults remain unchanged.

The [prepared stroke guard experiment](gpu-renderer-prototype.md#prepared-stroke-guard-experiment)
finds repeated width/tolerance parameters in about 75% of round-join guard
invocations within an outline. Preparing their bounds once reduces isolated
close-up stroke CPU **2.84% on Pi 4 / 4.51% on Pi 5**, preserving every vertex
bit, index and join decision. The savings are only **29 / 80 microseconds per
replay**, not a measured whole-frame improvement. The new ordinary-render
reference is +0.88% / +0.17% in close-up encoding CPU against the independent
previous executable; completed rendering differs by less than 1%. The isolated
result motivated the complete-render comparison below, not a default change.

The [prepared guard renderer integration](gpu-renderer-prototype.md#prepared-guard-renderer-integration)
now adds session-only `bulk-prepared` and checked controls. Against the same
build's concrete-output reference, close-up encoding CPU improves **0.94% on
Pi 4 / 2.80% on Pi 5**, with completed GL drawing effectively unchanged on Pi 4
and **1.39% faster on Pi 5**. The encoding saving is only **37 / 72 microseconds**;
the Pi 5 completed saving is about 67 microseconds. These are uncapped geometry
measurements, excluding simulation, text, UI and scanout. Ordinary-play toggles
barely affect encoding; Clock idle saves some Pi 4 encoding CPU without improving
completed drawing, and Pizza is effectively unchanged. Compiled-reference drift
still matters: late-match encoding increases 0.92% / 1.60% across builds, which
the guard does not recover. All 2,044 new-candidate image checks and 840 saved
cross-run PNG comparisons pass. Installed apps, preferences and defaults remain
unchanged. These geometry-only results motivate the live comparison below,
not a default change.

The [live prepared-guard comparison](gpu-renderer-prototype.md#prepared-guard-live-application-comparison)
now includes simulation, text/UI and all application threads. Four same-build
pairs per camera/device reduce Pi 5 CPU per rendered frame **1.26% ordinarily**
(5.5793 → 5.5088 ms) and **0.40% during zoom** (6.7135 → 6.6865 ms). Against
the independent older executable, those savings are **0.80% / 0.40%**. Pi 4
is effectively flat ordinarily (−0.19%, mixed pair results) and slightly worse
during zoom (+0.26%); zoom presentation remains about **57.4–57.6 flips/s**.
Fresh KMS draw counts normalize CPU: cached host submission counters can be
nearly a second old and are too imprecise for such small differences. All 576
accepted status windows, 48 image checks and 969 moving checked ticks pass.
One late-start Pi 4 old-build control is retained as rejected data and replaced
after the matrix; the four same-build pairs are unaffected. Keep the switch
opt-in. The next target should remove a larger amount of guarded-stroke work,
with exact rendering and full-app pacing controls, rather than another small
per-join calculation change. No application code or defaults change in this step.

The [folded stroke template audit](gpu-renderer-prototype.md#folded-stroke-template-feasibility)
tests a larger reuse opportunity before adding another cache policy. With
conservative merge/fold margins, a second folded template qualifies **none** of
the close-up outlines at either device viewport size. Only nine late-match
occurrences qualify across the two 24-frame histories. An unguarded negative
control reproduces real topology changes under 1% zoom, a rounding-sensitive
corner under translation, and incorrect rotation of a collapsed round cap.
These are workstation geometry checks, not device timings or visual-error
measurements. Keep the renderer unchanged. The next candidate needs explicit
merge/fold-state validation and cap handling, followed by image and throughput
checks; neither blindly reusing the previous mesh nor simply reducing the
safety margin is justified. Six regression/model tests and a bounded manual
audit retain the evidence for that work.

The [dynamic stroke topology prototype](gpu-renderer-prototype.md#dynamic-stroke-topology-reuse)
now recomputes current f32 positions and corner decisions while sharing only
triangle layouts. Exact geometry checks pass, including the earlier transition
counterexamples, but the performance tradeoff is mixed. Both Pis run both
viewports: isolated close-up stroke CPU improves **7.64% / 6.46% on Pi 4 / Pi 5
at 800×480**, yet regresses **18.93% / 19.52% at 1024×768**. All eight paired
comparisons agree in each case, and an independent older executable confirms
the directions. Eligibility is 68.18% versus 14.50%; refused strokes pay
classification overhead before ordinary tessellation. Each history uses just
two layouts with no warm misses, so more cache capacity is not the remedy.
Keep this benchmark-only and make rejection cheaper or reuse already-computed
geometry in the fallback before considering live integration. These are not
whole-frame gains. Both installed kiosks and their settings remain unchanged;
708 client and 201 tessellator tests pass.

The [single-pass stroke fallback](gpu-renderer-prototype.md#single-pass-stroke-fallback)
now reuses the computed corner prefix when an arc needs fresh emission. It
removes the 1024×768 regression: isolated close-up stroke CPU improves **3.93%
on Pi 4 / 1.68% on Pi 5**, while 800×480 improves **15.69% / 14.33%**. All twelve
pairs improve in every case; the independent old executable confirms the
directions. Exact vertex/index checks pass without loosening the guard or
growing the layout cache. Merged paths and fine arcs keep ordinary fallback;
coarse arc emission uses bounded scratch, not cached positions. The 800×480
saving is about 690 / 254 microseconds, not a measured whole-frame gain.
All 138,240 timed rows pass accounting checks, with 708 client and 204 vendor
tests passing. Installed apps and defaults remain unchanged. Next compare an
opt-in complete renderer with exact images and uncapped encode/completed-draw
timings, then live CPU/pacing, before considering promotion.

The [complete single-pass integration](gpu-renderer-prototype.md#single-pass-complete-geometry-integration)
now provides opt-in `bulk-single-pass` and checked writers. At native logical
viewports, Pi 5 close-up zoom improves **10.38% encoding CPU / 5.86% completed
geometry wall time**, with all eight pairs improving. Pi 4 encoding instead
regresses **1.11%** in all eight pairs, while completed drawing is unchanged.
Output remains exact and caches stay within their limits: 2,044 candidate image
comparisons and 840 saved cross-run PNG comparisons pass, alongside 712 client,
32 CLI and 204 release tessellator tests. Clock demonstrates workload sensitivity:
88 strokes use layouts on Pi 4, but require fine-arc fallback on Pi 5, adding
about 10 microseconds encoding per frame there. Pizza does not enter the path.
These devices currently use different viewports; next repeat the complete
comparison at both sizes and attribute adapter/fallback costs. Do not promote
this on the isolated-stroke or Pi 5 result alone. Installed apps, settings and
defaults remain unchanged; full-app CPU/pacing is still unmeasured for this writer.

The [fixed-build viewport crossover](gpu-renderer-prototype.md#single-pass-viewport-crossover)
shows that the earlier native-size contrast is largely workload-dependent:
at **800×480**, both Pis improve close-up encoding (**8.18% / 9.81%**) and
completed geometry wall time (**4.03% / 5.60%**). At **1024×768**, Pi 4 encoding
regresses 2.20% while completed drawing is flat; Pi 5 is effectively flat in
both scopes. Work counters match across CPUs at each viewport, and cached-layout
use changes from about 67% to 15% of close-up attempts. These sizes also change
aspect ratio/projected geometry, not just pixel count. Clock's 88 strokes switch
from all-refused at 800×480 to all-cached at 1024×768 on both devices. No larger
cache or device-name heuristic is justified.

The same executable still saves **6.00% / 4.67% isolated stroke CPU** at
1024×768, so losing the algorithmic benefit in the build is not the explanation
for the full-renderer result. Next compare the **actual adapter** against the
direct isolated path on identical captured strokes, separating accepted,
fresh-arc and refused work. Isolated replay already includes colored vertex
output and index rebasing; do not blame that conversion without evidence.
The crossover passes 4,056 exact same-state image checks and 528 saved repeat
PNG checks; 41,472 additional isolated timing rows match earlier geometry/work
counts. Small cross-GPU and shared history differences are recorded separately.
This step changes no runtime code, installed apps, settings or defaults; the
notes retain the measurement scope, raw evidence and reproduction commands.

The [actual-adapter replay](gpu-renderer-prototype.md#actual-stroke-adapter-replay)
now shows that the renderer adapter retains a close-up isolated saving at both
sizes: **14.50% / 13.45%** on Pi 4 / Pi 5 at 800×480 and **3.51% / 2.64%** at
1024×768. Its mean overhead over direct replay is about 1%, not a demonstrated
explanation for the earlier whole-renderer result. Filtered cached-layout groups
save **22–25%**; fresh arcs save only **0–2%** (mixed pairs on Pi 5), and merged
fallback costs **6% more**. Clock's fine-arc case costs about 26 / 10 µs extra
stroke CPU, mostly before the adapter wrapper. These are CPU-only group results,
not additive frame attribution or total Clock costs.

All 387,072 old/new timing rows pass accounting, accepted strokes pass over a
million checked-adapter calls including repeats, and 717 client tests pass.
Build drift and subgroup variability are documented; installed apps and defaults
stay unchanged. Next test cheaper refusal of demonstrably ineligible current
strokes, preserving exact successful cases and ordinary/fresh-arc controls, then
repeat complete rendering. Do not remove safety checks or choose a device-name
heuristic based on the isolated timings.

The [early fine-arc refusal](gpu-renderer-prototype.md#early-fine-arc-refusal)
now removes endpoint/first-corner setup only where the existing tolerance
bound already guarantees fallback. At 800×480, Clock's isolated adapter
overhead falls from **26 to 10 µs on Pi 4** and **10 to 3 µs on Pi 5**;
ordinary fallback still has a cost. Successful cases and cache eligibility
are unchanged, with small timing shifts in both directions. All 340,992
timing rows, 4,056 exact same-state GPU comparisons, 528 old/new saved images,
206 tessellator tests and 717 client tests pass. Installed apps/defaults stay
unchanged; both Clocks resume normally.

Complete rendering still favors single-pass in 800×480 close-up scenes
(**8.36% / 10.28% less encoding CPU**, **4.37% / 5.99% less completed wall
time**, Pi 4 / Pi 5), but not generally at 1024×768. Pi 4 mixed animation
still regresses, and Clock's full-render gains are small. This is a bounded
fallback improvement, not a resolution of the integration gap. Next sample
the complete 1024×768 encoder, retaining 800×480 as a positive control, rather
than extending the cache or chasing the last few fallback microseconds.
Live CPU/pacing verification remains necessary before changing any defaults.

The [complete encoder attribution](gpu-renderer-prototype.md#complete-encoder-stroke-attribution)
now separates ordinary objects from guarded terrain outlines. In instrumented
1024×768 close-up runs, the guarded-stroke stage itself is essentially tied;
extra ordinary geometry is not hiding a large saving. Both sizes add the same
66 ordinary polygon attempts, and the topology cache stays at two entries /
610 bytes without warm misses or evictions. Guarded cached-layout use instead
drops from **68% to 14%**, while scalable expansion emits nearly twice as many
vertices. This narrows the investigation, not its precise hardware cause.

Detailed clocks add **15–61%** close-up CPU depending on device/viewport. Even
the plain path in the alternating diagnostic harness understates Pi 4's 800
benefit. Independent full-render controls therefore remain authoritative: the
new build saves **6.57% / 10.18% encoding CPU** and **3.10% / 5.89% completed
wall time** at 800×480 on Pi 4 / Pi 5, with all four pairs improving. At 1024,
Pi 4 encoding is 1.80% worse and Pi 5 is tied (+0.33%). Small old/new build
shifts are recorded rather than claimed as an optimization. All 92,160 timing
rows, 4,184 same-state GPU comparisons, 480 saved old/new images and 720 client
tests pass; installed apps/defaults are unchanged and both Clocks resume.

The [selective full-render controls](gpu-renderer-prototype.md#selective-stroke-emission-in-the-full-renderer)
now independently disable layout or fresh-arc output before emission, without
new per-piece clocks. **Both contribute useful savings**: keeping only layouts
costs 8.86% / 7.91% extra encoding CPU at 1024×768 on Pi 4 / Pi 5, and keeping
only fresh arcs costs 6.08% / 4.57%. The combined path remains +1.64% / +0.32%
at that size, versus **5.09% / 8.43% less encoding CPU** and **2.05% / 5.57%
less completed-render wall time** in the denser 800×480 close-up. Early/late
gameplay does not show a substantial general benefit. Dropping an emitter is
not the fix, and two warm layouts / 610 bytes with no misses or evictions gives
no reason to grow the cache.

Classification followed by ordinary output is deliberately duplicate work, not
a direct measurement of wasted time in the combined path, which reuses its
preparation. The substitution results motivated testing **one bounded reduction
in shared preparation or scratch-buffer traversal**, retaining both
emitters, exact fallback and independent old/new controls. They do not resolve
the precise isolated-versus-integrated hardware mechanism. This diagnostic
build also has small timing shifts; no new production speedup is claimed.
All 46,080 timing rows, 8,780 same-state GPU comparisons, 1,920 saved cross-control
images, 722 client tests and 207 tessellator tests pass. Both Clocks resume with
unchanged installations/settings and no restarts. Keep defaults unchanged and
require live CPU/pacing checks before promotion.

The [same-polygon prefix experiment](gpu-renderer-prototype.md#same-polygon-prefix-reuse)
now retains already-prepared base positions when resuming fresh arcs. In the
new executable, close-up encoding CPU improves **2.14% / 1.31% on Pi 4** and
**2.50% / 1.13% on Pi 5** at 800×480 / 1024×768; completed rendering improves
0.40–1.26%, with all four pairs improving in each combination. The output and
cache decisions stay exact. It avoids regenerating about 3,048 / 1,768 positions
per close-up frame, but produces no useful early/late gameplay CPU win.

This is **not yet a net gain over the previous executable**: the new build's
original path also shifts, and prefix CPU versus the frozen previous path is
+0.56% / +0.10% on Pi 4 and −0.07% / +1.80% on Pi 5. Added mesh-result and
adapter bookkeeping and code layout are hypotheses, not a resolved cause.
That result motivated isolating diagnostic overhead from the original hot path
before adding another optimization. Keep both emitters and independent
previous-build controls. All 27,648 timing rows, 5,268 same-state GPU checks,
960 saved-image comparisons, 723 client tests and 209 tessellator tests pass; both original
Clocks resume with unchanged installations/settings. Defaults remain unchanged.

The [checked-only accounting follow-up](gpu-renderer-prototype.md#checked-only-prefix-accounting)
removes the reuse-count payload and per-polygon accumulation from timed writers,
without changing emitted geometry. Coverage moves to the untimed exact oracle;
the timed reuse column is zero/unmeasured, not a claim of zero reuse. The common
aggregate diagnostic remains cumulative, and unchecked frames do not increment
it. Frozen pre-prefix and first-prefix executables remain independent controls.

The cleanup does **not establish a dependable net prefix win**. Close-up encoding
CPU versus the pre-prefix executable is **−1.08% / +0.64% on Pi 4** and
**−0.63% / +0.12% on Pi 5** at 800/1024. Inside the cleaned build, prefix reuse
saves 0.42–1.96% CPU, but completed-render changes range from −1.08% to +0.06%
with mixed pair signs in two combinations. Original-path and bulk-static
controls still shift; these results do not establish that the counter alone
caused the earlier regression. Early/late gameplay shows no useful general improvement.

All 36,864 timing rows, 7,024 same-state GPU checks, 1,440 saved-image comparisons,
723 client tests and 209 tessellator tests pass. Both Clocks resume without
installation/settings changes or restarts. Keep the cleaner measurement design
and all policies opt-in.

The subsequent [live writer comparison](gpu-renderer-prototype.md#live-stroke-writer-comparison)
covers 36 seeded sessions on both Pis, with untraced whole-process CPU and
separate bounded pacing captures. Against bulk-static in the same executable,
prefix CPU per draw changes **−0.14% / +0.32% on Pi 4** and
**−2.54% / −2.55% on Pi 5**, normal/zoom. The extra prefix step over original
single-pass is effectively flat on Pi 4 and saves about 0.75–1.00% on Pi 5.
Keep the small Pi 5 saving scoped to this fixture, not a universal default win.

Pi 4 still averages about 57.4 completed flips/s during zoom, with approximately
33 ms p95 intervals in separate traces and occasional 50 ms gaps. Some long
loops spend 35–37 ms in host/timer CPU; others have 21–31 ms geometry encoding
wall-time. Those are distinct targets. Next use the existing work/geometry
trace controls to attribute those stalls before more stroke preparation changes.
All 48 image comparisons and 17,280 trace loops validate; both Clocks resume
with unchanged installed binaries/settings. Defaults remain unchanged.

Treat circle-only vertex/shader cost as a separate question. The offscreen
benchmark retains exact reference checks for GPU-only controls and tightly
bounded, measured subpixel-edge differences for CPU-expanded meshes, including
animation and retained-history resize checks. Do not use the extreme Pizza
result alone to choose the general rendering path. Continue full-match and
remaining Clock-effect checks. Repeat the corrected menu borders and capture
checks on other drivers. Pi 5 direct Clock and paused split-screen captures now
pass, including equivalent triangle/retained output and the return to menus.
Complex-scene Pi 5 throughput now includes uncapped capacity and short live
comparisons; physical touch verification and full-match coverage remain
outstanding. The Pi is left running normal Clock with direct
GPU triangles as a temporary session override; saved preferences and the
software default remain unchanged.

Compare three paths with the same scene, native viewport and fixed workload:

1. Current CPU scene rasterization plus software presentation: the reference.
2. CPU scene rasterization plus GPU image presentation: isolate output drawing,
   upload and synchronization costs, including text and UI. This is a milestone,
   not the final architecture.
3. GPU drawing of scene geometry/layers plus GPU presentation: use the existing
   scene representation so scenarios do not need GPU-specific code. Prototype
   Clock first, then check a terrain/HUD-heavy Spacewars frame before generalizing.

Report preparation, upload/presentation, total CPU, frame pacing and memory on
both Pi 4 Picade and rotated Pi 5 HyperPixel. A stable 60 FPS alone is insufficient
when the software reference already reaches it: the experiment must show useful
CPU headroom. Check alpha/layer order, clipping, fonts, menus, screenshots and
input/rotation against reference captures. GPU antialiasing need not be
byte-identical, but missing geometry or changed composition is not acceptable.
Retain an explicit software selection and test the failure/rollback route when
GPU initialization is unavailable. Do not change physics, event cadence or
resolution to make the comparison pass.

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
