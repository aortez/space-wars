# Search finds a placement, but the staging walk misses its deadline

Task-owned search progress fixes the repeated-neighbor problem. The live task
now tests **27 distinct refined bearings**, reaches the known placement at
**343**, and selects a measured staging walk at **24420**. The pilot physically
walks toward that point and remains supported and balanced throughout.

**Recovery remains unqualified.** At the original search deadline, the foot is
approximately **0.133 units** from the staging point; supported arrival requires
less than **0.12**. The task blocks at **24548** with
`rebuild staging exceeded the original search deadline`. There is no staging
arrival, final relocation, rebuild or boarding in this live continuation.
This is progress on the first walking leg; the longer route to a buildable
footing still needs physical validation.

## Frozen comparison

The [plan](rebuild-staging-plan.md) freezes source `22b3740`, 967 input hashes,
one copied release executable and two commands. The control runs the preceding
opt-in refinement; the candidate additionally enables persistent search and one
staging move after the original handoff at **23767**. Each command runs an
original prefix and recorded-controls/live-task continuations.

All **22 files** from the [preceding refinement](rebuild-refinement-results.md)
match the control byte for byte, including its two refinement audits. The new
staging audits are additional files. Both prefixes preserve 23,768 tick rows,
47,536 native pilot observations, 6,948 task/control steps and 396 world audits.
Task birth at **16820**, the high crossing completed at **23226**, the original
claim and the 5,400-tick ground allowance remain unchanged.

There are two planned prefixes and four continuations, no new seeds, no
simulation retries and no audit repairs. P1 follows its original tape. This is
an isolated task comparison, with no scored-match or default/frontier promotion.

## Persistent search reaches usable previews

Search history belongs to the task and is passed into read-only sensors. The
same request can be read repeatedly without consuming candidates or mutating
physics. Visited bearings reset after a planet/revision change, movement beyond
0.5 units from the search origin, or 300 ticks. Placement and route checks always
use fresh measurements.

The ordinary search and accepted original offsets retain precedence. Bounds
remain eight coarse candidates, eight refined candidates and 240 placement
offset checks per survey. Staging adds at most sixteen route queries; this live
run uses at most **two**. Actual native build attempts retain the same 26-offset
limit and retry cadence.

| Survey tick | Newly tested refined bearings | Cumulative distinct bearings |
| --- | --- | ---: |
| 24270 | None | 0 |
| 24300 | 311, 324, 310, 325, 312, 326, 313, 329 | 8 |
| 24330 | 330, 341, 331, 340, 332, 342, 333, 339 | 16 |
| 24360 | None | 16 |
| 24390 | 309, 314, 308, 307, 315 | 21 |
| 24420 | 334, 343, 344, 338, 345, 337 | 27 |

At **24420**, three measured footings pass native placement checks, but their
precise outbound routes exceed 24 units:

| Bearing | Selected offset | Precise route length | Predicted resting angle | Hatch route |
| --- | ---: | ---: | ---: | ---: |
| 343 | -11 | 24.0542 | 0.0198 degrees | 3.4903 |
| 344 | -10.5 | 24.4817 | 0 degrees | 3.0628 |
| 345 | -10 | 24.9058 | 0 degrees | 2.6387 |

These are current previews at planet zero, revision 21. The earlier read-only
probe was at 24330 and only evaluated placement where its ordinary outbound
route fit within 24 units. Its coverage and numbers should not be treated as
an exhaustive placement result for this later snapshot.

The first valid preview, 343, yields a **2.2213-unit walking prefix** and an
independently measured **21.8328-unit onward route**. The staging point is
`(18.992851, -30.823528)` in the planet's local frame. The measured full route
includes one flight and three jumps. Only its short walking prefix is attempted
here.

Each admitted leg satisfies its own limit: staging is a 2–4-unit walk, and the
remaining precise route is at most 24 units. The full candidate route is capped
at 28. This admits a staged proposal without changing the final-leg limit or
any placement, landing, hull or hatch guard. Only one staging move is allowed,
and it counts toward the existing four relocations.

## Actual movement stops just outside staging arrival

