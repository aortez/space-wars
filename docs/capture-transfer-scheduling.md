# Transfer forecast scheduling experiment

This follows the frozen [transfer forecast](capture-transfer-forecast.md). The
prediction model, playing controller, mission evaluator and live admission rules
stay unchanged. This slice measures whether historical forecasts finish in time
when dispatched incrementally, after every existing planner and observer.

## Fixed design and experiment

Retain one immutable forecast per actor, using the existing fair `PlanningQueue`.
One graph operation advances one predicted 60 Hz tick, at most 3,600. No physical
queries, hidden drain or unused quota carry-over. The source controller, first
command and privileged environment snapshot never refresh. Two-actor unit tests
exercise one-unit residual fairness and cancellation independently of this
single-nomination physical corpus.

Validate every retained request, including ready results, against the current
post-control observation before dispatch and publication. A missing observation,
observation gap, regressed clock, actor/episode/configuration change, changed trip,
ship loss, damage, contact, recovery, pursuit, assistance, match end, changed
ownership/material or unsupported dynamics cancels it. Measured zero-damage
contact still cancels; unmeasured contact is unknown. Generation tokens survive
reset; duplicate ticks cannot renew work. A source is fresh through age 120 and
expires at 121, reusing the existing evaluator lifetime without fitting it here.

Known celestial dynamics advance from the source exactly once per observed tick.
Compare current captures against that independent track: fixed parameters and
scripted phases are exact, body-position/angle/spin tolerances are 0.002 and
velocity tolerance is 0.02, inherited from native environment validation. Never
rebase that track onto the latest capture or catch up missed ticks. Validate
current wing limits against current sweep; normal sweep changes are supported.
Finite source/boundary checks reject unsupported inputs. Construction,
environment capture/validation and result publication are bounded separately
from charged prediction work and timed outside its quota; trace IO is separate.

Run **all 25** prior forecast cases, without selecting new sources or retries, at
total graph allowances **4 and 32**, with the same `defer_new` physical transfer.
The existing live planner, successor, evaluator, flag survey and flag-value
shadow retain their four-step allowance and priority. The forecast receives
only their residual. In the 32-operation diagnostic, the extra 28 operations
are available only to this forecast observer. This isolates throughput without
changing the playing planners' budgets. Both seats occur in the corpus, but each
replay has one nominated forecast; this is not a simultaneous-bot capacity trial.

Freeze this adapter, runner and plan before examining results. Keep expired,
starved, rejected and interrupted jobs in the denominator. Record real source,
completion, validation and cancellation ticks independently of the queue's
dispatch counter. Completion and later expiry can both happen to one request;
their counters are not mutually exclusive outcomes. Preserve a completed report
in the diagnostic archive after expiry, but never publish it as a fresh result.
Its duration remains relative to its source, not a current time-to-arrival.

Require byte-identical full controller/observation, probe, evaluator and survey
traces and identical physical outcomes against the prior frozen physical branch.
Every completed report must match its immediate frozen forecast exactly. Audit
the real per-tick upstream ledger, including flag-value shadow charges, the
remaining allowance, zero queries, charge totals and age boundaries. Preserve
source/binary hashes, raw commands and compressed traces. No tuning after results.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/schedule-transfer-references.py \
  --reference target/capture-flag-survey/transfer-forecast-v1 \
  --out target/capture-flag-survey/transfer-scheduling-v1
```

The default run leaves scheduling disabled. The opt-in requires an explicit
source nomination and `--objective-graph-budget 4`; use
`--schedule-transfer-forecast true --transfer-forecast-allowance 4` (or 32).
Immediate draining and queued forecasting are mutually exclusive. No new UI
setting or device deployment is part of this slice.
