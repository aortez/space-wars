# Weapon sources (#329)

`engine_material::Source` gives one shot or charge a finite energy budget and
delivers it through the [material laws](material-damage.md), so a recipient
responds to a weapon as it does to any other load. A source has up to three
parts, and each is attributed and accounted separately:

- **Kinetic**: a projectile, with its own material region, its mass and its
  launch speed. The budget is its kinetic energy at launch.
- **Payload**: stored explosive energy. The payload is released once.
- **Beam**: pulses, each carrying a fixed energy. One pulse is a beam's energy
  per shot. A continuous beam fires one pulse per tick of its lifetime, each
  pulse its power times the tick, so its damage depends on the tick (see
  [Limits](#limits)).

The budgets and the [geometry](#where-the-energy-goes) that divides them are
in the pure crate. `engine_rapier::world::weapon` flies real shots and beams
[in a physics world](#in-the-physics-world),
[Material Lab](material-lab.md#shell-through-moss) fires one shell through moss
into a plate, and [Scorched](#scorched-tanks) charges its tanks' armor from
shells it sweeps itself.

## Deliveries

| Call | Part | Law | Charged to the budget |
| --- | --- | --- | --- |
| `pass` | kinetic | `deposit` of the collision energy with the recipient's own mass | the work the recipient keeps |
| `hit` | kinetic | `impact`, as the host measured it | the impact's available energy; ends the part |
| `release` | payload | `deposit` of each share | each share's energy; once |
| `pulse` | beam | `deposit` of each share | each share's energy; one pulse |

Like the impact law, these calls return outcomes and leave the recipients
alone. The host absorbs each outcome's `consumed` work into its recipient
before that recipient's next event. The projectile's own region is the
exception: the source keeps it and charges it its side of the hard hit.

A share's coupling is its fraction of the whole payload or pulse. The couplings
may not sum past one, beyond a little rounding, and the shares never take more
than the energy in total. What the couplings leave reaches no recipient: a
blast's energy that meets nothing, or a beam's that goes on past its path.

## The deposit law

`deposit(region, face, energy)` loads one region's face with an energy pulse,
such as a blast's share or a beam's pulse. It is the impact law against a
rigid, unbreakable counterpart. A face's safe limit is the material's
resilience times the loaded volume, the face's area times its depth. Up to that
limit the energy is all recoverable or harmlessly damped. Past it, the region
yields and charges irreversible work against its capacity. For the tank's
armor, a face 1 wu wide, 1 wu thick and 0.2 wu deep is safe to 20, and a
share of 1500 charges it 1465 of its 200,000.

Nothing rebounds from a deposit, so its recoverable part needs no response. A
deposit charges the material only. The push a blast or a beam gives a body is
the host's physics, as the blast impulse is today.

## Grass and other nonblocking recipients

A shot sweeps through grass and moss without stopping. `pass` uses a
weak-coupling, ballistic approximation. It does not claim penetration physics.

- The recipient's face takes a deposit of what a perfectly inelastic
  collision with the recipient's own mass would make available:
  `0.5 * m_eff * v^2`, with `m_eff` from the shot's mass and the recipient's
  region and `v` the sweep's actual closing speed. No blade receives a copy of
  the shot's whole budget.
- The recipient keeps its irreversible and damped work, and one the shot cuts
  keeps its remaining capacity. Its recoverable energy stays with the shot,
  and so does the work a cut recipient could not take, as the outcome's
  resolved failure motion.
- The shot flies on unslowed and undeflected, so its hard hit is the one it
  would have made without the grass. The approximation neglects the work the
  grass keeps, which is the shot's real loss.
- The work the passes keep comes out of the kinetic budget, and a pass gets no
  more energy than the budget has left. A dense crowd cannot draw more than
  the shot carries; once the budget is gone, later passes get nothing, and the
  shot still makes its hard hit.

For scale: a 0.5 mu shell at 300 wu/s carries 22,500. A 0.02 mu blade of soft
tissue sees 865 from it. The blade keeps 5.5 of that: its whole capacity of
1.2, so the shell cuts it, and 4.3 damped. Half the budget would take more than
2,000 blades.

## Where the energy goes

The geometry functions find the recipients and give each its coupling and its
face's width. A recipient is one or more circles, each a `Target` with the
recipient's key, a center, a radius and the recipient's represented thickness
into the screen. Circles that belong to different recipients should not
overlap. A recipient's own circles may: each line of energy meets a recipient
once, as one body, at the first of its circles on that line.

Energy travels in straight lines: outward from a blast's center, or along a
beam. Wherever it meets a target, the target takes the fraction of what
arrives that its thickness covers of the front's height, and the rest carries
on behind it. A target as thick as the front stops everything in the lines it
covers. A grass blade 0.2 wu thick in a 1 wu front takes a fifth and passes
four fifths on. The shares can never sum past the whole, however many targets
crowd the front. The front's height is a property of the source, such as the
depth of the scene the blast fills.

- `blast_reach` spreads a blast's energy evenly over every direction in the
  plane. A target takes the directions in which the energy meets it within
  the blast's radius. That is every direction it subtends, unless it straddles
  the blast's edge, where it takes fewer, down to none as it leaves. Its face
  is the part of its diameter that the energy reaches. A blast inside a
  target meets it first in every direction.
- `beam_reach` spreads a beam's energy evenly across its width, from its start
  to the end of its trace, where it meets a hard surface or runs out. A target
  takes the part of the width that its diameter covers. Its face is the part
  of that width the energy reaches. `through` is what reaches the end of the
  path: the coupling of the hard surface there, or what goes on.
- `sweep_crossings` lists the targets a shot touches as it sweeps from one
  point to another, in the order it reaches them, for `pass`. A target it
  already touches at the start comes first. A blade the shot still touches
  when the next tick's sweep starts is listed again, and the source refuses
  the repeat.
- `footprint_overlaps` lists the targets under a footprint, such as the patch
  where a tank's tread rests, with each one's share of the load and its
  width. The load spreads evenly along the footprint, and recipients side by
  side under the same stretch share it equally, however many of their circles
  lie there. The host presses each tissue with
  its share through the tissue adapter's sustained law, so the crushing
  follows the #328 contract, with no timer and no body per blade.

Energy meets the targets covering each piece of a front in the order the
piece's middle line reaches them. That is exact for targets that do not
overlap: two such circles cannot change order within a piece without touching.
The order is not each target's nearest point. A blade in front of a large
plate's curved face is reached first, even where the plate's nearest point is
nearer.

Every width the functions give is within the lengths the laws accept, from
a millionth of a wu to a million, so a sliver of overlap or a vast body still
makes a valid face.

What the geometry leaves out: energy does not reflect or bend, the front loses
nothing to a medium within its radius, and a beam's ends treat a target as the
band across its diameter. The host decides which targets a front can meet,
such as the tissue within a blast's radius and the bodies its physics reports.

## In the physics world

`Weapons` holds the live sources of one `PhysicsWorld` and delivers them through
the laws above. The host keeps its tissue and answers for it through the
`Recipients` trait: the circles near a point that it admits, the region and
section it keeps under each key, and the work to charge. A body is charged
through the material registered for it with the world's
[loading response](material-native-response.md), or through a region the host
keeps for it, which `Recipients::body` names by a key from the same space as
the tissue's. A key names one region, kept for tissue or for bodies but not
both. A host keeps its own regions for bodies where the response cannot run, as
in a world whose contacts it refuses, or where the host repairs them.
A `Recipient` is one of the host's tissues, a body whose region the host keeps,
a registered region, or a body outside the material model. Tissue is what no
collider stands for, such as grass, which shots and beams pass through. A
recipient whose collider stops them, such as a pilot on foot, is a body, and
the host lists `Body` circles for it that cover its colliders.

- **Shots.** The host inserts the shot's body, moving at its launch velocity,
  and `launch` takes it over: the body's mass and speed are the projectile's.
  The body must be alone in its entity and bound to no loading region, and the
  world must collect contact events. Around each step, `prepare` records the
  motion each body brings into the step, a position-driven body's prescribed
  velocity included, and `settle` uses it. `settle` sweeps each shot from where
  the step found it to where it left it, passing the tissue on the way at the
  speed the shot entered the step.
- **The hard hit.** Of the contacts a shot made in a step, the one that took
  the most impulse stops it. A touch that pushed nothing, such as a predicted
  contact the shot slides past, does not. The impact is measured at the
  contact from both bodies' motion before the step, with the contact-effective
  mass of the shot's mass and the target's mass, inertia and lever arm; fixed
  and kinematic bodies are immovable. The patch is the shot's curvature against
  a flat face. Its outcome charges the body's registered material, or the
  region the host keeps for it. The shot's body is not bound to a loading
  region, so the contact response refuses the batch it arrives in, as it
  refuses any unbound contact, and charges nothing for the shot. Physics
  resolves the shot's momentum.
  A host that sweeps its own projectiles, which need not be bodies, measures
  their hits the same way with `projectile_impact`, from the projectile's mass
  and velocity and the struck body's motion, recorded entering the step or
  read now.
- **Blasts.** A shot that stops releases its payload where the step left it,
  divided by `blast_reach` among the tissue and body circles the host admits.
  `detonate` releases it wherever the shot is, for a fuse the host decides on.
  A `Burst` event gives the center, so the host can push bodies and draw it.
- **Beams.** `fire` traces one pulse by ray cast to the first solid surface of
  the beam's collision groups within the range the host gives the pulse,
  which can grow pulse by pulse as a beam extends. The pulse carries the
  energy the host gives it, at most the beam's pulse energy, so a beam can
  weaken with range. The tissue and bodies the host admits across the path in
  front of that surface take their shares and shadow what lies behind them,
  as in a blast. The beam passes through its protected entity, and reaches
  no other recipient past the surface it stops on. That surface's recipient
  takes the part of the beam its own circles within the pulse's range cover,
  however far past the surface they lie, and what they miss goes on past. The
  bodies of one entity whose region the host keeps under one key are one
  recipient, whichever of them the ray meets. A surface whose recipient has no
  circles takes all that reaches it.
- **Retirement.** A shot retires when it stops or its lifetime ends, and the
  module removes its body's entity once every shot has flown that step, so two
  shots that meet both stop. A beam retires after its last pulse. `retire` ends
  a shot or a beam early, without a blast. Each retirement is reported once,
  with the final ledger and the owner.

Every delivery, refusal and exclusion is an event, and `take_events` reports
each once. A paused step changes nothing. Each step takes its own `Prepared`
record, which `settle` uses up. Sources, recipients and contacts are visited in
a fixed order, so the same build replays the same events.

### Protection and bodies outside the material model

`protected` names an entity a shot or beam never charges, such as its firer. A
shot that touches it, or a blast that reaches it, reports the energy as
`Excluded` with the reason `Protected`, and a beam's trace passes through it.
Collision filtering stays with the host: a shot collides as its body's
colliders do, and a beam stops on the collision groups it was armed with.

The host's own policy can shield any recipient for a while through
`Recipients::protected`, such as a pilot just after ejection. A shielded
recipient still stops what it would stop: a shot stops on a shielded body and
bursts there, and a beam ends on it. The energy that meets it is reported as
`Excluded` with the reason `Protected` and is not charged, a pass through
shielded tissue included, which a shot meets once.

A body with neither a registered material nor a region the host keeps for it,
such as terrain, still stops shots and fronts. The energy it stops is reported
as `Excluded` with the reason `Unmodeled` and is not charged, and so is the
share of a blast or beam that reaches tissue the host keeps no region for. The
tissue and bodies with regions that the same shot or front reaches take their
shares regardless. A recipient with a circle the reach geometry refuses is
refused with `InvalidGeometry`, and the rest of the front goes on without it.

### The terrain boundary

Terrain brushes and chipping keep their current operation, admission and
conservation contracts. Terrain is not a registered material, so a weapon's
energy that terrain stops is reported as unmodeled, never charged, and the
legacy terrain effects have no energy model of their own. A crater brush or a
chip stays the host's separate effect, and a terrain edit that is refused or
unavailable does not suppress the source's charges to tissue and bodies. A host
that moves a weapon onto a source removes that weapon's legacy damage path, so
no budget is spent through both. Global energy closure waits for the terrain
migration.

## Scorched tanks

Scorched's shells are not bodies: the duel sweeps each one by ray cast every
step. For [#331](https://github.com/aortez/space-wars/issues/331) it delivers
their sources through the pure API rather than `Weapons`, and each tank's armor
is one region the duel keeps.

- A shell that strikes a tank is measured with `projectile_impact` and charges
  that tank's armor with its hit. A shell that strikes anything else reports its
  energy as unmodeled.
- Its payload bursts a shell's radius off the surface it struck, never inside a
  tank, and `blast_reach` divides it among the circles standing in for the tanks
  within 5 units of their surface. Terrain takes no share and shields nothing.
- A host puts its own recipients in the way through the same `Recipients`
  trait. A shell passes the tissue it crosses, a burst reaches tissue in one
  front with the tanks, and other bodies stop the energy that meets them.
  Regions bound to the world's loading response stop energy the same way,
  since the native response can't run in a world with gravity.
- The duel works in Scorched's own units at any placement scale, and sees a
  host's recipients at that scale: lengths divide by the scale, and a region
  keeps its material and the share of its capacity it has left. The host is
  charged the share of its capacity that the same delivery would cost at
  scale 1.

Contacts charge nothing yet, so a tank that falls or rams something is
unharmed. They wait on an experiment with measured contacts.
[Scorched Earth](../scorched-earth.md#armor-and-shells) has the calibration.

## Once only

- A shot reaches each recipient once. Another report of the same recipient,
  such as an overlapping sweep or a second collider, is refused as a
  `Duplicate`. Passes and the hard hit share one set of keys, so a host keeps
  keys from different id spaces, such as tissue and bodies, apart.
- The hard hit ends the kinetic part. If a detonating shell's direct-hit and
  generic contact callbacks both report the hit, only the first spends it.
- The payload is released once, and a beam fires at most its pulse count.
- A release or a pulse may name each recipient only once. The host merges a
  recipient's colliders into one share, as `ContactGeometry::merge` merges
  sub-contacts, so an extra collider cannot multiply a share.
- A refused call changes nothing. A release or pulse with one invalid share
  delivers none of them.

## Attribution and lifetime

The source holds its firing actor only as an opaque `owner` id. It never reads
the actor, so the actor's death changes nothing, and the shot keeps its
attribution. `retire` ends a source: later deliveries are refused, and its
owner and ledger stay readable. `spent` reports a source that is retired or has
used every part it has.

Arming, protection and similar policies stay at the gameplay boundary. The host
decides when a payload releases and which recipients a share list includes. A
recipient that a protection rule excludes is left out of the shares, and the
host's diagnostics say why.

## Capacity refusal and accounting

An exhausted recipient takes no more work. What it cannot take is its
outcome's excess, which the host resolves as failure motion or dissipation, as
for any impact. Each part's `PartLedger` records its `budget`, the energy its
recipients received (`delivered`), the irreversible work they took
(`absorbed`), and the `excess` they could not take. A hard hit's excess
includes the projectile's own: a shell spent on armor adds what it could not
take. For the payload, `budget - delivered` is what never reached a recipient.
For the beam it also holds what the host kept back from pulses it weakened.
Neither is ever negative: a share rounds down rather than past what is left.

The kinetic budget bounds the passes only. A hard hit is the ordinary impact
the host measured from actual motion, and it can make more available than the
launch energy: a target closing on the shot adds its own motion, and a shot
that fell gained speed.

## Limits

- No momentum. Passes do not deflect a shot, and deposits do not push.
- A shot stops at its first hard contact. It does not ricochet or pierce a
  body, and its patch assumes a round shot against a flat face.
- Fronts meet bodies as the circles the host admits for them, which may
  overstate a long body's shadow. A beam's stop with circles takes only the
  part of the beam they cover.
- A beam's collision groups choose only the surface that ends its trace. A
  body the host admits in front of that surface takes its share whatever the
  groups, as in a blast.
- A beam's trace ends at the surface its ray meets for every recipient but
  the stop. The stop's own circles past that surface take their part of a
  wide beam even through another recipient beside the ray and behind the
  surface, which the beam does not reach.
- A shot's body is its own entity. The module removes the entity when the shot
  retires.
- A shot launched after `prepare` sweeps from its launch, but a hit in that
  first step may be measured against the other body's motion after the step.
- The kinetic budget is the launch energy, not the energy where a pass
  happens.
- A beam's damage depends on its pulses' size. Each pulse charges only the
  energy past a face's safe limit, so the same power in shorter pulses does
  less, and none once a pulse falls within the limit. A continuous beam keeps
  its pulse to the game's tick, not the physics substeps.
- `Weapons` is not saved. A `Source` serializes for diagnostics, but nothing
  restores live sources, so a host that saves replays them, as Material Lab
  does.

## Qualification

`crates/engine-material/tests/source.rs` covers:

- a shell cutting 40 blades, each blade receiving its own collision's energy
  and keeping only its capacity and damping, with this note's figures, and
  the shell's hard hit equal to the plain impact without the grass, its own
  excess in the ledger;
- a crowd of 10,000 blades, which never takes more than the budget has left;
- brittle twigs, which keep all their work, taking the budget and never more,
  at a speed where the last share would round up in `f32`;
- duplicate pass reports and a second hard-hit callback, both refused, and
  the energy a pass would offer, refused as the pass would be;
- a pulse carrying part of the beam's energy, the rest never reaching a
  recipient, and pulses past that energy refused;
- one release divided among a crowd as it would go to one recipient, released
  once, with couplings past one and duplicate recipients refused, and shares
  whose `f32` rounding would pass the payload held to it;
- beam pulses giving 10, 9 and 81 of 100 to two blades and the plate where
  the beam ends, passing on what the blades leave without the plate, and
  stopping at their count;
- a worn plate taking what it has left and a failed one taking nothing, with
  the excess recorded;
- a retired source that keeps its owner and refuses every delivery;
- a source spent only once every part is;
- invalid specs, loads and faces, refused without change, and a projectile's
  region a rounding hair heavier than the projectile accepted;
- the deposit law's safe limit, with this note's armor figures, and its
  agreement with an impact against a rigid anvil.

`crates/engine-material/tests/reach.rs` covers:

- a lone target taking the directions it subtends wherever it lies, and a
  thinner one its thickness's part of them;
- twelve plates sharing every direction with nothing left over, a hundred more
  behind them getting nothing, and thin rings taking no more than their
  thickness allows;
- nearer plates hiding farther ones, wholly or in part, across the zero
  direction too;
- lines meeting a blade in front of a plate's curved face before the plate,
  in a blast and in a beam;
- a blade passing four fifths of a blast to the plate behind it;
- a blast's radius bounding what it reaches, a target straddling its edge
  taking only the directions that meet it within reach, and a blast inside a
  target;
- a recipient's two circles combined into one share, and two overlapping
  circles of one recipient taking from each line once, in a blast, a beam and
  a footprint;
- a beam's width divided between two blades, one partly in the other's
  shadow, a wall that stops it, and targets beside, behind and past it;
- a sweep listing what a shot touches, in order, and a shot that has not
  moved;
- a footprint's load divided among the blades under it;
- invalid geometry refused, and slivers of overlap and a vast body still
  giving valid face widths;
- every result the same for a scene moved and turned;
- the whole chain into a plant world's tissue: a blast, a beam and a shot
  through three blades of moss to a plate, each blade charged once and its
  failure reported once, and a tread crushing the two blades under it.

`crates/engine-rapier/tests/material_weapons.rs` flies real shots in a physics
world. It covers:

- a shot through four blades of moss to an armor plate, each blade passed once
  at its own collision's energy and cut, and the hit equal to the impact law's
  outcome without the moss, charged to the plate once;
- a shell's blast released once where it stops, the plate filling nearly half
  its directions, and both charges attributed to their parts;
- a plate shadowing a blade behind it, with a host that lists the shell's own
  body;
- a beam's three pulses dividing between three blades and the plate;
- a hit across two of a plate's colliders, made once, and a body appearing
  where the shot stopped, which nothing reaches;
- a protected firer's death mid-flight, after which the shot still delivers
  under its owner;
- paused steps, which change nothing, and a replay matching event for event;
- a shot that outlives its lifetime, retired once with nothing released;
- a worn plate taking what it has left and a failed one nothing;
- unregistered ground stopping a shot, its share reported, not charged, while
  the blast still reaches the moss;
- a protected firer that a beam passes through and a blast and a shot report,
  uncharged;
- moving and turning targets, whose speed and inertia shape the hit, measured
  where a sliding plate was when struck, and a driven plate's prescribed speed;
- a slow shot with predicted contacts, passing moss on the plate's face at the
  speed it arrived with and bursting where it stopped;
- refused launches: a used id, a body in use, a fixed body, a body sharing its
  entity, a body bound to a region, a world without contact events and
  geometry outside the laws' range;
- a touch that pushes nothing, which doesn't stop a shot;
- two shots striking one plate in one step, both charging it, and two shells
  meeting head on, each stopping on the other;
- a shot launched after `prepare`, which passes the blade in its first step;
- a beam and a shot retired early, with no blast, and a shot without a blast
  detonated;
- beams passing a body outside their collision groups, ending on an unmodeled
  one with its share reported, and running out at their range;
- a recipient with a circle the reach geometry refuses, refused while the beam
  and the shot reach the rest;
- tissue the host keeps no region for, its beam and blast shares reported;
- a plate whose armor the host keeps, hit as a registered plate is and charged
  the same work;
- a blade and a plate the host shields: the shot passes the blade, meeting it
  once, and stops and bursts on the plate, neither charged;
- a shielded blade already passed, or on a face the geometry refuses, refused
  as an unshielded one would be;
- a beam ending on a body the host keeps, charged with all that reaches it,
  and still ending there, uncharged, while the host shields it;
- a beam stopping on a ball that covers half its width, reaching no tissue
  past it;
- a ball off the ray taking its part of a beam and shadowing a blade, with the
  plate behind taking the rest;
- a rover's region over two bodies, one recipient whichever the ray meets,
  taking what its circles within the pulse's range cover;
- what a beam cannot deliver, to a shielded plate and tissue with no region,
  reported at each share, the shares adding up to the pulse;
- a stop with a circle the geometry refuses, a blade with a circle at no
  finite place and a blade with a negative radius, each refused whole while
  the rest of the beam delivers, and bad circles the beam cannot meet, not
  refused;
- a ball whose circle begins past the surface the ray meets, given nothing by
  a pulse that ends before the circle and the part the circle covers by a
  longer one;
- a pulse reaching only as far as its range and carrying the energy the host
  gives it, with no range, or energy past the beam's pulse, refused.

`scenarios/scorched-earth/src/armor/tests.rs` and `host/tests.rs` cover
Scorched's shells:

- a hit charged once and a burst released once on both running gears, one
  payload divided between two tanks, the reach and its bot notices, terrain
  hits reported and not charged, and bursts kept outside every tank;
- the calibration: about a third of a tank for a direct hit, so a tank falls to
  the third or fourth, and near misses costing less the farther they land;
- the same damage at scales 1 and 3, and a retired tank's share and airborne
  shell;
- a host's moss passed once by a shell that then hits as it would without it, a
  burst shared with moss and stopped by a body in its way, tissue the law
  refuses stopping energy rather than spoiling the tanks' shares, and moss
  losing the same share of its capacity at scales 1 and 3.
