# Following a source reference through site acquisition

The [source-local composition](capture-local-composition.md) leaves an explicit
gap between the transfer handoff and the selected landing site. Existing local
phase references start at site choice. This slice measures that boundary without
adding a duration constant, ranking missions or changing the playing policy.

## Frozen plan

Select **all 20 alternative candidates with numeric local evidence** from the
previous 62-candidate corpus. Selection uses only evidence available at their
source ticks, not physical outcomes. Preserve the denominator: 21 current
candidates, 20 numeric alternatives and 21 alternatives lacking local evidence.
All 20 selected references are remote neutral/no-flag measurements. They include
16 departure-frame alternatives (four nominations immediately enter capture)
and four historical transfers. Conditions, seats, source ticks, destinations
and historical reference sites remain fixed. These correlated cases cannot
establish enemy-flag acquisition costs or population success rates.

Run each nomination twice: the frozen previous executable stops at transfer
handoff as before, and the new executable enables
`--probe-acquisition-seconds 30`. Allocate the full 60-second transfer window
plus 30-second acquisition window and a final observation. A failed transfer
never starts the acquisition clock. Preserve every refusal, interruption and
censored trial; never replace one with a convenient success.

The transfer intervention retains the existing `defer_new` pursuit policy.
Pursuit deferral ends at actual arrival. The continuation executes that neutral
handoff command once, then uses the normal controller entry point, evaluator,
sensors, budgets and physics. No historical site is forced. Native
`--bounded-acquisition-seats none` remains explicit: the observer's horizon
censors the trial and is not a controller deadline.

Start from the actual coordinator `arrived` event. Stop at first site choice,
actual mission interruption/replacement/failure, physical progress without a
witnessed choice, match/runner end or the observation horizon. A changed capture
replan counter or departure from the destination's observation frame ends this
attempt. Solar avoidance can continue while that attempt and frame remain. A site choice requires
the current capture's started tick and a matching, current native acquisition
snapshot. A retained site during a solar override is not a fresh choice. Loss,
recovery and attempt replacement precede same-tick choice; a valid choice at the
horizon is observed. The endpoint controller command is unexecuted. A match
ending during physics has no synthetic final controller row.

Use dense traces to audit the half-open waiting interval and native wait reasons.
Keep absent native updates separate from measured rejections. Compare the actual
chosen site with the historical reference and track source, arrival and endpoint
planet identities and evidence ages. Matching bearing alone does not renew old
material/owner/flag/radius/claim evidence. Changes during the transfer remain
visible even if endpoint identity later matches again.

## Verification and reproduction

The driver freezes its complete plan and hashes before starting physics:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/probe-site-acquisition.py \
  --reference target/capture-flag-survey/local-composition-v1 \
  --baseline-binary target/capture-flag-survey/surface-mission-soak-5ab1644 \
  --out target/capture-flag-survey/acquisition-probe-v1
