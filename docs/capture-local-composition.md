# Source-bound transfer and local references

Follow-up: [physical handoff through first site choice](capture-site-acquisition-probe.md)
measures the acquisition gap for all 20 numeric alternatives and checks whether
the bot selects their historical reference sites.

Continue the [destination comparison](capture-transfer-comparison.md) with a
read-only local cost snapshot at each forecast source. This remains an observer
in PR #122. No controller selection, policy default or deployment changes.

## Frozen contract

The current and nominated branches use the same observation and evaluator view.
Read raw retained measurements and the source observation, not a previously
completed `MissionEvaluation`. Preserve original measurement and route source /
validation ticks. Check material, ownership, radius, flag location and interaction
range, claim duration, source age and local gravity. Explicit failed site/cover
measurements cannot revive cached successes. A missing cadenced route can retain
its original age within the existing route cadence. Local measurements from a
different gravity frame stay unknown; remote neutral-planet measurements retain
their separate no-flag reference basis. The separate published flag-survey shadow
is not implicitly imported into this snapshot.

Keep an independent diagnostic clock for a continuously observed selected site
and visit. Missing observations, a changed site/visit, material/flag or gravity
reset that clock. Subtract its elapsed ticks only from the landing reference for
the **current** uncommitted approach. Alternatives start a full site-choice
reference. No later cost publication updates the captured source snapshot.

An actual uncommitted capture, or a cloned accepted nomination that immediately
enters capture, explicitly has zero additional *transfer* time. That evidence
comes from controller state, never from a generic forecast refusal. Keep the
original free-flight unknown alongside it. Other branches require a completed
kinematic handoff before assigning numeric transfer time.

The empirical landing medians start at **observed site choice**, not at flight
handoff. Report the sum of known travel and local components separately from a
remaining trip reference. An unselected hypothetical site leaves the intervening
acquisition interval unmeasured and the whole trip unknown, even when both
components are numeric. Only a current approach bound to its observed selected
site can publish a conditional remaining reference. Successful phase medians
still do not predict stalls, interruption, survival or completion probability.
There is no mission-value ranking or live result consumer.

Candidate setup is bounded separately from graph work. Use the existing final
ranking operation to compose up to three references; motor equations, round
robin and allowances remain unchanged. Comparison tokens additionally invalidate
when surface identities, contributing evidence ages or observable local gravity
change, including after publication. A frame change cancels a contributing
local gravity reference. Archived results remain historical. Never subtract
publication delay from a hypothetical trip; a live proposal needs a new source.

The bounded diagnostic site/visit clock runs in every `MissionEvaluator::observe`,
including when transfer comparisons are disabled. Its time belongs to ordinary
evaluator construction, outside the separately reported comparison timings.

## Fixed replay plan

Use **all 21 source groups in the previous 13 ordinary trial specifications**,
at the previously tested shared graph allowance **64**. Reuse the archived
baseline comparison runs, verifying their recorded hashes. Run all 13
specifications again with this observer, including the same quiet/asteroid and
paired-seat conditions, sources and durations. Do not add, replace or tune sources
after observing results. Retain every unknown and cancellation in the denominator.

Require exact ordinary controls, observations, physics, evaluator/survey outputs
and upstream allocation. Audit old motor predictions or exact partial prefixes,
the unchanged one-unit final composition charge, source/candidate identities,
local evidence provenance, elapsed-time arithmetic and explicit missing phase
boundaries. New cancellations are recorded rather than forced to match the old
lifetimes. Unit regressions cover identity shifts, independent clocks, negative
observations, stale work, cadence gaps, gravity frames, new visits and expiry.

Preserved prior binary: `target/capture-flag-survey/surface-mission-soak-9f51d33`,
SHA-256 `ebf4d8d974bac8116effd4839ebf8c4bdfda4ec4b96a549a433cc472ac9b0e43`.
Archived comparison summary SHA-256:
`5009fcfc2c8e9f25e5fd6d2e1826a7746c6c65d19d71103a5f141a60fed3f8ce`.

This tests an engineering contract and its coverage, not bot strength, full-trip
accuracy or Raspberry Pi performance. No deployment is part of this slice.

## Results: 26 September 2026

Runtime, runner and plan were frozen at `5ab1644`; all 13 runs passed on their
first attempt without tuning, replacing sources or changing the matrix. Binary
SHA-256: `26e6cf18fa9c6c1d0c7d9475d6f38cbeeb5fada34999b7686d696c153fe025f5`.
The full record is [capture-local-composition-v1.json](data/capture-local-composition-v1.json).
Raw artifacts are in `target/capture-flag-survey/local-composition-v1`. The summary
SHA-256 is `ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`.

