# Shadow destination transfer comparisons

This follows [bounded transfer scheduling](capture-transfer-scheduling.md).
Compare conditional free-flight handoffs for the current destination and up to
two alternatives. This slice does not add travel estimates to the playing
evaluator: its surface evidence can have an older source tick, and combining
those costs would conceal incompatible contexts. No playing policy or UI changes.

## Frozen design

Capture the actual post-intent controller and command for the current option.
Each alternative independently clones the same pre-intent controller and uses
the same observation, environment epoch and evaluator view used by that actual
command. Keep every nomination refusal and unsupported current local phase as
an explicit unknown. Never retry a refused nomination from post-intent state.

Choose at most two other planets not owned by this actor (enemy or neutral), in
planet-index order. Unlike the older nomination sampler, the departure frame is
not excluded. Record truncation rather than treating a bounded shortlist as the
whole world. Candidate construction and nomination commands are bounded setup;
prediction equations remain `guided_transfer_forecast_v1`.

One comparison token per actor shares the existing queue and freshness guards.
Pin its **real** target, selection tick, switched state and capture presence/site.
An uncommitted capture approach may anchor a comparison, but its current local
phase has no free-flight prediction. Changed capture commitment, recovery,
pursuit, contact, health, material, ownership, environment or controller context
cancels the job. Missing observations cancel; source age 120 is valid, 121 is not.
Contact validation reads existing solver contacts and recorded debris contacts;
it does not cast terrain rays or run overlap queries.

An outer queue turn belongs to one actor. Within that actor, round robin advances
one runnable candidate motor tick per graph operation. One final graph operation
computes the ranking, even if all candidates were immediately unknown. Maximum
work is 3 × 3,600 + 1. No physics query allowance, hidden draining, unused fuel
carry-over or multiplication of the shared allowance by candidate/actor count.

Record equal-tick ties in destination order. A completed job may report the
fastest **known** handoff, but reports a preferred handoff only when at least two
candidates all have numeric handoffs and the shortlist is not truncated. Neither
field is a capture-value comparison, collision guarantee or permission to act.
Partial snapshots have no ranking; archived snapshots and publications remain
historical after cancellation. Construction, validation and bounded report copies
are outside graph accounting and measured separately; file IO is outside timing.

## Fixed physical study

Use all 25 prior immediate forecast cases, without new source selection. Collapse
duplicate destination nominations into **21 source groups**, then combine paired
seats from the eight fresh/holdout world conditions. This makes **13 ordinary
physical trial specifications**: five historical single-seat sources and eight
paired-seat trials. Both actors are active together in each paired trial.

For every specification, run the frozen ordinary binary from `9ff877b` and this
observer at shared total graph allowances **32 and 64**: **39 physical runs**.
The existing planners retain four operations and unchanged priority; only this
observer receives the extra 28/60 plus any remaining playing allowance. Both
actors and all their candidates share that one total. Run through the latest
source plus 121 ticks, rounding up to the next whole second. Do not physically
nominate a destination or defer pursuit. The previous nomination traces are
valid source-prefix references, not ordinary full-run baselines.

Freeze code, runner, this plan and budgets before outcomes. Require full ordinary
controller/observation trace equality, evaluator/survey output equality, physical
outcome equality and unchanged upstream allocation against the frozen baseline.
Independently reconcile each dispatch with the existing planner ledgers, per-actor
candidate motor counts and final ranking charge. Audit validation, completion and
cancellation ticks, including ready-then-stale jobs. Compare old source prefixes,
observations, environments and nominated first commands exactly. Where those old
hypotheses remain shortlisted, require complete reports or partial sample prefixes
to match the frozen immediate forecasts. Keep refusals, expiry and incomplete
current options in the denominator; do not fit the model or resample failures.

Preserved baseline: `target/capture-flag-survey/surface-mission-soak-9ff877b`, SHA-256
`5d3b5d16843fd822bc7dac7f43be75b5ea5c6674d48178a322dcfd24e09f6039`.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-transfer-destinations.py \
  --reference target/capture-flag-survey/transfer-forecast-v1 \
  --baseline-binary target/capture-flag-survey/surface-mission-soak-9ff877b \
  --out target/capture-flag-survey/transfer-comparison-v1
```

Explicit host options are `--compare-transfer-sources 0:132,1:134` and
`--transfer-comparison-allowance 32`. The observer is disabled by default and
cannot run alongside the physical nomination/continuation interventions. This
is a correlated engineering corpus, not a bot-strength or Pi performance study.
No deployment is part of this slice.
