# Rebuild alignment corrected; a native recovery completes on a different route

The shared placement check now rejects slopes that cannot meet the native
landing-angle requirement. The affected full game records a real rebuild,
settling, boarding and completed recovery for P2, followed by a win with one
ship loss. All three control games retain their actions and physical results.

**The original ledge-to-recovery chain remains unqualified.** The correction
also changes P1's earlier relocation preview, changing the match before P2
reaches that ledge. The successful recovery occurs on another planet. This is
evidence for the native placement correction and an actual recovery, not proof
that the previous ledge sequence now finishes. Default/frontier selection is
unchanged.

## Correction

The old placement test compared the proposed ship normal with the standing
pilot's support normal. Native landing instead compares the ship direction
with radial up at the ship's own origin. The retained first and replacement
builds passed the old test but remained above the native 20-degree limit.

For each existing candidate offset, the correction measures alignment at the
predicted resting ship origin. It shares the existing landing threshold and
rejects angles at or above 20 degrees as `landing_misaligned`. The angle is
recorded in both native placement reports and relocation previews. Existing
hull clearance, hatch footing, return-route and other-seat checks still apply.

This is a necessary placement condition, not a guarantee of two-foot support
after simulation. Boarding still requires actual native support, low motion,
and 0.25 seconds of settling. Physics, timers, relocation limits and the opt-in
terrain forecaster are unchanged. No empty ship is rotated or snapped into place.

The [frozen plan](rebuild-settling-plan.md) uses runtime source `8c9e333` on
`bot-rebuild-settling`, 950 input hashes and separate copied mission/lab release
executables. Four retained full games and 20 established lab trials ran without
retuning, game retries or audit repairs. No fresh seed search was performed.

## Complete games and native receipts

| Evaluated case | Outcome | End tick | Actions/native observations versus preceding run |
| --- | --- | ---: | --- |
| World 1 / P2 integrated | Loss | 13337 | Unchanged |
| World 3 / P1 integrated | Win | 21833 | Unchanged |
| World 3 / P1 no-stop | Loss | 11509 | Unchanged |
| World 1 / P2 no-stop | Win | 18215 | Changes after P1's relocation preview |

All three controls also retain their full non-timing reports. Placement metadata
is compared separately from physical state; status, counters, motion, support,
ownership and all actions remain part of the retention checks.

In the affected case, P1's preview at **12405** previously accepted offset -8
from the proposed standing point `(1.862, -30.309)` on planet zero. Its predicted
landing angle is **26.745 degrees**. The new check rejects it, and the remaining
preview candidates produce no usable site. P1's first changed action is at
**12422**, followed by the first changed native observation at **12423**. Earlier
metadata first differs at the unchanged P1 rebuild at 6395.

The subsequent P2 recovery is on its already-owned planet one:

| Native event | Tick | Evidence |
| --- | ---: | --- |
| Ship lost | 14295 | Native solar-heat loss and pod ejection |
| Leaves pod on planet one | 16682 | Native on-foot location |
| Replacement built | 17169 | Rebuild counter 1; selected predicted angle 0 degrees |
| Settled | 17244 | Two supported feet; angle 0.020 degrees; settled time 0.25 s |
| Boards replacement | 17245 | Native `aboard: 1`, ship form and ready transfer |
| Recovery completes | 17246 | Mission completed-recovery counter becomes 1 |
| Departs | 17252 | Aboard, no supported feet, ascending in native flight |

The portable bundle includes all 84 consecutive observations and actions from
rebuild through departure, plus the loss and landing handoff witnesses.

| Affected P2 result | Previous high-terrain run | Placement correction |
| --- | ---: | ---: |
| Outcome | Win at 36000 | Win at 18215 |
| Pilot deaths | 0 | 0 |
| Ship losses | 2 | 1 |
| Completed recoveries | 0 | 1 |
| Final pilot health | 100 | 66.72 |
| Owned planets | 2 | 2 |

The new game ends when P1 dies from a native boundary impact. The earlier route
change affects both players and the remainder of the battle; this comparison
does not isolate an improvement in combat strength. There are no high-flight
launches in this affected run, and no receipt for the prescribed high crossing,
new planet-zero claim, rebuild and boarding chain.

Across the four games, the audit checks 1255 distinct placement reports and
323 alignment rejections, including previews. These are candidate checks, not
323 actual rebuild attempts. All accepted candidates satisfy the angle gate.
Physical, capture, stopping, observer and applicable ground/terrain audits pass;
the observer performs no control overrides.

## Existing lab failure isolated with the preceding runtime

The candidate matrix accepts **19/20** cases: 17 expected completions and two
intentional bounded failures. Seed 42 / seat 1 / ordinary capture unexpectedly
stalls on its ground route at **3047**, before any rebuild. Its native physics
audit passes, but its required capture outcome fails.

A supplemental control was frozen before execution using the preceding source
`832228e`, with all 942 non-artifact inputs matching the earlier experiment.
Its own copied executable ran the same 20 cases once. It also accepts **19/20**,
with the identical capture failure. No candidate was rerun or retuned.

All 20 old/new cases have identical recorded physical samples, transition
events and final physics audits after removing placement diagnostics and wall
time measurements. Each case has 180 one-second physical samples; this is not
an every-tick action comparison. Full non-timing reports match in 16 cases,
including the failed capture case. The four rebuild-edit cases differ in
placement diagnostics while retaining their physical samples and events.

This establishes that the observed lab failure predates this correction. The
original frozen screen still required all 20 expected outcomes and the original
ledge chain, so its recorded result remains **not qualified**. The supplemental
comparison does not rewrite that result.

## Validation and retained evidence

Passing checks: **560 native tests**, **40 ground-navigation tests**, **33 bot
recovery tests**, **945 Python tests**, formatting, bot-scoped Clippy with
warnings denied, and the locked Rust 1.89 release build of both harnesses.
The native suite includes both retained failed poses and the
valid-placement/read-only query check. Scenario-wide Clippy was not part of
this check.

The [manifest](data/rebuild-settling-v1.json) records frozen commands, source,
binary and input hashes, full-game comparisons, recovery receipts, baseline
lab results, validation and 44 lossless archive inventories. The
[portable review bundle](data/rebuild-settling-v1.json.gz) includes frozen sources
and plans, full game reports/audits, all 40 lab reports, detailed placement and
boarding witnesses, logs and the reproducible exporter. Full raw game streams
remain under `target/rebuild-settling/v1/archives/`.

The next focused validation is a controlled replay of the original ledge
sequence, preserving its earlier native trajectory so the P1 preview change
cannot remove the scenario. Verify that trajectory before testing late rebuild
and boarding. The pre-existing lab capture stall and earlier narrow-flight
forecast discrepancy remain separate follow-ups. No bot promotion or remote
deployment is included here.
