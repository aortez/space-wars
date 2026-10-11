# Fresh recovery screen finds an earlier rebuild that fails to recover

**Keep `terrain_recovery_supported_hatch_v1` experimental.** All 48 matches
finish and pass the evidence audit, but the behavioral promotion gate fails.
Of 32 paired candidate comparisons, 29 preserve every encoded game input and
the native match result. One completes an existing recovery 13.55 seconds
earlier. Two have adverse results, including a lost recovery and a new pilot
death. No comparison adds a completed recovery.

This is new evidence on merged main, not a replay-only qualification or a
comparison against the deployed Legacy v9 / Planner v10 bots. Production
policy, device settings and defaults are unchanged.

## Frozen source and comparison

The [plan](terrain-recovery-heldout-plan.md), test-only driver, runner and
auditor were committed at `26dac7c8a9f8259ce176cd27b210c9c0e466b784`, on
merged main `f717a7f5c528c514de30dec08fc1b2a991c4b464`. An isolated Cargo
target avoids reuse of another worktree's build metadata. The frozen executable
SHA-256 is `97a2508c9d8fceb68ea252cdb6b48e39d708a600cc81c03aad1c5ef3827d1de2`.

Both seats use Value v13. Each setting has a standard/standard control, the
named candidate on P1, and the named candidate on P2, with reactive opponents.
Initial world observation bytes match; initial controller differences are
limited to the two selected recovery flags. The paired driver enables native
refinement only while the selected recovery task is active. Other experimental
options remain off.

Two seeds were derived and frozen before execution: **A** is
`13656578344987333250`, **B** is `10294368162809575034`. Both Weak dirt shapes,
collapse off/on, and quiet versus Mixed asteroids every eight seconds produce
16 settings and 48 matches. Weak cohesion and impact fracture remain enabled
with collapse off. Starts have full health and three generated planets;
native pilot death or the unchanged 600-second deadline ends each match.
Combat intervals remain 15 seconds with four-second breaks.

All runs execute sequentially in fresh processes. There are no watchdog exits,
simulation reruns, changed seeds, controller resets, world edits or deadline
extensions. These are two seeds with repeated settings and seats, not 32
independent estimates of bot strength.

## All controls and paired outcomes

The table records controls; the three changed candidate comparisons follow.
The [compact receipt](data/terrain-recovery-heldout-v1.json) includes all 32
paired metrics, native losses/rebuilds, blocked reasons and validation hashes.

| Seed | Shape | Collapse | Asteroids | Finish tick | Winner | Recoveries P1 / P2 |
| --- | --- | --- | --- | ---: | --- | --- |
| A | Round | Off | Quiet | 24,335 | P1 | 1 / 0 |
| A | Round | Off | Mixed / 8 s | 16,924 | P1 | 0 / 1 |
| A | Round | On | Quiet | 22,529 | P2 | 1 / 1 |
| A | Round | On | Mixed / 8 s | 32,037 | P1 | 2 / 1 |
| A | Angular | Off | Quiet | 17,472 | P1 | 1 / 0 |
| A | Angular | Off | Mixed / 8 s | 8,837 | P1 | 0 / 0 |
| A | Angular | On | Quiet | 36,000 | P1 | 1 / 1 |
| A | Angular | On | Mixed / 8 s | 36,000 | P1 | 0 / 0 |
| B | Round | Off | Quiet | 12,619 | P1 | 0 / 0 |
| B | Round | Off | Mixed / 8 s | 17,913 | P2 | 1 / 1 |
| B | Round | On | Quiet | 12,660 | P1 | 0 / 0 |
| B | Round | On | Mixed / 8 s | 5,771 | P1 | 0 / 0 |
| B | Angular | Off | Quiet | 12,640 | P1 | 0 / 0 |
| B | Angular | Off | Mixed / 8 s | 10,563 | P1 | 0 / 0 |
| B | Angular | On | Quiet | 13,391 | P1 | 0 / 0 |
| B | Angular | On | Mixed / 8 s | 11,248 | P1 | 1 / 0 |

