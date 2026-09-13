# Material-match raster cost on Picade

Follow-up: [native-resolution results and the remaining performance work](render-performance-roadmap.md).

This follows [the retained-text fix](raster-text-reuse.md). It adds measurements
and a repeatable frozen-scene probe; gameplay, raster scale and drawing algorithms
are unchanged. The target is the Pi 4 `sw-picade.local`, at 1024×768 output.

## What 2× actually means

The kiosk service launches with `--raster-scale 2.0`; the saved launcher setting
also has scale 2. This is a historical kiosk setting, independent of rounded
terrain. The general client fallback is scale 1.

Scale multiplies **both** image dimensions:

| Scale | Game image | Pixels | RGB buffer | Three reusable buffers |
| --- | --- | ---: | ---: | ---: |
| 1× | 1024×768 | 786,432 | 2.25 MiB | 6.75 MiB |
| 2× | 2048×1536 | 3,145,728 | 9 MiB | 27 MiB |

The physical display and its XRGB8888 output buffer remain 1024×768. HUD text
is drawn separately at output resolution. Four times the game-image pixels
therefore does not imply four times the whole frame time.

**The current Pi software path does not average the extra samples.** Its
specialized RGB blitter reads alternate source rows and columns at 2×, matching
Slint's generic nearest-neighbor sampler exactly. This is not conventional
supersampling anti-aliasing: three of each four generated source pixels are
discarded. See `renderer/sw/buffer/texture.rs` in the vendored LinuxKMS backend
and `software_renderer/draw_functions.rs` in the vendored core. These conclusions
concern the Pi software path, not the desktop GPU path.

Geometric projection scales with the image dimensions, but the custom raster
path's stroke widths are specified in image pixels. A one-pixel outline at 2×
can consequently lose samples at display size. The exported 1×/2× PNGs show
this in minimap outlines. Text retains its output size at both scales.

## Paired Pi results, 2026-09-12

The normal adapter uses seed 42, default settings and two rule bots. Seed 42
generates **three planets**: this is ordinary Spacewars, not the two-planet
Round lab. Snapshots freeze at ticks 120, 1800 and 3600. Pi labels confirm both
ships flying at 2 and 30 seconds, then one ship and a surviving pod at 60 seconds.

| Frozen Pi scene | Raster 1× | Raster 2× | Time saved by 1× |
| --- | ---: | ---: | ---: |
| 2 seconds | 6.54 ms | 16.29 ms | 9.75 ms |
| 30 seconds | 6.81 ms | 16.80 ms | 10.00 ms |
| 60 seconds | 7.23 ms | 17.88 ms | 10.64 ms |

These are rasterization wall times, **not complete-frame FPS**. The 2× image
takes approximately 2.4–2.5 times as long to rasterize in these fixtures.
Scene construction/destruction remains roughly 5.7–6.5 ms in the detailed Pi
run. Slint's separate generic-RAM draw is about 9.7–10.2 ms, mostly unchanged
between scales. Its frozen text is cheaper than a changing live HUD.

Breakdown of the 2-second scene, per rasterized frame:

| Work | 1× | 2× |
| --- | ---: | ---: |
| Clear RGB image | 0.71 ms | 2.84 ms |
| Visible material terrain, including culling | 3.59 ms | 5.39 ms |
| HUD backing and energy bars | 1.37 ms | 5.18 ms |
| Draw minimaps | 0.17 ms | 0.50 ms |
| Composite minimaps with opacity | 0.56 ms | 2.09 ms |
| Sun/corona in player views | 0.05 ms | 0.14 ms |

Selected rows omit small stages. The HUD's four translucent rectangles cover
about 44% of the player-view pixels, regardless of scene complexity. That is
around 1.38 million source pixels per 2× frame. Clearing, HUD blending and
minimap compositing account for much of the scale penalty.

Diagnostic removal at 2× provides an independent check:

| Fixture | 2-second scene | 30-second scene | 60-second scene |
| --- | ---: | ---: | ---: |
| Complete raster | 16.29 ms | 16.80 ms | 17.88 ms |
| Without HUD background rectangles | 11.07 ms | 11.43 ms | 12.71 ms |
| Without terrain strokes | 15.20 ms | 15.53 ms | 16.30 ms |
| Without terrain fills | 14.14 ms | 14.02 ms | 14.83 ms |
| Without terrain polygons | 10.77 ms | 10.92 ms | 11.07 ms |

Removing the HUD backing saves approximately 5.2 ms. Removing just terrain
strokes saves approximately 1.1–1.6 ms; they are not the entire problem. The
no-corona fixture is within cross-variant noise of the baseline in these views.
This does not establish the cost of a camera filled by the sun.

Both player views submit 19,926 terrain polygons combined in all three Pi
fixtures; only 2,376–3,013 pass visibility culling at 2×. Roughly 85–88% of that
submitted terrain is outside the views. Rasterization already culls it, but the
scene builder has first generated/transformed its vertices. Earlier chunk/body
culling is a separate promising target for scene construction.

## Live cabinet validation

The diagnostic build was fast-deployed without rebooting. Installed and running
SHA256 both match `6dd603c1ff8727b148e81eae0634c77878bd2b55f4c161df5708b294bca609af`.
The kiosk is active with zero crash restarts; its saved settings are byte-for-byte
unchanged, including raster 2× and automatic two-bot matches.

Ten status samples span 47.98 seconds of one normal match, all with 120-frame
host timing windows. Counter differences give **21.01 FPS / 60.07 updates/s**.
The rolling timing windows overlap slightly; these are descriptive means, not
independent trials or a matched FPS comparison with another build.

