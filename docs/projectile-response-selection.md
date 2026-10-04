# Conservative projectile-response selection checkpoint

## Scope and stopping point

Close the current local bot investigation with one frozen selector comparison
and a reviewable promote-or-retain decision. Diagnose the retained braking
regression from saved observations, implement one conservative laboratory
selector, and test it on declared independent worlds in both seats. Do not tune
the selector after seeing those match outcomes or start another response model
in this checkpoint. Defaults and remote state remain unchanged.

## Recorded-state diagnosis plan

Use the first warning in four existing observe controls: the clear-entry
regression and speed-limited success from `projectile-response-v1`, and the new
health/world-3 cases from `ordinary-transfer-response-v1`. Retain the correlated
world-1 health outcomes without calling them independent geometries. Verify
archive/input hashes and extract consumed source observations, initial native
commands, projected entry, motor acceleration and contact/loss outcomes.

Braking uses velocity relative to the **rotating planet at the ship position**,
not just planet-center translation. Compute that observed frame velocity and
the nominal initial brake vector from the published flight limits. Compare its
component toward each warned-about projectile with the initial native thrust.
This is an instantaneous motor comparison, not a prediction of gravity,
rotation, contacts or the controller's future commands. Check the physical
precontact tracks before attributing the regression to that geometry. Recorded
hull damage is capped by remaining health; a fatal receipt is not an uncapped
impact-energy measurement.

## Candidate specified before independent outcomes

Add opt-in laboratory mode `guarded_brake` under the existing response probe.
Use the unchanged two-second circle screen, all owners, earliest entry/ID tie,
ordinary-transfer eligibility and one attempt per complete match. At the first
warning choose **brake or unchanged native controls** once. An abstention
consumes that attempt; do not wait for another, retrospectively better warning.

Admit braking only when all of these hold:

1. The observed diagnostic is complete within its existing range/capacity,
   with no unavailable shell. The observation and motor inputs are finite.
2. The ship has fully open wings, the native command keeps them open, and no
   landing-assist force is active. Native braking is not already requested.
3. Earliest projected entry is at least the unchanged 30-tick / 0.5-second
   response duration. This conservative restriction lets the declared pulse
   finish before the screen's first possible contact; it is not a calibrated
   reaction-time threshold or proof that shorter responses cannot help.
4. For **every projectile currently flagged by the same screen**, the nominal
   brake acceleration has a strictly negative component toward that projectile,
   and its difference from the current native thrust also has a strictly
   negative component. Require both signs; do not fit a numeric margin.

Native turn, weapons and interaction are preserved. The existing priority and
episode/deadline guards remain authoritative every tick, and the pulse cannot
rearm. This does not predict miss distance, new projectiles, moving obstacles,
damage or match value. It can miss useful short-warning mitigation and can
still make a later outcome worse. Steering remains outside this selector
because the initial acceleration does not describe its delayed turning effect.

The candidate is an explicit, conservative filter of the measured brake pulse,
not a generally safe collision-avoidance policy. Keep the unconditional brake
and steering comparisons as historical evidence.

## Diagnosis and declared tradeoff

The [saved geometry report](data/projectile-response-selection-geometry-v1.json)
retains the four source observations, action bytes, rotating-frame calculation,
contact/loss outcomes and dense clear-entry precontact tracks. Positive radial
acceleration below points **toward** the warned-about projectile.

| Existing source | Screen entry, seconds | Initial brake motor toward projectile | Candidate choice |
| --- | ---: | ---: | --- |
| Clear-entry world 1 P1 | 0.185 | +37.049 | Native; warning shorter than pulse |
| Speed-limited world 1 P1 | 0.900 | −22.030 | Brake |
| Health world 0 P1 | 0.442 | −36.231 | Native; warning shorter than pulse |
| Fresh world 3 P1 | 0.041 | −8.060 | Native; warning shorter than pulse |

At clear-entry tick 9160, the ship is moving away from the missile. The planet
frame at the ship is (−12.704, −6.349), and the ship's frame-relative velocity
is (32.069, 4.613). Full brake therefore contributes (−39.592, −5.696) of motor
acceleration, toward the incoming projectile. The native command is thrusting
away. By tick 9172, before either contact, native ship velocity is (26.064, 4.519)
while brake is (11.539, −2.718); both still have 31.174 hull health. The brake
trajectory is also 1.65 units behind and 0.76 units lower than native.

