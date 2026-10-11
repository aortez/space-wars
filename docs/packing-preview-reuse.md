# Reusing local packing previews

The [gravity integration pass](loose-gravity-integration.md) made the collapsed
cave pile quiet enough to inspect. It also exposed a large packing attempt in
the curved-planet fixture. This pass preserves placement decisions while
reducing repeated work inside each shared packing query.

## What blocks the cave pile

Frozen captures at 30 seconds include five groups per shape. All ten bounded
queries accept zero grains. Their proposed additions encounter ships, detached
roof pieces and grains excluded from that particular transaction; other
proposals fail the repose rule or cannot find nearby connected vacancies.
Quiet motion alone does not guarantee room for a whole material cell.

Removing the actor obstacles from each frozen query independently permits
1/2/3/8/0 grains in the five Round groups, and 0/1/4/0/0 in the Angular groups.
These are separate diagnostic alternatives, not cumulative live deposits.
The nine-grain Round group targeting a detached roof piece now has a regression
fixture: ship 10000 blocks the current proposals, and omitting it allows three
stable, clear deposits. This proves that the actor affects the decision; it
does not prove that the planner finds every possible safe arrangement.

The live Weak roof still deposits zero cells by 30 seconds with either shape.
The subsequent [exhaustive cave investigation](cave-packing-oracle.md) finds no
legal assignment in six complete group searches and no legal one-, two-, or
three-grain deposit in the remaining four groups. This supports retaining the
current rules for these captured scenes. Larger coordinated deposits and
transactions spanning different destination pieces remain outside that search.

## Shared implementation

`SurfaceAdditionPreview` retains each tile's added polygons across alternative
plans only when all local reconstruction inputs match. Smooth surfaces depend
on a 5-by-5 stencil: tile owners can be one cell away, and each owner reads its
3-by-3 neighborhood. Interpolated surfaces use exact distance bits; contour and
block surfaces use occupancy. Coordinates and field layout are fixed for the
query, including boundary and ghost tiles. Equal revision numbers alone cannot
identify equal alternatives. Polygon coordinates and ordering are preserved.

Packing also constructs each grain's geometric candidate list once per query.
Later attempts still recheck their forbidden cells, the smaller single-grain
reach and clearance against exactly the bodies that remain. A grain dropped
from a plan remains an obstacle. Both caches are discarded after the query;
they do not survive a physics tick or a terrain commit.

The eight-assessment budget, matching order, gravity, material properties,
quiet timers, surface clearance and repose rules are unchanged. Both games use
these shared terrain and physics modules.

## Reproduction

The destruction benchmark now accepts `--packing-dir` and `--packing-tick`.
Capture runs outside step timing and verifies unchanged observation and motion
hashes. For the group just before the curved fixture's expensive tick:

```sh
cargo +1.89.0 run --locked --release -p scenario-spacewars \
  --example destruction_benchmark -- --curved --strength weak --shape round \
  --seconds 5 --packing-tick 292 --packing-dir target/curved-packing --replay
cargo +1.89.0 run --locked --release -p scenario-scorched-earth \
  --example packing_inspector -- target/curved-packing/group-0.packing \
  --output target/curved-packing/inspection --profile-repeats 50
```

Use `--roof --seconds 30` without `--packing-tick` for the cave endpoint, and
repeat with `--shape angular`. Inspector `--omit-body 10000` removes that body
only from a restored diagnostic query. Capture does not advance quiet timers;
inspection enables recovery and bypasses retry delays, so it is not a promise
to pack on the exact captured live tick.

The two new frozen regression cases are documented in the
[fixture notes](../crates/engine-rapier/tests/fixtures/README.md).

## Verification and timings

All **838 tests** pass across engine-terrain, engine-rapier, Scorched Earth,
Space Wars and Terrain Lab. Coverage includes exact patches across overlapping
alternatives, field edges, chunk seams, and distance-only edits with equal
revisions. The curved fixture checks 56 supported, clear deposits while
retaining each grain's material and durability. Shared engine/Scorched strict
Clippy passes; existing Space Wars warnings are unchanged. The capture example
has no warnings, and combining capture with profiling preserves every tick's
span counts and the exact frozen snapshot bytes.

The complete matrix ran **88 benchmark executions / 128 scene results** across
desktop and Pi. Every before/after scene signature and every diagnostic
checkpoint matches. All corresponding desktop/Pi results and diagnostics match
too. The 17 frozen cases have byte-identical reports before/after on both hosts,
including attempted cells, blockers, accepted grains and rejection reasons.

Pi 4 whole-scenario update times, mean / p95 / maximum in milliseconds:

| Workload / shape | Before | After |
| --- | --- | --- |
| Scorched barrage / Round | 4.089 / 14.796 / 40.914 | 3.804 / 13.869 / 32.812 |
| Scorched barrage / Angular | 2.140 / 5.590 / 27.539 | 2.135 / 5.613 / 25.029 |
| Weak roof / Round | 6.242 / 9.128 / 22.056 | 6.206 / 9.318 / 23.667 |
| Weak roof / Angular | 6.782 / 9.562 / 24.920 | 6.836 / 9.607 / 23.725 |
| Weak quarry / Round | 3.998 / 5.639 / 21.906 | 3.984 / 5.788 / 20.428 |
| Weak quarry / Angular | 4.721 / 6.955 / 21.365 | 4.753 / 7.080 / 20.857 |
| Weak curved planet / Round | 5.930 / 9.230 / 31.965 | 5.957 / 9.302 / 25.682 |
| Weak curved planet / Angular | 8.013 / 15.521 / 31.864 | 8.026 / 15.795 / 28.583 |

These are single uninstrumented runs per case/version, with before/after paired
by workload. Desktop timing overlapped builds and unrelated workspace work;
Pi timing used a paused kiosk. Audits, replay, capture and rendering are outside
the scenario step timer. The 120-second barrage includes 60 seconds of firing
and 60 seconds of quiet; other matrix cases run for 30 seconds.

Three separate, alternating before/after Pi profiles isolate curved Round tick
293. Median **packing time falls from 28.30 to
19.01 ms** (33% less), while the
whole tick falls from 33.18 to 23.68 ms. Surface
preview time falls from 16.21 to 8.76 ms. All six
profile runs reproduce the same outcome and pass per-second replay. Spans are
inclusive; do not add packing to its child stages. A separate 50-repeat frozen
query, including inspection tracing, drops from 32.52 to 22.60 ms median.

The large packing tick remains above a 16.7 ms update budget. This is a reduction
in an expensive query, not a claim of consistent rendered 60 FPS. Typical
curved-case update averages change little because the large transaction is rare.
The cave's blocked placements remain blocked.

## Review builds and data

The rebuilt application is deployed to `sw-picade.local`. Installed client and
CLI hashes match the fast-update bundle; the kiosk is active with no restarts or
UI error and is left paused. The local review window is a fresh, paused seed-42
material duel with Weak/Angular dirt and heavy asteroid arrivals every second.
Its live smoke test ran at about 60 updates per second. No commit or PR has been
created for this pass.

[Measurements and build identities](data/dirt-packing-reuse-20261008.json) ·
[per-second diagnostics](data/dirt-packing-reuse-traces-20261008.jsonl.gz) ·
[profiles](data/dirt-packing-reuse-profiles-20261008.jsonl.gz) ·
[frozen inspection reports](data/dirt-packing-reuse-inspections-20261008.jsonl.gz).
Local source snapshots, executables, raw output and reproduction scripts are in
`target/dirt-cave-packing-review/`.
