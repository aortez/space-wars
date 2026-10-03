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
