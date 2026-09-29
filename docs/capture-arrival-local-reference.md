# Same-site arrival-local capture reference

## Frozen experiment plan

The [fresh-arrival comparison](capture-surveyed-arrival-comparison.md) measures
and projects the eventual native site in four controlled transfers. Its older
local cost evidence still describes a different site. This slice adds a separate
conditional reference tied to the actual arrival sample; it does not combine
that sample with the old local reference or change playing decisions.

Opt-in `--arrival-local-reference on` applies only to the fresh comparison queue
(`--compare-surveyed-arrival measured`). Default `off` preserves serialization,
old costs, forecasts, solar screening and queue behavior. A separate
`arrival_local_reference_v1` record identifies its original source, measurement,
projected site and arrival tick, plus all eligible solar directions. No direction
or future native choice is selected. Reuse the existing six successful neutral
trip medians without fitting new coefficients or substituting solar arc times.
Their clock starts at a hypothetical future native choice, whose epoch remains
unknown, with no elapsed-time deduction. Site geometry gates this reference;
it does not make these medians geometry-specific duration predictions.

Require v13/full-ship source context, admitted material with a usable hatch and
clear measured climb, neutral idle claim rules (no claimant/progress, three-second
stage, finite positive interaction range), and at least one clear conditional
solar direction. Preserve failures and missing geometry explicitly. Opponent and
cover measurements remain historical. Numeric phases are conditional on native
selection of this site/direction, persistent material/rules, and successful
unexposed execution. Acquisition, future exposure and whole-trip time stay
unknown. Solar screening uses its own geometric approach clock and fixed parking
window; it is not a safety certificate over the empirical phase durations.

For source-admitted screens, each recorded site takes one additional charged
graph step after solar screening, including rejected site eligibility results.
Unsupported source policy/ship and screen-level refusals terminate with zero
reference charges. At most four records per candidate and three candidates.
Keep solar charges/completion separate; include reference
charges in the enclosing comparison total. No extra physical queries. The same
64-operation shared residual, source lifetime, validation and cancellation apply.
Only the opt-in path additionally cancels if the claim domain changes before or
after publication. Construction and validation remain bounded host work outside
graph accounting; this is not a hardware speed claim.

Freeze code, focused tests, command builder and this plan before replaying.
Reuse all eight measured conditions from the previous frozen corpus, each
reference off/on: four ordinary untriggered cases and four full controlled
transfer/capture continuations, original source ticks 3816/3876/3934/3997,
seed 3491156488288037499, seat zero, destination two, quiet world, unchanged
point-v1 forecast and playing settings. Sixteen runs, with no new case selection
based on outcomes. Check historical hashes before running.

Compare complete controls/observations, normalized sensor work, upstream evidence,
original comparisons, survey requests/results and physical capture outcomes.
The disabled fresh report/ledger must match its historical measured run exactly
apart from wall times. Audit enabled fresh admission, immutable samples, lifetime
and shared work with the extra reference charge. Existing frozen forecast, solar,
cost/ranking content must match, allowing only added reference work and its
publication delay. Independently check same-site provenance, domain and unchanged
phase constants (existing 0.000003-second phase / 0.00001-second total tolerances).

After replay, join references to the later native site/direction and independently
witnessed capture milestones. Preserve mismatches, interruptions, unknowns and
later exposure. Report phase errors as observed minus reference; keep observed
handoff-to-choice time separate. Do not use later native geometry, exposure or
milestones to change any earlier forecast. These four correlated flights can
show same-site reference coverage and calibration limits, not improved bot
strength, general timing accuracy or readiness for live cost ranking.

The retrospective match requires the existing age/continuous identity audit and
same-frame geometry reconstruction, not just a matching bearing number. Retain
observed durations but withhold numeric timing errors outside native eligibility
or when later exposure violates the reference conditions. A ranked report must
finish every opted-in reference; a pending/cancelled snapshot may be incomplete.

Obtain independent code/evidence review, update PR #122 and run exact-head CI.
No deployment is needed for an observational harness change.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-arrival-local.py \
  --out target/capture-flag-survey/arrival-local-repeat
