# Remaining hull at discretionary pursuit entry

The [pod-impact replay](pod-boundary-impact.md) traces a lost match back to a
new ownership-based pursuit after a completed capture: the pilot has 53.77 hull
against a visible full ship with 94.30 hull. The later pod is already braking
when a missile throws it into the wall. This experiment changes only admission
of a new discretionary pursuit, using current hull observations.

## Frozen candidate

`--pursuit-health-seats none|0|1|both` defaults to `none`. The profile is
`discretionary_pursuit_hull_v1`, available through `MissionBot::with_pursuit_health`
before its first intent. Policy IDs and normal bot/UI selections are unchanged.

For the existing `nearby opponent after securing ground` entry reason, require
**own remaining hull >= opponent remaining hull**. Both actors are full ships
at this branch. Compare raw remaining hull units, not inferred health maxima,
win probabilities or a threshold fitted around 53.77. Equality admits pursuit.
Missing/nonfinite or nonpositive hull values decline discretionary pursuit and
are recorded as unknown/invalid. The parity rule is a hypothesis, not evidence
that equal-hull fights are advantageous.

The new gate runs after existing eligibility and external calibration deferral.
Recent incoming-fire responses and vulnerable-opponent pursuits retain their
existing precedence. Ongoing pursuits retain their original deadline and loss
of sight/range rules. Committed captures, on-foot work, recovery and solar
avoidance keep their priority. Declining an entry adds no cooldown, refreshes
no clock and emits no replacement flight controls; ordinary mission selection
continues, and a new hit can still qualify for defensive combat on the next tick.
The existing all-planets-owned fallback to hunt is outside this admission gate.

Telemetry retains bounded counters and the last decision, including tick,
opponent identity and observed hull values. Counters count evaluated ticks, not
independent fights prevented. Clone/repeat-tick behavior is deterministic;
reset preserves the option and clears decisions. The optional evidence trace
records every fresh check with the current target and last incoming-hit tick.
The gate uses existing observations and adds no physics queries or route work.

## Frozen comparison plan

Freeze code, tests, this document and the runner before collecting enabled
outcomes. Use `target/flight-continuation/v1/summary.json` as the immutable
baseline. It already contains the repaired powered flight; later pod diagnostic
work changed no ordinary controller or physics behavior.

1. Replay all **14 existing cases with the gate disabled**, requiring exact
   control/evidence streams, physical/mission results, ordinary sensor work,
   allocation ledgers and non-timing planner telemetry.
2. Enable the gate for the evaluated seat in those same fourteen cases: six
   directed 180-second missions and eight armed 600-second matches, including
   both seats, two generated worlds, and walking/powered capture settings.
   Change only the new option, binary and output directory. Keep the other
   player, active-flight option, route evidence, 4 graph / 384 query allowance,
   120-tick publication lifetime and deadlines unchanged.
3. Audit every gate check against the current observation, reason eligibility,
   hull comparison and counters. Retain first changed controls, all declines,
   defensive reentries, completed physical visits, pod losses and match losses.
   If controls never change, require exact physical/evidence/sensor retention
   apart from the new telemetry. A declined entry is not itself a successful
   escape, new capture or survival improvement.

Apply the gate throughout each match. Do not delay it until the investigated
tick, select a winning threshold after observing results, or require that the
former trajectory's capture still occurs after an earlier changed decision.
Run at most two games concurrently and verify prior input/binary hashes before
and after all 28 runs. These are known development cases; outcomes do not supply
an independent win-rate estimate or justify a default change.

```sh
python3 tools/validate-pursuit-health.py \
  --prior target/flight-continuation/v1/summary.json \
  --binary target/pursuit-health/surface_mission_soak-COMMIT \
  --out target/pursuit-health/v1
```
