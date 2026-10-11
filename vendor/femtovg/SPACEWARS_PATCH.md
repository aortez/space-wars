# Experimental CPU glyph cache for Space Wars

This is the pinned FemtoVG 0.17.0 library from crates.io, with an optional
CPU-generated glyph-mask cache for controlled live Slint comparisons. The
upstream vector path remains the library default; the Space-Wars Pi application
explicitly selects `CpuShapeFaces` at startup for its supported GPU profile.
This is not an upstream feature or
a change to font selection, shaping output, UI layout, or scene geometry.

The original crate checksum is
`deda049f5a6dc3152b23263f72c575847f6aa0ace0045299d151f36af7aca136`.
The upstream MIT and Apache licenses are retained. `Cargo.toml.upstream` is
the original normalized manifest; `Cargo.toml.orig` is the original source
manifest. Demo targets, external test targets and dev dependencies are omitted
from the working manifest because this copy only builds the library and its
embedded unit tests. Their assets and external tests are not vendored.

## Local changes

- `Cargo.toml` adds the optional `cpu-glyph-cache` feature and pinned
  `ab_glyph_rasterizer` 0.1.10 dependency.
- `src/text/cpu.rs` implements rasterization, atlas storage, policy requests,
  diagnostics and lifecycle tests.
- `src/text.rs` declares that feature-gated module. `src/text/shaping.rs`
  supplies per-run borrowed faces to the unchanged word-shaping operation;
  reuse is enabled only by the `CpuShapeFaces` experiment.
- `src/lib.rs` exposes the experimental API, selects the cache in `draw_text`,
  and marks completed frames in both flush methods.
- The independent `render-diagnostics` feature adds coarse, thread-local
  begin/end hooks in `src/diagnostics.rs`, `src/lib.rs`, `src/text.rs` and
  `src/text/cpu.rs`. No observer is installed by default.
- `src/renderer/opengl/driver_workarounds.rs` and the fragment shader add the
  version-gated V3D stencil correction described below.

## V3D stencil compatibility

