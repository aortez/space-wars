# Following a native landing choice through the physical capture loop

The [landing-choice study](capture-landing-choice-comparison.md) explains why
native selection differed from eight historical references. This next slice
measures the **actual selected site's first physical attempt**, from selection
through landing, exit, claim, boarding and departure. It does not force the
historical site, fit costs, change controls or deploy to a device.

## Frozen study plan

Keep every one of the previous 20 neutral alternatives, with the same seats,
worlds, source ticks, destinations and pursuit policy. Preserve the original
62-candidate denominator: 21 current destinations, 20 numeric alternatives and
21 unknown alternatives. Only those same 20 numeric alternatives enter this
conditional study; no new outcome-based selection is allowed.

The transfer still has a 60-second horizon, followed by the existing 30-second
site-acquisition horizon. Add **120 seconds from the first native site choice**
for one capture attempt. The runner allocates source_tick/60 + 211 seconds,
rounded down, ensuring all three horizons plus their endpoint observations fit.
The normal match clock remains unchanged. Unknown estimates, interruptions and
match/runner censors stay in the results. A failed transfer or acquisition never
manufactures a capture source.

At choice, snapshot the existing source-local empirical reference using the
exact immutable observation passed to that tick's intent and trace. Bind it to
actor/vehicle, mission visit, material revision, selected site, direction, solar
plan and arrival pose. The reference remains fixed: later information neither
refreshes it nor expires the physical observation. Existing all-cover and
boarding-hatch evidence gates still apply to numeric costs, even when the native
unexposed pilot can choose a site without those cover preferences. An unavailable
cost remains unknown while physical follow-through continues. An unreproducible
native comparison or non-neutral source is an explicit unsupported outcome.

These are existing successful-trip phase medians, not estimates conditioned on
arrival geometry or success probabilities. For a supported neutral source they
remain 17.866667 seconds landing, 1/60 exit, 4/60 outbound, 3+1/60 claim, 2/60
return/board and 3.766667 departure. Do not substitute the native approach score
for seconds. Record touchdown offset in the planet's rotating local frame:
a native chosen site can lead to landing at another valid nearby pose.

## Observation boundaries

Milestones require both current native telemetry and actual physical witnesses:

| Milestone | Witness |
| --- | --- |
| Choice | Current native `selected_site`, verified by the landing comparison |
| Landed | New `landed_tick` and actual `LandingPhase::Landed` aboard the source ship |
| Exited | First actual on-foot row, exit transfer and increased transfer count |
| Claim started | On foot, actor claimant, raising/lowering with positive progress |
| Claimed | New `claimed_tick`, actor ownership/flag and increased capture count |
| Boarded | New `boarded_tick`, source ship aboard state, boarding transfer and two transfers |
| Departed | Current coordinator departure event, prior confirmed claim/boarding, incremented sortie count and either retained tactical completion or native destination clearance |

Take six adjacent differences starting at **choice**, not arrival or the mission
selection tick. A controller can begin acquisition before its first site choice.
The formerly terminal choice command now executes. Only the new endpoint command
is unexecuted. A synthetic finish row cannot establish a milestone.

Anchor one attempt: changed actor/vehicle, recovery/loss, material/rule changes,
foreign claims, lost ownership, replaced task/site, outer replans or inner
landing retries/invalidations interrupt it. Normal settling adjustments,
nonfatal damage, contacts and temporary advisory blockage do not. Temporary
sun avoidance can preserve an attempt; an actual solar replan still interrupts.
Frame changes are allowed after confirmed boarding. A valid departure is checked
before the coordinator's expected target/capture clearing. Real loss or changed
context wins over a same-tick departure.

A completed phase can be reported from a later interrupted trip. The interrupted
or censored phase retains its elapsed observation time without becoming a
completed duration or error sample. Later phases remain not started. No retries
are silently stitched into a successful first attempt.

## Verification and reproduction

Freeze runtime, runner, tolerances and this plan in a clean commit before
physical study outcomes. For all 20 runs, verify every archived file hash and
exact controller/observation bytes through the old first-choice endpoint,
including both players, sensor counters/stage calls, transfer/acquisition
reports, upstream output and planner allocations. Wall-time fields alone are
excluded from diagnostic comparisons. **Prior parity ends at first choice**;
the continuation is new physics and has no archived counterfactual suffix.
No additional playing ranker refactor is included in this slice.

