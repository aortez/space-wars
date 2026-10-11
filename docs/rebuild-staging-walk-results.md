# Measured staging handoff completes the retained recovery

Continuous staging plus reuse of the measured ground map completes the original
live recovery. The pilot reaches staging, follows a fresh build-site route,
recovers from a later loss of footing, rebuilds, waits for two-foot settling and
boards the replacement. Native rebuilding is first observed at **27114**,
boarding at **27603**, and task completion at **27604**.

This qualifies the **isolated recovery chain**, with **one original ship lost**,
one rebuild and no additional ship loss. The pilot finishes at **60.67 health**
after an impact during the recovery. It does not qualify a full match, establish
reliability across seeds, or promote the bot defaults/frontier. The changes
remain local and opt-in.

## Frozen comparison

The [plan](rebuild-staging-walk-plan.md) fixes three modes, source **f99d646**,
971 input hashes, 11 changed files and one copied release executable before
running three original prefixes and six continuations. P1 follows the original
tape throughout. No fresh games, simulation retries or adaptive tuning occurred.

Every prefix matches: 23,768 tick rows, 47,536 native pilot observations,
6,948 task/control steps and 396 world audits. Task birth stays at 16820, the
completed high crossing at 23226, the handoff at 23767 and the ground allowance
at 5,400 ticks. The control reproduces all **22 preceding staged files** byte
for byte, including both staging audits.

| Live mode | First walking control | Supported staging arrival | Deadline margin | Terminal result |
| --- | ---: | ---: | ---: | --- |
| Retained staged control | 24437 | None | None | Staging deadline at 24548 |
| Continuous staging | 24437 | 24539 | 8 ticks | Four relocations exhausted at 27300 |
| Continuous staging + map handoff | 24423 | 24521 | 26 ticks | Recovery complete at 27604 |

All modes select the same staging proposal at 24420. The original search began
at 24247; the last permitted search-control tick remains 24547. Neither proposal,
map reuse nor arrival renews that allowance. The strict supported arrival radius
remains below 0.12 units. Reconstructed arrival distances are approximately
0.1168 for continuous staging and 0.1179 for handoff; the old control stops at
0.1334 without arriving.

The handoff seeds the measured path `[296, 297, 298, 299, 300, 301]` on the
proposal tick. Its map retains native tick 24420, planet 0 and revision 21; the
ground controller validates it and measures a complete 2.2213-unit walking
route. Movement begins on later task steps, without another physics survey or
a fabricated forecast. The ordinary path becomes available only at 24435 in
the other modes.

Handoff starts walking **14 ticks earlier** and arrives **18 ticks earlier** than
continuous walking alone. Continuous input applies to forward walking interiors;
final approach retains proportional steering. Both new runs briefly lose support
while traversing staging, and neither reports arrival until actual support returns.
The new continuous behavior ends with that staging ground task.

Compared with control, continuous staging first changes controls at 24437 and
native pilot observations at 24441. Handoff adds request telemetry at 24270,
changes controls at 24423 and changes native pilot observations at 24424.

## The onward recovery and its remaining weakness

Both new modes request a fresh preview and select the same refined footing,
bearing **343**, at **24540**, before the unchanged search deadline. The measured
route is about 21.8328 units, including one 5.7429-unit jetpack flight. The live
controllers execute that flight and the following ground jumps; they physically
arrive at 343 at 25420 (continuous) and 25421 (handoff). Continuous staging is
disabled for this onward task.

Arrival does not guarantee that the pilot can remain there while building:

| After first build-site arrival | Continuous staging | With map handoff |
| --- | ---: | ---: |
| Terrain revision changes from 21 to 22 | 25537 | 25536 |
| Foot moves more than 0.5 units from the selected footing | 25672 | 25585 |
| First subsequent loss of support | None before next selection | 25585 |
| Next fresh relocation | 25860 | 26370 |

