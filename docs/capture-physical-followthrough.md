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
