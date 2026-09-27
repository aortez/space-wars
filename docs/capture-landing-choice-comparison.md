# Comparing a historical reference with the native landing choice

The [acquisition replay](capture-site-acquisition-probe.md) found eight different
landing sites among 20 neutral alternatives. Every reference site still existed
in the arrival survey. This slice explains the difference using the controller's
actual ranking rules. It does not turn scores into seconds or admit a new cost
into the playing planner.

## Contract

Native selection and the optional diagnostic share one ranker, preserving site
order, preferred/opposite direction order, gate precedence, first-minimum ties
and f32 arithmetic. The ordinary call retains no assessment ledger. The diagnostic
is read-only and runs only after a witnessed fresh `selected_site` update in the
current capture attempt. It requires agreement with the full winning site,
direction, solar plan and native rejection counters. Retained, replaced, stale,
non-finite or unreproducible choices return an explicit unknown.

Bound the comparison before work: at most 64 unique local sites, two directions
per site, 64 cover entries, 128 previous failures and 64 solar cooldown entries;
flag-route surveys retain their native eight-site bound. These diagnostic caps do
not change native admission or controller budgets. Solar forecasts use the
existing bounded arc and parking/departure samples. The host invokes the same
ranker with the exact immutable observation supplied to that tick's intent and
dense trace. Winner agreement alone is **not** an observation-identity certificate.

Each row records its site/revision and input order, direction/order, solar plan,
first rejection or approach/cover/ground/total score. Site-level rejection precedes
directions; skipped opposite directions are not invented. Cover is a preference
penalty, not a safety rejection. Compare the historical reference's best eligible
direction with the winner: same site, equal-score input-order tie, higher score,
all reference directions rejected, or absent reference. Missing comparisons
remain unknown. Historical evidence retains its source age and material/owner/flag
identity, even when the native site matches.

The example's optional `--probe-landing-reference-bearing` requires the existing
acquisition continuation. It never forces a site. It adds a terminal comparison
to the probe report, with its wall time separate from planner fuel and zero
physical queries. With the option absent, the existing report contract is unchanged.

## Frozen study plan

Keep all 20 source-numeric neutral alternatives from the prior 62-candidate corpus
and the complete denominator (21 current, 20 numeric alternatives, 21 unknown
alternatives). Run the same nominations, seats, source ticks, destinations,
`defer_new` transfer policy, ordinary post-handoff controls and 30-second censor.
Add only the historical reference bearing to request the diagnostic. Retain every
unknown, interruption and censor. Compare **the full trajectory through the first
choice** with the archived prior continuation, including both players' controller
and observation bytes, physics, transfer reports, sensors and upstream work.
Sensor-profile comparison retains actor/tick, query counters and per-stage call
counts, excluding only wall-time fields ending in `_ms`.

Because the native ranker is refactored, also repeat all 13 ordinary source-local
comparison runs, using their previous exact-source audits, 64-unit observational
allowance and unchanged four-unit playing allowance. No diagnostic is enabled in
those ordinary runs. Freeze runtime, runner, plan and tolerances before physics;
record executable/source/summary/file hashes. No deployment is part of this slice.

Independently rebuild every neutral direction's angle, approach score, cover
penalty and sampled solar geometry from its raw observation. Reconcile row order,
native counters, selected direction and classification. Elementary arithmetic is
rounded to f32. Allow 0.002 controller units for reconstructed approach scores and
0.01 world units for solar clearances because transcendental implementations can
differ. A clearance within 0.01 of zero is explicitly unresolved by this independent
sign check; tolerance never establishes safety. Native reported scores still
determine exact tie order, and the selected full solar plan must match telemetry.
Rebuild both departure-corridor clearances as well: verify the preferred departure
side unless their difference is at most 0.02, in which case record the direction
as independently unresolved. This numerical direction tie does not itself make
the safety sign uncertain.
Enemy-flag route guards are unit-tested but not physically sampled by this neutral
corpus. These correlated trials are neither a strength test nor a cost calibration.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-landing-choices.py \
  --reference target/capture-flag-survey/acquisition-probe-v1-rerun \
  --source-reference target/capture-flag-survey/local-composition-v1 \
  --out target/capture-flag-survey/landing-choice-v1
```

The acquisition summary SHA-256 is
`ba5db63795995442a17088eeda800957c931cd8d5b20d063c68b0a70fb3f78c5`;
the source-local summary is
`ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`.
The archived acquisition executable is retained as
`target/capture-flag-survey/surface-mission-soak-334a498`, SHA-256
`e99cc0a530f101d39b22ce8abbae28cd5b93bb0fda6a065a2487a12758bb3827`.
