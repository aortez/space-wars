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
