# Diagnose entry to the retained Round hatch route

The seed-20202 Round/collapse-off candidate-P2 mission rebuilds at tick 24743.
A one-jump route connects nodes 252 and 253, but the pilot starts 2.193 units
from node 252 and the ground task never advances its waypoint before blocking
at 26228. A route between standing nodes does not certify entry from actual
support. Inspect and exercise that entry before changing the jump executor.

Reconstruct the original mission through tick 26829, verifying every input,
decision checksum and scenario checksum against the retained 36000-tick trace.
Retain full per-decision ground execution and physical observations from 24743
onward, including native accepted jumps, contacts, gravity, posture, waypoints,
transfers and the selected nodes. Inspect native routes and orientation-independent
terrain connectivity at 24743 and the previously retained 26229 failure fork.
Clone the complete world, both controllers and host queues at those two ticks.

Run one unchanged control and the following prespecified 30 input recipes at
each fork. Each clone has a 600-tick maximum horizon, ending sooner only for
native boarding or match end. No state edits, controller resets, terrain edits,
ship teleportation or boarding-gate changes are permitted.

Recipes cross horizontal input `[-1, -0.5, 0, 0.5, 1]`, launch delay `[0, 12, 24]`
ticks, and steering duration `[12, 30]` ticks after the single jump press. Begin
with a released Jump button, apply the selected horizontal input, and press Jump
once at the first subsequent eligible balanced, retained-supported tick after
the delay. The native get-up gate must also allow an ordinary jump. If no launch
is eligible within 120 ticks, return input control to the original bot. After
the steering duration, return to that same bot without altering its memory.
This is a bounded diagnostic input search, not a proposed controller policy.

At any tick when the ordinary native boarding sensor reports Ready, the probe
may release movement and issue a fresh Interact press. Retain that sensor state
and require actual native boarding for success. Only P2's surface-control action
may be overridden; weapon/aim inputs, both reactive controllers and host planning
continue normally. A one-tick Jump press must be distinguished from a native
accepted jump and from the bot's later jumps. Continuous jetpack holding is not
part of the scripted intervention.

Report every trial, including failures. Separately measure proximity to the
start node, supported entry, advancement to the next node, native boarding
readiness and actual boarding. Compare controller jump counts to native launches.
If no trial succeeds, do not infer impossibility. If one succeeds, retain the
complete ordinary-input witness and identify the execution difference before
proposing a bounded, independently selectable policy change. Recheck the native
boarding gate and terrain/source provenance. Keep other recovery cases separate.
