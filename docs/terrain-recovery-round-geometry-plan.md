# Inspect the Round pocket in native retained material

The comparison frozen at `43323b7` completed once. All three fresh tasks block
at 16,830 with no start footing and produce the same 18,000 ticks of game
inputs as the original blocked control. The closest measured node at 16,500
is 3.018725 units from the foot, above the actor. The retained drawing suggests
an enclosed pocket under that node. This is a new diagnosis, not grounds to
increase a routing tolerance or enable retries.

Reconstruct only the unchanged Round control through 34,500. At 16,500 and
each subsequent 60-tick sample, read the native retained planet's material IDs,
grid dimensions, cell size, revision, actor cell, support contact and posture.
Record private controller state at the fork and end. Assert the reads never
change the scenario payload. Retain the unchanged per-tick action/decision/
scenario checksums and compare every tick with the preceding comparison.

Analyze empty-cell connectivity from the actor's native cell, using both four
and eight neighbors. A closed eight-neighbor component is stronger than merely
finding no bot route in this blocky retained surface; an open component alone
does not establish capsule clearance or an executable walk/jump/jetpack route.
Record the enclosing cells and any sampled transition to exterior connectivity.
Check the rendered polygons against the local native material boundary. Keep
geometry, controls, source terrain and production recovery policy unchanged.

Freeze the reader and executable before this additional reconstruction. Keep
the original four-arm experiment and its failures. This diagnostic cannot
establish permanent impossibility under continued bombardment, or exclude an
escape that requires mining rather than navigation. No gameplay default,
deployment or PR update is part of this investigation.