Mesa 25.1.6 on the Pi 4 incorrectly updates stencil for discarded fragments,
which removes curved parts of thin Slint borders. This matches the V3D
early-depth/stencil correction in the
[Mesa 25.2.3 release notes](https://docs.mesa3d.org/relnotes/25.2.3.html).
For renderer strings beginning with `V3D ` and Mesa versions at least 24.2.7
but below 25.2.3, fragment shaders write the unchanged `gl_FragCoord.z` through
`GL_EXT_frag_depth`. This forces late fragment tests while preserving stencil
overlap suppression. Depth testing remains disabled. The extension is required
on that path; missing support is an initialization error, not silent corruption.

Other drivers and newer Mesa keep upstream shader behavior. Driver strings are
queried during shader creation, not per frame. This is a compatibility workaround,
not a rendering optimization or a replacement for updating Mesa. The client
`--benchmark-presentation --presentation-gpu-ui` probe covers thin/thick rounded
borders, fractional coordinates, clipping, alpha and self-intersection. It fails
on the old Pi build and passes with the correction. See the renderer investigation
for device results and paired text timings.

## CPU glyph policies

`request_glyph_cache(Vector | Cpu | CpuRunFaces | CpuShapeFaces)` is thread-local and requests a fresh cache,
even when repeating the current policy. Each canvas applies requests before
its first text draw after a flush, never while its earlier glyph commands are
queued. Old textures are deleted only at that boundary. CPU allocations and
residency counters are also released when the canvas is dropped.

The CPU path handles ordinary outline fill text at scaled sizes 1 through 92.
It retains at most four 512 by 512 RGBA pages and 4096 entries per canvas:
up to 4 MiB of CPU pixels plus 4 MiB of GPU texture storage, excluding map
metadata, temporary rasterization, and driver overhead. Capacity pressure
uses fallback for the remainder of the frame and recycles at the next safe
boundary. Uploads are batched once per dirty page per text run, before queuing
its quads, preserving synchronous error handling.

`Cpu` is the comparison path: it parses a Rustybuzz face for each missing
glyph. `CpuRunFaces` produces the same masks, packing and uploads, but parses
each needed font once per text run. These faces borrow the existing font
bytes under an immutable text-context borrow and are dropped with that run.
An all-hit run does not parse faces or allocate the temporary map. There is
no persistent font-data copy, cross-run face cache or upstream shaping change.
The `face_parses` diagnostic counts only CPU-mask-generation parses, not
parsing elsewhere in the text stack.

`CpuShapeFaces` adds borrowed RustyBuzz-face reuse across uncached words within
one `shape_run`. It parses lazily and retains at most eight faces in that run;
additional fallback fonts use the original per-attempt construction. The list
is dropped on return and holds no font-data copies or persistent references.
Paint-font order, registered-font fallback, retries and final missing-glyph
fallback are preserved. Shaped-word/run cache keys, sizes and invalidation are
unchanged. Shaping uses the requested policy immediately; the separate GPU
atlas reset still waits for the safe frame boundary described above. Switching
policy does not clear existing shaping caches, so compare fresh text contexts
or processes when measuring this change.

Stroke, bitmap, variable-font, negative-position and other unsupported runs
use upstream rendering. Atlas fallback is frame-local in CPU mode, so it
does not accumulate an additional persistent atlas; its within-frame memory
and work are not covered by the CPU-page limit. Upstream direct rendering for
large outline text is unchanged. A CPU allocation/upload error latches that
cache into fallback until an explicit policy reset; the error is logged and
counted, not silently retried each frame.

CPU and vector antialiasing are deliberately not pixel-identical. Comparison
captures, placement checks and performance measurements belong in
`docs/gpu-renderer-prototype.md`. The library still requires an explicit request;
application rollout and remaining font/device coverage are documented there.

## Optional render diagnostics

`diagnostics::set_observer` installs a function pointer on the current thread.
The library does not collect clocks, allocate capture storage, record text
contents, or log. Without an observer, each instrumented operation just checks
the thread-local pointer. Existing scopes send their matching end notification
to the observer they began with, including early-return error paths.

The ten coarse stages cover text measurement, line breaking, shaping API calls,
shaping-run cache misses, text drawing, CPU mask construction, mask uploads,
glyph quads, path fills, and path strokes. They are inclusive and can nest:
shaping is part of measurement/breaking/drawing, and mask work is part of text
drawing. They are not ten independent costs to add together. Mask construction
includes cache lookup, rasterization, packing, page allocation and quad creation;
it is not a pure rasterizer measurement. Text drawing starts after deferred
cache-policy reset handling. Font selection and Slint layout bookkeeping are
not wholly covered, and shaping-run misses can still hit the word cache.

The client probe's `--gpu-text-detail` and the KMS backend's bounded
`host render-trace start-text` explicitly install observers with fixed-size
CPU/wall counters. Ordinary rendering and `host render-trace start` do not
enable these extra clocks. There are no per-glyph or per-vertex timers, and
live captures do not enable word-level timers. Neither API measures GPU execution. Compare with detail disabled to
quantify instrumentation overhead before using these spans to choose changes.

Three additional stages require the separate `set_word_detail(true)` opt-in:
face construction, RustyBuzz buffer setup/shaping, and glyph metrics/outline
discovery plus result assembly. The latter two run once per uncached word
attempt, not per glyph. Face construction can be shared across words in the
new mode. Only the client's CPU-only `--gpu-text-shaping --gpu-text-detail`
fixture enables these stages; the live KMS collector ignores them even if
another caller enables them. The fixture uses fresh contexts and explicit
font files, with font registration and output validation outside its timers.

## Tests

From the workspace, select a workspace package to enable the patched feature:

```sh
cargo test --locked --offline -p femtovg -p engine-client \
  --no-default-features --features engine-client/pi-kiosk-gpu --lib
```

The CPU-cache tests cover warm reuse, bounded pages and entries, safe mode
switches, explicit resets, frame-local fallback, injected upload failure,
texture validity at submission, and canvas destruction. Run-face tests also
check multiple fonts in one run, exact CPU-policy pixels, cold/hit/new-run
parse counts, and switching between CPU policies with queued draws. They use
the existing DejaVu Sans and Serif fixtures under `scenarios/clock/fonts`;
no system font or GPU is required. The upstream embedded tests remain enabled
as well.

Shaping tests compare exact metrics and line boundaries across policies,
font selection, missing glyphs, widths, sizes, combining marks and mixed
left/right-to-left text. They check fallback order against the upstream lookup,
the eight-face limit with overflow fonts, no-font handling, cold/warm parse
counts and new-run lifetime. CPU mask pixels remain identical in the new mode.

Diagnostic tests cover disabled observers, balanced nested/error scopes,
observer removal during a scope and complete stage indexing. Client/backend
tests cover opt-in parsing, nested accounting, capture cancellation, automatic
shutoff at 240 loops, and keeping missing CPU samples distinct from zero.