The same launched missile then contacts brake at 9173 and destroys the ship.
Native contact follows at 9174 for 30.198 recorded hull damage, leaving about
0.977 hull until laser destruction at 9185. This supports the diagnosis that
braking removes useful separation and changes contact geometry. It does not
isolate an uncapped impact-damage formula or prove a general survival rule.
Planet rotation matters: using only its center velocity misstates the radial
braking acceleration, even though the sign happens to agree in these cases.

The candidate preserves the successful old speed-limited pulse, but knowingly
forgoes the new health case's useful 51.049 → 19.883 damage mitigation and three
extra later captures. Its 0.442-second warning is shorter than the fixed pulse.
It also skips the fresh world-3 maneuvers that won despite no triggering-missile
contact in the control. Neither known outcome is used to make an exception.
This is a development comparison; these states are not independent validation.

## Frozen full-match comparison plan

Run **52 complete matches**, at most two concurrent, using one frozen profiled
binary and unchanged 600-second match limits:

- Seventeen historical selector runs: all 15 ordinary-transfer configurations,
  plus both original clear-entry primary/health cases. Compare to their saved
  observe controls. Preserve `escape` scope for the two clear-entry regressions
  so their first decision remains the exact previously tested source; all other
  selector cases use `transfer` scope.
- Sixteen new control runs with the probe disabled and sixteen matched selector
  runs: four independent generated worlds × both evaluated seats × quiet or
  asteroid interval 3 seconds. The evaluated v13 seat and opposing v10 seat
  retain the earlier powered integration options and shared planning allowances.
- Three legacy byte-parity replays: original clear-entry brake, original
  speed-limited left, and ordinary-transfer world-0 P2 observe. Retain complete
  old response logs, native/projectile streams, outcomes, visits and allocation.

New seeds are the first eight SHA-256 bytes, little-endian, of
`projectile-selection-v1:<world>`, declared before generating any new outcome:

| World | Seed |
| --- | ---: |
| 0 | 3644024478941633970 |
| 1 | 7072119405551378834 |
| 2 | 8616159069410658879 |
| 3 | 6744464469995572696 |

Both seats and asteroid variants of one world are related observations, not
independent seeds. Keep historical primary, historical health, historical
worlds-2/3, clear-entry regressions, new quiet and new asteroid cohorts separate.
Include every declared configuration, even if no warning or no response occurs.
Do not add replacement worlds to obtain more activations.

Independently reconstruct first-warning selection, every guard, motor-direction
calculation, action bytes, fixed deadline and priority stop. Require exact
two-seat native prefixes through the decision, and full native/projectile parity
for every abstention or inactive selector. Require any retained selected brake
to reproduce its corresponding old unconditional-brake continuation exactly.
Audit complete physical capture/return, ship/pilot loss and recovery, match
outcomes and existing planner work. Report warning and activation coverage,
including P2, and retain every regression. No-warning or abstention parity tests
compatibility, not response quality.

The selector adds at most one bounded pass over the already retained projectile
sample at the first warning, with no world query or future physics simulation.
This synchronous diagnostic decision and trace IO are outside the existing
charged graph/query ledger. Its bounded input is not a whole-frame latency
guarantee or Picade performance measurement.

An unchanged or sparsely activated held-out suite cannot justify promotion.
After this comparison, record the result and package a local review checkpoint;
do not silently change the policy, thresholds, defaults or experiment scope.

## Legacy audit correction before independent trials

The three legacy simulations ran first. All three reproduced their old response,
capture and projectile streams byte-for-byte, but their final Python comparison
failed: in-memory visit tables use integer seat keys, whereas archived JSON uses
strings. Round, visits and allocation are identical after JSON normalization.
P2 observe also passed its existing native replay checks before that final
comparison. The original correction note at `94683e3` was written while the
third result was arriving and described only the first two failures; the
preserved completed summary and correction record retain all three.

