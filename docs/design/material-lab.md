# Material Lab: loading, recovery, collapse, splits and welds (#327, #328)

`material-lab` is the first application scenario using the bounded native
[impact-to-support path](material-impact-support.md), its
[changing-load extension](material-changing-load.md) and the
[paired-material path](material-paired-compression.md). Two cases instead use
ordinary steps: one with the [native response](material-native-response.md) to
show a [region split](material-region-split.md), and one with a
[connection load](material-connection-loads.md), a weld that gives way. Select
**Show all scenarios** in the launcher, choose **Material Lab**, then start.
Direct launch:

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

Next case walks the five load-controlled platen cases, the two
[free-pair collisions](#free-pair-collisions), then the
[collapsing support](#collapsing-support), [paired collapse](#paired-collapse),
[split and rebuild](#split-and-rebuild), and
[branch gives way](#branch-gives-way), for eleven cases in total.

## Platen cases and playback

The five load-controlled cases use a mass-20 ideal rigid platen, a fixed full-face wood layer,
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
surface and the amber line its permanently indented surface. Both lines pass
behind the platen and show past its ends while covered. The contact history
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

## Collapsing support

**Support gives way** demonstrates the opt-in
[support-failure path](material-support-failure.md). Its wood layer has represented
volume 2 and starts with its full capacity of 120. The mass-20 platen falls from
a gap of 1/64 under a fixed load of 8000. The case completes after 144 steps
(0.03515625 s), within the original motion and numerical limits.

Run pauses automatically when the layer collapses. Capacity is zero, the phase
reads Collapsed, and contact history records the end of support and one Collapse.
The display adds **Collapse loss**, comprising the released elastic store and
any plastic work beyond the final capacity charge. This energy is dissipated;
it does not become another impulse or additional material damage.

Run again continues the platen's fall under gravity with no supporting reaction.
The failed layer is drawn dimly at its collapse position while the platen moves
through its envelope. Permanent set, damage and the failure observation stay
fixed. Pause, Single step and Save/Load work before and after collapse. The three
load controls refuse with a message because this mode uses constant gravity.

## Paired collapse

**Wood pair gives way** uses the same initial motion as Wood strikes wood:
mass-20 body A approaches mass-40 body B at speed 6 from a gap of 1/16. Both
layers start undamaged. A has represented volume 0.5 and capacity 30; B retains
volume 20 and capacity 1200. Both keep section/deformation depth 1. The case
uses the native [paired-collapse policy](material-support-failure.md#two-finite-material-layers).

The lab pauses at step 76 (0.0185546875 s), when A exhausts its capacity.
The phase reads Collapsed, A is dimmed and labelled failed, and contact history
records Contact ended and Collapse. B retains 1168.54 capacity after 31.46 of
damage work. Both layers keep their boundary deformation and permanent set;
neither takes further damage. B remains intact even though the shared contact
path no longer carries force.

The shared energy ledger adds **Collapse loss**: 45 of released elastic storage
from each layer, plus about 1.46 of plastic work beyond A's final charge. The
total is about 91.46. B's elastic release counts even though B does not fail.
This dissipation adds no impulse and consumes no further surviving capacity.

Run resumes with A coasting at about +3.942 and B at +1.029. Their schematic
envelopes overlap as they move through the failed contact. The run completes at
step 192 (0.046875 s), after about 28 ms of coasting, inside the native motion
window and numerical allowances. It does not extend those limits or demonstrate
fragment motion. Save/Load and Single step preserve the failure, both materials
and exact continuation before and after collapse. The load controls remain
unavailable for a free pair; Restart restores both layers to full capacity.

## Split and rebuild

**Wood split & rebuilt** shows that splitting struck wood, moving it to another
body and striking it again all keep its damage. A split or transfer ends a
bounded run for good, like [other external edits](material-region-split.md#refusals-and-runs),
so this case steps the world normally with the opt-in native response.

A free armor striker of mass 2 approaches a fixed wood block, volume 20 and
capacity 1200, at speed 16 from 1/16 away. Each step moves it 1/256, farther
than the solver's contact prediction distance, so it touches the wood exactly
at step 16. A slower striker would meet a speculative contact first, which the
response refuses while the solver brakes the striker. The strike is one impulse
at first touch: it charges the wood 251.29, and the striker leaves at 2.058.
The contact ends after capture's 1/60 s separation grace, and the lab pauses at
step 86 (0.02099609375 s) with 948.71 left.

Each Run then makes one edit at the start of the next step and pauses again:

1. **Split.** The back quarter of the volume goes to a fragment, a new fixed
   body set 0.25 behind the wood. The wood keeps 711.53 of its new capacity of
   900 and the fragment holds 237.18 of 300, together exactly the 948.71: both
   parts keep the integrity the strike left. The wood's collider shrinks to its
   front three quarters, so its struck face stays where it was. Each part's
   section deforms only as deep as the part reaches behind that face: 0.75 and
   0.25.
2. **Rebuild.** The kept wood moves to a replacement body of the same shape,
   body 4. The lab binds the new collider and only then removes body 2, whose
   removal would otherwise retire the region with its balance. The balance stays
   711.53.
3. **Strike again.** The striker returns to its start and is launched again.
   The second strike charges the damaged wood 251.21 and leaves 460.32. The run
   completes at step 174 (0.04248046875 s), when that contact ends. The fragment
   keeps its 237.18 throughout.

The schematic magnifies the striker's motion 20x and draws the wood and the
fragment to scale. One column per block gives its body, velocity, volume,
capacity and capacity left, with a bar for each block's remaining share of its
own capacity. **Wood balance** lists where the wood's capacity went: each
strike, the split's two parts and their sum, and the move. Contact history
records the split and the rebuild beside the contacts, and the status line names
the next edit at each pause. The load controls refuse with a message. Pause,
Single step and Save/Load work at every stage; Restart returns to the undamaged
wood.

## Branch gives way

**Branch gives way** shows a [connection load](material-connection-loads.md): a
weld holds a safe load for nothing, drains under an overload and severs. The
case steps the world normally under gravity 400. Ordinary steps refuse the
native response while any joint exists, so this world has no loading regions;
its material is the weld's.

A fixed trunk holds a wood branch welded by its end to the trunk's side. The
branch is four units long and half a unit high, with mass 20. A 20-mass weight
rests on it with its center 3.5 out. The load controls set the weight's gravity
scale, so Release load, Apply light load and Apply heavy load give it a weight
of 0, 1000 and 8000 without changing its mass or contacts. The weld's beam is
wood with the branch's height and length and a represented depth of 256. That
puts its safe moment at 32000, twice the moment of the branch's own weight,
and its capacity at 30720 (`toughness 60 * volume 512`).

The case starts under the light load. The root carries the branch's weight at
its center and the weight's at its reach, a moment of 19500 and a utilization
of 0.61. From the eighth step the weld reads that moment within 0.06% and the
shear within 0.17%, and it charges exactly nothing. The lab pauses at step 128
(0.03125 s).

Choose Apply heavy load, then Run. The moment rises to 44000 and the
utilization to 1.375. The weight takes a few steps to settle onto the branch;
from the seventh step after the change, each step charges the law's 187.5
within 0.35%. The weld severs at step 293 (0.071533203125 s), 165 steps later:
163.84 steps of the steady charge, plus the settling. The lab pauses with no
capacity left, the phase reads Falling, and the history records Severed. Run
then lets the branch fall with the weight. The run completes 384 steps later,
at step 677 (0.165283203125 s), when the branch has dropped 1.76, as in free
fall.

The schematic draws the trunk, the branch and the weight to scale. The weld
across the root section is cyan while it holds, amber while an overload drains
it and grey once it has severed, and an arrow over the weight shows its load.
The table gives the root moment, the safe moment, the utilization, the shear,
the capacity left, the work charged, the last step's charge, the time to
failure and the branch's drop. Bars show the moment against the safe moment
and the capacity left. Time to failure is the capacity left over the last
step's charge, in seconds. Once the weight has settled it names the severing
step to within 0.55 of a step, and while the load is safe it reads "never at
this load".

Lifting the load partway through an overload stops the drain. The capacity
spent stays spent, and the heavy load resumes the drain where it stopped. A
weld relieved after 60 overloaded steps severs after 166 in all, one more than
an unbroken overload, because the weight settles again. The load controls work
after the first step, with up to 32 changes, as in the platen cases. A severed
weld takes no more load. Pause, Single step and Save/Load work throughout, and
Restart returns to an undamaged weld. The run ends at the case's 1024-step
window if the weld never severs.

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
layout's cases. Fixed-support collapse uses `MATLAB04`, with the same bounded fields
and no command list; its native history becomes v21 on enrollment. Wrong versions,
impossible/no-op commands, reordered histories and mismatched physics refuse.
Paired collapse uses `MATLAB05`, again with those bounded fields and no command
list; its native history becomes v23 on enrollment. Split and rebuild uses
`MATLAB06` with the same fields and no command list: its edits happen at fixed
steps, so the replay repeats them. Its native snapshot keeps the
response-enabled v3 layout throughout. Branch gives way uses `MATLAB07`, with
the `MATLAB02` fields and command list even before any load change; the replay
applies its commands on ordinary steps. Its native snapshot keeps the v1
layout, with the weld in the connection section. Existing case tags and lab
envelopes retain their layouts. Native saves retain their v1–v23 compatibility.

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
part of the full UI suite, not the PR smoke suite. It also saves and reloads the
collapsed support, repeats a single step exactly, and checks the continued fall
with the same failure and damage. Scenario tests verify the collapse ledger,
automatic pause, completion, terminal replay and envelope restrictions.

The paired-collapse scenario test closes the shared ledger on every step and
checks the survivor, both unchanged coast velocities and frozen material state.
Save tests cover pre-contact, pre-failure, collapsed and terminal checkpoints,
wrong envelopes and forged display history. Its public UI workflow saves at the
automatic pause in the vector renderer, restores in the raster renderer, repeats
a single step exactly, checks load refusal and coasting to completion, and
restarts to full capacity. It is in the full UI suite; PR smoke selection is
unchanged.

The split-case scenario test draws every state. At each pause it checks the
wood's charge, the parts' volumes, capacities and integrities, their sum against
the split balance, the colliders' extents and the moved balance. It then checks
the events and completion. A copy of the world that skips the rebuild takes the
same second strike and is left with exactly the rebuilt wood's balance. A
striker launched at 5 meets a speculative contact, and the refusal stops the run
with the last valid world. Save tests cover every edit boundary, exact
continuation through each edit, the `MATLAB06` layout, wrong envelopes and
forged history. A client test follows the status through each edit and saves and
restores between the split and the rebuild. The public UI workflow splits in the
vector renderer, saves, rebuilds, restores in the raster renderer, repeats the
rebuild exactly, checks load refusal, runs the second strike to completion and
restarts. It is in the full UI suite.

The main branch-case scenario test draws every state. Under the light load it
checks the moment and shear against the two weights, that the weld charges
nothing and that the lab pauses at the checkpoint. Under the heavy load it
checks each step's charge against the law, each time-to-failure prediction
against the actual severing and the severing step against the law's schedule.
It then checks the pause and the history, the refused loads and the fall
against free fall. Another test lifts the overload partway and checks that the
spent capacity stays spent and that the overloaded steps in all stay within
six of the law's schedule. Save tests cover the first steps, two load changes
at the paused checkpoint, the drain, the severing and the step after, with
exact continuation, and the completed run. They also cover the `MATLAB07`
layout, wrong envelopes, forged history and forged commands. A client test
saves at the checkpoint with the heavy load applied and runs to the severing.
It checks the refused load, restores, severs again identically and runs to
completion. The public UI workflow holds the light load in the vector renderer
and saves with the heavy load applied. It runs to the severing, restores in
the raster renderer and severs again on the same step. It then checks the
refused load, runs the fall to completion and restarts. It is in the full UI
suite.

These are bounded demonstrations with two or three bodies. They do not enable
material response for existing gameplay, add fracture/topology consequences, or
broaden the native solver's geometry or connection-loading support. The two
explicit collapse cases demonstrate bounded #328 failure consequences; other
cases keep their refusal on exhaustion. The split case's fragment is cut by the
lab, not by a failure, and the branch case's severing removes only its weld. A
moving rigid platen over one material, and debris after failure, remain later
Lab slices. #327 remains open.
