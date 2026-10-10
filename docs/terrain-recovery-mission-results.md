# Mission recovery integration preserves ordinary play and retains on-foot failures

The opt-in recovery profile now reaches the mission-owned task through a paired
observation/intent driver. **15 of 16 candidate comparisons preserve every game
input and the complete native match result.** One Round comparison changes the
arrival at an already successful rebuild site and the subsequent battle. It
does not add a completed recovery. All 24 runs finish without an assertion or
material audit failure.

The generated worlds also expose **three unresolved on-foot failure cases**.
Selecting the candidate for the affected bot preserves its control inputs in
all three. This supports integration compatibility with the launcher's Value
bot, but does not establish additional recoverability, frontier strength or a
reason to change defaults.

## Driver and frozen comparison

The [plan](terrain-recovery-mission-plan.md), source and runtime were frozen at
`e36ee55149f5402fb7551e70d0515ac239ffc86b` before execution. The driver preserves
mission priority, one recovery observation per decision, the landing survey
cadence and post-intent planning. It configures newly created recovery tasks,
preserves selection across clone/reset, and clears native refinement on the
same decision that completes recovery, before the physical step. It remains
behind `sensor-profile`; the launcher and default bots are unchanged.

Both seats run ordinary `material_mission_v13`, matching the launcher's Value
bot host flow. No other experimental profile is enabled. Each setting has an
unchanged control, candidate P1 and candidate P2 from identical initial worlds.
The matrix uses generated three-planet starts, Mixed asteroids every eight
seconds, 15-second combat intervals and four-second breaks. Slumping controls
collapse; Weak cohesion and impact fracture remain enabled. Native pilot death
or the 600-second limit ends the match. No world edit, task reset, outcome-based
seed replacement or simulation rerun occurs.

| Seed | Shape | Collapse | Control finish tick | Winner | Recoveries P1/P2 | Completed sorties P1/P2 | Ship losses P1/P2 |
| ---: | --- | --- | ---: | --- | --- | --- | --- |
| 10101 | Round | Off | 29,840 | P2 | 0 / 1 | 4 / 5 | 1 / 1 |
| 10101 | Round | On | 5,116 | P1 | 0 / 0 | 1 / 0 | 0 / 1 |
| 10101 | Angular | Off | 16,611 | P1 | 1 / 0 | 4 / 1 | 1 / 1 |
| 10101 | Angular | On | 21,376 | P1 | 0 / 1 | 5 / 2 | 0 / 2 |
| 20202 | Round | Off | 36,000 | P1 | 1 / 1 | 3 / 0 | 1 / 2 |
| 20202 | Round | On | 9,231 | P2 | 0 / 0 | 2 / 1 | 1 / 0 |
| 20202 | Angular | Off | 36,000 | P1 | 0 / 0 | 2 / 0 | 0 / 1 |
| 20202 | Angular | On | 36,000 | P1 | 0 / 0 | 7 / 5 | 0 / 0 |

Controls record five completed recoveries, six native rebuilds, 12 ship losses,
42 completed sorties and 170 local-frame changes. These are eight settings
repeated three ways, not 24 independent seeds. Across all arms, all 15 recovery
completion receipts show an available full ship with the pilot aboard, two
supported feet and 0.25 seconds settled. Every completion is followed next tick
by launch or hunt. Candidate seats execute 82,711 recovery decisions and
complete five recoveries. Their 82,706 native-refinement decisions exclude the
five completion decisions, as required by immediate cleanup.

## The changed Round match

For seed 20202, Round, collapse off, candidate P1 changes a successful relocation.
Both arms select exactly the same standing site at tick 11,535. The first game
input difference is 11,723: the ordinary controller stops horizontal movement,
while the precise-arrival controller continues with horizontal input 0.08994.
The retained ground telemetry identifies the same route and target, with
`precise_rebuild` enabled only for the candidate.

The control reports arrival at 11,731, native rebuild at 12,216, recovery at
12,301 and launch at 12,302. The candidate reports arrival at 11,737, rebuild at
12,218, recovery at 12,299 and launch at 12,300. Both recover successfully.
Their later reactive battle diverges: the candidate match ends with P2's death
at 18,574, while the control reaches the limit at 36,000. P1 wins both; ship-loss
and completed-recovery counts remain 1/2 and 1/1 respectively. This one sample
does not establish a strength improvement.

