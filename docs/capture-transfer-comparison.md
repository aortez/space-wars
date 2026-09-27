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

## Results: 26 September 2026

Runtime, runner, budgets and this plan were frozen at `9f51d33`; the entire matrix
passed on its first attempt, with no tuning or source replacement. Release binary
SHA-256: `ebf4d8d974bac8116effd4839ebf8c4bdfda4ec4b96a549a433cc472ac9b0e43`.
The full record is [capture-transfer-comparison-v1.json](data/capture-transfer-comparison-v1.json),
with raw artifacts in `target/capture-flag-survey/transfer-comparison-v1`.

All **39 runs** complete: 13 frozen ordinary baselines and 26 comparison runs.
The latter preserve exact full controller/observation traces, physical outcomes,
evaluator/survey bytes and upstream work allocation. Total physical dispatches
are 74,160 (20.6 simulated minutes, including prefixes), with 148,320 controller
rows. The observer introduces no physics queries. All 25 old source prefixes,
observations, captured environments and hypothetical first commands match in
both budgets; every old alternative remains in the shortlist.

| Shared graph allowance | Completed jobs | Complete travel rankings | Charged graph work | Completion ages |
| --- | --- | --- | --- | --- |
| 32 | 3 / 21 | 0 / 21 | 37,989 | 64–73 ticks |
| 64 | 21 / 21 | 10 / 21 | 56,085 | 7–100 ticks |

The 64-operation jobs complete at mean age 70.05 ticks, median 76. Their 62
candidate slots contain **51 handoffs, two nominal planet-envelope exits and
nine explicit unknowns**. Five unknowns are current capture approaches, and four
are accepted alternatives that immediately enter an unsupported local phase.
The latter diagnosis is reconstructed from source observations and the controller
handoff predicates: target equals approach frame, range is below radius + 105,
relative speed is below 18 and local queries are ready. The raw constructor
refusal itself retains the generic unassisted-flight reason.
The two envelope exits are the current route for one holdout seat, repeated with
and without asteroid pressure. They are model-domain exits, not observed crashes.

All 25 previously forecast alternatives complete at allowance 64 and reproduce
their frozen immediate reports exactly. At allowance 32, seven complete with
exact reports; the other 18 preserve exact recorded trajectory sample prefixes.
Across all candidates at allowance 32, 23 finish a handoff and 30 remain pending
when the source is invalidated. No partial job publishes a ranking.

Both budgets retain the same cancellation causes: 18 source expiries at age 121,
two ends of unassisted flight, and one material revision change (planet 0, revision
14 to 15; ownership is unchanged). At 64,
all jobs publish before their later cancellation, including the job that completes
at age seven before the changed terrain at age 13. These historical results do
not remain fresh after cancellation. Completion counts include explicit unknowns;
they do not mean all destinations became comparable or physically succeeded.

The ten complete travel rankings all favor a departure-frame planet, with modeled
handoffs after 2–120 predicted ticks. That result follows directly from asking
which flight reaches a nearby kinematic handoff soonest. It says nothing about
whether landing, walking to a flag, surviving an opponent or actually capturing
that planet is preferable to continuing the current mission. The four unknown
departure-frame alternatives also show why an immediate local phase must not be
silently converted to zero-cost travel.

In eight of the ten complete rankings, the preferred hypothetical handoff occurs
only 2–10 ticks after its source, while publication takes 65–93 real ticks. The
other two predict a handoff at tick 120 and publish at age 92. These remain valid
historical model comparisons, but they cannot become current remaining times by
subtracting the elapsed age: the real ship followed its original route. Live
admission needs an explicit current-state revalidation/refresh rule as well as
compatible local-mission evidence.

Maximum observed desktop dispatch, including bounded progress/publication copies,
is 0.025078 ms at 32 and 0.040437 ms at 64. Recorded construction/validation peaks are
0.032371 and 0.053411 ms; these exclude the pre-intent controller clone. These are headless measurements across this fixed corpus,
not target-hardware frame-budget guarantees. Snapshot construction and validation
remain outside the graph-operation quota.

## Validation and next boundary

188 AI tests, 379 Python tests, formatting and strict AI Clippy pass. New tests
cover independent pre-intent clones, exact current commands, inner and outer work
sharing, unknown current/local options, ties, a separately charged ranking step,
and expiry when that final step is still pending. Scenario tests verify the
contact-only reader with airborne, settled and hull-only contact, and retain the
three 3,600-step environment/gravity checks.

Independent review replaced a diagnostic contact read that performed terrain
queries with a read of existing solver contacts. It also strengthened the audit
to derive source/shortlist identity from ordinary observations, validate every
candidate's geometry (including new current-route predictions), and retain
completion history after cancellation. Mutation tests reject negative/nonfinite
durations, invented preferences, lost completion history, candidate relabeling,
wrong source commands and unreconciled work. These fixes preceded the frozen run.

The independent post-run audit verifies 442 study-file hashes across 39 study
directories, plus 25 prior reference directories. It reads 148,320 decompressed
controller rows (1,174,553,040 bytes), checks all 74,160 physical ticks, and
reconciles 4,194 comparison dispatches against 49,440 upstream ledger ticks.
Independent reconstruction accounts for **94,050 motor operations and 24 final
ranking operations**, with exact rotation across candidates and actors, including
1,540 dispatches where both actors are pending. It also checks the two envelope
exits and the publication-age/local-phase diagnoses above. The audit is embedded
in the committed data; its raw artifact is
`target/capture-flag-survey/comparison-post-audit.json`.
Its SHA-256 is `6879b9a683fcb25fc6e2d71dc208064740c49d25f697f3bda609f3d23e49c3d0`.

The next useful slice is a **same-source handoff to local-mission cost contract**:
define how the current capture approach and a candidate already near its planet
provide remaining landing/flag/return evidence. Combine compatible travel and
local evidence in shadow mode; retain unknowns when either side is absent. Use
mission value, interruption risk and current-state revalidation before proposing
any live destination change.
This comparison establishes concurrent scheduling and provenance, not a stronger
playing bot, a calibrated capture-cost model or a hardware budget recommendation.

The follow-up [source-local composition study](capture-local-composition.md)
implements that contract and explicitly retains the unmeasured interval between
flight handoff and landing-site choice.
