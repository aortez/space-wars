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
what a severing means. A connection that [bends](#connections-that-bend)
(#224) is a hinge instead of a weld: a spring with the beam's own bending
stiffness holds the held body's turn, so a branch sags under its load and
springs back when the load is lifted. An [anchor](#anchors) (#225) holds a
body in its support by springs in every direction, as the ground holds a root,
so that several anchors share a load by where they hold, and passes to another
support where the ground it holds in breaks away.

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

## Connections that bend

With `ConnectionSpec::bends`, the connection is a hinge at the root rather than
a weld. Its two linear axes are locked, as a weld's are, and a spring holds its
turn: a Rapier motor, force-based so that its stiffness is a real moment per
radian, aimed at the angle the bodies were joined at. Its stiffness is
`hinge_stiffness`, the connection law's own rotation stiffness
`stiffness * I / length` for the beam. A settled hinge therefore turns by the
law's elastic rotation for the moment it carries. Nothing else holds or resets
the angle: a heavier load bends the branch further, a lifted one lets it
spring back, and the rest angle never moves.

The spring's damping is set when the connection is inserted, or reshaped, from
the held body's moment of inertia about the root, `J`. It is `2 z sqrt(k J)`,
where `k` is the stiffness and `z = -ln(1 - damping) / 4 pi` is the damping
ratio at which a free swing of that body alone loses the material's `damping`,
its fraction of stored elastic energy, each cycle. That ratio is at most
critical. Wood's damping of 0.2 gives a ratio of 0.018, so a branch rings for a
while.

The spring's impulse is a solver row of its own, so it is the moment at the
root. Neither locked linear row adds to that moment, since both act at the
root. Rapier orthogonalizes the two linear rows in the bodies' mass metric, the
one along the beam first, and the world undoes that as it does for a weld.
`ConnectionState::deflection` is the held body's turn about the root since the
bodies were joined, read from their poses and positive counterclockwise. A weld
reports its own slight give, which reads zero in the tests, and a severed
connection reports none.

A hinge is charged as a weld is, from the moment and axial force it carried.
Its turn is all elastic, so it measures no plastic rotation either, and the
law's fallback progression stands in for it.

`reshape_connection` gives a live connection a new beam where it is joined, as
wood does when it thickens. The joint's frames stay, so a hinge keeps the rest
it was joined at rather than taking the turn it has. Its spring takes the new
beam's stiffness, damped as at insertion for the held body as it then is, so a
host that changes the held body's mass first, as a growing tree does, has it
damped for its new mass. The work the connection has absorbed stays absorbed:
it keeps the new beam's capacity less what the old one used. A connection the
world does not have is refused with `Missing`; one that has severed, or a beam
whose capacity is no more than the work already absorbed, with `Exhausted`;
and a beam that is not valid for the material with `Geometry`. A refusal
changes nothing.

## Anchors

`PhysicsWorld::insert_anchor` holds a body in its support as the ground holds a
root. An `AnchorSpec` names the support, the held body, the point where it
holds, the held length's direction and the connection. No axis of an anchor is
locked. A force-based spring holds each: the two linear ones with
`anchor_stiffness`, the connection law's own axial `stiffness * area / length`
for the beam, and the turn with `hinge_stiffness`. A settled anchor therefore
gives by the law's elastic extension for the force it carries, whichever way
that points, and turns by its elastic rotation for the moment.

Two bodies may share any number of anchors, and their give is what lets them
share a load. Rigid joints between the same two bodies would leave the solver
to split it as it happened to. Each anchor carries its stiffness times how far
its own point has moved. So like anchors share a weight evenly, and one farther
from the center the held body turns about carries more of a turning load, at a
longer arm. An anchor's frame is fixed in its support, as any joint's is: it
follows a support that moves, and a dynamic support takes the reaction.

Each spring's impulse is a solver row of its own, which Rapier leaves as it is,
so the three impulses are the load: along the beam, across it and about the
anchor. The sample reports them as a weld's, with tension positive. The law is
charged with the moment and with the whole force as its axial load: what holds
an anchor gives way to a pull whichever way it points. A weld's and a hinge's
shear still charge nothing. An exhausted anchor severs as any connection does,
and `connection`, `remove_connection`, `severed_connections` and the save treat
it as one. `reshape_connection` gives its springs the new beam's stiffnesses.
Contacts between the two bodies stay off while any anchor joins them. Each
spring is damped at insertion for the held body alone, the linear ones for its
mass and the turning one for its inertia about the anchor, at the ratio a
hinge's is.

`rebind_connection` hands a live connection's root end to another body where
it lies, as when the dirt that holds a root breaks away from the ground with
it. Its frame on the new body is where its frame on the old one lay, so the
connection keeps the give it has, its springs, its beam, the work it has
absorbed and its last load, and nothing stirs. From the next step the new body
takes what the old one took, and contacts between the new pair stay off while
it holds. It is refused, with nothing changed, for a connection that is
missing or has severed, and for a body that is missing or is the one the
connection holds. A save keeps the new root body as any other.

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
or hinge and lists the connection in `severed_connections()`, so a severing
never happens mid-solve. Removing the joint wakes both bodies, which keep their
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
connections refuses a save that has them, as trailing snapshot data. A hinge's
spring is saved with the joints, so a record that bends holds nothing more and
the section's layout is unchanged. Restoring checks each record against the
saved joints. A live connection needs its own fixed weld, its own hinge or its
own anchor between its own bodies, and a severed one needs no joint. A hinge's
spring must have its beam's stiffness, a force-based model, a rest angle of
zero, unbounded force and finite damping, and so must each of an anchor's
three, on no locked axis. A record must hold a failure exactly when its
capacity is exhausted. As for registered
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
Its [Branch bends](material-lab.md#branch-bends) case runs the same branch on
a hinge at a quarter of real time: it sags from level under its own weight,
bends further under a light load, springs back when it is lifted and breaks
under the heavy one.

`world::tests::bending` runs the same branch on a hinge, at both scales, in
wood-like brittle material with wood's damping of 0.2. The hinge's stiffness
turns the level branch's moment into 0.05 radians, and its balance is the
turn at which the spring holds the weight at its lever arm as it then lies,
`k turn = weight * reach * cos(turn)`. The branch never sleeps, so that it
settles all the way.

- **Sag:** released level, the branch swings down to 1.87 times its balance at
  60 Hz and settles within a minute. It rests 0.03% past its balance at 60 Hz
  with four substeps, 0.001% at 240 Hz and 0.004% with eight. With one substep
  it rests 2.8% past: the solver takes the spring's row before the two locked
  rows in each substep, so it converges a little past the balance. Either way
  the moment it reports matches the weight at its lever arm within 2e-6, and
  the shear is the weight within the 0.2% the test allows. It charges nothing.
- **Settling back:** three times the weight bends it to 0.148, within 0.03% of
  its balance. Lifted, the branch springs back to 0.036 above level and settles
  within 0.01% of where it rested, with its capacity untouched.
- **Thickness:** a beam twice as high turns the same branch 7.94 times less at
  Clock's scale and 7.95 in metres, where the balances give 7.97. The stiffer
  hinge settles 0.4% past its balance.
- **Thickening:** 30 steps into an overload at twice its safe stress that would
  use its capacity in 90, a hinge has used 40% of it and turned 0.012. Given a
  beam twice as deep there, it reports the same turn and keeps that work
  absorbed. It springs up and settles at half its safe stress within 4e-6 of
  where a hinge of the deep beam joined level settles, 0.9% past its balance,
  rather than 0.006 below where it hung, as a hinge joined there afresh would.
  Refused requests change nothing, and the world saves and restores byte for
  byte.
- **Direction:** a branch on the trunk's left turns counterclockwise by the
  same amount as the right one turns clockwise. A load that lifts the right
  branch turns it counterclockwise by as much.
- **Chain:** in two branches end to end, damped nearly critically, each hinge's
  moment is the weight beyond it at its lever arm as the branches now lie,
  within 0.2%. Both turn clockwise, the outer one less.
- **Impact:** a stone as heavy as the branch falls onto its end. The hinge is
  stiffer, turning the level branch 0.02, and safe to eight times the
  branch's own moment, with almost no capacity beyond. Dropped from a quarter
  of a unit above the branch's top face, the stone swings it to 0.79 of its
  safe stress, then rests on it at 0.34, turned 2.74 times as far as the
  branch alone. The moment matches the two weights at their lever arms as
  they lie within 2e-5. Lifted off, the branch springs back to where it
  rested. Dropped from two units, the same stone breaks the hinge four steps
  after it lands.
- **Overload:** a hinge at twice its safe stress whose capacity lasts 240 steps
  of the level branch's charge severs on step 233 at Clock's scale and 234 in
  metres, a few steps sooner than a steady load would, since it swings while
  overloaded. The charges sum to the capacity within 1e-5 of it. The branch
  then falls onto fixed ground and rests flat on it.
- **Saves and clones:** a world saved 40 steps into an overload is restored,
  and the original is cloned. Both copies swing and sever exactly as the
  original, and the restored world writes the same save bytes.

`world::tests::anchor` hangs a level bar eight units long below a fixed
support, at both scales, from anchors of a brittle material whose beam is half
a unit square and a unit long.

- **Give:** one anchor at the bar's middle, safe to four times its weight,
  gives 0.05 units under it, the law's extension, within the 0.1% the test
  allows. That holds at 60 and 240 Hz with four solver iterations and at 60 Hz
  with one and with eight, and for an anchor that points down, across or
  between the two. The weight reads as tension along a beam that points down
  and as shear across one that points right, and either way the anchor is
  charged for a quarter of its safe stress, which costs nothing.
- **Sharing:** four anchors at 3 and 1 units either side of the middle hold a
  bar whose center of mass lies a unit off it. They carry 0.10, 0.20, 0.30 and
  0.40 of the weight from the far side to the near, each also bent by the bar's
  slight turn, within 0.2% of the weight of what the springs' stiffnesses give
  for a rigid bar. Together they carry the weight where it lies.
- **Letting go:** with the outer anchor toward the weight as stiff as the rest
  but safe to three tenths of the weight, it lets go within half a second as
  the bar settles onto it, alone. The three that are left carry the weight as
  they then lie, within 0.5% of the weight: the one beside the weight five
  sixths of it, and the far one holding its end down.
- **Following:** with every anchor safe to three tenths of the weight, and the
  weight coming on over two seconds, the outer anchor toward it lets go once
  about three quarters of it is on, the rest follow within half a second, the
  far one last, and the bar falls. With its weight in the middle, the same
  anchors hold the bar at five sixths of their safe stress and charge nothing.
- **Saves and clones:** a world saved with half the weight on is restored, and
  the original is cloned. Both copies let all four anchors go exactly as the
  original does, and the restored world writes the same save bytes.
- **Reshaping:** an anchor given a beam twice as thick where it holds gives
  half as far and is charged for half the stress.
- **Refusals:** an anchor is refused as a connection is, its identity is taken
  by an anchor or a weld alike, and removing the held body removes its anchors.
- **Moving supports:** with the bar hung by its middle from a kinematic clod a
  unit square, four units above it, the clod is carried three units right and
  two up and turned a quarter turn over four seconds, setting off and coming to
  rest gently. The bar comes to hang by the anchor's give below where the clod
  now holds it, turned the quarter turn with it, and its weight now pulls
  across the anchor's beam, at the same quarter of its safe stress.
- **Loose supports:** a dynamic clod as heavy as the bar, holding it at its
  middle, falls with it: after a second both fall at the same speed, gravity's
  times the second, and the anchor carries nothing. Pushed sideways where
  nothing weighs, the bar pulls the clod after it at the same speed, and the
  anchor carries half the push, the share that moves the clod.
- **Turned gravity:** with gravity half a radian off straight down, two anchors
  share the weight evenly along it, by its cosine along their beams and its
  sine across them, at an eighth of their safe stress. The bar sinks by the
  give along gravity and does not turn.
- **Handing on:** a bar hung from one anchor in the trunk is handed to a
  kinematic clod six units to the side and three up, clear of both. The anchor
  keeps its connection and its load,
  the bar does not stir, and it still carries the weight along its root. With
  the trunk removed the anchor stays, and carrying the clod two units right and
  one up carries the bar the same. A restored copy writes the same save and
  steps exactly as the original. Handing on is refused for a missing anchor, a
  missing body, the bar itself and an anchor that has let go.

## Limits

- **No shear:** the law does not model shear, so a weld's or a hinge's `shear`
  is reported but never charged. An anchor's is charged, as part of its pull.
- **One grip in every direction:** an anchor holds along its beam and across
  it with the same stiffness, and gives way to the same force either way,
  pushed or pulled.
- **Anchors damped singly:** each anchor is damped for the held body alone, so
  many anchors on one body damp it by more than the material's ratio, by the
  square root of their number, and less where that body carries others.
- **Elastic sag only:** a weld does not sag at all, and a hinge's turn is all
  elastic. A yielding branch keeps no bend before it breaks, and the law's
  fallback progression stands in for plastic rotation.
- **Damping set once:** a hinge's damping comes from the held body's inertia
  when it is inserted. A body that gains mass later keeps that damping, and a
  hinge that holds a chain is damped as if it held its first body alone.
- **Sleep mid-swing:** a branch that may sleep can fall asleep before its
  swing has died out. In the sag fixture, it slept after 2.7 s 13% short of its
  balance in metres and after 4 s 3% past it at Clock's scale. It keeps that
  pose and that last load until something wakes it.
- **Coarse steps under strong gravity:** the solver leaves each joint a
  steady give of about gravity times a substep squared, and a hinge rests past
  its balance by a share that grows with it. Under Material Lab's gravity of
  400 per unit, a branch stepped at 60 Hz with four substeps hung about 1% past
  its balance, with its fastened weight 0.006 low, so Branch bends steps at
  240 Hz, where they are within 0.05% and 0.0004.
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