Independently reconstruct all milestones, attempt termination and phase
intervals from dense raw observations, not the continuation's reported clocks.
Recheck the source cost values and native comparison; independently transform
the selected and touchdown positions into the body's local frame. Tolerances
are 0.000003 seconds for existing f32 cost constants, 0.002 world units per pose
coordinate and 0.003 world units for touchdown offset. Reuse the prior native
choice audit's frozen geometry tolerances. These tolerances never establish a
physical milestone or resolve an unsafe sign.

Audit whole-run planner work against the shared four graph / 384 query allowance
and actual charged evaluator work. Hash the executable, source commit, summaries
and all raw output. Use focused lifecycle/mutation tests, the existing AI and
Python suites, formatting, strict AI Clippy, independent review before and after
physics, and exact-head CI. No ordinary-policy or device performance claim is
made by these observational desktop traces.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/probe-capture-followthrough.py \
  --reference target/capture-flag-survey/landing-choice-v1 \
  --source-reference target/capture-flag-survey/local-composition-v1 \
  --out target/capture-flag-survey/capture-followthrough-v1
```

Reference summary SHA-256:
`3af41999d1a280bef54838ace5be018cd505ef5607adf414428c548e06a73e60`.
Source-local summary SHA-256:
`ba33e865b39eb887671cf2675fe682f375694f88c64001273e4ed86a41d2ad10`.
The previous executable is retained locally as
`target/capture-flag-survey/surface-mission-soak-2796bb2`, SHA-256
`c52acdf36943c6a36c16615efa2992783a12171984d70d452e4750bd53515000`.

## Results: 27 September 2026

Runtime, runner and plan were frozen at `f1781c0`, with a clean worktree. All
20 runs and their audits pass without runtime or plan changes after outcomes.
The executable SHA-256 is
`29effd6a1078c39e823a912deee5e0155fcb390dc13f0a53feb94cf145c2b9dc`.
The raw summary is
`target/capture-flag-survey/capture-followthrough-v1/summary.json`, SHA-256
`d76b29911b49e61279661e75e5d8f85252e95213011594d36c0890fffbcf6475`.
[The tracked projection](data/capture-followthrough-v1.json) retains the complete
plan, denominator, source bindings, phase results and raw hashes.

**Fourteen first attempts complete the entire physical loop.** They take
20.800–26.133 seconds from choice through departure, median 22.950 seconds.
Each has an actual landed state, exit, claim progress, new ownership, boarding
and native departure. Touchdown offsets from the selected pose range from
0.011 to 2.052 world units, median 0.666. The site ID alone would have hidden
that variation.

**Six observations stop on actual terrain revision changes before landing.**
All six occur in the asteroid-pressure conditions; each has a recorded asteroid
impact editing the original destination on the preceding physics tick. The
endpoint ticks are 327, 283, 466, 417, 1044 and 884, after 2.467–13.217 seconds
of local observation. These are interrupted source-bound attempts, not observed
bot deaths or proof that subsequent replanning fails. The probe deliberately
stops before following a replacement attempt. No native retry, task replacement,
recovery, unwitnessed milestone or horizon/match censor occurs in this corpus;
those boundaries have focused tests.

**Only four current references are numeric.** They are the four correlated
historical-world-1 snapshots, all completing. Their fixed prediction is 24.766667
seconds; actual totals are 24.883, 25.183, 22.983 and 23.833 seconds. Mean absolute
total error is 0.8125 seconds, with maximum absolute error 1.7833 seconds. Landing
accounts for most variation: its mean absolute error is 0.8000 seconds, maximum
1.8167 seconds. Four snapshots from one world are not an independent validation
set or evidence to recalibrate the model.

The other **16 references remain explicitly unknown**, including ten completed
physical loops. All 20 selected sites have two clear boarding hatches and current
site geometry. The difference is the existing reference's all-cover gate:
the four historical sources have all three cover booleans true; the other 16
have all three false at the chosen site. These booleans describe cover from the
opponent at ground/approach/departure heights, not physical standing support.
All 20 native choices are unexposed under the playing selector's immediate-threat
rule. Its committed unexposed approach can choose a physically viable site
without the reference's all-cover requirement. The unknown is therefore a real
admission mismatch, not a missing landing or hatch. We preserve that distinction
instead of copying the historical estimate onto the new choice.

Observed phases across the 14 complete loops:

| Phase | Minimum seconds | Median seconds | Maximum seconds |
| --- | ---: | ---: | ---: |
| Choice → landed | 13.867 | 15.917 | 19.267 |
| Landed → exited | 0.017 | 0.017 | 0.017 |
| Exited → claim started | 0.100 | 0.100 | 0.133 |
| Claim started → claimed | 2.983 | 2.983 | 3.833 |
| Claimed → boarded | 0.033 | 0.033 | 0.033 |
| Boarded → departed | 3.667 | 3.792 | 4.667 |

The two longer quiet holdout seat-1 claims contain two actual progress resets
on `need_settle` observations: ticks 1037/1068 in holdout world 0 and 1007/1038
in holdout world 1. The actor remains supported and balanced. Keeping the clock
from the first real raising progress includes those pauses correctly; restarting
it after the reset would hide part of the physical claim cost.

All **48,940 archived controller/observation prefix rows** remain byte-identical
through the old first-choice endpoints (398,322,559 decompressed bytes). Both
players' sensor counters and stage calls, transfer/acquisition reports, upstream
outputs and work allocations match through that boundary. The full new traces
contain **92,958 controller rows**, the same number of sensor rows, and
841,620,742 decompressed bytes. The runs execute 46,459 physical ticks, 12.91
simulated minutes including prefixes; 22,009 of those ticks (6.11 minutes) are
new local continuation. All 220 new raw files have recorded hashes.

Whole-run reconciliation accounts for 151,135 live queries, 89,549 evaluator
graph operations, 3,803 flag-survey graph operations and 7,543 queries, and 552
shadow graph operations. Combined per-tick maxima remain four graph operations
and 126 queries, within the unchanged four/384 allowance. The observer adds zero
world queries or playing planner work; diagnostic construction and trace IO are
outside live fuel and do not establish device performance.

Validation includes all 210 AI library tests, the full AI integration/example
suite (including 14 mission-harness tests), 425 Python tests, formatting and
strict all-target/all-feature AI Clippy. Independent pre-run review corrected
advisory hatch blockage and retained sun avoidance being treated as terminal,
delayed-first-choice clock handling, partial touchdown retention, and complete
suffix work reconciliation before the frozen study.


The independent post-run reviewer imports neither the production runner nor its
analysis helpers. It verifies **220 new, 220 archived and 156 source-corpus file
hashes**, reconstructs the complete denominator, all controller and sensor rows,
exact old prefixes, all-consumer budgets, source estimate admission and physical
milestones. It separately confirms the six matching asteroid edits, both pairs
of claim resets, all cover/hatch observations, durations, offsets, error summaries
and tracked projection. No findings remain. The audit is embedded in the data
record; raw report
`target/capture-flag-survey/capture-followthrough-post-audit.json` has SHA-256
`5243b90f469784404b4f1f1e2d2ee86b2ae0d394b4050c85cfc50f3666bfe305`.
Its sibling `.py` script has SHA-256
`81dcc2eb129c90ea538b53e3d15bccd5fd5d0911fca56798f880f3992cd04262`.

## Next boundary

The next bounded experiment should separate physical neutral-capture evidence
from combat-cover admission **in an observational reference**, preserving the
native site's actual threat/solar context and retaining explicit unsupported
domains. Replay this same denominator to determine whether those 16 unknowns
can be priced honestly. Do not remove shelter requirements from exposed or
enemy-flag planning or change playing destination selection based on this study.
Changed material also requires a fresh attempt and a fresh reference; the six
interrupted snapshots must not silently survive an impact. Current-state
validation, explicit enemy-flag admission and mission value remain prerequisites
for the broader planning goal. Deployment remains paused.

The follow-up plan is [neutral timing with separate combat cover](capture-neutral-timing.md).
