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

## Results: 26 September 2026

Runtime, runner, tolerances and plan were frozen at `2796bb2`, with a clean
worktree. All **33 runs pass**: 20 acquisition comparisons and 13 ordinary
regressions. There were no runtime/plan adjustments after an outcome. The
executable SHA-256 is
`c52acdf36943c6a36c16615efa2992783a12171984d70d452e4750bd53515000`.
The raw main summary is
`target/capture-flag-survey/landing-choice-v1/summary.json`, SHA-256
`3af41999d1a280bef54838ace5be018cd505ef5607adf414428c548e06a73e60`.
The ordinary summary is in its `ordinary/` subdirectory, SHA-256
`3d8eefb818859405daa23ab31daccb87c266c4bebe761cd0629ab6de514d6a9b`.
[The tracked record](data/capture-landing-choice-v1.json) retains the full plan,
audited comparisons, selected/reference assessments and raw hashes; full
per-direction ledgers remain in the hashed raw reports.

**All eight mismatches are higher approach scores, not rejection of the entire
historical site.** Every historical reference has an eligible direction. The
other 12 choices match the historical site. None of the 20 native choices is
exposed to a nearby opponent, so cover and ground-route contributions are zero.
In this corpus, the winning score is the shorter safe approach arc from the
actual arrival position. These are controller score units, not duration estimates.

| Differing choices | Cases | Historical → selected bearing | Historical score | Selected score |
| --- | ---: | --- | ---: | ---: |
| Historical world 1 sources | 4 | 31 → 33 | 16.69–18.25 | 1.25–2.81 |
| Fresh world 1, seat 0, both pressure settings | 2 | 33 → 17 | 265.66 | 4.17 |
| Holdout world 1, seat 0, both pressure settings | 2 | 45 → 46 | 12.90 | 0.38 |

Fresh-world-1's reference bearing 33 also has an unsafe **opposite** direction:
its forecast approach clearance is −50.24. Its preferred direction is safe and
still loses on score. Describing that reference site as wholly unsafe would be
incorrect. The four historical and two holdout mismatches have both reference
directions eligible. Repeated pressure settings and source snapshots remain
correlated, rather than eight independent worlds.

Across the full local surveys, the diagnostic emits **2,502 direction assessments**:
1,914 eligible and 588 rejected by solar checks. Independent raw-geometry
reconstruction agrees within 0.000123 world units, comfortably inside the frozen
0.01 tolerance; no clearance sign is independently unresolved. It explicitly
retains 351 departure-side near ties, including exact ties, within the 0.02
two-corridor tolerance. Those are direction-preference ambiguities, not uncertain
safety signs. Each selected full solar plan also matches native telemetry exactly.
No comparison returns unknown in these 20 trials; unknown, absent, rejected,
stale and constrained cases have focused tests.

The new 20 trajectories preserve all **48,940 controller/observation rows**
(398,322,559 decompressed bytes), complete transfer/acquisition reports and traces,
physical outcomes, observer output and upstream allocations. The 13 ordinary
regressions preserve another **49,440 rows** and all previous source-local and
forecast results. Sensor profile actor/tick, counters and stage calls also match
for all 98,380 rows. There are 376 hashed raw files across the 33 runs. They execute
49,170 physical ticks, **13.66 simulated minutes including repeated prefixes**;
each acquisition continuation still ends at its first choice one tick after
arrival. This does not measure the remaining physical landing or capture loop.

The optional comparison alone takes 0.128–0.168 ms per case on this desktop
(median 0.148 ms), with zero extra queries. This is a host diagnostic outside live
planner fuel; these timings do not establish a device or live scheduling budget.

Validation: 206 AI tests (10 new selection/diagnostic tests), seven example tests,
412 Python tests (11 new audit/mutation tests), formatting and strict AI Clippy
pass. Independent pre-run review strengthened non-winning departure-side checks,
required-site context validation and sensor-profile parity. The playing policy,
destination selection, acquisition deadline and live cost admission remain
unchanged. Nothing was deployed.

The independent post-run audit found no remaining issues. It verifies all 376
new and 376 reference file hashes, all 98,380 trace rows (789,840,239 decompressed
bytes), complete reports outside wall times and the added comparison, sensor
calls/counters and upstream work. Its separate geometry reconstruction reproduces
all 20 exact winners and approach scores. Original source/evidence ages remain
11–1,765 ticks at choice; ordinary comparison work remains 56,085 graph operations
for the same 21 completed jobs. The tracked projection and the quantities above
also reconcile. The audit is embedded in the tracked data record, with raw file
`target/capture-flag-survey/landing-choice-post-audit.json`, SHA-256
`43db6fc3c16f7a3a949380be6d048b37cb0c96e56f28254c03dbfe5c638298a5`.
Its sibling `.py` script has SHA-256
`70d1f6842feb1cdffe9d074a476cdebfaa6d841341e03edd794ebdbb33e821ef`.

## Next boundary

The mismatch has a concrete explanation: the historical two-bearing evidence
does not specify the approach the controller will choose from its arrival pose.
It should remain a conditional site hypothesis. It should not be forced into
execution or relabeled as the chosen site's landing cost.

Next, bind a local phase reference to the **actual chosen site and direction**,
with current material/claim identity and the native approach context, then follow
ordinary landing → exit → claim → board → departure. Preserve replans,
interruptions and censors instead of treating choice as completed travel. This
will test whether the remaining phase references describe the executed capture
loop. Source-time prediction of that arrival choice, enemy-flag evidence admission,
current-state refresh and whole-mission value remain separate unresolved steps.