Controls total 13 recoveries, nine native rebuilds, 23 ship losses and 54
completed sorties. Candidate seats complete 12 recoveries; recovery can also
mean boarding an existing ship, so this count is distinct from native rebuilds.

All three changed comparisons use seed A with collapse on. Metrics below are
for the selected candidate seat, shown as control → candidate.

| Setting / seat | First changed input | Recoveries | Blocked ticks | Completed sorties | Finding |
| --- | ---: | --- | --- | --- | --- |
| Round, Mixed / P1 | 22,866 | 2 → 2 | 104 → 0 | 2 → 2 | Second recovery completes 813 ticks earlier; P1 wins both. |
| Angular, quiet / P1 | 32,159 | 1 → 1 | 1,743 → 2,058 | 3 → 3 | Native rebuild occurs 315 ticks later; boarding remains incomplete at the deadline. |
| Angular, quiet / P2 | 23,659 | 1 → 0 | 17,796 → 25,287 | 1 → 0 | Earlier refined rebuild is followed by an inaccessible return; candidate pilot dies. |

### Round improvement and Angular P1 delay

The Round arms select the same rebuild location. The precise approach first
changes input at 22,866; the controller moves to rebuilding at 22,953 instead
of 22,870. Subsequent placement attempts differ. The second native rebuild is
observed at 23,968 instead of 25,076, and recovery completes at 24,336 instead
of 25,149. The candidate avoids the control's 104-tick blocked interval.
Both seats then react to the changed battle. P1 retains two recoveries, two
sorties, two ship losses and its win; match finishes differ.

Angular P1 also first changes input during the precise approach to the same
rebuild target. Both arms exceed the task budget at 33,566. The control's
native rebuild grants its existing boarding opportunity at 35,309; the
candidate's does so at 35,624. Neither completes this recovery by 36,000.
The candidate therefore spends 315 more ticks (5.25 seconds) blocked, while
its earlier recovery, sorties, ownership and winner remain unchanged. The
trace identifies where the branches separate; it does not isolate which
later interaction accounts for every tick of delay.

### Angular P2: refined placement precedes the changed input

Both arms block at 7,410 with no reachable standing site with hatch access.
At **23,638**, the candidate's native refinement accepts offset **−11.5**,
after the ordinary −8 offset is rejected as hull-obstructed. The retained
placement receipt reports a complete 1.63-unit route and a settling angle
of about 7.71 degrees. The controller begins boarding the new ship.

The first encoded input difference is **23,659**, after that native rebuild.
Thus first changed input is not the first physical change in this case.
Earlier decision/scenario checksum changes at 4,910/7,109 include profile and
placement diagnostics; they alone do not identify physical divergence.

At 24,063 the candidate pilot is unsupported, the ship is not settled, and
the retained ground route reports `no_start_footing`. At **24,504**, the
pilot has support on planet 0 while the ship is landed on planet 1. Recovery
blocks with `other_planet` / “rebuilt ship has no accessible return.” It never
completes. The pilot dies at **33,563**; final ownership falls from the
control's one planet to zero. Both runs are P1 wins, so winner alone would
hide the regression.

The control instead rebuilds at offset **−8** at **25,206**, completes recovery
at **25,282**, completes one sortie, and remains alive at the match deadline.
Both arms record one native ship loss and one native rebuild. Rebuilding
earlier has not produced a successful recovery.

This is a reproducible adverse result of the selected profile. The retained
observations do not yet isolate placement stability, subsequent terrain motion
and return-controller execution. Do not attribute the failure solely to the
hatch-arrival option or loosen a native support/boarding rule on this evidence.

## Stalls, audit and cost

The 29 unchanged-input pairs also preserve native match results; 18 preserve
all compared action-record fields. Diagnostic differences in the remaining
unchanged-input pairs are not counted as new behavior.