Correct only that archive comparison and retain a regression test that still
rejects changed physical visit ticks. Resume the declared plan using the same
frozen runtime binary; do not rerun the three completed matches. Preserve the
original failed summary, tracebacks and hashes, recheck the existing files and
normalized results, and record the audit correction's commit/tool hashes. No
independent-world simulation ran before this correction, and no decision rule,
case, seed, duration or physical result changes.

## Complete results and activation coverage

All **52 declared full matches pass** their physical, planner-work, projectile
and response audits. The implementation and plan were frozen at `532462e`;
the archive-key audit correction is `94683e3`. The three completed legacy
simulations were retained, and the remaining 49 simulations ran once. No seed,
configuration or outcome was replaced.

Across **33 selector runs**, seven encounter a warning: two select brake and
five abstain because entry is sooner than the existing pulse duration. The
other 26 encounter no eligible warning. All 31 inactive/abstaining runs retain
complete native action/observation and projectile parity with their controls.
Both selected brakes retain exact native/projectile streams, complete rounds,
visits and allocation from their old unconditional-brake runs. All three legacy
response logs also remain byte-for-byte identical. Selector runs include
139,569 eligible ticks in total; that count includes changed continuations.

The selected brakes are only the already known, correlated world-1 P1 primary
and health cases at tick 8837. They apply the full 30 ticks. The clear-entry
primary and health cases abstain at 9160 and reproduce observe exactly, avoiding
the earlier destruction caused by the unconditional brake. They still lose
their ships later as in the controls; this is not a saved match.

All five physical abstentions are decided by the duration guard before the
direction calculation. Thus this matrix does not isolate an incremental benefit
from the direction guard or physically exercise its rejection path. Its
opposing-motion, conflicting-warning and stronger-native-command refusals are
covered by unit tests, not newly activated full-match evidence.

The new health world-0 warning at 13515 and the historical fresh world-3 warning
at 12087 also abstain, as declared. The health case remains a loss with 3/3
captures/departures, forgoing unconditional brake's 6/6. The world-3 case remains
a loss with 1/1, forgoing the earlier maneuvers' wins. The stricter rule is not a
claim that those beneficial interventions were impossible.

## Independent worlds

All **16 new selector matches** retain their disabled controls' complete native
and projectile streams, physical outcomes and planner allocations. Fifteen
never encounter an eligible warning. Only quiet world-0 P1 warns, at **19148**,
on missile 100054 launched at 19147 with **0.173 seconds** projected entry. The
selector abstains and the native priority/episode changes at 19161. No matching
contact from that missile is recorded through match end. The ship is later
lost aboard at 19305 to cannon damage, and the pilot dies at 19752; the evaluated
seat loses with two complete capture/departure cycles and no rebuild.

Every P2 candidate has ordinary-transfer eligibility but no warning in this
set. There is consequently **no new activated braking trajectory and no
physical P2 braking validation**. These worlds validate compatibility and the
declared abstention behavior, not response effectiveness. No extra worlds were
selected to increase activation coverage.

Each cell is the evaluated v13 seat's outcome and physical captures/departures;
control and selector are identical in every cell. Asteroid interval is 3 seconds.

| New world | Evaluated seat | Quiet, control = selector | Asteroids, control = selector |
| --- | --- | --- | --- |
| 0 | P1 | Loss, 2/2 | Loss, 2/2 |
| 0 | P2 | Win, 4/4 | Win, 3/3 |
| 1 | P1 | Loss, 2/2 | Win, 6/6 |
| 1 | P2 | Win, 5/5 | Loss, 3/3 |
| 2 | P1 | Win, 1/1 | Win, 3/3 |
| 2 | P2 | Loss, 1/1 | Loss, 1/1 |
| 3 | P1 | Win, 6/6 | Win, 3/3 |
| 3 | P2 | Win, 5/5 | Loss, 1/1 |

## Cohort outcomes

Each outcome cell is **wins / captures / departures**. Legacy parity replays
are excluded. Full per-case outcomes, pilot deaths, ship losses, rebuilds and
paired deltas remain in the manifest.