The runs cover 24,720 physical ticks (6.87 simulated minutes, including prefixes)
and 49,440 controller observations. Full controls/observations, physical outcomes,
ordinary evaluator/survey output and upstream allocation remain exact against
the prior budget-64 recordings. All **53 model forecasts reproduce their previous
reports exactly**: 51 handoffs and two nominal envelope exits. The nine original
free-flight unknowns also remain intact. All 21 jobs finish at their previous
ages, 7–100 ticks, using exactly **56,085 graph operations** and zero queries.

| Source-local outcome | Candidate options |
| --- | ---: |
| Current selected-site approach, conditional remaining trip | 5 |
| Alternative neutral site, known component sum but acquisition time missing | 20 |
| Compatible local evidence unavailable | 37 |
| Total | 62 |

The five current approaches explicitly have zero additional transfer time. Their
independently observed site-choice ticks are 5,766 for the world-0 source and
3,797 for all four world-1 sources. Landing references subtract 186 ticks in the
former, and 19/79/137/200 ticks in the latter. Conditional remaining references
are approximately 21.67, 24.45, 23.45, 22.48 and 21.43 seconds. These are **five
source snapshots of two local attempts**, not five independent completed trips.
This replay does not measure their eventual completion-time error.

All 25 numeric local references use the existing **neutral, no-flag calibration**:
five local measurements at the source and 20 retained remote measurements. Their
evidence ages range from zero to 227 ticks. No physical enemy-flag walking
reference becomes numeric in this corpus; walking admission and its original
route/flag provenance are exercised by focused tests. The separately certified
remote flag surveys remain outside this intake contract.

The 20 alternative component sums leave the handoff-to-site-choice interval
explicitly unknown. The four accepted nominations that immediately enter capture
now record that source phase and zero transfer. They have retained remote neutral
site evidence, but still lack an observed selected site and its acquisition
interval; they are included in those 20 component sums.
Of the 37 missing local references, two also have transfer-envelope
exits; the final composition reports these two travel failures and 35 missing
local references. There is no complete alternative mission comparison or new
capture-value preference.

All cancellation causes and ticks match the previous recordings: 18 source
expiries, two ends of unassisted flight, and one terrain revision change. Every
published result is eventually stale. The stricter local dependencies do not
trigger additional physical cancellations in this corpus; focused tests cover
their expiry and gravity/frame/flag invalidation, including Pending and Ready.

Peak recorded comparison construction/validation is 0.036509 ms; dispatch,
including bounded publication/progress copies, is 0.044113 ms on this desktop.
These exclude the pre-intent clone and file IO. The continuous choice clock is
part of ordinary evaluator construction, not these comparison timings. No Pi
budget or performance improvement is inferred.

196 AI tests, 386 Python tests, formatting and strict AI Clippy pass. Independent
review found and fixed rebasing of cached gravity and strengthened the audit to
reject invented walking costs, removed route provenance, altered choice clocks,
unjustified cancellation and incomplete reports disguised at a terminal tick.

The independent post-run audit verifies **312 file hashes** across 13 new and
13 prior run directories. It checks 49,440 decompressed new controller rows
(391,517,680 bytes), reconstructs all continuous choice clocks, and checks 2,348
local-dependency observations through publication and cancellation. The 2,097
comparison dispatches reconcile exactly to **56,064 motor operations plus 21
composition/ranking operations**, including 588 dispatches with both actors
pending. Every normalized comparison allocation also matches the prior run.
No discrepancies remain. The audit is embedded in the committed data; its raw
artifact is `target/capture-flag-survey/local-composition-post-audit.json`, SHA-256
`c3a4e667381358d8bbf24b0f78b0cf93a114aa899d8af459cb2375a12a72f04b`.

## Next boundary

The useful next measurement is **capture entry to observed site choice**: how
long acquisition takes, which site is actually selected, and which attempts
fail or are interrupted. A remote site timing reference is not a commitment to
that site. Collect those transitions before supplying the missing phase or
comparing whole alternative missions. Importing certified enemy-flag evidence
also needs an explicit source-compatible admission path. Current-state refresh,
mission value and interruption risk remain requirements before live selection.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compose-capture-references.py \
  --reference target/capture-flag-survey/transfer-comparison-v1 \
  --out target/capture-flag-survey/local-composition-replay
```