Decision/scenario checksums first differ earlier, at 11,519, because native
placement observations contain additional attempts. The retained observation
comparison differs only in that attempt array's length at this point. Those
diagnostic differences must not be described as earlier physical divergence.

## Unresolved on-foot cases

These cases all affect P2 on seed 20202. Candidate P2 preserves every input and
the native result of its control. No pod-reconsideration review occurs.

| Shape / collapse | First block | Retained failure |
| --- | ---: | --- |
| Round / off | 19,696 | Ground navigation stops progressing toward the flag. A later native rebuild at 24,743 grants the existing bounded boarding opportunity, but the hatch route blocks again at 26,228. Recovery remains incomplete at the limit. |
| Angular / off | 8,326 | No reachable standing site with hatch access. The native placement receipt includes missing start footing; neither arm rebuilds. |
| Angular / on | 27,919 | Replacement of an unreachable ship cannot start or finish. The existing ship remains available, with no native ship loss or completed recovery. |

The Round block spans 14,819 decisions in the unchanged control and candidate
P2; the Angular blocks span 27,674 and 8,081. These observations do not prove
that every possible physical route is impossible. They also do not demonstrate
a successful route that would justify resetting an on-foot controller.

The Angular/off candidate receives 879 rebuild surveys, reaching eight coarse
candidates but no refined candidates, offset checks or staging-route checks.
Its diagnostic checksums differ while inputs and outcome stay unchanged. Thus
14 comparisons preserve all checksums; 15 preserve inputs and native outcomes.
The changed Round candidate receives one survey with three coarse candidates
and 12 offset checks. Across this matrix there is no terrain-flight forecast,
staging execution or pod reconsideration. Their earlier targeted evidence is
not replaced by these generated-world results.

## Audit, validation and cost

The [compact receipt](data/terrain-recovery-missions-v1.json) links the full
audit and inspection by SHA-256. The audit verifies 2,388 frozen source files,
the executable, 603 case artifacts, 553,096 action ticks, 362,922 overlapping
comparison ticks, 9,226 material/physics samples and 3,628 transition
observations. Initial controller memory differs only in the selected profile
flag. Per-tick native option ownership and completion cleanup pass, as do
material conservation, speed alarms and work limits. Maximum measured shared
host work is four graph units and 142 physics queries, within the 4/384 budget.

**438 AI library tests, 117 recovery/navigation/gameplay tests and 15 native
mission/combat observation tests pass.** Strict AI Clippy, the default-feature
library check and formatting pass. Native Clippy still reports the same seven
warnings in unchanged source files; there are no new warnings from this change.
The auditor succeeds without a correction or simulation rerun. A preliminary
harness compile error was fixed before the runtime freeze; its log is retained.

Sequential desktop simulations take 407.62 wall seconds. Across the matrix,
mean observation-plus-policy time is 0.0940 ms per decision, shared planning
0.00286 ms per tick and native stepping 0.4179 ms per tick. These descriptive
measurements exclude rendering and trace serialization.

There is a cost worth investigating before promotion: in Angular/off, enabling
the candidate for the blocked P2 leaves inputs unchanged but raises measured
mean native step time from 0.211 to 0.330 ms. During the blocked interval it is
0.217 versus 0.370 ms. This is consistent with additional native placement work;
internal profiling is needed to isolate the cost. One desktop run is not a
statistical performance result or a Pi frame-rate qualification.

## Handoff and next gate

Raw runs, frozen binary, source manifest, commands, audits, inspection and
validation logs are retained in
`spacewars-handoffs/mission-recovery-20261010`. The portable package is
`spacewars-handoffs/terrain-recovery-missions-20261010.tar.gz`.

Work is local on `bot-mission-terrain-recovery`. The original #178 checkout
remains clean at `cada1eb`. No deployment, bot-default or GitHub change was made.
The runtime uses the frozen #174 terrain handoff; its eventual final published
source still needs verification. This screen does not qualify the whole
experimental frontier profile or the alternate live-local-observation host.

The [retained-state investigation](terrain-recovery-mission-investigation-results.md)
now inspects support and hatch/flag clearance, compares unchanged and fresh
recovery controllers, and profiles native placement work. None of its three
resets completes recovery. It identifies separate ground-execution, missing-
start-footing and replacement-distance failures, plus repeated hull-filtered
graph cost. That report defines the next focused experiments. Retry policy and
defaults remain unchanged; any cost reduction must preserve the earlier
heavy-Angular late native rebuild opportunity.
