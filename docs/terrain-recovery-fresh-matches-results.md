# The opt-in recovery driver preserves ordinary combat matches

All **32 candidate-versus-control comparisons** retain identical encoded inputs,
site requests, decision checksums and scenario checksums through their complete
native matches. Winners, finish ticks, deaths, ship losses, native rebuilds and
combat returns are unchanged. No controller blocks during any of the 48 runs.

This is an ordinary-combat compatibility result. **None of these matches needs
pod reconsideration, an advanced rebuild survey or terrain-flight forecasting.**
The candidate does enter ordinary recovery: its selected seats execute 65,203
recovery decisions and complete 19 recoveries. The difficult recovery features
remain covered by their retained targeted experiments, not newly exercised here.

## Source and comparison

The [plan](terrain-recovery-fresh-matches-plan.md) and runtime were frozen at
`9d781bc` before execution. There were 16 settings, each with an unchanged
control, candidate P1 and candidate P2. Every arm starts from tick zero, with
full-health ordinary material-combat starts and a reactive opponent. Match rules
include pilot deaths and the unchanged ten-minute limit. No controller is reset.

The matrix uses seeds 42 and 777, Weak Round/Angular, slumping off/on, and no
asteroids versus Mixed every eight seconds. Slumping is the collapse switch;
Weak cohesion and impact fracture stay enabled in both slumping configurations.
All bots retain 15-second combat breaks lasting four seconds. The candidate is
the ordinary opt-in search/arrival driver used by the retained Angular result;
native saved-normal and settling-forecast selection remain off.

The neighboring #174 checkout still matches all frozen terrain source bytes.
Only three documentation/results paths differ from `f5c368b`. Its published
head remains `52f9e18`; **verification against an eventual final published
terrain source is still open**. The source comparison and both PR head receipts
are retained with the run.

Gameplay source also remains unchanged from the retained automatic Angular
runtime `7134dbe`. Subsequent Rust changes are developer test harnesses and the
unused read-only posture diagnostic. The source comparison checks that removing
that diagnostic's module declaration restores the original posture source.
Therefore the [heavy Angular result](terrain-recovery-reconsideration-results.md)
is reused as historical evidence, not claimed as a new execution: recovery at
21,849, combat return at 22,523, and a later ship loss at 23,824. The
[Round limitation](terrain-recovery-round-opening-results.md) also remains.

## Native outcomes

Each row below describes the control and both matching candidate arms. Counts
are P1/P2. These are 16 settings repeated three ways, not 48 independent seeds.

| Seed | Shape | Slumping | Asteroid interval | Finish tick | Winner | Completed recoveries | Ship losses |
| ---: | --- | --- | ---: | ---: | --- | --- | --- |
| 42 | Round | Off | Off | 4,506 | P1 | 0 / 1 | 0 / 2 |
| 42 | Round | Off | 8 s | 3,960 | P1 | 1 / 0 | 1 / 1 |
| 42 | Round | On | Off | 3,571 | P1 | 0 / 1 | 0 / 2 |
| 42 | Round | On | 8 s | 3,960 | P1 | 1 / 0 | 1 / 1 |
| 42 | Angular | Off | Off | 17,896 | P2 | 2 / 2 | 3 / 2 |
| 42 | Angular | Off | 8 s | 3,960 | P1 | 1 / 0 | 1 / 1 |
| 42 | Angular | On | Off | 3,570 | P1 | 0 / 1 | 0 / 2 |
| 42 | Angular | On | 8 s | 3,960 | P1 | 1 / 0 | 1 / 1 |
| 777 | Round | Off | Off | 4,610 | P2 | 0 / 1 | 1 / 1 |
| 777 | Round | Off | 8 s | 5,582 | P2 | 0 / 1 | 1 / 1 |
| 777 | Round | On | Off | 4,610 | P2 | 0 / 1 | 1 / 1 |
| 777 | Round | On | 8 s | 4,882 | P2 | 0 / 1 | 1 / 1 |
| 777 | Angular | Off | Off | 4,610 | P2 | 0 / 1 | 1 / 1 |
| 777 | Angular | Off | 8 s | 5,886 | P2 | 0 / 1 | 1 / 1 |
| 777 | Angular | On | Off | 4,610 | P2 | 0 / 1 | 1 / 1 |
| 777 | Angular | On | 8 s | 6,053 | P2 | 0 / 1 | 1 / 1 |

Every match ends by pilot death, between 59.5 and 298.27 simulated seconds.
The unchanged controls record 19 native rebuilds/completed recoveries, 18 combat
returns and 35 ship losses. Each candidate group preserves those totals. Across
the three arms, all 57 completion receipts show an available full ship, the
pilot aboard, native landed status, two supported feet and 0.25 seconds settled.
Completion and return to combat are counted separately.

These matches stop at death, unlike the earlier five-minute survival duels.
They are light-to-moderate terrain workloads and do not exercise the long blocked
wait of the heavy Angular case. No positive claim about additional recoverability
or candidate strength follows from the unchanged outcomes.

## Checks and cost

The [compact audit receipt](data/terrain-recovery-fresh-matches-v1.json) links the
full audit by SHA-256. The audit verifies 2,379 frozen source inputs, the retained
binary, 1,197 case artifacts, 258,678 action ticks and 4,329 material/physics
samples. Candidate/control comparisons cover 172,452 overlapping ticks exactly.
It also verifies that the only initial private-controller difference is the
single selected recovery-profile flag.

All 48 simulations ran once and passed. **433 AI library tests, 117 ordinary
recovery/navigation/gameplay tests, strict AI Clippy, the default-feature library
check, formatting and the locked Rust 1.89 release build pass.** The scenario
crate was not changed or rechecked for Clippy; the previous seven existing
native warnings remain an open whole-workspace check limitation.

The first auditor attempt incorrectly equated full pre-decision and
post-completion pilot observations. Completion clears the task and changes the
requested landing-site survey; typed f32 JSON also differs textually from
snapshot JSON promoted through Value. The corrected audit compares all remaining
pilot fields at their original f32 bits, and separately verifies native completion.
Both audit logs are retained. No source runtime, controls or simulation outcome
was changed or rerun for this correction.

Sequential desktop runs take 54.13 wall seconds in total. Mean sensor time is
approximately 0.02514 ms per bot decision in both groups; mean native step time
is approximately 0.105 ms. Timings exclude trace serialization, other diagnostic
reads and rendering. The advanced features never execute, so these measurements
do not qualify their cost or establish Pi frame rates.

## Handoff and remaining gates

Raw output, frozen executable, source manifest, commands, source comparisons,
timings, failed/successful audit logs and validation logs are in
`spacewars-handoffs/recovery-fresh-matches-20261010`. The portable package is
`spacewars-handoffs/terrain-recovery-fresh-matches-20261010.tar.gz`; its source
bundle builds on the prior `6600659` Round-opening results commit.

Work is local on `bot-terrain-fresh-matches`. The original #178 checkout remains
at `cada1eb`. No PR update, deployment or bot-default change was made.

The next implementation gate is opt-in integration with the launcher's mission
bot and its observation adapter, followed by fresh generated-world comparisons
in both seats. Preserve the selected recovery profile across task creation and
mission transitions, clear per-pilot native options on completion/replacement,
and retain the existing work limits. This combat screen does not certify those
mission paths, selected-preview handoff, depositing terrain during approach or
construction, or device cost. Final #174 source verification remains necessary
before declaring the combined work ready to merge.
