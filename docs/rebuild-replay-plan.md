# Isolated replay of the retained ledge recovery

The placement correction changes P1's route before P2 reaches the retained
ledge. Isolate the original sequence with recorded controls and reconstruct
P2's existing recovery task from its original birth, not from a fresh task at
the flag. This is a counterfactual native recovery probe, not a scored match.

## Frozen source and input

Use the complete World 1/P2 no-stop high-terrain run from runtime `e46f5a5`.
Preserve its world seed, geometry, match rules, asteroid interval and initial
state construction. Extract both actors' actions and native observations for
ticks 0 through 29421, the target's recovery telemetry, and every retained
one-second world physics audit. Hash the source archive, tape and source
summaries before executing either replay.

Build two executables with identical current replay/controller source. The
reference substitutes only the original `landing.rs` and `rebuild_placement.rs`;
the candidate uses the accepted alignment correction. Build the reference in
its own Cargo target directory to avoid sharing workspace artifact caches.
Freeze both binaries, source trees, commands and hashes before running.

## Prefix and warm task

Replay both actors' actual recorded actions. Require both native pilot
observations to match every tick through **23767**, removing only placement
diagnostics. Require every one-second world audit to match. Preserve the
recorded landing-query cadence with a read-only diagnostic sensor entry point.

Create the ordinary `RecoverShipTask` at its original start **16820** and advance
it on real observations. Add the unchanged high-terrain forecast. On all 6948
steps through the handoff, require its entire telemetry and all three encoded
controls to match the retained mission task exactly. No deserialized task
state, reset at handoff, pose edit, rule bypass or timer extension is allowed.
If either prefix check fails, retain the failure and do not run its forks.

The handoff must retain the original ground start 18278, high crossing completed
at 23226, new flag ownership at 23767 and 5400-tick ground allowance. Thus the
warm task contains the original route, history, charge and remaining time.

## Two continuations per executable

Clone the same native world and warm task at 23767:

1. Recorded continuation: keep both actors' original inputs. Run through 29421
   or native round end and record the first physical/task/control differences.
2. Live recovery: keep P1's original inputs, but use the same warm P2 recovery
   task's actual controls. Stop at native boarding/task success, bounded task
   failure, round end, or the fixed end 29421. This is the original task start
   plus its existing combined allowance and the first exceeded-budget tick.

Keep native build, landing, hatch, flight and relocation limits. Record dense
observations, actual and proposed actions, placement reports, task telemetry,
settling, boarding and world physics audits. Distinguish generated controls in
the recorded arm from controls actually applied to native physics.

## Acceptance and interpretation

The reference must exactly reproduce both the native recorded continuation and
the original live recovery's telemetry/actions/physical observations through
its terminal failure. Then require the candidate to retain its exact prefix
and complete a native rebuild followed by settled boarding and task success,
without another ship loss or pilot death. A placement rejection or preview
alone cannot qualify.

Preserve failures. There are two prefixes and at most four continuations, no
fresh random games, tuning or game retries. Audit-only repairs may reuse
hash-verified output if needed. Archive all raw evidence and export a compact
review bundle. Even success establishes only this isolated recovery sequence:
P1 follows a tape and cannot react, and this probe does not return P2 to its
mission controller after task completion. No full-match win or default bot
promotion follows from this result.
