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

## Results

Implementation, tests, plan and command builder froze at
`3b76865fd2b9a4f5a56f8030281ea292af125d28`. Profiled binary SHA-256:
`1a796d9858d160630afd38492e41dac62f838f2e53b4ce3287aa680fbeb1c7da`.
All **16 runs** pass, spanning **88,118 physical ticks**. Complete control and
observation traces, normalized sensors, existing work/evidence, original surveys
and comparisons, native choices and full capture outcomes match the historical
survey-on runs and paired arms. No runs were interrupted or dropped, and all
frozen experiment-runner audits passed. Review corrected analysis clock,
partial-output and provenance checks before the freeze; runtime and experiment
runner remained unchanged after outcomes.

The four ordinary conditions remain untriggered in both arms because local
landing work prevents a charged arrival sample. Each controlled condition freezes
exactly one later source in each arm, using the first sample at its actual age of
**one tick**. Both arms admit and record the same historical object; only the
attached geometry differs. Later samples do not refresh it.

| Original source | Sample | Fresh source | Ready, empty / measured | Graph work, empty / measured | Native choice |
| --- | --- | --- | --- | --- | --- |
| 3816 | 3853 | 3854 | 3880 / 3881 | 1659 / 1661 | 5509 |
| 3876 | 3913 | 3914 | 3939 / 3939 | 1590 / 1592 | 5493 |
| 3934 | 3967 | 3968 | 3993 / 3993 | 1555 / 1557 | 5509 |
| 3997 | 4036 | 4037 | 4061 / 4061 | 1512 / 1514 | 5535 |

The empty arms have no remote sites. Measured arms each project planet 2,
bearing **33**, and screen both circling directions. That site is later selected
natively in all four cases. All **eight directional solar screens** report clear,
agreeing with the later native assessment. Reprojecting the original geometry
into the actual native-choice frame differs by at most **0.000173 world units**,
within the frozen 0.002 tolerance. Samples are **1499–1656 ticks old** at native
choice, with unchanged actor/material identity and within the 1800-tick limit.
The retained site contains the eventual native best assessment in all four cases;
this is a retrospective coverage check, not a predicted native choice.

The new screen adds **two graph operations per source**, eight across the four
measured comparisons: **6316** total fresh-comparison operations with evidence
withheld, **6324** with evidence included. It adds **zero physical queries**.
Playing, original-comparison and fresh-comparison work together stay at or below
**64 operations per tick**. One source publishes a tick later due to the extra
work; the other completion ticks are unchanged. The combined-work maximum in the
data covers fresh-dispatch ticks only; zero in an untriggered ordinary case does
not imply that the original comparison used no graph work.

Each fresh source expires at its own age 121, after continuing to validate for
**38, 38, 34 and 40 ticks** following its original source's retirement. No old
token is reused or old source extended. The paired current-route forecasts and
existing local references/cost compositions are identical. Both alternative
nominations per fresh source remain rejected because the bot has already changed
destination on that trip. This experiment therefore improves current-route
screening coverage; it does not demonstrate a choice among three viable routes.

The point-v1 model remains unchanged. Its new predictions arrive **4, 11, 14 and
14 ticks later** than the actual handoffs. These are later actual source frames,
so they are not a controlled motion-model comparison with the earlier forecasts.
First native site choice follows each actual handoff by one tick. That observed
interval is not imported into any earlier prediction or declared a general cost.
Whole-trip capture costs, future enemy exposure and native acquisition remain
unknown. The minimum frozen solar clearance is about **318 world units**: these
are comfortable margins, not tests near a solar decision boundary. Frozen versus
later native clearances differ by up to 3.54 units; matching signs do not imply
identical timing or geometry at those different epochs.

The [tracked projection](data/capture-surveyed-arrival-v1.json) preserves commands,
hashes, all trigger/refusal states, budgets, screens and separate retrospective
joins. Raw summary:
`target/capture-flag-survey/surveyed-arrival-v1/summary.json`, SHA-256
`fdcffd7a04cda2218f3ea4bc1c334f3117edd2b91346a3b98dc6f89093647ce3`.

The [independent audit](data/capture-surveyed-arrival-audit-v1.json) verifies all
232 raw files, 16 logs and 108 baseline files, 176,252 control/observation rows
and the same number of sensor rows, 1,562,311,426 decompressed trace bytes,
24,172 fresh-ledger rows and 968 active source validations. It checks historical
and paired parity, causal admission, identity, immutable evidence, budgets,
retirement, projection, solar reconstruction, native joins and these notes
without importing the new experiment runner. No substantive findings remain.
The checker is retained at
`target/capture-flag-survey/surveyed-arrival-post-audit.py`, SHA-256
`c4ff05e7ab73a063f97e728d5032c534eb7c3184ab3d72b0d0a32b93b89964d0`.
Its two initial checker failures remain archived: an empty geometry screen can
still retain a numerical arrival frame, and explicit zero Counter entries need
to compare equal to absent zero entries. These corrections changed neither the
runtime, frozen experiment runner, corpus nor recorded outcomes.

Local validation passes **398 Rust tests** across all spacewars-ai targets with
sensor profiling, **487 Python analysis tests**, formatting and diff checks.
Clippy completes with eight existing dependency warnings and none in the AI or
harness code. Focused tests cover first charged failures, absent evidence,
duplicate/delayed observations, changed material/vehicle/episode, immutable
historical ages, paired attachment isolation, two-actor residual sharing and
zero-budget cancellation. Independent review also strengthened the audit to
reject missing source rows, replaced tokens, wrong modes and lost refusals.

The coverage gap is now closed for these four correlated flights. The next
bounded step is to assess arrival-local landing/capture references using this
same measured site, keeping their missing acquisition and threat terms explicit.
In all four cases the existing local cost evidence still describes bearing 31,
measured at tick 3770; the new arrival screen describes bearing 33. Those distinct
site references must not silently be combined. No live ranking, playing
controller or device build changes in this slice.
