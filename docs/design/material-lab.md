# Material Lab: loading, recovery and paired collisions (#327)

`material-lab` is the first application scenario using the bounded native
[impact-to-support path](material-impact-support.md), its
[changing-load extension](material-changing-load.md) and the
[paired-material path](material-paired-compression.md). Select **Show all scenarios**
in the launcher, choose **Material Lab**, then start. Direct launch:

```sh
cargo run -p engine-client --profile ci -- --scenario material-lab
```

The lab starts paused. Up/down selects a control; Space or the gamepad bottom
face activates it. The controls are Run/Pause, Single step, Restart case, Next
case, Save, Load, Release load (0), Apply light load (1000) and Apply heavy load
(8000). Esc/Start opens the ordinary host pause menu. R restarts
the lab with its default case. The scene renders through both vector and raster
paths. It uses a labelled schematic with normal displacement magnified 20x;
the displayed time, velocity, work, remaining capacity and permanent indentation
come from the physical history.

Next case walks the five platen cases and then the two
[free-pair collisions](#free-pair-collisions).

## Platen cases and playback

Every platen case uses a mass-20 ideal rigid platen, a fixed full-face wood layer,
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

All five platen cases accept the three load controls after their first physical step.
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

## Free-pair collisions

Two cases put finite material on both bodies. Both bodies are free and
rotation-locked, meet along a horizontal normal with no external force, and
start 1/16 apart. Each carries its own full-face layer with section/deformation
depth 1 and represented volume 20, at the full capacity of its reference material
(`toughness * 20`: 1200 for wood, 200000 for armor).

| Case | Body A | Body B | Observation window |
| --- | --- | --- | ---: |
| Wood strikes wood | wood, mass 20, velocity +6 | wood, mass 40, at rest | 352 steps (0.0859375 s) |
| Wood meets armor | wood, mass 80, velocity +2 | armor, mass 80, velocity -2 | 512 steps (0.125 s) |

In **Wood strikes wood** the bodies touch from step 43 to step 152. Body A is
thrown back at 0.450 and body B leaves at 3.225. The two equal layers yield
equally although the masses differ: each keeps a permanent set of 0.00906 and
has 1145.67 of its 1200 capacity left, after 54.33 of damage work and 20.66 of
viscous loss.

In **Wood meets armor** the bodies touch from step 64 to step 220, and both
reverse to 0.787. Only the wood yields: it keeps a set of 0.02878 and has 1027.34
capacity left, after 172.66 of damage work and 97.78 of viscous loss. The armor
flexes, returns its elastic energy and keeps all of its capacity.

The schematic draws both bodies. Each block shortens at its contact face by its
own layer's deformation; a grey line marks the face's original position on the
body and an amber line its permanent set. The arrow over a body shows its
velocity, scaled to the fastest body at the start of the case. The phase reads
Approach, Impact, Compression, Rebound or Separated.

The table gives one column per body: velocity, momentum, motion energy, deformation,
permanent set, elastic energy, damage work, viscous loss and capacity left, with
a bar for each body's remaining share of its own capacity. Deformation is the
layer's present compression; after separation it equals the permanent set.

**Shared energy** is one ledger for the pair. A bar splits the initial motion
energy into the motion of both bodies, damage work, elastic energy and viscous
loss, with the figures beside it. The numerical defect is the energy and
capacity-charge defect the solver retains; it is never material damage. Those
five lines sum to the initial motion energy shown under them, and the momentum
of the pair stays at its initial value.

Run/Pause, Single step, Restart case, Save and Load work as in the platen cases.
The three load controls pause playback and refuse with a message, because this
solver path admits no external force.

Armor decides the masses of the second case. Its viscosity sets the solver's
internal time step, and every internal solve spends part of fixed error
allowances that nothing renews. Lighter pairs need more solves per displayed
step and run out soon after separation; some unequal masses run out before it.
Mass 80, the largest the solver admits, needs two solves per displayed step, and
the run ends with under half of any allowance used. The wood pair uses about a
third.
If a free pair reaches a numerical allowance, playback stops with “Numerical
accuracy limit reached” and retains the last valid state.

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
saved envelope. The two free-pair cases use `MATLAB03`: the `MATLAB01` fields
with no command list. Their native snapshot becomes v20 on the first physical
step, when the paired history is enrolled. No layout accepts another
layout's cases. Wrong versions, impossible/no-op commands, reordered histories
and mismatched physics refuse. Native saves retain their v1–v20 compatibility.

## Evidence and limits

Scenario tests qualify each constant-load run and the guided release/return,
pause, legacy layout, exact save/load continuation, same-tick command limits,
terminal saves and malformed envelopes. Client tests exercise all three load
controls, storage failure and exact continuation after load. A public UI workflow
uses both renderers, the yielding/separation checkpoints, load changes without
time advancement, host pause, single step, save/load, process restart and corrupt input;
it is included in the PR smoke suite.

For the free pairs, scenario tests check at every step that the ledger closes
and momentum holds, then the contact events, both bodies' recoil, equal damage
in equal layers and untouched armor. They also cover exact save/load
continuation, the `MATLAB03` layout, its refusal of other envelopes and the
refused load commands. A client test saves mid-collision, steps and restores
through the storage worker. A second public UI workflow runs the wood pair in
the vector renderer with a mid-run save, step and reload, reloads that save in
the raster renderer after leaving the lab, and runs the armor case there. It is
part of the full UI suite, not the PR smoke suite.

This is a bounded two-body demonstration. It does not enable material response
for existing gameplay, add fracture/topology consequences, or broaden the native
solver's geometry or connection-loading support. A layer that ran out of capacity
would refuse the step; turning exhausted capacity into breakage is #328. #327
remains open.
