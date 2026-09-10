# Returning to parked ships on material corners

This follows [landing on generated material planets](large-planet-landing.md).
The target is seed 0/P1 mirrored in the generated quiet arena: the previous Pi
build landed, exited and claimed planet 2, then waited beside its healthy ship
for the rest of the three-minute run.

## Recorded cause

Replays of the archived `cc5b030` binaries match all 180 per-second motion hashes
on each platform. The Pi completes no departure; the desktop control completes
two. A temporary read-only contact trace also retains every Pi motion hash.

Both feet still touch retained planet material. At 59.5 seconds their contact
normals have outward components 0.978 and 0.717, qualifying for initial landing.
By 60 seconds the round foot on a voxel corner has settled to 0.692, just below
the per-foot 0.7 cutoff. At 150 seconds it is 0.643. Both contacts remain within
contact slop and nearly motionless relative to their supporting surface. The
hatch position remains measurable, but the landing gate becomes `Settling`.

Ground navigation considers the nearby hatch reached and repeatedly refreshes
its progress time. A later recovery task inherits the same misconception. The
existing fallback only recognizes a missing hatch or a landed ship on another
planet, so this present but unboardable hatch never reaches it.

## Changes

Initial touchdown retains its existing per-foot 0.7 alignment threshold and
quarter-second settling gate. An already landed full ship on material terrain
can retain two-foot support if each current contact has outward alignment at
least 0.5 and their mean remains at least 0.7. Strictly qualifying contacts keep
their original selection. The tolerance only applies on the same planet and
only while both real retained contacts survive; contact separation, separating
velocity, ship angle, speed, spin, wings and thrust still gate landing. It is
lost on failed revalidation, so it cannot be used to acquire a new landing.
`corner_support` reports when the retained pair uses this tolerance. Material
edit checks record the same qualifying foot cells.

This changes support recognition, not ship orientation, gravity, forces or the
physics step. It gives humans and bots the same boarding behavior on corners.

`ground_navigation_v9` waits at a visible hatch when the shared world reports
`ShipNotSettled`. Arriving there no longer continuously renews host progress.
After fifteen seconds from supported arrival it reports `UnsettledShip`.
World motion, local hatch movement and brief contact loss do not restart that
wait. An actually settled ship returns to ordinary boarding, and active jetpack
crossings retain their existing completion and interruption rules.

`recover_ship_v8` can route this typed failure into the existing bounded
replacement action. The balanced spaceling must have real support and stable
relative motion. The nearby assigned full ship must be in the same planet
frame, within 24 units, moving radially and tangentially below one unit/second,
and spinning below 0.2 radians/second relative to that frame. A visible hatch
alone cannot cancel replacement while the ship remains unsettled. A real landed
hatch or ready boarding gate releases the chord. The ordinary human three-second
scuttle and eight-second rebuild rules perform the loss and replacement; the
policy makes no world writes. The one-replacement limit and existing task
budgets remain in force.

## Validation

Five new tests cover physical corner contacts and prior landing, a complete
original-ship return after generated flight/capture in both reflections, the
bounded visible-hatch wait, settlement during that wait, and replacement
cancellation/unstable-motion exclusion. Existing terrain-removal, ground-route,
jetpack, impact/recovery, landing and interplanetary regressions remain required.

The corner-contact unit fixture uses an archived physical pose and supplies
prior landing state explicitly to isolate the gate. The mission regression and
long-run replays earn landing through ordinary flight and physics. Decision
fixtures isolate timeout and cancellation rules; real losses and replacement
remain covered by the unchanged physical return and impact matrices.

Artifacts and exact replay/build/validation scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/parked-ship-return-20260910/
```

Final matrix and deployment results will be recorded after the source checkpoint.
