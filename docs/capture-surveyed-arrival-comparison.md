# Fresh comparisons from real arrival surveys

## Frozen experiment plan

The [previous survey experiment](capture-arrival-survey.md) measured the later
native landing site in four controlled transfers. Those measurements happened
after their originating forecasts were frozen. This slice asks whether a new
source can use that genuinely earlier geometry for the existing remote arrival
screen, without changing the playing bot or claiming complete capture costs.

Add opt-in `--compare-surveyed-arrival none|empty|measured` to the headless
comparison harness. Default `none` retains existing behavior. Both active arms
require arrival surveys and latch the first charged attempt per actor, including
negative, incomplete or missing evidence. Freeze at the next real pre-intent
observation only; same-tick or delayed delivery consumes the event without retry.
Observe the real measurement frame before controls, then admit the sample through
`RemoteSurveyMemory` on the following tick. Preserve original request generation,
measurement tick, planet motion, revision, actor, vehicle and episode identity.
Raw candidate status is historical metadata: finding and actual age govern
admission. No old measurement becomes current by changing its timestamp.

Both arms record the same full retained snapshot and freeze the same actual
pre/post-intent controllers, evaluator and environment. `empty` attaches an empty
typed snapshot from that actor/episode/frame; `measured` attaches the retained
snapshot. Both use the same remote-screen submission API. Invalid full snapshots
cannot be rescued by the empty arm. Missing or rejected geometry remains unknown.
Later successful surveys cannot replace the first trigger, retry failed admission
or update this frozen source.

The fresh queue has its own token namespace and 120-tick source lifetime. Validate
it on every real observation even after the original source retires. Dispatch
after the original comparison using the same 64-operation allowance: subtract
playing work (capped at four) and actual original comparison charges first. Both
actors share that residual. Zero budget still processes invalidation. No physical
queries are added by the fresh comparison. Existing arrival surveys keep exactly
their earlier schedule and residual query rules. Construction, validation, cloning
and output are bounded host work outside the graph quota; this is not a wall-time
or hardware performance claim.

Before replaying, commit code, tests, this plan and the command builder. Use all
eight survey-enabled conditions from the prior experiment, each with `empty` and
`measured`: four ordinary sources and four controlled full capture continuations,
source ticks 3816, 3876, 3934 and 3997, seed 3491156488288037499, seat zero,
destination two, no asteroids, point-v1 forecasts and unchanged playing settings.
This gives sixteen runs. The event rule selects fresh source ticks causally;
do not select later measurements based on eventual success. Ordinary cases with
no charged attempt must remain explicitly untriggered.

Audit full control/observation traces, normalized sensor work, upstream work,
original reports/ledgers, survey reports/ledgers, physical outcomes and native
choices against the archived survey-on runs and between arms. Audit the new
ledger separately for causal admission, immutable samples, shared budgets,
source lifetime, validation and continuation after original-source retirement.
Independently reconstruct projection and solar screens using existing tolerances.
Compare common frozen forecast content separately from legitimate differences in
completion tick or expiry caused by the extra screening work. Whole-trip costs,
site-acquisition time and future enemy exposure remain unknown.

For complete screens on controlled transfers, join the prediction to the later
physical handoff and native choice for analysis only. Retain failures and unknowns;
never adjust sources, models, thresholds or controls after seeing the outcome.
These remain four correlated cases in one quiet world. Coverage and conditional
screen agreement cannot establish improved playing strength. No device deployment
is warranted for this observational harness. Obtain independent code/evidence
review and exact-head CI on PR #122.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-surveyed-arrivals.py \
  --out target/capture-flag-survey/surveyed-arrival-repeat
```

Use a clean checkout and a new output directory. The archived arrival-survey
raw runs are required; their summary and all baseline file hashes are verified
against the tracked projection before running.
