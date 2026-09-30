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

## Audit correction before the completed matrix

Runtime and plan were frozen at `036a36e`. The first physical run preserved full
control/output and upstream-work parity, then stopped at an audit assertion:
the script incorrectly equated the flag survey's allowance with the entire
residual. That planner already caps its graph allowance at two operations.
Correct the audit to enforce that cap and cover it in the ledger test. Runtime,
forecast equations, budgets, lifetime and source list stay frozen. Preserve that
initial raw run under `target/capture-flag-survey/transfer-scheduling-v1-initial-audit`
and rerun the same fixed matrix; this is an audit repair, not source resampling.

## Results: 26 September 2026

The complete record is [capture-transfer-scheduling-v1.json](data/capture-transfer-scheduling-v1.json).
Raw artifacts remain under `target/capture-flag-survey/transfer-scheduling-v1`.
The completed matrix records clean audit revision `de54ba6`, with runtime
unchanged from `036a36e`; only the documented audit correction separates them.
Release example SHA-256 is
`5d3b5d16843fd822bc7dac7f43be75b5ea5c6674d48178a322dcfd24e09f6039`.

All 50 runs pass full controller/observation, probe, evaluator, survey and
physical-outcome parity against the frozen reference. They also preserve the
exact upstream survey/shadow allocations, normalized live-planner work and
evaluator/telemetry counters. The audit reconciles 71,874 physical probe rows
and 150,444 physical dispatch ticks (41.79 simulated minutes, including prefixes).
No completed report differs from its frozen immediate forecast.

| Total graph allowance | Cases | Forecasts ready before cancellation | Completion age | Charged graph work |
| --- | ---: | ---: | --- | ---: |
| 4, existing shared allowance | 25 | 0 | None | 6,542 |
| 32, diagnostic allowance | 25 | 24 | 11–78 ticks / 0.18–1.30 s | 37,620 |

At four operations, 24 pending jobs expire at source age 121; they receive only
185–298 operations apiece. There are 456 pending dispatches with no remaining
work across the 25 cases. The other request cancels on changed terrain. Even the
shortest frozen forecast needs 356 operations, so none of these residual
allocations can complete it. This establishes a scheduling bottleneck in this
corpus, not a CPU bottleneck or a reason to relax freshness.

At 32 operations, 24 forecasts complete at mean source age 50.79 ticks (0.847 s),
median 53 ticks, maximum 78. None of the pending dispatches gets zero work.
Those same 24 ready entries expire at age 121: completion and expiry both count,
while the archival report remains explicitly historical. Zero physics queries
are consumed in either configuration.

The remaining case is `world0-asteroids3-on-t5952-p0`. At tick 5965 (age 13),
destination planet 0's terrain revision changes from 14 to 15, with ownership
unchanged. Both jobs cancel before that tick's work: after 6 operations at budget
4 and 370 at budget 32. Its frozen forecast requires 442 operations. The physical
ship later hands off at tick 6343; that later success does not make the invalidated
source fresh or justify completing its old job. The recorded observation proves
a material revision change; this experiment does not identify its physical cause.

The earlier asteroid-interrupted case, `fresh1-asteroids3-s0-t131-p2`, still
interrupts at tick 552 (421 ticks after its source). Its 32-operation forecast
completes at age 78 and expires at 121. A ready historical forecast remains a
conditional prediction, not evidence that the real trip will survive.

Maximum measured forecast dispatch is 0.00434 ms at budget 4 and 0.01503 ms at
budget 32. Maximum source capture/validation is 0.03659 ms across the study.
These desktop microbenchmarks exclude diagnostic IO and include only one
nominated forecast per replay. They do not establish Pi cost, multiple physical
forecast throughput or a worst-case wall-clock guarantee. No device was deployed.

## What this supports next

The queue can spread a frozen prediction across real ticks, charge every model
step and retire it when its source becomes unusable. It also shows why putting
the unchanged job at the end of the existing four-step queue is insufficient.
The 32-operation setting is a useful headless comparison, not a new playing
default or a universal budget recommendation.

The next experiment can compare candidate destinations in shadow mode with an
explicit prediction allowance and source identity. It must distinguish the
actual controller state from a hypothetical alternative nomination; this slice
validates forecasts for the physically nominated trip only. Keep historical
duration separate from current remaining time, uncertainty and interruption
risk. Before live admission, define the privileged orbit/mass sensor contract
and measure concurrent actor cost on the target hardware. Combat priority and
unsupported live mission costs remain unchanged.

## Validation

181 AI unit tests, 373 Python tests, formatting and strict AI Clippy pass.
Two scenario tests cover independent-source parameter/drift rejection and three
motion/gravity fixtures of 3,600 native steps each. Queue tests exercise pending
and ready expiry, completion at age 120, same-tick dependency changes, measured
and unmeasured contact, missing observations with zero fuel, clock gaps/regression,
reset generations and two-actor fairness with a one-operation residual.

Independent code review found no remaining runtime blocker. Review strengthened
the audit's first-ready timestamp, terminal cancellation linkage, per-job work
reconciliation and exact upstream allocation parity. Mutation tests reject the
previously accepted backdated cancellation and the omitted-stage budget errors.
The first matrix attempt additionally exposed the documented flag-cap audit
assumption; its corrected check passes without changing runtime behavior.

An independent post-run audit verified 75 scheduled/reference directories,
650 scheduled raw-file hashes and 300,988 controller rows. It reconciled all
5,884 forecast dispatches with the independent upstream ledger and reconstructed
each charge from the actual residual and remaining model steps. It separately
checked 3,050 source/age observations, all completed reports, the first terrain
revision change and the distinction between later expiry and physical outcome.
The complete audit is embedded in the committed data; the original is
`target/capture-flag-survey/queue-post-audit.json`, SHA-256
`e2b13ca58b012aa42d1f10af0631661743149ae201fa77d783a0f99589bfd87a`.
