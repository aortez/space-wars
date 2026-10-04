# Remaining hull at discretionary pursuit entry

The [pod-impact replay](pod-boundary-impact.md) traces a lost match back to a
new ownership-based pursuit after a completed capture: the pilot has 53.77 hull
against a visible full ship with 94.30 hull. The later pod is already braking
when a missile throws it into the wall. This experiment changes only admission
of a new discretionary pursuit, using current hull observations.

**Result: keep the option disabled.** The frozen comparison reduces evaluated
armed wins from 2/8 to 1/8 and completed departures from 25 to 24. It preserves
the earlier powered capture, but the damaged pilot still dies, 1,297 ticks
earlier. This rejects promotion of the simple hull-parity gate on this corpus;
it does not establish that health is irrelevant to combat decisions.

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

## Frozen results

Code, tests, runner and the original plan were committed as `639dbde` before
collecting enabled outcomes. All **14 disabled replays** retain the seven exact
control/evidence streams, physical and mission results, ordinary sensor work,
allocation ledgers and non-timing planner telemetry. The same fourteen cases
then ran with only the evaluated seat's health option enabled.

Five enabled cases change controls; nine retain exact streams apart from gate
telemetry. Those nine include all six directed cases, whose evaluated completed
sorties remain 1 for cover-on/off and 2 for the successful control, in both
walking and powered modes. Every one of the **4,171 checks** passes the observed
hull, eligibility, tick and counter audit: 22 admit pursuit and 4,149 decline it.
These are tick counts, not 4,149 independent fights avoided.

The table shows the evaluated seat. End ticks include time-limit finishes;
losses are retained even where more surface work completed.

| World / seat / capture | Completed departures, prior → gate | Result, prior → gate | End tick, prior → gate |
| --- | ---: | --- | ---: |
| 0 / P1 / walking | 4 → 3 | loss → loss | 21,907 → 16,190 |
| 0 / P1 / powered | 5 → 3 | win → loss | 36,000 → 15,014 |
| 0 / P2 / walking | 4 → 4 | loss → loss | 26,591 → 26,591 |
| 0 / P2 / powered | 3 → 3 | win → win | 26,264 → 26,264 |
| 1 / P1 / walking | 1 → 1 | loss → loss | 10,040 → 10,040 |
| 1 / P1 / powered | 2 → 2 | loss → loss | 15,435 → 14,138 |
| 1 / P2 / walking | 3 → 5 | loss → loss | 36,000 → 36,000 |
| 1 / P2 / powered | 3 → 3 | loss → loss | 36,000 → 21,748 |

Evaluated claims remain 25 in total. The new world-1 P2 powered run claims and
boards at its fourth visit but never departs, so it is not a fourth completed
sortie. The world-1 P2 walking run completes two additional sorties and still
loses at the time limit. These are known, correlated development matches rather
than an independent estimate of bot strength.

## What the changed decisions do

In both world-0 P1 cases, the first declined fight compares **99.6182 hull
against 100**. The original policy would start pursuit; the gate continues
ordinary mission control. Incoming fire subsequently starts a defensive
pursuit at 14677 (walking) or 13552 (powered). Both ships are lost and their
pods die at the boundary. The powered case loses a former win. This strict
rule treats a small hull difference as sufficient reason to change missions,
without measuring whether the resulting transfer is safer.

Both world-1 P2 cases first decline pursuit at 4032 with **94.3363 against
100**. They also enter defensive pursuit later, at 4455. Four defensive entries
after a first deferral are retained across the full corpus. Preserving that
branch works as specified; declining discretionary pursuit does not guarantee
that combat is avoided.

The originally investigated world-1 P1 powered case changes later:

| Tick | Gate-enabled observation |
| ---: | --- |
| 2882 | Equal 100/100 hull admits the original first pursuit. |
| 8765 | The earlier powered crossing still completes. |
| 10713 / 12297 / 12520 | Claim, original-ship boarding and departure are retained. |
| 12521 | Declines 53.7729 versus 94.2950 hull; selects planet 1 for transfer. |
| 12653 | First changed control; earlier physical observations and controls match. |
| 13746 | Arrives in planet 1's approach frame and starts a capture task. |
| 13921 | Laser damage destroys the ship while the capture task seeks cover. |
| 14138 | Escape pod dies on planet contact; the match is lost. |

There is no defensive reentry after this case's deferral. The new approach
never reaches touchdown, exit or claim. Its final death is **21.62 seconds
earlier** than the baseline wall impact at 15435. Replacing pursuit with an
ordinary transfer therefore changes the failure rather than solving survival.
The audit also verifies that a decline may start a capture on the same tick,
while never interrupting a capture committed on the preceding tick.

The next bounded investigation is the threatened transfer/approach: determine
why the destination and cover controller leave this damaged ship exposed before
landing. Hull parity supplies no evidence that the alternative is safer. Keep
the failed candidate and both near-full-hull regressions available; do not tune
a new cutoff against these results or promote it through the bot defaults.

## Verification and retained evidence

All **1,023 Rust tests and 669 Python tests pass**, including six new Rust tests
and six new audit tests. Formatting, strict AI Clippy (`--no-deps`) and profiled
and ordinary release builds pass. Scenario Clippy retains seven pre-existing
findings in unchanged code. The focused tests cover damaged/equal/invalid hull,
defensive and vulnerable-target responses, active fights, capture priority,
external deferral, retry clocks, immutable observations, repeated ticks, clone
and reset behavior.

The enabled batch audits 396,770 pilot rows and 230,785 dispatch ticks. Charged
work remains at most **4 graph / 384 physics queries per dispatch**, with a
maximum publication age of **120 ticks**. The gate itself adds no queries.
Aggregate graph/query work falls because the trajectories and several match
durations change; this is not an efficiency result or a Pi performance claim.

[The result manifest](data/pursuit-health-gate-v1.json) retains every outcome,
physical visit, pursuit entry, vehicle transition and comparison.
[The compressed evidence archive](data/pursuit-health-gate-v1.json.gz) contains
73 exact documents: the original plan, all gate checks, physical/route/flight
witnesses, first changed controls, pursuit and damage records, validation logs,
and hashes for 407 raw files. Embedded text and raw-file hashes were verified;
both the prior and candidate frozen binaries retain their hashes. Full streams
and binaries remain under `target/pursuit-health` and the prior corpus path.

- Frozen binary SHA-256:
  `bf1da1296f349a7abb59953f31206e947e28d0b2c23ed49d110c2cae5a8868a5`
- Frozen summary SHA-256:
  `937a17093cd0754b95a49b1447eafbf37f2320bd1608563da78bd24b715123d7`
- Evidence archive SHA-256:
  `78aea01f0407961cfee1d89a5878e10732be70ae0a5113b70f32dd2e4168cb0e`

The subsequent [threatened-approach diagnosis](threatened-capture-approach.md)
finds thirteen covered modeled walking round trips at the new destination, but
none is published when the pilot chooses the first exposed route. The replay
preserves the loss exactly and motivates testing earlier covered-route evidence
and initial commitment qualification, rather than changing the hull threshold.
