# Acquisition-defense clearance admission

**Retain the experiment; keep defaults unchanged.** The clearance check removes
the diagnosed survival regression while preserving the known rescue. All 31
games complete and qualification passes, but none of the eight fresh pairs
produces an eligible defense proposal. Their gameplay is identical and the
screen fails `no_useful_fresh_change`. The [frozen plan](acquisition-clearance-plan.md)
defined this decision before games; there is no runtime retuning or promotion.

## Implementation

`--acquisition-clearance-seats` defaults to `none` and requires the original
acquisition-defense option for that v13 seat. It admits the existing escape
proposal only when its minimum forecast clearance is finite and nonnegative.
Negative or nonfinite proposals retain the native capture intent and task before
any defense clock, source cooldown, pursuit deferral or coordinator event starts.
Later fresh hits may propose again. The forecast and subsequent escape guidance
are unchanged.

Separate bounded telemetry records the check counters and last decision, including
the consumed clock, native capture, original actions, proposed direction and
forecast. Reset retains configuration while clearing the receipt. With the new
option disabled, its telemetry is absent and the original experiment remains
reproducible. No new sensor queries or planner budgets are introduced.

Runtime, runner, tests and plan were committed at `2d38826` before the binary and
matrix were frozen. The runtime binary SHA-256 is
`ce89044e82ef2a283acb95f61b8637e6243bb63fb4fd620c622b480aa61b643c`.
All 57 frozen input hashes remain bound to that experiment.

## Known qualification

Ten new-option-disabled replays first reproduce both original defense states for
all five known cases: **94 original streams match byte for byte**, and all
non-timing reports, sensors, charged planning and derived gameplay audits agree.
Only then are the five gated qualification games run. All five qualify.

| Case | Defense disabled | Original defense | Gated defense |
| --- | --- | --- | --- |
| Diagnosed loss / known rescue | loss | win | win, original gameplay retained |
| Same world, climb laser off | win | win | win, unchanged |
| Earlier rescued draw | draw | draw | draw, unchanged |
| Earlier ordinary-v13 reference | win | win | win, unchanged |
| Boundary counterexample | loss at 18,397 | loss at 15,971 | loss at 18,397, baseline gameplay restored |

The known rescue admits its sole proposal at **31,361**, with forecast clearance
**69.30**. The entire game matches original defense after removing only clearance
telemetry. It retains the four earlier completed departures, the fixed deadline
at 32,081, later ship loss and the pilot's survival to the 36,000-tick win with
**83.44 health**. This preserves the previously observed survival rescue; it does
not add a later capture or guarantee permanent ship survival.

The boundary case rejects **75 proposals**, from **15,455 through 15,579**, with
forecast clearances between **−16.64 and −15.94**. None starts defense. Every
rejection preserves its native actions, capture receipt and coordinator task.
The complete game matches defense-disabled after stripping only option telemetry,
including non-timing reports, sensors, charged planning and physical results.
The same departures at **3,155 and 5,545** remain complete. The ship is lost at
15,580 and the pilot at 18,397, restoring **2,426 ticks / 40.43 seconds** versus
the failed original defense. It remains a loss, not an improvement over the
no-defense baseline.

Against original defense, the first rejection also has the exact same source
observation, capture receipt, proposed forecast and saved native actions. This
tests the admission rule's effect on the diagnosed case without changing the
collision model. The earlier diagnosis's limits on full-ship contact attribution
still apply; the repaired trajectory does not establish a new physics claim.

## Fresh screen

The separately frozen fresh screen compares defense-disabled against defense
plus clearance admission in **two new world clusters**, both evaluated seats,
asteroid intervals 0/3 seconds, and retained v10. Climb laser stays enabled for
the evaluated seat. This measures the combined defensive policy, not the
admission rule in isolation. The ordinary-v13 reference appears only in known
qualification.

All eight pairs have **zero proposals and zero handoffs**. They retain complete
gameplay parity after removing only option telemetry, including non-timing
reports, sensors, charged planning and both players' physical results. Counts
below refer to the evaluated bot and are identical in both options. `A3` means
an asteroid every three seconds.

| Fresh pair | Result, disabled → gated | Ship losses | Completed departures |
| --- | --- | ---: | ---: |
| World 0, A0, P1 | win → win | 0 | 3 |
| World 0, A0, P2 | loss → loss | 1 | 0 |
| World 0, A3, P1 | win → win | 0 | 3 |
| World 0, A3, P2 | win → win | 0 | 0 |
| World 1, A0, P1 | loss → loss | 1 | 3 |
| World 1, A0, P2 | loss → loss | 1 | 3 |
| World 1, A3, P1 | win → win | 1 | 4 |
| World 1, A3, P2 | loss → loss | 1 | 1 |

Fresh totals are **4 wins / 4 losses, 17 completed departures, one completed
recovery, five ship losses and three pilot deaths** per option. Both options
have the same 20 abandoned visits and one unfinished visit. Progress eligibility,
no-progress duration and ownership also match. No earlier victory censors a
different baseline continuation in any fresh pair. There are no individual
regressions and no observed benefits.

This screen demonstrates inactivity on these worlds, not general effectiveness
or a broadly tested safety benefit. The active evidence remains the selected
known rescue and diagnosed counterexample. The next bounded correctness task is
the separately documented [stale recovery after a physical rebuild](pursuit-climb-laser-loss.md):
a previously blocked recovery task can remain active after the ship is restored.
That task needs its own reproduction, fix and qualification.

## Validation and evidence

**608 Rust tests pass**, covering library, integration and example tests, including
seven new clearance regressions. **874 Python tests pass**, including ten new
matrix, command, rejection, admission, provenance and survival-gate tests.
Formatting, package Clippy with `--no-deps`, and the profiled release build pass.
All **1,556,494 dense actor rows** pass physical, action/observation, route,
budget, defense, laser and clearance audits. The six qualification games with
inherited partial impact observers retain **275,268 native impact rows**, with
zero overrides. No audit corrections or game retries are needed.

The [manifest](data/acquisition-clearance-v1.json) and
[portable review bundle](data/acquisition-clearance-v1.json.gz) bind the frozen
inputs, commands, binary, comparisons, audits, selected native witnesses and logs.
Raw archives remain under `target/acquisition-clearance/v1/archives`: **471 files,
32,368,892,683 raw bytes in 4,474,915,241 compressed bytes**. Every member is
verified before generated raw copies and source extractions are removed. Prior
archives and failed experiments remain intact. Work is committed locally;
defaults remain unchanged.
