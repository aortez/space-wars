# Connection loads (#328)

`PhysicsWorld::insert_connection` welds two existing bodies where they lie with
a structural connection: an `engine_material::Connection`, a beam with its own
material and capacity. Each step charges the
[connection law](material-damage.md#sustained-and-connection-loads) for the load
the weld carried. A safe load costs exactly nothing however long it rests, and
an overload charges until the capacity runs out. The connection then severs:
its weld leaves at the end of that step, both bodies carry on under ordinary
physics, and its record stays as one failure observation. Hosts use it for
branches, limbs and fixtures that hold until they are overloaded, and decide
what a severing means.

## Welds and identity

`ConnectionSpec` names the root body, such as a trunk, and the held body, such
as a branch. It also gives the root point, the center of the root section in
world coordinates, and the beam's direction from the root into the held body,
along with the connection itself. The weld is a Rapier fixed joint at the root,
built from that direction without trigonometry, with its frame's x axis along
the beam. Contacts between the two bodies are off while they are welded, so the
weld carries the whole load. They collide again after a severing, so their
colliders should meet at the root without overlapping.

A connection takes a `JointId`, as a tether does, and shares that namespace: an
identity held by any joint or by a connection record, live or severed, is
refused with `ConnectionRejection::Taken`, and a tether cannot take a severed
connection's identity either. The record keeps its identity until the host
calls `remove_connection`. On a live connection that call is the host's own
cut: it removes the weld and records no failure. Other refusals cover a missing
body or one body named twice (`Bodies`), a root or direction that is not
finite or a direction of zero length (`Geometry`), and a connection with no
capacity left (`Exhausted`).

`connection(id)` reports the bodies, the beam and its material with the
capacity left, the last load and any failure. `severed_connections()` lists the
connections the last step severed, in identity order.

## Measuring the load

`ConnectionLoadSample` is the load the held body put on the root section in the
step, in the weld's frame, which turns with the root body. `axial` is positive
in tension and `shear` positive toward the counterclockwise side of the beam's
direction. `moment` is positive counterclockwise, so a branch reaching right
under gravity bends its root clockwise. As for a tether's tension, a force is
the solver's final-substep impulse divided by that substep's duration.

Rapier writes back one impulse per locked axis, but not the impulse along that
axis. Before it solves a joint, it orthogonalizes the locked rows in the
bodies' mass metric by modified Gram-Schmidt, the angular row first and then the
two linear ones, and the impulses it keeps are coefficients of those changed
rows. When the root body is fixed, the changed linear rows act at the branch's
center of mass and carry the whole moment, so the angular coefficient is zero
at rest. Read directly, a resting cantilever reported a moment of exactly zero.
The world undoes the orthogonalization with the solver's own inputs: each
body's effective inverse mass and inertia, its arm from its center of mass to
the root, and the weld's axes. It uses the poses the step ended with. A weld
turns as one piece, and the coefficients depend on the pair's shape and mass
but not on how it is turned, so they match the solver's up to the weld's own
slight deflection within the step.

A sleeping pair keeps its last impulses, so a resting load keeps its last
reading and an overload keeps charging while the branch sleeps.

## Charging and severing

Each step makes one `engine_material::connection` call with the step's duration
and the sample's moment and axial force. The weld is rigid and measures no
plastic rotation or extension, so the law's fallback progression stands in for
both. A safe load charges exactly zero. An overload charges the fallback work,
which `Connection::absorb` takes from the capacity. `utilization` is the law's
combined stress over the material's strength. A load the law refuses, one that
is not finite or is past its bound, records an infinite utilization and charges
nothing. Live connections are charged in identity order after the solve,
before contact events are rebuilt.

When a charge exhausts the capacity, the step records a `ConnectionFailure`
with the work the overload asked beyond what was left. Then it removes the weld
and lists the connection in `severed_connections()`, so a severing never
happens mid-solve. Removing the weld wakes both bodies, which keep their
motion. The next step clears the list. A severed record keeps the load of the
step that severed it, and nothing charges it again.

## Edits and other worlds

`remove_entity` removes every connection, live or severed, with a body in the
entity. `transfer_entity` refuses an entity that any connection record names,
including a severed one, so the host removes the record first. Inserting or
removing a weld is an external edit like any joint change, which ends an
active gravity run. Bounded compression, arrival and gravity runs already
refuse worlds with joints, so welds never step inside them. A region split
changes no bodies or welds, and a connection's material is not a loading
region.

## Persistence

Connection records are saved in a tagged section after the carried-weight
section, in identity order. Each record holds its bodies, its material's
numeric parameters, its beam, its exact f64 remaining capacity, its last load
and any failure. A world without connections writes no section, so its saves
are unchanged byte for byte and no snapshot version changes. A build without
connections refuses a save that has them, as trailing snapshot data. Restoring
checks each record against the saved joints. A live connection needs its own
fixed weld between its own bodies, and a severed one needs no joint. A record
must hold a failure exactly when its capacity is exhausted. As for registered
regions, material names are not saved: a restored connection's material is
named `connection`. Clones copy the records. The severed list is transient, as
contact events are, so neither a save nor a clone keeps it.

## Qualification

`world::tests::connection` runs at Clock's scale (gravity 400, length unit 20,
20 wu per fixture unit) and in metres (gravity 9.81, 0.5 m per unit). The
branch is a box four units long and half a unit high, welded by its end to a
fixed trunk. Its beam is the branch's own square section over its length.