The missing-site clock begins at **24247**. Selecting staging at 24420 retains
that clock. Arrival would also retain it and require a fresh target check;
neither a preview nor a staging proposal grants build validity.

| Milestone | Tick |
| --- | ---: |
| Staging proposed and accepted | 24420 |
| New ground task begins | 24421 |
| Measured walking path becomes available | 24435 |
| First walking control is applied | 24437 |
| First changed native pilot observation | 24438 |
| Last allowed search control tick | 24547 |
| Original search deadline blocks further control | 24548 |

The new ground task spends **16 rows surveying** before its first walking
control. It follows bearings `296 → 297 → 298 → 299 → 300 → 301` and then slows
proportionally on the final approach. The complete 129-row interval from
proposal through stopping is retained. All rows have native support on planet
zero and a balanced pilot.

| Tick | Reconstructed distance to staging point | Generated horizontal control |
| --- | ---: | ---: |
| 24420 | 2.2406 | 0 |
| 24480 | 1.0024 | -0.3609 |
| 24500 | 0.5488 | -0.1976 |
| 24530 | 0.2254 | -0.0811 |
| 24547 | 0.1370 | -0.0493 |
| 24548 | 0.1334 | 0; terminal row is not applied |

Distances are approximate reconstructions from recorded world poses and the
shared foot height. The runtime's unchanged float32 arrival check is strictly
below 0.12. The final error is almost entirely along the walking direction:
about 0.1334 tangentially and 0.0023 normally. The dedicated staging telemetry
never reports arrival. The old `arrived` ground telemetry on the proposal row
belongs to the earlier claim task, before the new ground task starts.

P2 ends alive at **100 health**, with the original one ship lost and no rebuild.
Movement restarts the native build timer; its final rebuilding progress is
0.0979, with no ship spawned. The task cannot treat that timer as a completed
placement search or use it to renew the missing-site allowance.

The next focused change should investigate the delay before walking and the
short walk's proportional slowdown, preserving supported arrival and the
original deadline. Reusing the already measured staging route or using the
existing continuous-walk behavior are candidates to evaluate, not proven fixes.
After staging, fresh placement, the onward flight/jump route, precise final
arrival, native rebuilding, settling and boarding remain to be qualified.

## Recorded controls remain the preceding native result

Across all **5,655 recorded-controls rows**, native pilot observations,
applied controls, generated controls, landing diagnostics and world audits
match the new control exactly. Task telemetry differs only by added search
history. There is no staging proposal in that arm.

It therefore repeats the prior native build at **27819**, two-foot settling
at **28227** and no boarding. Its final diagnostic task reason remains
`spaceling stopped making progress on ground route`. This is the same tape-driven
result already recorded in the preceding refinement, with no new autonomous
recovery evidence.

## Validation and retained evidence

Passed: **147 Rust tests** (6 native placement, 41 ground navigation, 37 surface
recovery and 63 replay harness), **956 Python tests**, bot library/tests/example
Clippy with warnings denied, formatting, the locked Rust 1.89 release build,
and the default-feature AI library check with its native dependency.

The new tests cover advancing search, geometry/history invalidation, bounded
staging routes, supported arrival, cloned and repeated task steps, fresh target
checks while rebuilding, the original missing-site deadline, and native build
precedence even while controls are disarmed. Broad native Clippy still **fails
with the same 16 message/file diagnostics** as the preceding refinement. Its
failed log and the baseline comparison are retained.

The [manifest](data/rebuild-staging-v1.json) and
[portable bundle](data/rebuild-staging-v1.json.gz) retain all reports/audits,
control comparisons, all 129 staging rows, derived movement diagnostics,
recorded witnesses, the 11 frozen changed files, validation logs, exporter and
source/command/binary provenance. Complete raw traces remain in verified
lossless local archives:

| Archive under `target/rebuild-staging/v1/archives/` | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control.tar.gz` | 256,959,297 | 38,682,860 |
| `staged.tar.gz` | 254,978,770 | 38,589,547 |

Work remains local on `bot-rebuild-staging`. The new search is opt-in; defaults
and deployment are unchanged.
