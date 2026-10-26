# Falling-platen Material Lab (#327)

`material-lab` is the first application scenario using the bounded native
[impact-to-support path](material-impact-support.md). Select **Show all scenarios**
in the launcher, choose **Material Lab**, then start. Direct launch:

```sh
cargo run -p engine-client --profile ci -- --scenario material-lab
```

The lab starts paused. Up/down selects a control; Space or the gamepad bottom
face activates it. The controls are Run/Pause, Single step, Restart case, Next
case, Save and Load. Esc/Start opens the ordinary host pause menu. R restarts
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

| Case | Constant weight | Initial gap | Observation window |
| --- | ---: | ---: | ---: |
| Elastic bounce | 1000 | 1/64 | 512 steps (0.125 s) |
| Yield, rebound & return (default) | 1000 | 1/16 | 716 steps (0.1748046875 s) |
| Yield & continuing support | 4000 | 1/64 | 512 steps (0.125 s) |
| Overloaded support | 8000 | 1/64 | 272 steps (0.06640625 s) |

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
Damage work, viscous loss and recovered elastic energy remain distinct.

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
replays the selected bounded case to verify tick count, events, completion/refusal
and exact physical bytes. This extra verification runs on the storage worker and
cannot renew numerical budgets. Lab save compatibility is deliberately tied to
this case/replay contract; an incompatible future version must reject explicitly
or supply a migration. This does not change native v1–v17 snapshot compatibility.

## Evidence and limits

Scenario tests qualify each complete run, pause, exact save/load continuation,
terminal saves and malformed envelopes. Client tests exercise controls, storage
failure and exact continuation after load. A public UI workflow uses both
renderers, host pause, single step, save/load, process restart and corrupt input;
it is included in the PR smoke suite.

This is a bounded two-body demonstration. It does not enable material response
for existing gameplay, add fracture/topology consequences, or broaden the native
solver's geometry, force-history or connection-loading support. #327 remains open.