- **Resting:** at rest, the root moment equals the branch's weight times its
  two-unit reach, and the shear equals its weight. Both match to f32 rounding,
  at most 1.2e-7 relative, from the first step. That holds at 60, 120 and
  240 Hz with four solver iterations and at 60 Hz with one and with eight. The
  run lasts ten seconds, in which the branch falls asleep and keeps its reading.
  The charge is exactly zero, and the capacity ends untouched.
- **Hanging:** a body hanging below its root pulls with its weight as tension,
  with no shear or moment, within the 1% the test allows.
- **Chain:** in two branches end to end, each weld carries everything beyond
  it. The tip weld carries the outer weight at two units. The root weld carries
  the inner weight at two units and the outer at six. Released unloaded, the
  two welds read about half their load on the first step and ring. They stay
  within 0.1% from the 25th step.
- **Spinning pair:** in zero gravity, a free hub twenty times as dense as the
  branch turns with it at 2 rad/s about their shared center of mass. The weld's
  tension matches the branch's mass times ω²r within 0.32%, and its moment is
  under 0.37% of tension times reach. This is the case where both bodies are
  dynamic, so every term of the reconstruction counts.
- **Overload:** the branch is loaded to twice its safe stress, with a capacity
  of 300 steps of the law's charge for its weight. It sleeps by step 280, and
  its weld keeps charging its last load. It severs on step 300 in metres and
  301 at Clock's scale, within the law's one tick plus rounding. The charges sum
  to the capacity within 1e-5 of it. The branch then wakes and falls at least
  80% of the free-fall distance past the trunk's face.
- **Saves and clones:** a world saved after 60 steps of a 90-step overload is
  restored, and the original is cloned. Both copies sever on the same step as
  the original, with the same motion, loads, failure and capacity. The restored
  world writes the same save bytes.
- **Order:** two welds exhausted on the same step are listed in identity
  order, whatever order they were inserted in.
- **Refusals and edits:** the tests cover every refusal, removal with either
  body, transfers before and after the host removes a severed record, and the
  host's own cut.

Material Lab's [Branch gives way](material-lab.md#branch-gives-way) case runs
one weld through a safe load, an overload and its severing in both renderers.

## Limits

- **No shear:** the law does not model shear, so `shear` is reported but
  never charged.
- **No sag:** the weld is rigid. A yielding branch neither sags nor keeps a bend
  before it breaks, and the law's fallback progression stands in for plastic
  rotation.
- **Final substep only:** the load is the final substep's. A hit reaches the
  weld only as the load it leaves in that substep, and a jolt that ends before
  it goes unseen.
- **Extra solver iterations:** as for a tether, the reading assumes the world's
  substep count. A weld whose island has a body asking for additional solver
  iterations under-reports by the ratio of the counts, and only the rover asks
  for them.
- **No native response:** ordinary steps refuse the native material response
  while the world has any joint, as they do under gravity. Regions in a world
  with welds take no native impact charges.
- **No automatic follow-up:** a severing does not fracture, split or repair
  anything, and a connection is never re-welded automatically. Moving a body
  that any connection record names to another world is refused.
