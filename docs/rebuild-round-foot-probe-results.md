# Round-foot sweeps still miss a valid landing because the descent moves sideways

Using the actual foot radius identifies the failed coarse build's poor contact,
but a straight sweep still falsely rejects the valid recorded handoff. The
missing behavior starts **before contact**: the native ship drifts sideways into
a corner that neither assumed descent line reaches. Keep this predictor out of
placement decisions. No bot default, frontier choice or placement behavior
changes in this experiment.

The [frozen probe](rebuild-round-foot-probe-plan.md) casts each 0.45-unit foot from
its actual rotated spawn position along both ship-normal down and radial down.
It records first-hit points, normals, identities, distances and convergence
status. Each normal is compared with the unchanged native 0.7 threshold using
radial up at that foot's corresponding impact pose. These are independent
first contacts, not a simultaneous settled pose.

| Retained build | Minimum alignment, normal sweep | Minimum alignment, radial sweep | Actual settling | Prediction at 0.7 |
| --- | --- | --- | --- | --- |
| Live handoff, 27114 | 0.99555 | 0.99556 | 27188 | Correctly supported |
| Live coarse, 25373 | 0.33548 | 0.64399 | Never | Correctly unsupported |
| Recorded handoff, 27459 | 0.62786 | 0.63010 | 27557 | **False rejection** |

All twelve foot sweeps converge and hit retained planet geometry. Thus the
counterexample is not an inconclusive query, foreign collider or overlap case.
Both variants fail to distinguish the valid recorded landing using the native
support threshold. No threshold or trajectory was retuned after measurement.

## The valid landing reaches a different contact

At build tick 27459, the ship origin is planet-local **(40.60307, -13.18388)**.
At its first native foot contact, tick **27510**, that origin is
**(39.47820, -12.59438)**. This is **0.21328 units** off the assumed radial
descent line and **0.09561 units** off the ship-normal line. The angle relative
to the planet changes only about 0.098 degrees by this first contact. Significant
rotation follows touchdown; rotation alone cannot explain the earlier drift.

The sweeps predict the second foot hitting a sloping face with alignment about
0.63. The actual foot reaches the corner at **(33.45464, -13.0)** and already
has alignment **0.78838** on its first native contact. Its normal speed is
0.12654 and separation 0.01388, within the existing native support limits.

The ship then rotates and lowers its other foot. Native two-foot support begins
at **27543**, and the required 0.25 seconds of settling completes at **27557**.
The final contact alignments are **0.99555 / 0.77429**, at an overall landing
angle of **3.31003 degrees**. Terrain revision remains **22** throughout. The
portable bundle retains the complete build-to-settle trace and additional
contact observations, including local poses and displacement from both sweep
directions.

The round-foot query itself is useful: in the failed live coarse build, its
normal sweep finds the opposite-facing slope from the original point sample.
Its predicted alignment **0.33548** closely matches the actual **0.33790** at
25535, before the terrain changes at 25537. The other foot has valid support,
but the ship never earns two supported feet or any settled time. The radial
sweep hits a different face and predicts 0.64399. That difference further
shows why the descent direction matters.

## Recovery outcomes and verification

All preceding runtime files and all **38 archive files per control** remain
byte-identical. Existing audits were explicitly reused only after verifying
that equality. The new contact audit ran on both forks. The live handoff still
builds / settles / boards at **27114 / 27188 / 27232** and completes at 27233,
with health 60.66692 and one original ship loss. The live coarse case still
times out, scuttles its unusable replacement, and blocks at 27349 with health
100 and two losses. The recorded handoff settles but does not board before the
fixed end; the recorded coarse case builds nothing.

Source **e737911**, **1,006 input hashes**, nine changed inputs and a copied
release binary were frozen before the two prefixes and four continuations.
There were **zero simulation retries and zero audit resumes**. The probes
also verify unchanged physics snapshots and native contacts around each query.

Passed: **189 Rust tests**, **988 Python tests**, bot Clippy, formatting, the
locked Rust 1.89 release build and default-feature check. Native Clippy retains
its existing 16 diagnostic identities. Engine Clippy reports one existing
warning in `spaceling.rs`, reproduced on the preceding commit in an isolated
checkout; no new engine diagnostics were introduced. The new engine tests
cover a rounded corner missed by a center ray, rotated/translated and compound
geometry, filtering, sensors, exclusions, initial overlap, invalid inputs and
unchanged physics.

The [manifest](data/rebuild-round-foot-probe-v1.json) and
[portable bundle](data/rebuild-round-foot-probe-v1.json.gz) contain the frozen
sources, probes, audits, native contact traces, predictions, validation logs and
exporter. Verified full raw archives remain under
`target/rebuild-round-foot-probe/v1/archives/`.

The next experiment should test a short, read-only native settling forecast
against these three builds, measuring both correctness and cost. It needs to
include pre-contact lateral motion and subsequent rotation, and evaluate the
actual two-foot support and settling rules. A forecast must pass that comparison
before it can inform placement. Complete coarse-site recovery, precise-position
holding through construction and the earlier bearing-343 query mismatch remain
unresolved. This is diagnostic evidence, not a scored match or a promotion.
