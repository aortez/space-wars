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

## Results: 27 September 2026

Runtime, runner and plan were frozen at clean commit `3eaaa6f`. All 52 planned
runs completed with no changes to the runtime or study after outcomes. The
executable SHA-256 is
`e06625f1be4fd9eb6fef3fcd3a0825651e23ab03dd8203e1b52dbb63d4ed9ed2`.
It is retained locally as `target/capture-flag-survey/surface-mission-soak-3eaaa6f`.
The raw summary SHA-256 is
`805c2bc5d090a06b87b11e61cb8904cefadde1c9a101bddf08d614592e4a9333`.
The [tracked record](data/neutral-capture-timing-v1.json) preserves plans,
sources, both estimates, outcomes, phase errors, commands and raw file hashes.
It is a compact projection: chosen-site geometry is retained; full candidate
lists and direction ledgers remain in the hashed raw reports and traces.

| Result | Historical 20 | Fresh 16 on-arms |
| --- | ---: | ---: |
| Capture and departure completed | 14 | 8 |
| Material/rule interruption | 6 | 6 |
| Foreign claim interruption | 0 | 1 |
| No native choice within 60 seconds | 0 | 1 |
| Existing covered reference numeric | 4 | 4 |
| New unexposed timing numeric | 20 | 12 |
| New timing unknown due to source exposure | 0 | 3 |

All 16 previously unknown historical references gain a separate neutral timing
record. Ten of those complete; six remain material interruptions, not successful
cost samples. The four existing covered references and every historical physical
trajectory remain identical to the prior executable. No historical trip becomes
exposed during its observed attempt.

The new record is **not a superset** of the covered reference's domain. In fresh
worlds, only one source has both numeric records; eleven have only the new
record, three exposed sources have only the old covered record, and one has no
choice. All seven fresh completions with a new numeric prediction previously
had an unknown covered estimate. The eighth completion began exposed and remains
unknown under the new model, even though it has an old covered reference.
Do not silently combine these two admission rules.

The four seeds are 7688236076919084596, 14157161121037389508,
5243208827122263174 and 7378283519567069983. Each retains all four seat/pressure
slots and both diagnostic arms. All 16 pairs have identical complete physical
trajectories, reports (excluding the added timing record and wall times), sensor
work, upstream bytes and planner allocations.

### Timing accuracy and phase cancellation

Errors below mean observed seconds minus the sum of predicted phase seconds.
They include only completed trips with numeric new references. Every prediction
still uses the unchanged approximately 24.77-second sum; nothing was fitted.

| Completed-trip errors | n | Mean error | Mean absolute error | Maximum absolute error |
| --- | ---: | ---: | ---: | ---: |
| Historical sources | 14 | -1.527 s | 1.799 s | 3.967 s |
| Fresh sources | 7 | -0.576 s | 2.671 s | 3.817 s |
| Fresh, never observed exposed later | 4 | +0.821 s | 2.846 s | 3.683 s |
| Fresh, observed exposed later | 3 | -2.439 s | 2.439 s | 3.817 s |

The latter rows describe context, not an effect of exposure. They are tiny,
correlated samples. Four fresh initially unexposed attempts become exposed later:
three complete and one is interrupted by material change. Three other attempts
are already exposed at source; the new reference stays unknown for them.

Total error can hide larger phase errors. In `fresh1-asteroids0-s0-on`, landing
takes 11.483333 seconds versus 17.866667 predicted; claiming takes 7.083333
versus 3.016667. The -6.383333-second landing error partly cancels the
+4.066667-second claim error, yielding a -2.100000-second total error.
Nine `need_settle` rows reset positive claim progress at ticks 782, 812, 842,
872, 902, 932, 962, 992 and 1022 while the spaceling remains balanced and
supported by planet 1. The retained first claim clock is 777; capture is 1202.
First later exposure is 1272, after boarding at 1204, so this trace does not
support blaming those claim resets on observed immediate exposure.

Across the seven fresh numeric completions, landing mean absolute error is
3.264 seconds (maximum 6.383), claim mean absolute error 0.610 (maximum 4.067),
and departure mean absolute error 0.060 (maximum 0.167). This supports keeping
phase diagnostics; it does not validate one scalar as a precise mission score.