In the handoff run, the pilot loses balance at 25586 and takes a **39.33-health
planet impact at 25613**. The recorded foot moves as far as about 8.32 units from
the first selected footing before recovering. The task subsequently selects
nearby bearing **344** at **26370**, its third relocation including staging,
and reaches that measured footing at **26687**.

Native construction succeeds at **27114** with refined offset **-10**, predicted
settling angle **0 degrees** and a valid 2.1354-unit hatch route. The original
eight-second native build interval and placement guards remain in force. The
replacement reaches two-foot landed status at **27188**, and stays landed for
all **417 rows through 27604**. Actual boarding at **27603** has two supported
feet, 0.25 seconds settled and about 0.0343 degrees of landing error. Controls
are briefly disarmed on the build/boarding event frames; the task records the
build and completion on the following ticks, 27115 and 27604 respectively.

Continuous staging alone reaches the first preview, but its later native
placement attempts fail. It consumes relocations at 24420, 24540, 25860 and
26700 and blocks at 27300, with no rebuilt ship and 100 health. These preserved
differences show why this single successful trajectory is insufficient to claim
general reliability: both modes selected the same initial build site and arrived
one tick apart, then experienced different physical outcomes.

The next focused work should investigate holding or revalidating build footing
after terrain changes, and validate this complete chain on a predetermined
broader set of recoveries with both opponents reacting. Keep the successful
case and the failed continuous-only case as regression evidence.

## Recorded controls and bounded work

All three recorded-controls arms preserve the same 5,655 native pilot rows,
applied/generated controls, landing diagnostics and world audits. Continuous
staging is byte-identical to control throughout that trace. Handoff differs only
by its map-request flag in observations/search telemetry; there is no staging
proposal in the recorded arm. All repeat the preceding tape-driven build at
27819, settling at 28227 and no boarding through fixed end 29421.

Live survey counts are 6, 12 and 8 for control, continuous and handoff. Every
survey stays within eight coarse/eight refined candidates, 240 offset checks
and the existing staging-route bound; observed staging-route checks peak at two.
Handoff adds one pure route query when accepting the single staging proposal.
Relocations stay within four, world audits pass, and terminal controls are not
applied.

## Validation and retained evidence

Passed before freezing: **151 Rust tests** (6 native placement, 41 ground
navigation, 41 surface recovery and 63 harness), **958 Python tests**, bot
library/tests/example Clippy with warnings denied, formatting, the locked
Rust 1.89 release build and the default-feature AI library/native dependency
check. Broad native Clippy still **fails with the same 16 message/file diagnostic
identities** as the preceding staging baseline; this remains a failed check.

One **audit-only repair** followed the completed simulations. Exact endpoint
equality rejected `-30.823528289794922` versus `-30.82352828979492`, two JSON
encodings of the same native float32 coordinate. The auditor now uses the
runtime's 0.01 endpoint tolerance with a 0.000001 reconstruction allowance,
and a regression test verifies this case and rejects out-of-range endpoints.
All **959 Python tests** then passed. The audit resumed from hash-verified raw
outputs for all three modes; source/binary/commands used by the simulations were
unchanged. The original failure, corrected auditor, frozen originals and audit
resume are retained.

The [manifest](data/rebuild-staging-walk-v1.json) and
[portable bundle](data/rebuild-staging-walk-v1.json.gz) include all reports and
audits, dense staging rows, onward movement timelines, impact/placement/boarding
witnesses, first differences, validation logs, frozen source, exporter and
source/command/binary provenance. Full raw traces remain in verified lossless
local archives:

| Archive under `target/rebuild-staging-walk/v1/archives/` | Raw bytes | Compressed bytes |
| --- | ---: | ---: |
| `control.tar.gz` | 254,979,038 | 38,589,787 |
| `walk.tar.gz` | 305,650,835 | 47,160,847 |
| `handoff.tar.gz` | 310,176,213 | 48,001,540 |

No push, PR, merge, deployment or default change is included.
