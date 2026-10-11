# Recovery after a late native rebuild: results

**The stale recovery task is fixed and both selected native games qualify.**
P2 now boards and completes recovery at tick **22,023**, just **75 ticks / 1.25
seconds** after its physical rebuild. It retains the original task start and
four exhausted relocations; no further ship loss or coordinator reset is needed.
The [plan](recovery-rebuild-plan.md), runtime, runner and contract tests were
committed at `aee2c82` before either game ran.

## Behavior and bounds

`recover_ship_v10` tracks the native rebuild counter for the assigned vehicle.
An advance accompanied by an available full ship can release a blocked or
expired task once, into boarding. Availability alone, initial/repeated/rolled-back
counters and incomplete rebuilds do not renew the task. Identity/version errors
and backwards ticks remain terminal until reset, including duplicate calls that
might otherwise replay cached input after an identity error.

A bounded receipt retains the new generation, prior failure and a fixed
90-second boarding deadline, using the existing ground traversal limit. Native
control disarming and dirty queries cannot lose or restart this opportunity.
Old routes and pending destinations are cleared while the original task start,
relocation count, ground allowance and replacement history remain intact.
Existing hatch and progress checks still apply. This opportunity cannot start
another scuttle, and loss of the full ship blocks it. Physical boarding completes
the goal even when transfer is observed just beyond the deadline with controls
disarmed. Normal recoveries within their original budget keep their behavior.

The fix applies to both actors through the shared task, including retained v10
and the integrated v13 candidate. It adds no world writes, sensing work or host
resets. Strategy options and bot selection defaults are unchanged. The frozen
runtime binary SHA-256 is
`e8a4c7515a568aa27569dc821bb8f553e36657f9462b970dff86b2a2e75a11ed`.

## Native qualification

Both games use seed **11223442104665788832**, integrated v13 in P1, v10 in P2,
no asteroids and the same shared execution-routes host. They retain their source
commands, including climb-laser off/on, defense and clearance disabled, full
dense traces, native endings and unchanged planning budgets. Only executable
and output paths change. There are no game retries, auditor corrections or
runtime retuning.

Before tick 21,948, all **87,792 trace rows** across both games agree exactly
with their immutable baselines after normalizing only `recover_ship_v10` to
`recover_ship_v9`. Full source observations and actions also agree on the native
rebuild tick. The new recovery receipt is the intended change there.

| Event | Both corrected games |
| --- | ---: |
| Original recovery starts | 3,220 |
| Four relocation attempts exhausted | 8,328 |
| Native rebuild; boarding opportunity recorded | 21,948 |
| Fixed boarding deadline | 27,348 |
| First action difference: P2 presses interact | 22,022 |
| First physical difference: P2 boards; host completes recovery | 22,023 |

The receipt preserves four relocations, zero scuttle attempts and the original
start at 3,220. Native `rebuilds` and `ships_lost` both remain **one** through
boarding. There is exactly one new boarding receipt per game, for P2, and neither
attempt blocks or extends its deadline. In the old baselines P2 instead stays
blocked until another destruction at **28,861** with climb laser off or
**28,129** with it on—6,913 or 6,181 ticks after the successful rebuild.

## Changed outcomes

Fixing the shared task lets the opponent return earlier. These outcomes are
part of the result, rather than grounds for retaining its old recovery failure.

| P1 climb laser | Original P1 result | Corrected P1 result | P2 ship losses, original → corrected | P2 completed recoveries, original → corrected |
| --- | --- | --- | --- | --- |
| Off | Win at 36,000 | Loss at 25,876 | 2 → 1 | 0 → 1 |
| On | Loss at 32,268 | Loss at 25,876 | 2 → 1 | 1 → 1 |

P1 completes the same first three departures at **2,585 / 6,940 / 18,575**.
The original fourth departure at 26,526 lies after the new match ending. Both
corrected games end in P1's pilot death at 25,876, with native final damage
attributed to a planet impact. P1 still owns two planets to P2's one. P1 loses
one ship in each corrected game, versus zero/one in the original off/on games.
P2 survives with 77.33/77.22 health respectively. No full impact observer was
added, so this result does not attribute every intervening collision or shot.

These are two selected lifecycle qualification cases in one world and seat
configuration, not a general strength screen. The older climb-laser and defense
experiments remain reproducible on their frozen binaries. Any future promotion
comparison must account for the corrected shared recovery in both players;
the older stuck-opponent result is not a current baseline.

## Validation and retained evidence

**515 Rust tests pass:** 482 library/example tests and all 33 recovery integration
tests, including eight new regressions. Existing physical strike, terrain-loss,
boarding and departure tests cover both seats and varied bearings. The new
recorded-timeline regression fails on the old runtime and passes on the fix.
**879 Python tests pass**, including five new command/provenance and handoff-audit
tests. Package Clippy with `--no-deps`, formatting and the profiled release build
pass. Other integration suites were not rerun for this bounded task.

All **103,504 dense actor rows** pass the recovery, physical visit, route/budget,
laser and disabled-defense/clearance audits. The [manifest](data/recovery-rebuild-v1.json)
and [portable review bundle](data/recovery-rebuild-v1.json.gz) bind all **64 frozen
inputs**, both commands, baseline records, native witnesses, complete audits,
test/build logs and runtime binary. The two raw archives retain **32 files**:
**2,156,084,298 raw bytes** compressed to **313,923,568 bytes**. Every archive
member was verified before generated raw copies and source extractions were
removed. Original baseline archives remain intact. The work is committed locally.