### Interruptions and observation limits

All six fresh material interruptions have a destination asteroid terrain edit
on the preceding physics tick. They occur before landing. Two seats in fresh
world 2 observe the same edit, so these are not six independent impacts:

| Fresh slot | Choice tick | Endpoint | Planet | Impact tick |
| --- | ---: | ---: | ---: | ---: |
| world 0, pressure 3, seat 0 | 1482 | 1694 | 2 | 1693 |
| world 1, pressure 3, seat 0 | 80 | 480 | 1 | 479 |
| world 2, pressure 3, seat 0 | 1383 | 1411 | 1 | 1410 |
| world 2, pressure 3, seat 1 | 740 | 1411 | 1 | 1410 |
| world 3, pressure 3, seat 0 | 1625 | 1994 | 1 | 1993 |
| world 3, pressure 3, seat 1 | 2295 | 2329 | 0 | 2328 |

In fresh world 2 quiet seat 0, player 2 starts raising the destination flag at
tick 1727, interrupting player 1's still-airborne attempt. In fresh world 1
pressure 3 seat 1, no native choice appears by tick 3600; the coordinator is
still transferring to planet 0. That slot says nothing about eventual success
after the frozen 60-second source window. No source or completion was invented.

All 52 runs total 127,005 executed ticks (35.279167 simulated minutes), 254,114
controller/sensor rows and 2,124,334,122 decompressed trace bytes. Historical
replay and fresh-pair parity cover 173,536 controller rows. Raw archives retain
540 files plus 52 run logs. The shared budget reconciles to zero live graph,
568,755 live queries, 276,377 evaluator graph, 9,251 flag graph/15,601 flag queries
and 1,152 shadow graph work. Maximum combined per-tick work remains four graph
and 126 queries, within the four/384 allowance. The timing record adds no query
or planner job; its diagnostic CPU time is outside fuel and is not a Pi benchmark.

Local validation passes 216 AI library tests, 17 mission-harness tests, 437 Python
tests, formatting and strict all-target/all-feature AI Clippy. Independent review
before the freeze corrected CLI defaults, version/nonfinite-domain admission,
first-choice/terminal audit clocks and selected-solar audit coverage.

The independent post-run audit imports no production analysis helpers. It
verifies all 52 runs, 36 complete parity comparisons, 540 new/220 prior/156
source-corpus files and 52 logs, then independently reconstructs sources,
domains, phase clocks, context, budgets and both aggregates. The compact
projection and written tables/claim-reset/impact examples reconcile. No blockers
remain. Its report is embedded in the tracked record and retained as
`target/capture-flag-survey/neutral-timing-post-audit.json`, SHA-256
`1191ca1361463a962913f640425af4e2571ccf33bb0ca50c7711760c34a1f59a`.
The sibling audit script SHA-256 is
`43dd8015ba82550b4fc679b85ba22c207ec01733e0e8cfb6e2acd1a278ee432f`.
Exposure counts include terminal observations: fresh world 2 quiet seat 1 has
279 later-exposed observations but 278 executed rows, because the departure
endpoint command is unexecuted. The audit retains both counts.

## Next boundary

This slice supports separating successful neutral-trip timing from combat cover
as observational evidence. It does not authorize remote landing sites, estimate
survival or change playing destination choice. Keep exposed sources, foreign
flags and future terrain changes explicit.

The next bounded step is to evaluate whether this new record can join the
existing transfer/local comparison under the same actor, visit, material, site,
solar and threat guards, while retaining the covered record's different domain.
Before allowing those comparisons to affect control, keep arrival and claim
errors separate and test current-state invalidation; a low total error can be
accidental cancellation. To revisit the settling case, use the frozen command
and raw trace for `fresh1-asteroids0-s0-on`, ticks 769–1204, with the reset clocks
above. To study the no-choice case, use `fresh1-asteroids3-s1-on`; extending its
source window is a new experiment, not an amendment to this denominator.
Deployment remains paused.