```

Prior executable SHA-256:
`26e6cf18fa9c6c1d0c7d9475d6f38cbeeb5fada34999b7686d696c153fe025f5`.
Source composition summary SHA-256:
`ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`.

Require the original ordinary prefix and exact source observations, nominations
and candidate controls. Compare old/new transfer traces and all controller
observations byte for byte through the old terminal observation, including both
players. Verify ordinary evaluator, survey and upstream-work prefixes. This
includes reconstructing evaluator charges from the live planner's usage and the
flag allocator's post-evaluation residual; transfer probes do not emit the
separate evaluator-work file used by scheduled comparisons. Reconcile the
reconstructed total to the evaluator report. This
establishes parity **through arrival**, not a post-arrival counterfactual: the old
executable stops there. Later intents follow the ordinary call path by construction.
Independently reconcile the new endpoint to native telemetry and lifecycle events,
retain raw hashes and exercise censoring/precedence/staleness in focused tests.

No deployment, live cost admission or bot-strength claim is part of this slice.
The first pair was produced at runtime/runner freeze `334a498`, then its audit
stopped on the absent evaluator-work file. Its raw artifacts are retained in
`target/capture-flag-survey/acquisition-probe-v1`. The correction reconstructs
that work from existing logs; it does not change runtime, sources, horizon or
outcome rules. The full unchanged matrix will run in
`target/capture-flag-survey/acquisition-probe-v1-rerun`, including the same first
pair. Both repeated trajectories match the retained initial files exactly:
11,018 off and 11,020 on controller rows, physical outcomes, probe reports,
ordinary observer outputs and upstream work.

## Results: 26 September 2026

Runtime was frozen at `334a498`; the corrected runner and unchanged plan were
frozen at `de053d0`, with a clean worktree before the full matrix. All **40 runs
completed and passed**: 20 frozen-executable transfer probes and 20 acquisition
continuations. No source, runtime rule or observation horizon was tuned after
an outcome. The executable SHA-256 is
`e99cc0a530f101d39b22ce8abbae28cd5b93bb0fda6a065a2487a12758bb3827`.
Raw summary SHA-256:
`ba5db63795995442a17088eeda800957c931cd8d5b20d063c68b0a70fb3f78c5`.
The record is [capture-site-acquisition-v1.json](data/capture-site-acquisition-v1.json).

All 20 transfers arrive, and **every first site choice is observed one tick
later (1/60 second)**. Their neutral handoff consumes one physics step; the next
controller observation makes the choice. The 30-second observation horizon is
never reached. Each waiting interval has one `no_current_native_update` row,
the initial handoff. No acquisition rejection, interruption or censor occurs in
these physical trials. Those boundaries are tested by focused unit/mutation
cases, not demonstrated as successful failure handling in this corpus.

| Nominated alternatives | Cases | Transfer duration | Same historical site |
| --- | ---: | --- | ---: |
| Immediate capture entry | 4 | 0 ticks | 4 |
| Other departure-frame destinations | 12 | 1–120 ticks | 8 |
| Historical remote destination | 4 | 1,537–1,692 ticks | 0 |
| Total | 20 | | 12 |

The **12 matching choices** retain compatible material, owner, flag, radius and
claim settings; their evidence is still within its age bound. **Eight choices
use another site.** The four historical snapshots all estimated bearing 31 and
choose bearing 33. Fresh-world-1 seat 0 chooses 17 instead of 33 in both pressure
settings; holdout-world-1 seat 0 chooses 46 instead of 45 in both settings.
These are repeated conditions/source snapshots, not eight independent worlds.
Across all 20 runs, source identities remain unchanged and evidence ages at
choice range from 11 to 1,765 ticks. No age is refreshed by the continuation.

Every historical reference site is still present in the first post-handoff survey,
including all eight mismatches. The remote evaluator tests two bearings and
retains measured cover evidence; the native landing controller considers the
full local survey with current approach, solar and cover conditions. They need
not select the same site. Aggregate solar rejection counters do not identify
which particular direction/site failed; this study does not assign that cause
to a mismatch. Native choice snapshots are retained in the data record.

The old/new probes preserve **48,900 controller/observation rows byte for byte**
through the original transfer endpoints, all 20 transfer traces/reports and the
ordinary evaluator/survey/upstream prefixes. The on runs add 20 physics steps
and 40 controller rows, totaling 48,940 rows. Their extra evaluator work belongs
to those executed handoff ticks, after the old trials stopped. The 40 runs cover
48,880 physical ticks, **13.58 simulated minutes including repeated prefixes**;
they do not provide 13 minutes of acquisition waiting. The raw record binds
440 file hashes. The initial pair retained after the audit-file error adds two
separate reproducibility runs and is excluded from these matrix totals.

Validation: 196 AI tests, seven example tests (five new observer boundary tests),
401 Python tests (15 new acquisition tests), formatting and strict AI Clippy
pass. Independent pre-run review corrected range binding, attempt/frame guards
and censored endpoint labels. No playing controller, native acquisition deadline,
destination ranking, live cost model or device deployment changed.

The independent post-run audit found no blockers. It checks all 440 recorded
file hashes and 97,840 controller rows (794,703,410 decompressed bytes), the
original 62-candidate denominator and source bindings, transfer/control prefixes,
native site choices and historical evidence ages. It reconstructs all upstream
graph/query allocations and verifies the retained first-pair reproduction.
The audit is embedded in the data record; its raw artifact is
`target/capture-flag-survey/acquisition-post-audit.json`, SHA-256
`d909a43f7093ce7375ab5cca638ae80a379b2182af7d23f4cdf600b1ddffc8d1`.
The independent script is beside it as `acquisition-post-audit.py`, SHA-256
`12412796a8b3e12766350231b071e644da18108271682d8fe11436bae6be1593`.

## Implication for the next slice

For this source-numeric neutral subset, waiting to choose a site is small;
**binding an estimate to the landing plan actually selected remains unresolved**.
Do not insert a universal one-tick acquisition constant or treat all 20 component
sums as complete trip predictions. This selection excludes the unknown enemy-flag
alternatives where the [broader acquisition investigation](bot-site-acquisition.md)
found substantial waiting and abandonment. It also ends before executing the
first selected-site command, so it measures neither successful landing nor the
remaining capture/departure duration.

The next bounded step should compare the remote site's proposal with the native
arrival choice and its safety/approach requirements, preserving disagreements
explicitly. That can establish what a proposed landing plan must contain before
we test its remaining physical capture loop and admit whole-trip costs into
mission selection. Current-state refresh and explicit enemy-flag evidence
admission remain separate requirements.

The [native landing-choice comparison](capture-landing-choice-comparison.md)
now resolves this slice's eight mismatches: all retain a safe reference direction,
but the native approach score favors the chosen site. Its shared ranker and raw
audits leave playing controls unchanged. Physical follow-through after choice
remains the next measurement boundary.