```

Use a clean checkout, a new output directory, and the archived raw runs at
`target/capture-flag-survey/surveyed-arrival-v1`.

## Results

Implementation, tests, plan and runner froze at
`76e7eb263c8d77b18790a3b6beb422ca7b4bba61`. Profiled binary SHA-256:
`2848910c6fe363eab1fdfe47b7f08cbea8dcd3d4e26971b16bd66db0025c6f3f`.
All **16 runs** pass, spanning **88,118 physical ticks**. Complete playing
controls/observations, normalized sensor work, upstream evidence, original
comparisons/surveys, native choices and physical outcomes match the historical
measured runs. The disabled fresh reports and ledgers match too. No experiment
runner or runtime changes were made after the freeze; no runs were dropped.

All four ordinary conditions remain untriggered in both arms. Each controlled
enabled source produces one numeric reference for planet 2, bearing **33**,
using the original sample and its projected geometry. Both screened directions
are eligible. The later native choice matches that site and an eligible direction
in all four cases. Measurement age, continuous identity and geometry checks pass;
the largest same-frame reconstruction difference remains **0.000173 world units**.
All four native choices satisfy the neutral timing domain, with no later exposure
before departure. This is retrospective confirmation, not earlier knowledge of
the choice or future threat.

Each reference adds **one graph operation**, raising total fresh comparison work
from **6324 to 6328**, with **zero additional physical queries**. Shared playing,
original comparison and fresh comparison work stays at or below **64 operations
per tick**. The four publication ticks remain **3881, 3939, 3993 and 4061**.
Source lifetimes, original forecast/solar content, old local costs, capture-cost
composition and rankings remain unchanged. Rejected alternative nominations stay
explicitly unknown. The old local reference still describes bearing **31**; the
new record never substitutes or combines that site's evidence.

The unchanged reference is **24.7667 seconds** from a hypothetical native choice
through departure, including **17.8667 seconds** for landing. All six phases are
independently witnessed in each physical continuation:

| Original source | Actual landing, s | Actual choice to departure, s | Total error, actual minus reference, s |
| --- | ---: | ---: | ---: |
| 3816 | 17.917 | 24.883 | +0.117 |
| 3876 | 18.233 | 25.183 | +0.417 |
| 3934 | 16.050 | 22.983 | -1.783 |
| 3997 | 16.900 | 23.833 | -0.933 |

Landing accounts for most of the variation: its errors range from **-1.817 to
+0.367 seconds**. Exit and boarding each match their one- and two-tick references;
outbound motion takes 6–8 ticks versus a four-tick reference, observed claiming
takes 179 ticks versus 181, and departure takes 228 ticks versus 226. The phase
boundaries follow the existing independently witnessed telemetry convention;
these differences do not redefine the game's three-second claim rule.

Actual handoff to native choice takes one tick in all four cases. That observed
interval remains separate and is not inserted into the frozen prediction.
Acquisition, future threat and the complete trip estimate remain unknown. Solar
screening still uses its own approach/parking model. No coefficients, thresholds,
playing controls, rankers or evaluator caches were changed from these outcomes.

The [tracked projection](data/capture-arrival-local-v1.json) preserves all commands,
hashes, refusals, references, budgets and retrospective comparisons. Raw summary:
`target/capture-flag-survey/arrival-local-v1/summary.json`, SHA-256
`d9d93793be4fbb6f3ac3ae4870da4daf9d0c9a557103263a41da4c064d1419b2`.

Local validation passes **406 Rust tests** across all spacewars-ai targets with
sensor profiling, **499 Python analysis tests**, formatting and diff checks.
Clippy finishes with eight existing dependency warnings and none in the AI or
harness. New cases exercise source/claim/material/solar refusals, one usable
hatch, mixed valid/invalid sites, separate clocks and charges, exact preservation
of old comparison content, and pending/ready claim invalidation. The two-actor
residual-sharing and zero-budget cancellation test runs on the new path.
Independent pre-freeze review also strengthened the experiment checks for valid
publication delays, unfinished ranked references and retrospective identity/age/
geometry matches. Numeric error joins are withheld outside native eligibility
or after observed exposure. No runtime or frozen experiment-runner changes
followed the results.

The [independent post-run audit](data/capture-arrival-local-audit-v1.json) passes
all 16 runs, verifying 232 raw files, 16 logs, 116 prior measured-run files,
176,252 control/observation rows and the same number of sensor rows. It checks
the new work ledger and claim guards, preserves the pinned prior geometry proof,
and independently reconstructs all 24 completed phases and their errors. There
were no checker failures. The retained checker is
`target/capture-flag-survey/arrival-local-post-audit.py`, SHA-256
`2ead1addc3db67c73487e80ad7314e2a84d196b9fb686c7530a6699b9c938ef9`.

This closes the same-site provenance gap for the four controlled flights. It
does not improve the underlying timing model: the same successful-trip medians
are now attached to compatible material evidence. The next bounded question is
native acquisition: whether a frozen source can predict which measured site and
direction the local controller will select, or explicitly decline to do so, with
a separate acquisition-time term. Landing-duration variation and future enemy
exposure remain distinct uncertainties. Wider cases are needed before live
capture-cost ranking; repeating this one quiet world cannot establish that.
