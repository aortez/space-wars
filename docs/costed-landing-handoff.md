# Costed landing handoff experiment

This follows [v14's behavior study](capture-survey-value.md), which recorded
six completed directed switches but no improvement in fresh matches. Every
accepted switch's predicted landing bearing differed from the eventual native
landing. One enemy trip underestimated departure by 37 seconds.

V15 (`material_mission_v15`, **Landing plan bot v15**) carries the selected
candidate's original site and evidence identity into arrival. V14 remains the
predecessor. Both retain the same evidence admission, value weights, switch
margin, transfer reference and operation allowance. The evaluator model remains
`capture_mission_survey_value_v1`; the separate v15 policy/sensor identity names
the execution change. V15 is headless, with no device or UI default change.

## Contract

The historical reference nominates a bearing, never geometry or a ground route.
The existing required-site sensor request and native selector must acquire fresh
landing, hatch, objective-route and solar evidence. Acquisition gets at most
120 ticks after the normal arrival handoff, within the original evidence's
1,800-tick lifetime. Missing queries or route work do not extend that deadline.
Once accepted, the ordinary native approach owns the site and the constraint
is released. A refusal or retry releases it without resetting the capture task.

While unexposed, the native cover rule still applies. An exposed preference
requires complete cover because constraining the site would otherwise exclude
the native ranker's sheltered alternatives. One unsafe solar direction does
not reject a site whose other direction is awaiting route evidence.

Tracking continues through a native accepted touchdown and physical departure.
A bare `Landed` state does not establish usable hatch or actual ground access.
Terrain/flag identity changes, expiry before acquisition, recovery, pursuit,
solar escape, native retries and different touchdown positions invalidate the
reference. After landing, this pilot's physical lowering/raising/ownership
transitions are expected; an unrelated claimant or changed enemy flag is not.
The old switch forecast remains frozen. Subsequent ready/pending comparisons
lose the refused journey's cost; only fresh native-selected local evidence may
replace it. A later visit can reconsider the same bearing.

Matching a bearing and native touchdown does not prove the same walking route,
arrival duration or future survival. These estimates remain conditional and
uncalibrated for exposure/acquisition. This experiment changes no cost constants.

The handoff adds no world queries or physics steps itself. Targeted native
sensing can change synchronous sensor work; it is outside the shared remote /
evaluation / flag-survey allowance of 4 graph operations and 384 queries. No
whole-bot CPU or Pi performance claim follows.

## Development checks

Two initial owned-base replays (P1 bearings 0.8 and 1.2) rejected the reference
for current cover. They are retained under
`target/costed-landing-handoff/exploration-v1`, including their ordinary fallback
outcomes. Review narrowed cover refusal to exposed flight, revoked ongoing cost
reports after refusal, retained tracking past blocked touchdown, and separated
one rejected solar direction from rejection of a whole site. These are development
records, not held-out outcomes. A mixed-solar unit fixture initially made both
directions unsafe; it uses the existing selector's known mixed-safety geometry.

The three `exploration-v2` replays retain a positive original-destination case
(P1 bearing 0.8: accepted tick 498, native touchdown 1,801, departure 2,211)
and both owned-base refusals. The latter initially accepted their site, then
released it as cover became unsafe, and completed through ordinary fallback.
The new physical regression test covers acquisition, touchdown, claim, boarding
and departure. Independent review also checked adversarial audit mutations:
missing milestone observations, stale acquisition, changed source material,
different touchdown site and stale post-refusal cost publication are rejected.

## Frozen comparison plan

Freeze implementation, tests, this plan and the runner before execution:

- All 32 directed cases: original destination and owned-base fixtures, both seats,
  bearings 0.0/0.4/0.8/1.2, v14 and v15, 180 seconds each. Keep every refusal,
  failed/unfinished trip and slowdown.
- The known seed 186767996776005237 regression with v14 and v15 in seat two
  against v10, combat enabled, no asteroids, normal ten-minute deadline.
- Twelve finished-match smoke cases: two fresh SHA-256-derived worlds from
  `costed-landing-handoff-v1:{0..1}`, no asteroids / three-second asteroids,
  v14/v14 controls and v15 in each seat against v14. Rotate execution order.
  These are two independent worlds and correlated variants, not a strength study.

Recheck every directed/regression v14 run against the archived v14 study:
exact evaluator bytes, mission telemetry and recorded physical outcomes.
Retain every case, command, source/binary/artifact hash, original switch forecast,
visit, result and shared-work audit. Complete compact action traces must match
before the first switch; pairs with no switches must retain physical outcomes.

`landing-handoffs.jsonl` records changes in handoff state or native site, with
the existing observation and mission telemetry. It performs no new queries.
Independently bind each handoff to its accepted switch source and validate native
acceptance/touchdown/completion. After invalidation, any new numeric current-trip
reference must use fresh evidence for the native selected site. Report progress
with the retained direct transfer-range measure and its phase limitations.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-landing-handoff.py \
  --predecessor target/flag-value-behavior/frozen-v1 \
  --out target/costed-landing-handoff/frozen-v1
```

Use a clean checkout and new output directory. The runner binds the archived
predecessor summary and checks its input hashes. Publish the full outcomes and
a retain/promote decision after independent review. Leave #142 open unless
useful general decisions and regressions actually support promotion.


## Results at `ea0acc3`

All **46 runs / 25 comparisons** completed on the first frozen execution:
**657,559 physical ticks / 182.66 simulated minutes**. All physics checks,
flag-publication joins, handoff joins and per-tick shared-work audits passed.
The largest combined allocation was **4 graph operations / 160 queries**,
within 4/384. All **17** directed/regression v14 replays matched the predecessor's
exact evaluator bytes, mission telemetry and recorded physical outcomes. Every
paired action/range/ownership prefix check passed.

The [complete summary](data/costed-landing-handoff-v1.json) is an unchanged copy
of the raw summary, SHA-256
`03e2ca83139228fcb78fe998df4afa2f3c4fcc4e671a5c08d8080800e8813f3c`.
It retains the full plan, commands, accepted switch forecasts, exact visits,
prediction attempts, unknowns, work/progress audits, outcomes and artifact hashes.
Full reports, native observations, timing records and traces remain under
`target/costed-landing-handoff/frozen-v1`. Timing is instrumented desktop data;
an unrelated build ran concurrently, so this is not a CPU speed comparison.

### Directed behavior

V15 made the same **six destination switches** as v14. All six switched visits
physically captured, boarded and departed, but only **four** executed the original
landing reference through departure. These were the neutral destinations in
both seats at bearings 0.8 and 1.2. Their native selected site matched the source
site; observed touchdown offsets were 0.89–1.79 units, within the existing
10-unit validation envelope. They were **slower** than v14:

| Case | Handoff result | First claim change | First departure change | All planets owned change |
| --- | --- | ---: | ---: | ---: |
| Neutral, P1, 0.8 | Completed | +3.57 s | +3.55 s | +4.48 s |
| Neutral, P1, 1.2 | Completed | +3.63 s | +3.63 s | +6.50 s |
| Neutral, P2, 0.8 | Completed | +2.02 s | +1.98 s | +2.75 s |
| Neutral, P2, 1.2 | Completed | +1.65 s | +1.63 s | +3.22 s |
| Enemy, P1, 0.8 | Cover refusal; fallback completed | −0.45 s | +0.23 s | −0.83 s |
| Enemy, P1, 1.2 | Cover refusal; fallback completed | +5.18 s | +9.48 s | +10.03 s |

Changes are v15 minus v14; positive means later. The other ten directed pairs
remained unchanged. Both bearing-zero original-destination cases still made no
capture; failures and incomplete trips are retained.

The four completed reference trips had whole-trip forecast errors from −1.05 to
+0.59 seconds, versus +1.56 to +4.22 seconds for v14. That closer agreement came
with slower execution; it is not evidence of better site selection or a general
calibration result. No constants were fitted and all four share a small fixture
family. Matching a site does not establish an identical walking route.

Both enemy references were initially accepted, then invalidated when exposure
made cover unsuitable. Their original departure forecasts remained frozen and
underestimated the eventual fallback trips by **37.31 s** and **9.18 s**. Those
fallback completions do not count as executing the original landing plan.
After refusal, **194** reports left the current-trip cost unknown and
**410** reports used fresh native-selected local evidence. These are repeated
reports, not independent decisions. The original remote costs were not readmitted
for those visits.

The four neutral cases' longest transfer interval without a two-unit range gain
increased by 3–46 ticks (0.05–0.77 s); none exceeded ten seconds. Enemy-case
transfer progress remained unchanged. This metric excludes landing, walking,
recovery and combat, so it cannot explain the full trip regressions.

### Fresh matches and regression

All **12 fresh matches** finished. V15 made **zero switches**, and all eight
candidate/control comparisons retained identical recorded physical outcomes
and complete action/range/ownership traces. Candidate seats won four and lost
four, exactly matching those seats in the controls. Captures, departures,
recoveries, pilot losses and the progress measure were unchanged. These are two
fresh worlds with correlated seat/asteroid variants, not eight independent
strength samples. Longest transfer intervals without a two-unit range gain
reached 664 ticks (11.07 s) in both policies; the handoff did not resolve them.

The recorded regression also remained identical: the candidate seat won,
completed five sorties and made no destination switch. Fresh matches therefore
establish unchanged behavior with the handoff inactive, not improved strategic
decisions. The two directed enemy refusals exercise handoff fallback.

### Decision and next boundary

**Retain v15 as headless experimental; leave defaults unchanged and #142 open.**
The useful result is an observable, safely revocable connection between a
forecast site and ordinary native arrival. Pinning that site by itself made
four neutral trips slower and one enemy fallback materially slower.

The next behavior change should address **which arrival site is worth choosing**:
compare the planned site's acquisition/approach and exposure assumptions with
the alternatives the native ranker would choose, using the retained refusal
and phase records. First explain the neutral slowdown and enemy cover refusal;
then test a bounded site-ranking or admission change. Keep v14 and this frozen
v15 corpus as controls. This does not justify wider search, new value weights,
or promoting a new default yet.

Local validation: 286 AI unit tests, six physical destination integration tests,
31 soak-harness tests and 573 Python tests passed. Formatting and strict Clippy
for the changed AI library/example/integration targets passed. Independent review
covered runtime safety and the source/native evidence joins before freezing.
