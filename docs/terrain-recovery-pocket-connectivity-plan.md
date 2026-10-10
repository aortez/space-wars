# Native collision connectivity of the retained Angular pocket

The preceding direction experiment did not reach standing ground. Rendered
geometry suggests a pocket, but straight sweeps and a picture cannot establish
connectivity. This leg changes diagnostic code only, with no new bot behavior.

Reconstruct seed 20202, Angular dirt, collapse off, candidate P2 from the exact
original mission prefix and frozen target-direction traces. Inspect the two
prespecified attempt starts (8025, 8325) and candidate timeouts (8506, 8806).
Verify every reconstructed action, decision and physical scenario checksum;
also compare each checkpoint's native physics fingerprint with the retained
trace. Select the same three existing opt-in options, without resetting either
bot or changing physics. Stop at each retained timeout. Do not run new matches.

For each checkpoint, query ONLY that planet's retained native terrain chunks,
using the actual capsule's collision groups and excluding sensors and self.
Other obstacles are omitted, which makes possible space larger. Use grids
centered on the actual collider origin, radial up, extending six units in each
direction. Prespecify all three spacings: 0.1, 0.05 and 0.025 units, with 60, 120
and 240 cells on each side of the middle cell. No offset of the physical actor
or query origin is allowed. Bounds: 304083 native point queries per checkpoint.

A centered capsule contains a disk of its radius at every orientation. Shrink
that disk by 0.025 units as a penetration allowance. A native solid-point witness
at distance d from a cell center certifies the entire cell blocked only when
`d + cell circumradius + numerical margin < shrunken radius`. The margin is
0.002 units plus a world-coordinate roundoff allowance. This is a triangle
inequality bound over each complete square, rather than point occupancy. The
native point projection provides an actual solid point; nearest-point optimality
is unnecessary for this exclusion argument.

Flood all uncertified cells, including diagonal passages. Retain the complete
classification and the native solid-point witness for every blocked cell next
to the source component. Independently audit that this witness set alone
separates the source from every boundary cell when all other cells are open.
Retain measured outer standing-node centers and locate them relative to the
component. Assert serialized native physics, public scenario and controllers
unchanged by inspection. Keep hashes for the runtime, sources and references.

A nonblocked source component separated from the boundary supplies a native
geometric enclosure certificate for all capsule orientations at that instant,
subject to the stated numerical/penetration allowance. It does not prove future
terrain remains sealed, prevent solver tunneling, or establish whether native
rebuilding elsewhere could succeed. An open component is inconclusive: it
may contain uncertain or physically unusable space, and is not movement
permission. If an opening is found, require a separately bounded demonstration
with ordinary controls before changing routing or recovery policy. If enclosed,
retain this as an expected blocked case; do not add blind retries or promote
the failed direction selector.

Validate synthetic closed rooms, narrow openings, traversable openings, rotated
compound geometry, native groups, sensors, exclusions and invalid grids. Check
that retained boundary witnesses suffice on their own. Run the existing AI
library tests, targeted native posture tests, default-feature compilation,
Clippy and formatting. Produce an independent auditor and a standard plot of
the certified barriers and standing targets, plus a verified local handoff.