| Cohort | Cases | Control | Selector | Brake selections |
| --- | ---: | ---: | ---: | ---: |
| Historical retained primary | 8 | 2 / 26 / 25 | 3 / 32 / 30 | 1 |
| Historical health | 3 | 0 / 7 / 7 | 0 / 12 / 11 | 1 |
| Historical worlds 2–3 | 4 | 2 / 9 / 9 | 2 / 9 / 9 | 0 |
| Historical clear-entry primary/health | 2 | 0 / 2 / 2 | 0 / 2 / 2 | 0 |
| New quiet worlds | 8 | 5 / 26 / 26 | 5 / 26 / 26 | 0 |
| New asteroid worlds | 8 | 4 / 22 / 22 | 4 / 22 / 22 | 0 |

The historical primary and health gains are replays of the same previously
measured braking trajectory, not additional successful geometries. They also
retain later loss/recovery costs: historical primary ship losses/rebuilds change
from 6/1 to 7/3, and health from 3/0 to 4/1, over complete matches. Pilot deaths
decrease from four to three and three to two respectively. The stronger earlier
no-escape primary reference remains **2 wins / 28 captures / 27 departures**.
Do not pool these results with independent worlds or replace the earlier
unconditional-brake/steering outcomes with the selector's narrower coverage.

## Verification and reproduction

All **1,103 Rust tests** and **775 Python tests** pass. Formatting, strict AI
Clippy and both profiled and ordinary release builds pass. New checks cover the
rotating motor frame, approaching/receding geometry, a stronger native command,
conflicting warnings including own projectiles, exact pulse-duration admission,
unsupported inputs and an abstention consuming the only attempt. Independent
audits verify every chosen action and all priority/deadline stops. Tests also
cover archived seat-key normalization and seat-aware cohort reporting.

All **2,221,634 projectile rows** pass their physical-frame, identity, ordering
and capacity audits. Observed maxima are four retained/in-range projectiles
and 73 scanned debris entries, with no unavailable shell or sample truncation.
The frozen plan's single-pass wording was too narrow: the selector performs
several bounded passes over the retained sample in one invocation. It uses no
additional world query or future simulation. That reporting clarification does
not change the implementation or assert a whole-frame latency bound.

Frozen binary: `target/projectile-selection/surface_mission_soak-532462e`, SHA-256
`bfed13d163a38a5cb4d1c390bc46a8110b6c89162bf848880af74662a70905c2`.
Completed summary: `target/projectile-selection/v1/summary.json`, SHA-256
`ca5ec19deb0a097182af44ed2e03746464e9aac49c6952a55a010976130eccfb`.

The [manifest](data/projectile-selection-v1.json) includes all 52 outcomes,
33 control/selector comparisons, six separate cohort totals, selection reasons,
compatibility checks, source/tool/binary hashes and **800 raw-file hashes**.
The [compressed archive](data/projectile-selection-v1.json.gz) preserves the
complete runner summary, physical visit audits, response/selection witnesses,
projectile tracks and final reports/checkpoints. The initial failed legacy
audit summary and its hash, exact tracebacks and correction provenance remain
in the record. Full raw reports and streams remain at their hashed local paths.

```sh
python3 tools/validate-projectile-selection.py \
  --ordinary target/ordinary-transfer-response/v1/summary.json \
  --original target/projectile-response/v1/summary.json \
  --binary target/projectile-selection/surface_mission_soak-532462e \
  --out /tmp/projectile-selection
python3 tools/analyze-projectile-selection.py \
  --summary /tmp/projectile-selection/summary.json \
  --out /tmp/projectile-selection-results.json
```

Use fresh output paths and a clean checkout. The corrected runner needs no
resume flag for a new reproduction. The one-time recorded continuation used
`--resume-audit-correction` on the original output directory after preserving
and verifying its three completed legacy runs.

## Closing decision

**Retain the selector as an opt-in laboratory candidate; do not promote it.**
The braking regression is explained well enough to reject that intervention
using current observations, and the selector preserves the known useful brake.
The independent worlds add no activated response evidence, while the duration
guard deliberately gives up useful short-warning mitigation. Neither direction
alone nor this duration gate establishes a general response-choice policy.

This completes the declared stopping point. The [branch review guide](bot-route-evidence-checkpoint.md)
packages the broader execution work and remaining policy-quality/device gates.
Leave new motion prediction, threshold changes and additional seed searches for
a separately scoped effort. Default probe mode is still `none`, default scope
is still `escape`, and no interactive choice, device default or remote state
changes. All work remains local.