Twelve selected candidate seats spend time controller-blocked. Their receipts
retain every reason, first block, longest interval and completion. The longest
single interval is 28,638 ticks in seed A / Angular / collapse on / Mixed / P2,
unchanged from its control. Ground-route stalls, unavailable standing sites and
pod-stabilization stalls remain. “Blocked ticks” measures the controller's
reported blocked state, not every possible form of inactivity.

The audit passes **2,461 frozen source files, 1,446 case artifacts, 872,482
action ticks, 579,441 overlapping comparison ticks, 14,566 material/physics
samples and 5,913 transition observations**. All 38 completion receipts show
the pilot aboard an available full ship with recovery/native refinement
cleared; all have later mission activity. Candidate seats execute 217,936
recovery decisions and 217,924 native-refinement decisions, excluding their
12 completion decisions.

Material conservation, finite/speed bounds, native refinement ownership and
shared work limits pass. Maximum shared work is four graph units and 138
physics queries within 4/384. Survey maxima are eight coarse candidates,
eight refined candidates and 240 offset checks. No staging-route check,
terrain-flight forecast or pod-reconsideration review is recorded. This screen
therefore does not newly exercise every component of the profile.

The matrix takes 484.99 wall seconds. Weighted desktop means are 0.07439 ms per
bot observation/decision, 0.002259 ms for shared planning, and 0.27665 ms for
native stepping. These are single-run descriptive costs excluding rendering
and trace serialization, not a whole-path Pi performance qualification.

**454 AI library tests and two Python tests pass**, as do strict AI Clippy,
default-feature compilation and formatting. Before this fresh matrix, the
frozen executable reproduces all 52,611 decisions, initial controllers and
initial/final world-observation bytes of the retained Angular 10101/P1 and
Round 20202/P2 named-profile checks. Those reused cases are not counted as
new outcomes. The fresh auditor passes without correction or simulation rerun.

## Next focused experiment and reproduction

Keep the profile opt-in. First reproduce seed A / Angular / collapse on /
quiet / candidate P2 from tick zero and instrument the native rebuild at
23,638 through the return failure at 24,504. Retain an unchanged control.
Compare refined placement and return execution separately from the same
pre-rebuild physical/controller state, preserving the native validity rules.
This will distinguish an unstable placement from a route/execution problem
before choosing a policy fix. The current saved observations are not complete
solver snapshots; a controlled fork requires replaying and cloning native
state, not reconstructing a world from observation JSON.

Keep the Angular P1 delay as a separate adverse case and the Round improvement
as a preservation check. Any eventual fix needs those retained checks plus a
new frozen seed set; tuning on these outcomes cannot turn this screen into
fresh validation. Deployed-policy comparison and current 1× Pi whole-path
timing remain later gates. The older Round opponent stall, sealed Angular
pocket, replacement-distance gate and late-match frame-time limits remain.

Raw commands, observations, controller states, inputs, binary, source manifest,
audits and validation logs are in `spacewars-handoffs/recovery-heldout-20261011`.
The portable package is `spacewars-handoffs/terrain-recovery-heldout-20261011.tar.gz`;
its incremental source bundle requires main `f717a7f`. Audit without simulating,
using the frozen source checkout:

```sh
python3 tools/audit-terrain-recovery-heldout.py /path/to/recovery-heldout-20261011
python3 /path/to/recovery-heldout-20261011/inspect-results.py
```

For a fresh reproduction, copy `runtime.json`, `source-inputs.json`, `matrix.json`
and `recovery-heldout` into a new output directory, then invoke
`tools/run-terrain-recovery-heldout.py` on it. The runner refuses an existing
`cases` directory and validates source-tool, matrix and binary hashes.

The qualification and report are local on `bot/recovery-heldout-qualification`;
the original checkout remains on merged main. No GitHub or deployment change
was made during this screen.
