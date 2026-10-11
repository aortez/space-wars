# One retained terrain failure recovers with the explicit bot candidate

The immediate Angular case completes a native rebuild, settles, boards, and
rejoins combat with the explicitly enabled #178 recovery candidate. The fresh
existing controller on identical combined physics remains blocked. The other
three candidate continuations still fail and produce the same encoded controls
as their fresh existing-controller comparisons. This establishes one recoverable
state, not a general retry policy or frontier qualification.

The [frozen comparison plan](terrain-recovery-integration-plan.md) preserves
the two different failure classes and the exact uncommitted terrain source
supplied by #174. Production retry eligibility, deadlines, and defaults remain
unchanged. All integration work is local on `bot-terrain-recovery-integration`;
neither GitHub PR was merged or updated.

## Controls and source integration

The incoming archive SHA-256 is
`cc8df0c516ab948690ce0ba704745266b4600705544e2acec5fac63ee86750e9`.
All 182 package files verify. Reconstructing the archive and diagnostic patch
verifies all 2,138 tested source files. Running the retained executable repeats
all four supplied cases; all **152 case files match byte for byte**.

Local snapshot `f5c368b` preserves the supplied terrain source and diagnostic.
Local merge `6083226` combines it with bot checkpoint `cada1eb`. The sole text
conflict is in the engine-rapier module declarations; both the terrain CCD and
clearance modules and the bot ball-cast/forecast modules are retained.

The unchanged diagnostic also passes on the combined source. All original
control and reset **actions and site requests** remain identical throughout
their full streams. Some decision/scenario checksums differ: the shared rebuild
alignment correction adds landing-angle evidence and can reject a candidate
earlier as `landing_misaligned`. The report retains these differences; it does
not claim that the two runtimes have identical observations.

The candidate runtime is `326aab6`, with 2,347 frozen input hashes and binary
SHA-256 `238e9d1ed8d16e4f0fe367f5ee296b7c0472e5e349bb8f971e36d399418040fc`.
Its test-only adapter enables the existing experimental recovery components
while preserving both RulePilotV4 combat controllers. Fresh existing and fresh
candidate tasks have identical initialization before those options are enabled.
There is one recovery survey per subject decision, and the opponent is reactive.

All four candidate comparisons retain the combined runtime's blocked and fresh
existing controls exactly: actions, samples, events, fork observations, blocking
decisions, and initial decisions. Their diagnostic-only controller reset is
not proposed as a production implementation. There were no simulation retries,
outcome tuning, seed substitutions, or post-run runtime changes.

## Results

Every fork continues for another 18,000 ticks (300 simulated seconds). These
are continuations of four fixed states from two encounters, not four independent
worlds or ordinary scored matches. There are no match-finish rules.

| Case | Fork tick | Fresh existing recovery | Fresh candidate recovery |
| --- | ---: | --- | --- |
| Angular P2, immediate | 8,065 | Blocks at 10,782 after four relocations | Blocks at 12,255, then rebuilds and completes at **21,917** |
| Angular P2, later | 14,400 | Pod stabilization blocks at 15,301 | Same controls and failure |
| Round P1, immediate | 3,072 | Supported-claim progress blocks at 3,973 | Same controls and failure |
| Round P1, later | 14,400 | No measured route at 14,730 | Same controls and failure |

No candidate subject incurs another ship loss during its continuation. The
unchanged blocked controls complete no subject recovery. The earlier handoff's
four fresh existing-controller failures are preserved.

## What the Angular success actually exercises

The two fresh Angular arms share landing at 9,668, pod exit at 9,670 and the
start of the rebuild stage at 9,861. Candidate actions first differ at **10,577**.
The candidate still exhausts its four relocation moves and blocks at **12,255**.
It then remains blocked for another **9,406 ticks**, until a native rebuild
at **21,661** triggers the already-existing one-time boarding exception.

| Milestone | Tick | Evidence |
| --- | ---: | --- |
| Settling forecast starts | 21,621 | Actual standing anchor `(32.404106, 48.5)`, terrain revision 800 |
| Forecast accepted and ship built | 21,661 | Offset -10.5, current checks pass, launch pose matches |
| First retained sample with settled landing | 21,780 | Native `landed`, two supported feet, 0.25 seconds settled; sampling is once per second |
| Recovery observed complete | 21,917 | Physically aboard its full ship, native settled landing, one original ship loss |
| Combat rejoined | 23,018 | RulePilotV4 records one combat return |
| Continuation ends | 26,065 | Aboard and flying, one completed recovery, still one ship loss |

The completion occurs after the original review's 18,000-tick horizon. Both
comparison arms receive the same extended continuation, so this is a matched
late recovery rather than a change in observation duration.

Four preview normals are captured, and all four are cleared after their
contexts change. **Zero reach arrival activation.** The accepted forecast has
no saved preview-normal context. The result therefore does not demonstrate
that saved-normal handoff fixed this terrain case. It establishes a gain from
the enabled recovery configuration as a whole; which active components are
necessary remains to be isolated.

The Angular candidate records 218 relocation surveys (11 consumed by the task),
with per-survey maxima of eight coarse/eight refined candidates and **236 offset
checks**, within the 240 limit. It performs no staging-route checks and provides
no high-terrain flight forecast. Its one settling forecast uses **160 physics
steps**, at most **four per tick**. The original construction and boarding
guards remain authoritative.

The later Round candidate records 305 surveys but consumes no normal capture
or settling-forecast job and changes no encoded inputs. At its fork and next
block it has no supported footing. The other two failed candidates reach none
of the rebuild mechanisms. A configured feature is not evidence that it executed
or addressed those failures.

## Validation, retained evidence, and next boundary

Passed on the combined candidate source: **425 AI library tests**, **96
ground/recovery tests**, **46 native rebuild tests**, strict bot Clippy with
Rust 1.89, formatting, a locked release build and the default-feature library
check. The supplied controls, combined controls, and four three-arm candidate
comparisons each pass their explicit diagnostic and material/physical audits.
This is not full workspace CI or Pi performance qualification.

The [machine-readable audit](data/terrain-recovery-integration-v1.json) contains
the runtime identity, comparisons, native completion receipt, actual feature
events and SHA-256 receipts for **496 generated evidence files**. The
[auditor](../tools/audit-terrain-recovery-integration.py) checks frozen inputs,
control retention, work bounds and native completion without running games.
Full evidence and the portable result archive live outside Cargo's target tree
under `/home/data/workspace/spacewars-handoffs/`.

Next, isolate the active changes responsible for the immediate Angular success
on this same frozen case before designing a bounded way to reconsider pod
landing. Keep the Round work separate: establish a usable starting footing and
route before expecting another claim/rebuild attempt to help. Failed attempts
still do not establish that either remaining state is unrecoverable. No terrain
or support defect has been established by this comparison.
