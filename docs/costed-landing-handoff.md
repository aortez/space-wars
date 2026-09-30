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
