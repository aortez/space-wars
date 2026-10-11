# Exact stroke experiments

This is `lyon_tessellation` 1.0.20 from the crates.io release, whose recorded
upstream revision is
[`7fa75ea8eea36255cae2abcf709900b0c7a5c309`](https://github.com/nical/lyon/tree/7fa75ea8eea36255cae2abcf709900b0c7a5c309/crates/tessellation).
The MIT and Apache licenses are copied from that same revision. Only
`src/stroke.rs`, the new `src/stroke/round_join.rs`,
`src/stroke/edge_reuse.rs`, `src/stroke/attribution.rs` and
`src/stroke/static_output_tests.rs` differ from
upstream source.
Cargo metadata and the original README are preserved; the local lockfile
makes standalone regression tests repeatable.

## Default and control

The Space-Wars GPU-default profile selects exact joins and concrete bulk output
at scenario start. Library defaults and diagnostic controls below remain
reference-based. See the GPU-default milestone in the renderer notes for the
application policy; later single-pass experiments are not release defaults.

`StrokeTessellator` and `StrokeBuilder` name the reference specialization. Its
original round-join calculation remains intact, with zero-sized policy and
counter storage. `ExactStrokeTessellator` and `CheckedStrokeTessellator` compile
separate paths from the same shared implementation. The sealed policies remove
per-join runtime mode selection without duplicating the algorithm's source.

The experimental GPU renderer selects a specialization once per uncached
stroke, using `RoundJoinMode`. The benchmark uses `--geometry-round-joins`;
live `retained-culled` sessions use `host gpu-joins reference|exact|checked`.
Cached template builders and other callers continue using the reference.
There is no environment/global switch or saved setting, and no change to
`StrokeOptions` or its serialization. Leaving the supported geometry mode or
session resets the live control to reference.

The shortcut skips angle, subdivision and arc work only when conservative
bounds guarantee zero arc subdivisions. It preserves the reference's update
to `vertex.side`. Colors, widths, caps, join style, vertex arithmetic, indices,
point merging, folding and tolerances stay unchanged.

`Checked` runs the original calculation and asserts that each proposed shortcut
has zero subdivisions. Both enabled specializations count shortcuts and
fallbacks per tessellator. These counters are cumulative, saturating, and
included in timed cost; reference has no diagnostic counters. The client sums
its two enabled instances' counts so switching modes preserves cumulative
diagnostics.

## Conservative domain

Only fixed-width, non-folded joins with a finite radius in 0.001–1024 and finite
center coordinates within ±16384 qualify. Normals must have squared lengths in
one quarter to four times the squared radius. Tolerance must be finite and at
least half the radius, making the original flattening step at least 120°.

The dot-product bound limits the angle to less than 98°. The oriented cross
product selects the minor arc, with a margin excluding near-collinear and
branch-cut ambiguity. This intentionally leaves a wide angular safety margin.
Variable-width strokes, folds, degeneracy and boundary cases take the original
path; this is not a general geometry simplifier.

## Verification

Run the upstream suite plus local regression tests with:

```sh
cargo test --locked --manifest-path vendor/lyon_tessellation/Cargo.toml \
  --target-dir target/vendor-lyon --release --lib
```

Local tests compare all vertex bits, indices, side/source metadata and custom
attributes across reference/exact/checked paths. Coverage includes concave,
folded, repeated-point and self-crossing polygons, curves, caps, translations,
rotations, variable widths, and a numerical angle/radius/tolerance sweep.

## Concrete output dispatch

`tessellate_static` is an opt-in fixed-width path-event entrypoint which retains
the concrete `StrokeGeometryBuilder` type. `StrokeBuilderImpl` and its output
helpers are generic over that type; the geometry algorithm is shared with the
ordinary `tessellate` entrypoint. Existing APIs still use trait-object output,
including attributed paths and direct builders. No arithmetic, point merging,
join/cap handling, output order or error transaction semantics change.

The client selects this entrypoint for polygon and line strokes with
`host gpu-stroke-writer bulk-static`; circles and retained templates keep their
existing path. `bulk` remains the dynamic-dispatch control with the same writer.
The probe uses `--geometry-stroke-static` with `--geometry-stroke-bulk` and
`--geometry-round-joins`. All controls remain session-only, default to stock,
and reset on the same unsupported-mode, session and failure boundaries.

The difficult-polygon tests compare dynamic and concrete output for all three
join policies. Additional tests cover open/closed and empty subpaths, quadratic
and cubic curves, all cap/join styles, and simulated writer failures at the
first, middle and last vertex. They compare every vertex field, triangle index,
join counter and begin/end/abort call, preserving existing buffer prefixes
after failure and allowing reuse after an abort. The client oracle checks all
all six writers by reference/exact/checked combinations outside
timed samples. This changes compiler specialization, not geometry quality.

## Direct polygon input

`tessellate_polygon_static` feeds a polygon's endpoints directly into the same
fixed-width builder. It avoids creating and matching path events, but retains
the begin/line/end sequence, endpoint IDs, per-endpoint error checks and output
transaction. Empty, single-point, merged, open and closed polygons use the same
cap and join logic. No shape-specific shortcut or stroke approximation is used.

The client selects it with `host gpu-stroke-writer bulk-polygon`; circles,
curves and retained templates keep their previous path. The benchmark flag
`--geometry-stroke-polygon` requires the explicit static/bulk/round-join flags.
It compares `bulk-static` against `bulk-polygon`, both with exact joins, so only
the polygon input route differs. Existing benchmark combinations are unchanged.
All controls are session-only and the default writer remains `stock`.

The difficult-path corpus compares all vertex fields, indices and join counts
through the new entrypoint. Additional tests cover every cap/join style on
empty, merged, open and closed inputs, plus identical begin/end/abort and vertex
failure order. The client oracle checks all eighteen writer/join combinations
outside timing, alongside exact GPU image comparisons through zoom and edits.

## Adjacent edge reuse

`tessellate_static_edges` tests carrying an outgoing edge's length and normalized
direction forward to the next join. It holds one edge for the duration of a
stroke, with a key containing all four endpoint coordinate bit patterns. Merged
closing points, flattened curves and changed subpaths therefore cannot reuse
an edge with different endpoints. The first accepted edge seeds the cache with
the calculation already used to construct its side points. There is no
cross-frame or cross-scale reuse and no additional allocation.

`CHECKED=true` recalculates every hit and asserts exact length/direction bits.
Both enabled variants count hits and misses; that bookkeeping is included in
timing. The ordinary builder uses a zero-sized policy retaining the original
arithmetic, without per-join cache storage or mode selection. All later join,
folding, cap and vertex-generation code is shared. Variable-width and
attributed APIs keep their ordinary route.

The client exposes `bulk-edges` and validation-only `bulk-edges-checked`. Both
use path-event input and concrete bulk output; circles and retained templates
are unchanged. `--geometry-stroke-edges` requires the explicit static/bulk/join
flags and conflicts with `--geometry-stroke-polygon`. It compares `bulk-static`
against `bulk-edges`, with exact joins in both, isolating edge reuse from the
previous input experiment. Defaults and saved settings are unchanged.

The polygon corpus and curve/cap/subpath/error-rollback tests compare both edge
variants against the original, including every vertex field and output
transaction. The GPU probe's eighteen-way oracle additionally compares counts,
cache decisions, batches and exact vertex bits outside timed samples. Per-frame
reuse diagnostics are emitted outside timing, and the Spacewars replay rejects
a run that never exercises reuse. Circle-only fixtures can legitimately report
zero reuse. See the renderer investigation for measurements and adoption status.

## Fixed width stroke attribution

`tessellate_static_profiled` accepts a concrete `StrokeProfiler` policy for
diagnostic replays. It wraps five nonoverlapping regions of fixed-width steps:
join side geometry, base-vertex construction/output, edge triangles, mixed join
extras, and flattened curve steps. It does not instrument setup, point merging,
path decoding, caps, or all closing bookkeeping. Base vertices include normal
construction; join extras include both round-join calculations and output.
Neither is a pure memory-write measurement.

The ordinary entrypoints use the zero-sized `NoStrokeProfile` policy, whose
inline closure simply executes the original work. There are no clocks,
counters, allocations, or runtime profiling selections in that policy. The
optional policy is independent of edge reuse; the diagnostic entrypoint keeps
ordinary edge calculations and the selected join policy. The existing corpus
compares every output field, source, index and begin/end/abort operation, and
checks completed hooks even when vertex output fails.

The client `--geometry-stroke-attribution` probe records actual surviving
guarded polygons from a warmed moving history and replays them with concrete
bulk output and exact joins. Plain, explicit no-op, counter-only, one-stage and
all-stage controls expose the observer cost. Empty-hook calibration is only a
timer-floor diagnostic, not an exact subtractable correction. See the renderer
investigation for bounded capture, device results and interpretation limits.
No new live renderer mode or saved preference is added.

## Prepared fixed width guards

`tessellate_static_invariants` is a comparison-only path-event entrypoint. It
prepares the fixed-width round-join eligibility and squared-radius bounds once
per stroke, using the original arithmetic. Join positions, folds, normals,
dot/cross products, angle calculations and arc subdivision remain per join.
In particular, it does not cache the flattening step or alter tolerances.
State lives only in the builder; widths and tolerances are freshly prepared
for each stroke, including after an output failure. There is no allocation or
cross-frame reuse. The ordinary builder uses a zero-sized guard policy calling
the unchanged original predicate.

`CHECKED=true` verifies the fixed width and compares every prepared decision
against that original predicate. The difficult-polygon and curve/cap/subpath
corpus compares both variants' complete output and failure transactions; the
angle/radius/tolerance sweep and numerical rejection tests compare decisions.
The client captures actual guarded strokes, checks their vertex bits, index
order and join counters in all ten replay variants, then compares plain,
prepared and checked replay without region clocks using
`--geometry-stroke-attribution --geometry-stroke-invariants`. A per-outline
histogram records opportunities to reuse the parameters, outside all timings.
The client also exposes session-only `bulk-prepared` and
`bulk-prepared-checked` writers. These use the same entrypoint for uncached
polygon/line strokes; circles and retained templates remain unchanged.
`--geometry-stroke-prepared` compares full geometry encoding and completed GL
drawing against `bulk-static`, with exact joins in both controls. The checked
variant stays outside timing. Neither selection changes the default or saved
preferences; see the renderer investigation for measurements and rollout limits.

## Device measurements

The client probe additionally compares every rendered pass, cache decisions,
CPU/GPU geometry counts and exact GPU pixels through moving scenes, cache
warmup, zoom, resizing and terrain edits. Use `--geometry-round-joins` with
`--benchmark-presentation --presentation-geometry --geometry-stroke-profile`.
See [the renderer investigation](../../docs/gpu-renderer-prototype.md) for
device measurements and rollout status.

The first runtime-switch design added reference-mode encoding overhead despite
unchanged output. Compile-time specialization removes most of that cost. The
latest paired Pi 5 comparison measures a net close-up gain over the original
binary (8.6% encoding CPU, 6.4% completed wall time), but retains a small
reference overhead and a 1.2–1.3% completed-wall regression in ordinary views.
The subsequent matched live comparison finds a smaller whole-application
benefit against the original: 1.8% less process CPU in ordinary views and 2.2%
in a zoom sweep (1.2% and 2.4% less CPU/frame). The new reference still carries
a small penalty, and aligned RGBA remains cheaper for both live cases. Keep
this session-only and experimental, not a universal default. The renderer
investigation records the separate live and offscreen results and exact-output
checks; do not transfer percentages between those workloads.

The later concrete-output comparison measures a further 10.9% close-up
encoding-CPU and 6.2% completed-wall reduction against the same-build dynamic
bulk writer with exact joins. In matched live ordinary/zoom sessions it saves
about 3.1% process CPU; RGBA remains cheaper on this Pi 5. All nine writer/join
combinations preserve exact geometry. Keep the policy opt-in pending Pi 4 and
broader-scene checks; the renderer investigation records the full controls,
old-executable baseline, images, lifecycle checks and artifacts.

The standalone `--no-default-features` check currently stops in unmodified
`lyon_geom`/`euclid` because the `Trig` API is feature-gated. The application's
standard-library configuration is tested; this patch does not claim a repaired
no-std dependency configuration.
