# Combat firing opportunities (#155)

**Current decision: retain the existing defaults.** The independent four-world
confirmation below fails the predeclared outcome/survival screen. The opt-in
firing experiment remains in [draft PR #206](https://github.com/aortez/space-wars/pull/206)
for investigation; no device qualification or default rollout follows this result.

The integration baseline is merged main `44623ecc98d5a689d2ca94cfa402fc7be34c8255`:
the #204 durability rules and #201 physics work together. The local client builds
and all 456 existing AI unit tests pass. This investigation uses the production
Value v13 / Recovery v1 host, its shared planning quota, 15s/4s combat breaks,
Weak Round/Angular dirt with slumping, and native 600-second match endings.
Recovery v1 uses `terrain_recovery_supported_hatch_ordinary_offsets_v1`.

## Retained diagnosis

Four discovery games reuse seed `6365347488789871033` from the durability work,
with both dirt shapes, both Recovery v1 seats and no random asteroids. Neither
health nor controller state is edited during these matches. Every tick records
the actual controls, ship/target motion, visibility, readiness and supply, both
aiming angles, mission/flight safety states, breaks, clearance and native hits.

At tick **4990**, P2 in the Round game is actively engaging an unoccluded pod
123.205 units away. Its current heading error is **-0.00542 radians**, but the
missile intercept error is **0.14223**. The shared 0.08-radian firing gate rejects
both weapons, although the laser is ready. Five steps replayed on copies of the
same physical state, changing only the laser request and preserving the control
run's subsequent inputs, produce **five additional native laser-hit ticks**.
The primary game remains unchanged. These controlled probes include the beam's
50-unit-per-tick growth; target-center visibility alone is not called a hit.

The first four games contain no direct climb → engage → climb sequence with an
engagement shorter than two seconds. This does not disprove the reported
oscillation in other worlds. Clearance and avoidance behavior remain unchanged.
Existing pursuit-climb laser experiments remain disabled; their earlier mixed
results are documented in [the separate comparison](pursuit-climb-laser-results.md).

## Candidate and frozen comparison

`material_direct_laser_window_v1` is an opt-in experiment. It uses the current
target bearing for laser eligibility while retaining the missile intercept for
steering and cannon fire. It changes neither the 0.08-radian tolerance nor the
250-unit laser range. All safety, mission priorities and deliberate breaks run
before this check. Standard bot selections keep their existing behavior.

Before inspecting fresh results, the comparison is fixed as follows:

- Seeds **6603551523184921372** and **3297281283817221183**, derived from the first
  eight SHA-256 bytes of `combat-155-heldout-v1:0` and `:1` in little-endian order.
- Both dirt shapes, asteroid intervals 0/8 seconds, both evaluated seats:
  **16 control/candidate pairs**. Only the Recovery v1 seat receives the candidate
  firing gate. Its recovery profile and the opponent remain identical per pair.
- Keep audits, outcome, survival, ship losses, recovery/mission completions,
  recorded damage/contact provenance, weapon hits, energy readiness, direct-aim
  opportunities and clearance transitions. Report opportunity counts on each
  actual trajectory, not as interchangeable denominators.
- Check the first different input: it must only change the evaluated laser
  request. Separately replay discovery controls with probing disabled and demand
  identical native actions and scenario hashes to establish diagnostic parity.
- Advance the candidate only if it passes physical/work audits and the isolated
  gate checks, improves the demonstrated firing defect, and does not reduce
  total match points or increase evaluated-seat pilot deaths/ship losses.
  Preserve every lost win and failed committed mission for review. Completions
  unavailable after an earlier native victory are censored, not failed missions.
  A failure means retain the current default, without changing this screen.

This is two fresh world clusters, not evidence of a general win-rate increase.
Desktop timings exclude diagnostic probes and trace serialization. Current
device cost and live playtesting are required before a default rollout.

The runner copies/hashes its executable and preserves source changes, case
specifications, commands, per-tick inputs and diagnostic results under a new
output directory:

```sh
cargo +1.89.0 test --locked --profile ci -p spacewars-ai \
  --features sensor-profile --lib --no-run
python3 tools/run-combat-opportunities.py target/combat-155/control-v1 \
  --binary target/ci/deps/spacewars_ai-4e4b7bd406488e9d \
  --seeds 6603551523184921372 3297281283817221183
python3 tools/run-combat-opportunities.py target/combat-155/candidate-v1 \
  --binary target/ci/deps/spacewars_ai-4e4b7bd406488e9d \
  --seeds 6603551523184921372 3297281283817221183 --direct-laser-window
```

Use the executable path printed by the build; Cargo's suffix can differ across
hosts. All full matches run native controllers with no resets or input overrides;
the optional five-step probes are isolated diagnostics, explicitly marked in
their own file. No device is changed by this runner.

## Completed initial comparison

All **32 games / 16 pairs** passed native physics/material and shared planning-work
audits. The candidate passes the predeclared screen and advances to review.
The current bot selections remain unchanged. These are two fresh world clusters;
the result does not establish a general win rate or device performance.

| Evaluated Recovery v1 seat | Control | Direct laser window |
| --- | ---: | ---: |
| Wins | 8 | 10 |
| Ship losses | 7 | 7 |
| Pilot deaths | 7 | 6 |
| Completed capture departures | 38 | 47 |
| Completed recoveries | 6 | 7 |
| Native laser-hit ticks | 4,598 | 7,074 |
| Eligible direct-aim windows taken / available | 7,862 / 9,478 | 13,908 / 13,908 |

An eligible window requires the present-bearing/range test, target visibility,
query and laser readiness, armed controls, active combat, no scheduled break, and no cannon
request suppressing the laser. It is an opportunity gate, not guaranteed damage:
the beam still has to grow and intersect the actual moving collision shape.
The waiting-for-a-site patrol delegates to the same combat pilot and is included.
Capture, recovery, clearance and scheduled-break refusals are reported separately.

There are **13 changed pairs** and three with identical complete native input and
scenario-hash streams. Every changed pair has identical preceding scenario hashes;
its first different input changes only the evaluated seat's laser request.
The four discovery controls also match all **102,815 ticks** of input, controller
decision hashes and scenario hashes with the isolated diagnostic probes disabled.

The three gained wins are first-world Round/P1/asteroids, first-world
Angular/P2/asteroids, and second-world Angular/P2/asteroids. One win is lost:
first-world Angular/P1/asteroids. Points by world are 4→5 and 4→5; by dirt shape
4→5 for each; without asteroids 4→4 and with asteroids 4→6. These aggregates
overlap and do not turn sixteen pairs into sixteen independent worlds.

Both match duration and opportunity counts change after the intervention.
Summed laser contacts are therefore not an equal-exposure accuracy comparison.
Recorded debris-contact ticks increase 274→324 across the evaluated seats while
their total elapsed ticks increase 283,656→349,651. Weapon energy averages across
the sixteen per-game means are 94.63%→93.92%; recharge ticks increase 600→1,259.
There is no separate flight-fuel pool in these observations. Recorded damage
amounts/source labels are retained per case, including terminal pilot hits;
simultaneous full-ship damage is attributed to the native last source for that
tick, so those labels are not a causal damage decomposition.

No pair records a direct climb→engage→climb cycle with an engagement shorter than
two seconds. The longest continuous climb is 567 ticks in control and 574 in the
candidate (9.45/9.57 seconds). This narrow screen does not resolve every form of
the reported oscillation, and no clearance policy is changed by this work.

### Retained counterexample and completion review

For seed `6603551523184921372`, Angular, eight-second asteroids, evaluated P1:

- The first added laser is at tick **3258**. Both runs retain capture departures
  at **2671** and **7095**, with identical arrival/departure events through them.
- Control wins at **9154** when P2 dies from an on-foot world-boundary impact.
- Candidate continues. A later flag trip is rejected for lack of a grounded
  hatch at **11808**; recovery completes at **13588**. Further approach-frame
  retries are retained, and P1 exits onto the surface at **17281**.
- P1 dies from a native missile hit while on foot at **19374**, losing the match.
  This is a real lost win. The first changed request identifies the intervention;
  it does not establish a causal chain from one beam to that much later death.

The only pair with fewer evaluated-seat departures is seed
`3297281283817221183`, Round, eight-second asteroids, P1: **2→1**, both wins.
Both complete their first departure at **16102**. Candidate pauses the later
transfer for a vulnerable opponent at **18623** and wins at **19056**. Control's
second departure is **20328**, after the candidate's native ending. That completion
is unavailable in the shorter winning match; the trace does not prove the
interrupted transfer would have completed if play continued.

## Validation and retained evidence

The final source passes **461 AI unit tests**, **21 physical combat/mission
integration tests**, four analysis tests, formatting, required Clippy and the
client build. Standard constructors leave the option disabled; both mission and
combat resets preserve an explicitly selected profile.
Merged main's [eight CI checks](https://github.com/aortez/space-wars/actions/runs/38169193384)
also pass. The candidate's draft PR has its own CI run, separate from those
merged-main checks.

The frozen comparison binary SHA-256 is
`d930f61b9520fb9a80e3dd477428076159f16c80c2181f5064f7a82fd329a89d`.
The [machine-readable report](data/combat-opportunities-v1.json) binds the source
snapshots, original plan, matrices, per-case results and retained trace hashes.
Raw evidence is under `target/combat-155/{discovery-v1,parity-v1,control-v1,candidate-v1}`.
The retained plan is the pre-run copy of this document in each matrix's
`untracked-source/docs/combat-opportunities.md`; later results do not overwrite it.

Desktop sensors + both decisions + planning measure mean/p95/p99/max
**0.202/0.118/4.464/14.601 ms** in control and
**0.198/0.121/4.507/13.728 ms** in the candidate. Timings exclude tracing, probes,
rendering and physics; the two trajectories differ and this is not a device-cost
comparison. The additional gate performs scalar math with no new physics queries.
Existing device measurements remain tied to their earlier binaries. Review,
independent-world coverage and current-device playtesting precede any rollout.

```sh
python3 tools/analyze-combat-opportunities.py target/combat-155/control-v1 \
  --candidate target/combat-155/candidate-v1 --out /tmp/combat-comparison.json
python3 -m unittest discover -s tools/tests -p test_combat_opportunities.py
```

## Review and independent confirmation

The local review found no blocking runtime defect in the opt-in change. It
tightened the analyzer to exclude unarmed controls despite a stale combat label,
check every action row after the first changed input, and reject a missing case
against the frozen matrix. Regression tests cover these evidence failures.
Reanalysis preserves every original comparison result; all five laser tests and
four analyzer tests pass. The confirmation uses the same binary SHA-256 recorded
above, so the gameplay candidate has not changed during review.

Before running further matches, freeze one confirmation batch:

- Four new world seeds, derived from the first eight SHA-256 bytes of
  `combat-155-confirmation-v1:0` through `:3` in little-endian order:
  **16775363816852070040**, **2178709192992772276**,
  **8780702509180422506**, **14910522168241314925**.
- Both Weak Round/Angular shapes with slumping, asteroid intervals 0/8 seconds,
  both evaluated seats: **32 pairs / 64 games**, each capped by the native
  600-second ending. Use the same frozen binary in both arms with probes off.
- Retain the candidate unchanged: only the evaluated Recovery v1 seat enables
  `material_direct_laser_window_v1`; keep recovery, health, opponent, planning
  quota and 15s/4s combat breaks fixed.
- Apply the original advance screen to this batch independently: physical/work
  audits pass, the demonstrated defect is exercised and eliminated, total points
  do not fall, and pilot deaths and ship losses do not rise. Report results by
  world and terrain/asteroid/seat strata as well as aggregate totals.
- Review every lost win and reduced completion count with the actual terminal
  time, failed committed missions and recovery state. Keep the earlier lost win.
  Earlier victories censor subsequent completion opportunities; they do not
  establish success of an unfinished mission.
- A failed screen retains the current defaults and stops rollout qualification.
  Do not alter the candidate or add seeds to obtain a passing result. A pass
  advances to device testing at 1× on `spacewars.local`, followed by restoring
  automatic Clock mode, before deciding on default selection.

The firing change is staged separately from the unresolved attack/avoidance
investigation in #155. This confirmation adds four independent worlds; the
conditions within each world remain correlated.

## Independent confirmation result: retain defaults

All **64 games / 32 pairs** completed and passed the native physical/material and
planning-work audits. The same frozen executable was used for both arms, with
the plan committed as `f8bbcda` before any confirmation match. No candidate code,
seed or acceptance criterion was changed after seeing the results.

| Evaluated Recovery v1 seat | Control | Direct laser window |
| --- | ---: | ---: |
| Wins | 16 | 14 |
| Ship losses | 16 | 17 |
| Pilot deaths | 10 | 12 |
| Completed capture departures | 81 | 79 |
| Completed recoveries | 15 | 15 |
| Native laser-hit ticks | 8,722 | 10,631 |
| Eligible direct-aim windows taken / available | 13,825 / 16,022 | 16,180 / 16,180 |

The candidate removes the measured declined-shot pattern, but fails all three
outcome/survival conditions. There are **three lost wins and one gained win**.
Twenty-three pairs change inputs; their first changed input is only the evaluated
laser request, with identical preceding physical-state hashes. The other nine
pairs have identical complete native input and physical-state streams.

| World seed | Control wins | Candidate wins |
| --- | ---: | ---: |
| 16775363816852070040 | 4 | 2 |
| 2178709192992772276 | 4 | 4 |
| 8780702509180422506 | 4 | 4 |
| 14910522168241314925 | 4 | 4 |

Each terrain shape scores 8→7, and the ordinary and asteroid strata each score
8→7. Evaluated P1 scores 7→6; P2 scores 9→8. These overlapping groups remain four
independent worlds. Pooling the earlier favorable batch to override this failed
confirmation would change the frozen decision rule.

Exposure still differs: total evaluated ticks are 687,687→718,231, recorded
debris-contact ticks 470→466, laser-recharge ticks 2,861→3,056, and the mean of the
32 per-game energy means is 94.93%→95.37%. More aggregate hits do not establish
better accuracy or explain the later deaths. Desktop timings are not device
qualification.

### Lost wins and mission review

The [confirmation report](data/combat-opportunities-confirmation-v1.json.gz)
retains every evaluated seat's mission events, arrived-then-replanned trips,
unfinished arrivals, final capture/recovery state and terminal pilot damage in
both arms. The longest event list uses 57 of 128 retained slots; none is truncated.
An interrupted trip is not automatically an implementation defect, and final
controller telemetry precedes the native terminal physics step.

| Lost win | First added laser | Control ending | Candidate ending |
| --- | ---: | --- | --- |
| First world, Round, asteroids, P2 | 7,680 | Recovers at 11,539; wins at 12,871 on opponent impact death | Pod lands at 12,407; pilot exits at 12,409 and dies to a missile at 13,403 during claim |
| First world, Angular, asteroids, P1 | 7,924 | Seven departures; wins at 34,397 on opponent impact death | Three departures, later capture timeouts and a recovery blocked for no measured route at 34,500; pilot dies from world impact at 34,696 |
| Second world, Round, no asteroids, P2 | 3,779 | Wins at 6,036 on opponent missile death | Later completes more trips and recovery, then dies to a missile at 20,455 while walking during capture |

The first differing request establishes the intervention, not a causal account
of a death thousands of ticks later. The original batch's lost win remains
retained above.

Four other pairs have fewer capture or recovery completions:

- **Second world, Round/asteroids/P1:** both win. Candidate ends at 19,520 with
  four departures and no ship losses. Control's fifth departure is at 30,723,
  and it loses two ships by 36,000. Later opportunities are censored; zero
  candidate recoveries does not indicate failed recovery without a ship loss.
  Its approach-frame replan at 19,334 is still retained.
- **Second world, Round/asteroids/P2:** candidate gains a win but fails recovery.
  Both lose their first ship at 4,202. Control recovers at 14,076; candidate
  blocks at 13,005 after exceeding the ground-traversal deadline and stays
  blocked until the opponent's impact death at 19,361. The later control recovery
  at 35,085 is censored, but early victory does not explain the earlier failure.
- **Fourth world, Round/asteroids/P1:** both win at the same 36,000-tick limit.
  Four departures match; control completes a fifth at 35,036. Candidate is still
  walking during capture, with progress last recorded at 35,995. It reports no
  task failure, but the completion deficit has no earlier-victory explanation.
- **Fourth world, Angular/asteroids/P1:** both win. Candidate ends at 9,150,
  before control's third departure at 10,476. The first two departures match;
  the later completion is unavailable after the candidate's earlier victory.

### Retained clearance cycle and next boundary

The new batch contains a direct short climb→engage→climb sequence in the second
world, Angular/asteroids/P2. Control climbs through 14,682, engages at
14,683–14,800, and climbs again at 14,801–15,402. Candidate's first engagement
ends at 14,767 and a second short engagement occurs at 15,403–15,514. This is one
world with an actionable transition window, not a diagnosis of all avoidance
behavior. Clearance policy remains unchanged.

The rollout decision is **retain default**, so no device settings, build or
automatic Clock mode were changed. Keep #206 as a draft and retain the failed
candidate without tuning this confirmation set. Further work should explain a
retained regression or the clearance cycle before selecting another intervention.

To recompute the paired metrics from the retained raw evidence:

```sh
python3 tools/analyze-combat-opportunities.py target/combat-155/confirmation-control-v1 \
  --candidate target/combat-155/confirmation-candidate-v1 \
  --out /tmp/combat-confirmation.json
```

The compressed report adds strata, mission-review notes and review receipts to
that analyzer output. Both matrix manifests bind source, executable and traces;
the committed `f8bbcda` plan preserves the criteria as they stood before the run.

## Lost-win diagnosis: shared energy and missile reload

The first confirmation lost win (seed `16775363816852070040`, Round, eight-second
asteroids, evaluated P2) is selected for a controlled diagnosis. Initial trace
inspection shows extra laser use draining the shared energy pool. Energy is paid
when an automatic missile reload **starts**, not when an already-loaded round
fires: each reload costs 25, regeneration is 10/second, and the laser draws
12/second. At 8,231 both bots fire a loaded missile, but only the original has
enough energy to start the next reload. At 8,351 that replacement can fire in the
original; the candidate is empty with no reload in progress. The first ship-motion
difference follows at 8,352. This suggests reload competition, without yet
establishing its role in the final loss.

Before running the diagnostic, fix its scope:

- Replay the complete original and candidate games, verifying every native input,
  controller-decision hash, physical-state hash and final snapshot against the
  retained references. Their published outcomes must remain unchanged.
- Fork the candidate at **8,230**, immediately before the current reload finishes.
  Preserve the whole physical world, both controllers, planning memory and clocks.
- Compare its unchanged continuation with one input-only intervention: suppress
  an otherwise requested laser when the cannon has zero loaded rounds, no reload
  in progress and less than the native 25-energy reload cost. Preserve every
  cannon request, flight control, opponent action and all native supply rules.
  Stop suppressing once the automatic loader starts; apply no energy injection,
  controller reset, terrain edit or recovery override.
- Continue both arms to the native match ending. Record reload starts, actual
  shots, damage, recovery and outcome; check physical/material and work bounds.
  This is one explanatory continuation, not another qualification set or a
  production policy. Do not adjust the fork or rule to recover this win.

The direct-laser candidate and the current defaults remain unchanged. The draft
PR's eight CI checks pass on `85003d1`; the new diagnostic requires its own build
and verification.

### Completed reload diagnosis

The diagnostic reproduces **all 26,274 reference ticks** across the original and
candidate games, including their inputs, decision hashes and physical-state
hashes. Both final snapshots, controller dumps and player-frame files match the
retained references byte for byte. The unchanged candidate and intervention also
start with identical fork snapshots, controllers and player frames.

This confirms a specific weapon-resource interaction:

1. At **8,231**, both versions launch the same loaded missile. The original
   starts its next reload with 57.57 energy available on the preceding tick; the
   direct-laser candidate has 14.57 and cannot pay the 25-energy reload cost.
2. At **8,351**, the original's next missile is loaded and launches. The candidate
   has no loaded round or active reload, despite having the same ship positions
   and a valid missile aim. It does not begin another reload until **8,567**.
3. The original's launch recoil first changes P2 motion at **8,352**; P1 motion
   first differs at **8,365**. Thus the first movement divergence follows a
   changed weapon action; steering and recovery policy were not edited.

The fixed input intervention suppresses **106** otherwise requested laser ticks,
in intervals **8,246–8,293** and **8,415–8,472**. Every changed action is verified
to be only P2's laser changing from on to off, under the stated empty-ammunition,
unpaid-reload and energy conditions. The first reload starts at **8,294**, 273
ticks earlier than the unchanged candidate, and a native missile launches at
**8,414**. Its recoil first changes P2 motion at **8,415**, and P1 motion first
differs at **8,427**. No energy, ammunition, cooldown or controller state is
injected.

| P2 result | Original | Direct laser | Direct laser + diagnostic reservation |
| --- | ---: | ---: | ---: |
| Next reload after 8,231 | Already started at 8,231 | 8,567 | 8,294 |
| Next missile launch after 8,231 | 8,351 | None before ship loss | 8,414 |
| First ship loss | 8,799 | 8,807 | 8,797 |
| Completed recoveries | 1 | 0 | 0 |
| Native ending | Wins at 12,871 | Loses at 13,403 | Loses at 11,669 |

The reservation restores a missile launch but **does not restore the win**.
The unchanged candidate dies from a missile during on-foot recovery. In the
reservation continuation its pod is destroyed, releasing the pilot at 9,287;
the pilot later dies from a native planet impact at 11,669. The original wins
after rebuilding and boarding while its opponent dies from a planet impact.
This single fork establishes reload competition and a movement-divergence
mechanism; it does not establish an effective resource policy or fully attribute
the eventual winner to one missed missile.

The next weapon-policy design needs to consider reload timing before ammunition
is exhausted and the laser's need for a sustained beam. The tested empty-cannon
reservation is retained as a failed continuation, with no production option,
default change, extra seeds or revised thresholds. Recovery remains outside this
firing change.

The [machine-readable diagnosis](data/combat-reload-diagnosis-v1.json.gz) retains
all three summaries, timing checkpoints, exact overrides, native death evidence,
source/binary identity and SHA-256 receipts. Raw evidence is under
`target/combat-155/reload-competition-v2`. The first attempt is retained as `v1`:
it stopped before any intervention when parsed floating-point JSON values
compared differently despite identical serialized bytes. The corrected diagnostic
compares final files byte for byte; its fork and intervention are unchanged.

Validation on diagnostic source `b5127e9`: **461 AI tests**, the explicit native
replay/intervention, strict Clippy and formatting pass. The physical/material
audits and shared planning-work assertions pass in every arm. This is diagnostic
validation; the prior failed rollout decision still stands.

Reproduce from the repository root with the retained executable (or rebuild the
AI tests with `sensor-profile` and use the executable path Cargo prints):

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_COMBAT_CONTROL="$PWD/target/combat-155/confirmation-control-v1/03-round-8s-p2/output" \
SPACEWARS_COMBAT_CANDIDATE="$PWD/target/combat-155/confirmation-candidate-v1/03-round-8s-p2/output" \
SPACEWARS_COMBAT_ENERGY_OUT=/tmp/spacewars-combat-reload-replay \
target/combat-155/reload-competition-v2/ai-tests \
  mission_pilot::terrain_recovery::qualification::combat_energy::retained_combat_reload_competition \
  --ignored --exact --nocapture
```

Choose a fresh output directory; the diagnostic refuses to overwrite evidence.

## Early energy reserve: frozen diagnostic plan

The late empty-cannon reservation established reload competition but lost its
continuation. Test one separate question: can an earlier reserve improve native
missile availability while avoiding frame-by-frame interruptions of the laser?
This is a mechanism study on the same retained failure, not qualification for
promotion. Keep both previous failures and the independent confirmation screen.

Freeze the following before running the new continuation:

- Use the same first lost win (world `16775363816852070040`, Round, eight-second
  asteroids, P2). Replay the direct-laser candidate from its initial state to
  **7,680**, its first changed laser request relative to the original bot.
  Clone the complete native state and both controllers. Verify the unchanged
  arm against all 13,403 reference ticks and its four final files byte for byte.
- Start a diagnostic reserve latch open. Update it every decision, including
  aim/safety breaks: close below **25 energy**; once closed, reopen at **29**.
  These are one native reload cost plus a restart margin of
  `(12 - 10) × 2 = 4` energy, covering one reload period of net continuous laser
  drain. Clear the latch if supply is absent. Apply it at any ammunition count
  or reload progress, so saving can start before an empty, unpaid loader.
- Suppress only an already-proposed P2 laser request while the latch is closed
  and cannon is false. Preserve cannon requests, steering, safety, aiming,
  recovery, the reactive opponent and native supply rules. Never force the
  laser on. Native reload charges and cannon priority can still interrupt a
  burst; the two-second margin is conditional on no new reload charge.
- Continue both arms to their native ending. Retain exact input overrides,
  latch transitions, loaded ammunition and reload starts, launches, uninterrupted
  laser-request intervals, native hit ticks, damage, recovery and outcome.
  Verify the first override's physical state and all subsequent changes are
  only the stated laser suppression. Check material conservation, physical
  health and the existing shared planning-work bounds.
- Do not adjust thresholds, fork time or seed after observing the result.
  A restored shot alone is insufficient. Inspect whether saving starts before
  ammunition exhaustion, whether the previously missed reload is affordable
  on time, and whether the latch itself fragments permitted beam bursts.
  Any later production candidate needs separate qualification; this retained
  game cannot establish an outcome improvement.

No production resource option, default, recovery behavior or device changes are
part of this diagnostic. Maximum missile reload cadence itself costs
`25 / 2 = 12.5` energy/second, exceeding regeneration even without laser fire.
The reserve therefore cannot guarantee indefinitely sustained missile cadence.

### Completed early-reserve diagnostic

The plan and rule were committed as `54d4478` before either native continuation
ran. The unchanged arm reproduces all **13,403** candidate reference ticks,
including the replayed prefix, and its final state, controllers and both player
frames match byte for byte. Both arms start from identical physical and
controller copies at 7,680. The old empty-reload diagnostic was rerun with the
shared harness: all three summaries, final files and **21,483** recorded decision
rows still match its retained evidence, apart from added diagnostic metadata.

The earlier reserve does protect a reload before ammunition runs out:

- The latch closes at **7,991** after a native reload charge. The first altered
  request is **7,997**, with one missile loaded, a paid reload underway and
  22.53 energy. All inputs, decision hashes and physical-state hashes match the
  candidate through the decision before applying that first suppression.
- At **8,230**, energy is **27.57**, versus **14.57** in the unchanged candidate.
  The native loader starts the next reload at **8,231**, and the replacement
  missile launches at **8,351**, the previously missed opportunity.
- The next reload must still wait until **8,366**: at 8,351 only 22.53 energy
  remains. Reserving energy does not make maximum missile cadence sustainable.

Every one of the **258** altered input ticks satisfies the frozen latch rule;
relative to that arm's bot decision, only P2's laser flag changes from on to off.
No cannon request is overridden.
The latch closes three times, at native reload charges, and never cuts an
already-running laser-only request interval in this continuation. The resumed
burst at **8,186–8,230** lasts 45 ticks and ends with a cannon launch. All ten
applied laser-only intervals last at least six ticks; no one-tick switching
appears. These intervals describe input continuity, not measured visible beam
length or a universal continuity guarantee.

That stability has a substantial cost: the latch stays closed for **459 ticks
(7.65 seconds)** at 8,231–8,689 while the native loader replenishes ammunition.
Before its first ship loss the early-reserve bot records **194 laser hit ticks**,
versus **466** in the unchanged candidate. The two first ships survive for
different lengths of time, so these totals are not equal-exposure accuracy rates.

| Evaluated P2 result | Direct laser | Direct laser + early reserve |
| --- | ---: | ---: |
| Reload after the one ending at 8,231 | 8,567 | 8,231 |
| Missile launch at 8,351 | No | Yes |
| Missiles launched before first ship loss | 6 | 8 |
| Laser hit ticks before first ship loss | 466 | 194 |
| First ship loss | 8,807 | 8,920 |
| Completed recoveries | 0 | 1 |
| Total ship losses | 1 | 2 |
| Native ending | Loses at 13,403 | Loses at 18,466 |

The reserve continuation rebuilds at **10,780**, boards at **10,854** and records
recovery completion at **10,855**. Its rebuilt ship is lost at **15,107**; the
second pod is destroyed at **17,944**, releasing the pilot. Native world impact
kills the on-foot pilot at **18,466**. Recovery policy was never reset or edited.

Do not attribute that different recovery history solely to the restored missile.
The first opponent-health difference is already at **7,999**; P1 motion differs
at **8,034** and P2 motion at **8,045**, before the restored reload or shot.
Suppressing laser fire changes the fight itself, not just the resource balance.
The retained report includes these first differences and the native damage data.

**Decision: retain defaults and keep the resource rule diagnostic-only.** This
fixed early reserve restores a specific missile opportunity without the feared
frame-by-frame switching, but withholds considerable laser fire and still loses
the retained game. One continuation cannot establish a general outcome benefit
or invalidate the independent qualification failure. Do not tune these
thresholds or select a new production candidate from this result. The separate
retained attack/avoidance cycle remains available for the next focused diagnosis.

The [machine-readable early-reserve report](data/combat-early-reserve-v1.json.gz)
contains both summaries, exact override intervals, latch transitions, reloads,
actual launches, input intervals, mission events, checkpoints, source/binary
identity and artifact SHA-256 receipts. Raw evidence and the report-generation
script are retained under `target/combat-155/early-reserve-v1`.

Validation: **463 AI unit tests**, strict Clippy, formatting, both explicit native
diagnostics, all override checks, exact replay comparisons and physical/material
and shared planning-work audits pass. No device or production setting changed.

Reproduce the early diagnostic using a fresh output directory:

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_COMBAT_CANDIDATE="$PWD/target/combat-155/confirmation-candidate-v1/03-round-8s-p2/output" \
SPACEWARS_COMBAT_ENERGY_OUT=/tmp/spacewars-combat-early-reserve \
target/combat-155/early-reserve-v1/ai-tests \
  mission_pilot::terrain_recovery::qualification::combat_energy::retained_combat_early_reserve \
  --ignored --exact --nocapture
```

## Clearance cycle: pursuit bookkeeping discards an active climb

The retained second-world Angular/asteroids/P2 cycle has two premature releases
that also occur with the **current standard laser policy**. Its pursuit starts
at 12,883. At **14,683**, the 30-second pursuit budget expires and `end_pursuit`
clears `pursuit_climb`. Because P2 already owns every planet, destination
selection falls straight back to hunting. At **15,403**, the 12-second pursuit
retry expires; starting a pursuit calls `reconsider`, which clears the climb
again. Neither event is physical evidence that the climb has finished.

| Handoff | Standard laser clearance | Direct laser clearance | Required clearance |
| --- | ---: | ---: | ---: |
| Pursuit ends, 14,683 | 107.72 | 105.43 | 140 |
| Pursuit restarts, 15,403 | 132.40 | 107.16 | 140 |

In both games the opponent remains within 300 units and the original anchor
planet still exists. The ordinary climb release conditions therefore have not
been met. The resumed inward firing turns later trigger genuine falling-speed
clearance checks; those entries should remain authoritative. The first is at
14,801 for the standard laser and 14,768 for the direct-laser candidate.

### Frozen handoff experiment

Test one opt-in profile, `material_hunt_clearance_handoff_v1`, before broader
qualification. Keep the standard bot defaults and failed laser/resource
experiments unchanged.

- At the start of a mission decision, remember an active climb only if the
  previous mission goal is Hunt or Watch. If pursuit bookkeeping selects hunting
  again during that same decision, carry that climb into the existing `hunt`
  logic. Its normal anchor, all-planet clearance, falling-speed and opponent-range
  checks still determine whether to release it.
- Do not restore it for a real transition to travel, landing, recovery or another
  mission. Preserve pursuit budgets/retries, mission selection, guidance,
  weapons, break schedules, native physics and shared work limits. The option
  is configured before the first intent; it survives reset while transient
  climb memory does not. No runtime selection or device default enables it.
- Run exactly four native games in the retained world
  `2178709192992772276`, Angular/slumping, eight-second mixed asteroids, P2
  Recovery v1 versus P1 standard Value v13: original standard laser, original
  direct laser, and each with the handoff option. Establish both unchanged
  references before running either experimental game.
- The unchanged games must reproduce all **17,146 / 16,116** original inputs,
  decision hashes, physical-state hashes, final JSON and player frames. A new
  disabled profile field appears as `None` only in derived controller Debug;
  remove exactly those two new lines for the old controller-file comparison.
  All other controller text must match. Retain that schema distinction.
- Enabled games must reproduce their reference inputs, physical state and
  decisions through the first changed action, ignoring only the new profile
  identifier in typed decision serialization. The first expected action change
  is **14,683**, on identical physical state with the opponent's action unchanged.
  Continue to native match endings; record climb entry/releases, every planet's
  clearance/radial motion, pursuit events, weapons, damage, recovery and outcome.
  Do not inject inputs, reset controllers, change thresholds or add seeds.
- Success for this diagnosis means the timer handoffs no longer bypass an active
  climb, normal release still occurs, and the retained wins and physical/work
  audits hold. This does not prove stronger combat or eliminate every legitimate
  climb/attack pass. Any promotion still requires a separate comparison.

Focused tests cover expiry, restart, ordinary clearance/range release, a real
destination change, default omission, clone/cache consistency and reset semantics.

### Completed handoff comparison

Implementation, tests and the four-game plan were frozen at **`abe7e92`**.
Both unchanged games reproduce all **33,262** reference ticks, their final JSON
and both player frames byte for byte. Controller text matches after removing
exactly two new `hunt_clearance_handoff: None,` lines; it is not claimed to be
byte-identical before that schema normalization.

Both enabled runs first change an action at **14,683**. All preceding actions,
physical-state hashes and typed decision hashes match their references after
omitting only the new profile identifier. At the first changed action the
physical state and P1 input still match. Inputs remain native bot decisions;
there are no controller resets, forced actions or world edits.

The premature releases at **14,683 and 15,403 fall from two to zero in each
configuration**. With standard laser, the original climb now continues until
**15,174**: anchor clearance is 153.40 and the other nearby planet finally clears
140.10. With direct laser it continues until **15,278**, with clearances 140.09
and 145.78. Both have valid outward radial motion. At the later pursuit restart
there is no active climb to discard in either changed trajectory. The focused
restart test independently covers preserving one when present.

| Evaluated P2 result | Standard laser | Standard + handoff | Direct laser | Direct + handoff |
| --- | ---: | ---: | ---: | ---: |
| Native outcome | Win | Win | Win | Win |
| Finished tick | 17,146 | 17,585 | 16,116 | 23,744 |
| Completed departures | 3 | 3 | 3 | 3 |
| Ship losses | 0 | 0 | 0 | 0 |
| Final pilot health | 100% | 100% | 100% | 100% |
| Final ship hull | 26.53% | 26.53% | 26.53% | 24.37% |
| Owned planets at ending | 3 | 3 | 3 | 2 |

This fixes the identified loss of safety memory, **not every climb/attack
cycle**. Standard-laser pursuit still makes inward passes and re-enters clearance
when falling speed warrants it. The direct-laser continuation also has ordinary
300-unit opponent-range releases before full clearance; that existing rule was
not changed. Every recorded release is retained and independently classified
from the before/after controller state and native observation geometry.

Neither retained win demonstrates stronger combat. All four endings are opponent
deaths from native impacts. Standard control ends on a planet impact; its handoff
run ends on an asteroid impact. Direct control ends on a terrain-fragment impact;
its handoff run ends on world impact. Both changed games take longer. The latter
continues for another **7,628 ticks**, loses ownership of one planet, takes some
additional hull damage and ends during a new capture attempt. These later costs
remain visible; the earlier control ending does not provide matched exposure.

**Decision: advance the handoff option with the standard laser policy to a
separate comparison, while keeping defaults unchanged.** The frozen mechanism
check passes, including both wins and safety/work audits. It does not qualify
the handoff option for rollout, rescue the failed direct-laser candidate, or
establish that their combination should be selected. The next comparison should
use both terrain shapes, both asteroid settings and both evaluated seats, and
check survival, missions, outcomes and actual handoff exercise.

The [handoff report](data/combat-clearance-handoff-v1.json.gz) retains all four
summaries, native terminal damage, premature and ordinary releases, pursuit
handoffs, phase intervals, first-change checkpoints, validation and source/binary/
trace SHA-256 receipts. Its **74,591** trace rows are complete and ordered, and
their shared charged-work totals stay within the configured limits. Raw files
and the report-generation script are under
`target/combat-155/clearance-handoff-v1`.

Validation: **467 AI unit tests**, **21 physical combat/mission integration
tests**, strict AI Clippy, formatting, desktop client checking and all four native
games pass. The additional local Pi GPU client check could not run to completion
because this host lacks the `libseat.pc` development metadata. Its failure log is
retained; no dependency workaround, device deployment or setting change was made.

Reproduce with the retained binary and a fresh output directory:

```sh
RUST_MIN_STACK=16777216 \
SPACEWARS_COMBAT_CONTROL="$PWD/target/combat-155/confirmation-control-v1/15-angular-8s-p2/output" \
SPACEWARS_COMBAT_CANDIDATE="$PWD/target/combat-155/confirmation-candidate-v1/15-angular-8s-p2/output" \
SPACEWARS_COMBAT_CLEARANCE_OUT=/tmp/spacewars-combat-clearance-handoff \
target/combat-155/clearance-handoff-v1/ai-tests \
  mission_pilot::terrain_recovery::qualification::combat_clearance::retained_combat_clearance_handoff \
  --ignored --exact --nocapture
```
