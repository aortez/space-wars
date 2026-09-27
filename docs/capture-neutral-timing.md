# Neutral capture timing with combat cover recorded separately

The [physical follow-through study](capture-physical-followthrough.md) completed
14 of 20 first attempts. Ten completions had no time reference solely because
their sites lacked combat cover, despite a current material survey and usable
boarding hatches. All 20 choices were unexposed under the native immediate-threat
test. This slice measures that missing timing domain without changing controls.

## Frozen design and study plan

`neutral_capture_timing_v1` is a stateless record made alongside a validated
native `selected_site` event. It reuses the existing six successful-trip medians:
17.866667 seconds landing, 1/60 exit, 4/60 outbound, 3+1/60 claim, 2/60 boarding,
and 3.766667 departure. These are conditional durations, not success odds,
arrival-specific predictions, remaining-time updates or route scores.

The old covered `LocalCostReference` remains unchanged. The new record is never
cached, submitted to a planner or used to choose destinations. At source it binds
actor, ship, spaceling, policy, mission visit/attempt, site/revision, current
planet/ship poses, gravity, claim rules, hatch and native direction/solar plan.
Only current v13 committed neutral choices aboard an available ship are admitted.
Reject stale/progressed choices, missing/invalid geometry or versions, absent
boarding hatches, unsafe solar plans, foreign/in-progress claims and exposure.
One clear boarding hatch suffices. False and missing cover remain distinct data.

Exposure means the native combat target is within 300 units and not ground
occluded. This is a **source-only** condition. Audit the first later exposed tick
and exposed-row count separately, without censoring or restarting an attempt
because of exposure. Existing material/rule changes, retries, recovery/loss and
foreign claims still interrupt the physical attempt; contacts and nonfatal
damage alone do not. Existing milestone witnesses and censors remain unchanged.

Freeze runtime, runner, tests and this plan in a clean commit before new study
outcomes. No constants, thresholds, worlds or sources are chosen from outcomes.

1. Repeat **all 20** previous nominated alternatives, retaining their original
   62-candidate denominator and every old source, control and horizon setting.
   Enable only the new timing diagnostic. Require full controller/observation
   byte parity through each physical endpoint against `capture-followthrough-v1`,
   plus identical old reports, sensors, upstream output and planner charges.
   The old four covered references must remain unchanged.
2. Add **four fresh worlds × both seats × quiet/3-second asteroid pressure**:
   16 slots, each with timing **off/on**. Derive world `i` from the first eight
   SHA-256 bytes of `neutral-capture-timing-v1/fresh/{i}`, interpreted little
   endian, for i=0..3. Verify seeds differ from the previous corpus before physics.
   Both arms run the same v13/v13 duel and read-only first-attempt observer.
   There is no nomination, pursuit deferral or forced site in these fresh runs.
3. Select the first current native choice in **[0, 3600)** ticks, irrespective of
   ownership, exposure or timing admission. Retain unsupported/no-choice slots;
   never search onward for a supported source. Use that actual site as the
   comparison's self-reference. Follow for up to 120 seconds from choice; the
   runner allows 181 seconds including endpoint observation. Match rules and
   match time limits remain unchanged. A finish cannot manufacture a source.

This is **52 new physical runs**: 20 historical replays and 32 fresh paired runs.
The 16 fresh on-arms are the fresh sample; paired off-arms are control checks,
not extra independent observations. Same-world seats/pressure cases and the
historical source snapshots are correlated engineering cases, not strength data.

## Validation

Audit all source identities, admission/unknown reasons and exact old/new phase
constants against raw observations. Reconstruct physical milestones and attempt
interruptions from dense traces using the existing independent Python observer.
The old20 retain their full ranker reconstruction. For fresh sources separately
reconstruct the selected direction's geometry/solar clearance; this is not a
full independent rerank of every fresh candidate. Retain near-zero sign ambiguity.

Whole traces include both players and sensor call/count parity. Compare complete
reports excluding only the added `neutral_timing` field and diagnostic wall
times; compare every upstream byte and planner allocation. Reconcile all live,
evaluator, flag-survey and shadow work against four graph/384 query fuel per tick.
Phase errors require completed phase witnesses. Interrupted/censored elapsed
time never becomes a successful duration. Report old20 and fresh16 separately,
including later exposure and every unsupported/unknown/censored case.

Use the prior frozen tolerances: 0.000003 seconds per f32 median, 0.00001 seconds
for the summed f32 total, 0.002 world units per touchdown coordinate, 0.003 for
touchdown distance; solar/score tolerances from the native-choice audit. The
opponent-distance reconstruction tolerance is 0.001; it never resolves exposure
at a threshold. Hash source/executable/plan and all raw artifacts. Run focused
mutation/lifecycle tests, AI and Python tests, formatting/strict Clippy,
independent review before and after outcomes, and exact-head CI. No deployment.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-neutral-capture-timing.py \
  --reference target/capture-flag-survey/capture-followthrough-v1 \
  --out target/capture-flag-survey/neutral-timing-v1
```

Previous raw summary SHA-256:
`d76b29911b49e61279661e75e5d8f85252e95213011594d36c0890fffbcf6475`.
Previous executable retained as `surface-mission-soak-f1781c0`, SHA-256
`29effd6a1078c39e823a912deee5e0155fcb390dc13f0a53feb94cf145c2b9dc`.