| Live stage at 2× | Mean time |
| --- | ---: |
| Simulation/control per displayed frame | 2.58 ms |
| Construct drawing commands | 5.30 ms |
| Full host preparation | 20.05 ms |
| Nested raster buffer clear | 2.85 ms |
| Nested terrain culling/drawing | 5.58 ms |
| Nested HUD backing/energy bars | 5.24 ms |
| Nested minimap drawing | 0.59 ms |
| Nested minimap opacity composite | 1.93 ms |
| Full KMS/Slint presentation | 19.75 ms |
| Nested text drawing within KMS | 8.04 ms |

The disjoint raster stages total 16.50 ms. A further **3.55 ms of host
preparation remains unseparated**: the enclosing code also prepares/publishes
text, updates window properties and disposes the generated frames. Buffer
ownership preparation itself is about 0.001 ms; repeated large buffer allocation
or cloning is not the measured problem in this steady-state sample. Nested
rows above are included in their parent and must not be summed twice.

The next experiment should be a native 1× playtest, with attention to thin
outlines, ship detail and readability. The frozen Pi evidence supports roughly
10 ms less raster work, but does not predict exact live FPS. Then consider
preserving the HUD's appearance while reducing its blending work, and culling
terrain before generating per-player polygons. The live 8 ms text draw and
3.55 ms preparation residual remain separate targets to investigate. These
measurements do not justify removing terrain strokes or the corona wholesale.

## Instrumentation and limits

The old raster sub-timers described `SpacewarsLocalPlay`, the legacy layout.
Material matches use `PlayerViewsWithMinimaps`, whose player drawing previously
appeared only in `other_frames`. That path now uses the same layer timer while
preserving uncached drawing. Live status includes bounded 120-frame
`host_raster_*` averages, p95 and maxima. Buffer preparation is measured too.

Names inherited from the legacy layout need interpretation. In material player
views, `player_sun_planets` is layer -10, the material planet and detached-fragment
polygons. The material sun and corona use layers -21/-22 and have a separate
`player_sun_corona` timer. `player_hud` covers layers 15/20: translucent panels
and energy bars; raster text primitives are ignored here. HUD and sun/corona
are **nested in** `player_other`, itself nested in `player_views`. They must not
be added twice. Material minimaps are uncached, with drawing and opacity blit
measured separately from player views.

The probe freezes the actual frames/cameras before rasterization. Each fixture
runs three paired blocks per scale, with 10 warm-up and 30 measured frames per
block; scale order alternates. Terrain/HUD/corona removals deliberately change
pixels and exist only in the diagnostic. Their differences estimate avoided
work, not additive self-times or production improvements. Variant order is
fixed, so small differences between variants can include temperature/cache
effects. Temperatures were 68.2–71.1 °C; 65 of 66 sampled CPU frequencies were
1.5 GHz and one was 1.4 GHz. The governor was not fixed.

Scene construction **and destruction** are timed separately from rasterization.
The latter uses the production renderer and triple-buffer ownership, keeping
the preceding image alive until publication. Model/image publication and Slint
draws have separate timers. Slint uses its generic sampler in ordinary RAM,
without KMS scanout or pacing; the probe opens no input/audio devices or saved
settings. It omits the real host's FPS/game-over panels. Final buffers match
across repeated blocks at each scale/variant.

A separate Pi process repeats the 2-second complete scene without detailed
Slint timestamps: raster 6.61/16.60 ms at 1×/2×, Slint draw 9.72/10.16 ms. Its
pixels match the detailed run. Raster layer clocks remain enabled. This rules
out the nested Slint clocks as the source of the large raster-scale difference.
Desktop final raster measurements are 1.26/3.05, 1.49/3.63 and 0.60/1.95 ms.
The bot simulations diverge across architectures after the common opening;
only **within-platform, same-frame** scale comparisons are controlled.

The three binary probe tests, 23 raster tests and two host-profile tests pass.
These include pixel equivalence between the newly timed and original uncached
player paths, and scale/variant checksum equivalence with Slint timestamps on
and off. Desktop release and Yocto kiosk builds, formatting and diff checks pass.

## Reproduction and evidence

```sh
cargo +1.89.0 build --locked --release -p engine-client
target/release/engine-client --benchmark-presentation --presentation-raster \
  --presentation-match-ticks 120,1800,3600 --presentation-raster-ablation \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 30 --presentation-repeats 3 --presentation-detail \
  --presentation-output /tmp/spacewars-raster-new-run
```

Use a fresh output directory; existing artifact files are not overwritten.
Omit `--presentation-detail` for the Slint timestamp-overhead control, or
`--presentation-raster-ablation` to measure only the complete scene. A match
that ends before a requested tick stops advancing; its saved clock/labels
identify the actual state. Default requested ticks include 7200, whereas the
reported final runs explicitly use 120,1800,3600.

Frozen frames (TOML), per-view/layer primitive counts, PNGs, paired CSVs,
temperature/frequency history, exact binaries, source patch, tests and deployment
logs are archived outside Git at:

`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/raster-profile/`

The preliminary `desktop-detail.csv` predates the HUD/corona timers and includes
an ended-match fixture at requested tick 7200. Use `desktop-final.csv` and the
Pi CSVs for the final experiment. `analyze.py` recreates `summary.json`.
The archived Pi runner pauses and suspends the kiosk with a resume trap, runs
a separate executable, resumes the game and verifies transferred results.

Validation:

```sh
cargo +1.89.0 test --locked --release -p engine-client --bin engine-client raster::tests
cargo +1.89.0 test --locked --release -p engine-client --bin engine-client host::profiling::tests
cargo +1.89.0 test --locked --release -p engine-client --test presentation_probe
cargo +1.89.0 fmt --all -- --check
git diff --check
```
