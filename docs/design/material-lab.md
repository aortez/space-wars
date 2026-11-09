# Material Lab: loading, release and return (#327)

`material-lab` is the first application scenario using the bounded native
[impact-to-support path](material-impact-support.md) and its
[changing-load extension](material-changing-load.md). Select **Show all scenarios**
in the launcher, choose **Material Lab**, then start. Direct launch:

```sh
cargo run -p engine-client --profile ci -- --scenario material-lab
```

The lab starts paused. Up/down selects a control; Space or the gamepad bottom
face activates it. The controls are Run/Pause, Single step, Restart case, Next
case, Save, Load, Release load (0), Apply light load (1000) and Apply heavy load
(8000). Esc/Start opens the ordinary host pause menu. R restarts
the lab with its default case. The scene renders through both vector and raster
paths. It uses a labelled schematic with vertical displacement magnified 20x;
the displayed time, velocity, work, remaining capacity and permanent indentation
come from the physical history.

## Cases and playback

Every case uses a mass-20 ideal rigid platen, a fixed full-face wood layer,
section/deformation depth 1, represented volume 20 and initial remaining capacity
1200. The physical frame is turned exactly 270 degrees so gravity points down.
This is the full capacity of the reference wood material (`toughness 60 * volume 20`).
No prior damage or plastic indentation is imported.

| Case | Initial weight | Initial gap | Observation window |
| --- | ---: | ---: | ---: |
| Elastic bounce | 1000 | 1/64 | 512 steps (0.125 s) |
| Yield, rebound & return (default) | 1000 | 1/16 | 716 steps (0.1748046875 s) |
| Yield & continuing support | 4000 | 1/64 | 512 steps (0.125 s) |
| Overloaded support | 8000 | 1/64 | 272 steps (0.06640625 s) |
| Load, release & return | 8000 | 1/64 | First return, or at most 768 steps (0.1875 s) |

In **Load, release & return**, choose Run. At step 128 (0.03125 s), the lab pauses
with yielded wood. Choose Release load, then Run to watch elastic recovery.
It pauses again when the platen separates. Choose Apply light load, then Run;
the case completes at the next impact on the permanently indented surface.
The on-screen prompt guides these checkpoints. Single step can inspect each
interval. Resuming without following a prompt still advances the actual chosen
force; it may reach a native limit rather than complete the guided return.

All five cases accept the three load controls after their first physical step.
A load command pauses playback and changes only the force; it does not add a
tick, reset velocity, repair wood or allocate a second impact budget. Subsequent
Run/Single step uses the new load. Up to 32 actual changes are allowed, including
several while paused. Selecting the current load is a no-op. Restart begins a
fresh run with its initial load; a completed or refused run cannot accept changes.

One 60 Hz display update advances physical time by exactly 1/4096 s, approximately
68 times slower than real time. Single step advances one such update. Pausing or
saving does not call the ordinary physics stepper or reset the continuation.
The demo ends visibly at the case's observation window; this is not static
settlement. A native refusal also stops playback and retains the last valid
physical state, with its reason visible. Restart explicitly creates a new run.
The underlying error allowances and supported scene restrictions are unchanged.

The cyan platen compresses the brown layer. The grey line marks its original
surface and the amber line its permanently indented surface. The contact history
shows Impact, Support and Separation observations. Contact numbers are local
ordinals for presentation; native episode identities remain in the physics save.
Damage work, viscous loss and recovered elastic energy remain distinct. The
display shows the applied load, change count, gravity work and gravity impulse;
the downward force arrow disappears on release. Gravity work integrates each
load over its own displacement, including negative work on upward motion.

## Save slot

Save pauses the run and replaces `material-lab.bin` in the application's actual
configuration directory, including the `--config-dir` override. Load restores the
saved case and physical history **paused**, so it cannot advance before inspection.
The slot survives case changes, scenario restarts and application restarts. It is
only read when Load is activated; starting the lab begins a fresh paused run.

A single ordered worker handles storage outside the UI thread, including across
scenario restarts. While a request is pending the lab does not advance and ignores
further activation commands. Writes use a same-directory temporary file, sync and
atomic replacement. Wait for the Saved message before quitting. Failed reads or
validation leave the current physical state intact and display an error.

The versioned, size-bounded lab envelope contains the native PhysicsWorld snapshot
and bounded presentation history. Loading validates the native snapshot and
replays the selected bounded case and its ordered load commands to verify tick
count, events, completion/refusal and exact physical bytes. Commands at the saved
tick are applied before any next step, including multiple changes while paused.
This extra verification runs on the storage worker and
cannot renew numerical budgets. Lab save compatibility is deliberately tied to
this case/replay contract; an incompatible future version must reject explicitly
or supply a migration. The existing four cases without load changes retain the
`MATLAB01` layout and bytes. Changed loads and the new guided case use `MATLAB02`;
its command list is bounded to 32 entries and replay must reproduce the complete
saved envelope. Wrong versions, impossible/no-op commands, reordered histories
and mismatched physics refuse. Native saves retain their v1–v18 compatibility.

## Evidence and limits

Scenario tests qualify each constant-load run and the guided release/return,
pause, legacy layout, exact save/load continuation, same-tick command limits,
terminal saves and malformed envelopes. Client tests exercise all three load
controls, storage failure and exact continuation after load. A public UI workflow
uses both renderers, the yielding/separation checkpoints, load changes without
time advancement, host pause, single step, save/load, process restart and corrupt input;
it is included in the PR smoke suite.

This is a bounded two-body demonstration. It does not enable material response
for existing gameplay, add fracture/topology consequences, or broaden the native
solver's geometry or connection-loading support. #327 remains open.
