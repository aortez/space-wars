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
shells it sweeps itself. [Spacewars](#spacewars-bodies) keeps a region over the
health of each body a weapon can harm, flies its cannon shells as shots and
fires its lasers as beams.

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
deposit charges the material only. A blast that pushes pays for the push on
each recipient's bodies from that recipient's share first, and the deposit
takes the rest (see Blasts below). A beam's push is the host's physics.

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
  world must collect contact events. The shot follows its body by id, so a
  host that replaces the body, as Spacewars does when a shell arms, keeps the
  shot flying. Around each step, `prepare` records the motion each body brings
  into the step, a position-driven body's prescribed velocity included, and
  `settle` uses it. `settle` sweeps each shot from where the step found it to
  where it left it, passing the tissue on the way at the speed the shot
  entered the step.
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
  The blast starts outside the bodies near it: where their circles hold the
  center, `blast_center` moves it to the nearest point just past their edges
  that none holds, so no body takes every direction. Moving out can bring
  other bodies in reach, so the weapons ask the host again near each new
  start, a few times at most. A blast with a `BlastPush` pushes the dynamic
  bodies of each recipient that takes a share, outward from its start and
  weaker with distance, each body once. The share pays for the push first and
  the recipient's region takes the rest; a share too small for the whole push
  buys the part it can. Protected and unmodeled recipients take no share, so
  pushing their bodies is the host's affair. A `Burst` event gives the start,
  so the host can draw it, and a `Pushed` event what each share paid. Scorched
  Earth's shells burst by the same rule, through `blast_center`, `body_push`,
  `pay_push` and `bought`, except that a Scorched burst pushes only each
  tank's hull, from where the blast starts, and throws loose dirt from the
  point the shell struck.
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
  circles takes all that reaches it, or, for a beam taller than the surface is
  thick, the share its thickness spans, as a circle does.
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
  Each tank's share pays for the burst's push on it first, and its armor takes
  the rest.
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

A tank's contacts charge the same armor, through
[contact charges](material-contact-charges.md#scorched-host-331).
[Scorched Earth](../scorched-earth.md#armor-and-shells) has the calibration.

## Spacewars bodies

For [#330](https://github.com/aortez/space-wars/issues/330), each Spacewars
body a weapon can harm has a material region, which
`scenarios/spacewars/src/integrity.rs` defines. A hull is plate 0.2 deep
around its collision shape's perimeter.

| Body | Material | How much | Capacity at full health | Work per point of health |
|---|---|---|---|---|
| Ship hull | ship hull | plate around the open-winged hull's 28.35 perimeter, 2 deep into the screen | 88,455, for 100 health | 884.6 |
| Escape pod hull, in a match | pod hull | plate around the pod's 7.39 perimeter, 1 deep | 17,724, for 100 health | 177.2 |
| Pilot on foot | pilot tissue | the pilot's capsule, 0.3 deep | 2,632, for 100 health | 26.3 |
| Asteroid, chip or breakup fragment | asteroid rock | what the body weighed when made, giving way 1 deep behind a struck face | 4,398 per unit of radius | 1,400 |
| Cannon shell | shell | what the shell weighed when fired | 375 for a round, or 4,712 for a classic shell, for 1 health | 375 or 4,712 |
| Tank's round | shell | the shared cannon's casing, 0.025 | 75, for 1 health | 75 |
| Rover hull | hull | plate around the chassis's 11.2 perimeter, 2 deep | 8,960, for 25 health | 358.4 |
| Tank armor | tank armor | plate around the chassis's 11.2 perimeter, 2 deep | 58,240, for 100 health | 582.4 |

The game still stores health, and a body's region is a view of it: the
region's integrity is the body's health over its full health. Contacts, solar
heat, repairs and rebuilding still change health, so they change the region
with it. `HealthMap` turns the work charged to a region
into the health it costs, and health back into work, as healing paid for in
energy would need ([#246](https://github.com/aortez/space-wars/issues/246)).

- **Debris breaks early.** By the game's rule, debris breaks once it loses a
  fifth of its health. So a fifth of the capacity in the table breaks it, and
  its region never fails.
- **Work per point.** A ship or a rover built with more health has more hull,
  not tougher hull, so a point of its health stands for the same work however
  it was built. Rock takes the same work per point at any size, because its
  health and its weight both grow with its radius. Two kinds of debris
  differ: a supplied round's breakup pieces weigh what the round did, and
  each shell's 1 health stands for what that shell weighs.
- **Region mass.** A hull region is the plate a weapon can harm, which can
  weigh more than its body. A hit takes the body's mass from the physics, not
  from the region.

The ship hull, the pod hull, tank armor and asteroid rock are the reference
hull, armor and rock with only their toughness raised. The match tests'
fixture sets it: a still target, struck every two seconds by a supplied round
closing at 300. There a round fails a fresh ship's hull on the 4th hit, a fresh
pod's on the 2nd and a pilot on foot on the 1st, the survival #330 set. In
open space a still ship takes a 5th. Pilot tissue is soft tissue with only its
toughness raised, for the laser, as [Lasers as beams](#lasers-as-beams) says.

- **Rock.** Rock gives way 1 deep behind the face it is struck on, about the
  width of the patch a shell strikes. Were the whole asteroid to give way, rock
  would be soft enough to take nearly all of a shell's energy. A round breaks
  the asteroids the game spawns, of radius 9 or less, as a shell did, and
  costs rock of radius 40 to 150 about the 15 health a shell's damage did.
  From radius 21 to 36, a shell broke the rock where a round chips it.
- **Closing speed.** A hit's cost follows its energy, so closing speed matters
  more than it did. Closing at 150, a round takes 5.3 from a fresh, still ship
  in open space, where the old rules took 17.9. At 500 it takes 51.4, where
  they took 29.4. Shells carry the firing ship's velocity, so a kill takes
  about 2 to 19 rounds, against about 3 to 6 before.
- **Estimates.** The rover keeps the reference hull, and a round still
  destroys one. The plate depths and the depths into the screen are estimates
  from each body's size, which the toughness absorbs.
- **Tank armor.** A tank's armor is plate 0.2 deep around its chassis, as a
  hull's is, in the reference armor with only its toughness raised
  ([#238](https://github.com/aortez/space-wars/issues/238)). A round shoves a
  tank about 40 u/s, so the fixture holds the tank still before each round.
  There each round costs a fresh tank 13.19, and the 8th fails it, twice a
  ship's 4. At the reference armor's toughness a round costs 17.14, and the
  6th fails it. A round's work grows with the depth of plate behind the face
  it strikes, and a beam's does not, so the laser fails a tank in 9.33 seconds
  from 40 units off, sooner than a ship's 13.87. Armor is twice as strong as
  planet rock, so the rock gives way first. A tank's settling, driving and
  braking cost it nothing, and neither does a bump from another tank or a hop
  that lands at up to 16 u/s. Thrown down onto the planet at 50 u/s, a tank
  loses 1.99. A tank whose armor fails breaks into six breakup fragments,
  which share its bodies' mass and so carry its momentum.

### Cannon shells as shots

The game flies each cannon shell as a `Weapons` shot from the step it is fired
in. It still replaces a shell's body when the shell arms, and the shot flies on
with the new one. The shot stops on the body its strongest contact pushed, and
the impact law charges that body's region. `HealthMap` turns the work into
health, and the contact rules charge that health to the body, so telemetry and
breakups follow as before. Sparks keep the old shell's size, set by how hard
the shell hit, so a hit that costs nothing, such as one on a pod outside a
match or on a protected pod, still throws them. A shell's contacts do no
damage of their own, so a shell whose shot is refused charges nothing.

- The first contact that pushes anything stops a shell, so it no longer
  bounces off the edge of the universe. Planets, the sun, terrain and the edge
  stop it uncharged, and terrain still takes its crater.
- An armed shell can hit its own ship, as before.
- A tank's round meets every solid from launch, so its body is never
  replaced. It passes through its own tank on its first 8 steps instead, and
  its shot expires after 600 ticks without a burst, as
  [Tank fire](../surface-expedition.md#tank-fire) says.
- In a match, a pod or a pilot on foot takes a shot's hit unless ejection
  protection covers it, and a protected one still stops the shell. Outside a
  match, pods and pilots are outside the model and stop a shell uncharged: a
  pod's health there is its rebuild's progress. A hit on a pod there still
  counts, as a protected one's does.
- A shot whose shell dies earlier in the step, such as to a laser or terrain,
  or leaves the game retires before it can land a hit. The old rules spared a
  pod or a pilot on foot such a hit, but not a ship.
- Every ship's cannon fires a shell that weighs what a supplied round does, in
  a match or not. A classic shell weighs 12.57 times as much, and the impact
  law charges its energy: it failed a fresh ship's hull on the 2nd hit, where
  the old rules took the 3rd, and broke asteroids up to a radius of about 120
  in one hit, where the old rules broke those up to about 20. In a default game
  with seed 7, the first one took all of the struck ship's health, where the
  old rules left it 4.8. At a round's weight, the first takes 51.2 and the
  second 47.4, which leaves the ship 1.4. Only the missile a recovery lab fires
  at a seat without weapons still weighs a classic shell, so it shoves as it
  did. The impact law charges its energy too, so its hit costs a full ship
  about 70 health, in flight or parked, where the old rules took about 32 in
  flight and 48 parked. A pod there is outside the model and still takes
  nothing.

### Payloads

A shell can carry a payload. No ship's shell does, and each tank's round
carries the shared cannon's, as [Tank fire](../surface-expedition.md#tank-fire)
says ([#239](https://github.com/aortez/space-wars/issues/239)). The payload
bursts by the blast rule ([In the physics world](#in-the-physics-world)) where
its shot stops, or where its shell dies earlier in the step, such as to a
laser or on terrain. A shell that leaves the game retires without a burst. A
blast meets each body a weapon can harm as circles within its collision
shapes, which `weapon::body_circles` lists: a ball as itself, a capsule as a
row of balls along it, and a box or other convex polygon as a row of up to 16
along its long axis, each the widest the shape holds there. The bodies take
circles only while a payload flies, so lasers and other shells meet them as
they did.

- A body's share costs it health through its region's `HealthMap`, where a
  shot's hit is charged. A tank takes it as a round's hit, naming the side
  whose round it was: the player whose ship fired it, or the side of the tank
  that did. In a match, a pod or a pilot on foot takes it as a missile's hit
  unless ejection protection covers it. A match records it as the shell's hit
  on a ship, a pod or a pilot on foot, unless the shell struck that body
  directly, which the match recorded already.
- The blast's push carries into the game's own record of each body's motion,
  so it lasts.
- Terrain takes the shell's crater and nothing more. Terrain, planets and the
  sun don't shield what lies behind them from the blast, as terrain doesn't in
  Scorched.
- A blast passes shells by, as in Scorched and the Clock: a shell beside a
  burst flies on, neither charged nor pushed.
- A shell whose payload bursts is spent and leaves no breakup fragment.
- Combat telemetry counts a shell's hit and not a blast's.

### Lasers as beams

Each ship's laser is a `Weapons` beam. Each tick a laser shines, it fires one
pulse from the muzzle along the ship's heading, as
far as the beam has grown, a tick's growth past where it last ended. The
pulse's energy is 8,950 over that length, as the old damage was 10 over it, so
the beam keeps the old laser's falloff. The first solid surface ends the pulse,
and the beam passes through its own ship. The body there takes the share of the
pulse that its thickness spans of the beam's height, 2, by the deposit law, and
`HealthMap` turns the work into health. The laser rules take that health where
they took the old damage, so hits, records and deaths follow as before. Sparks
keep their old size.

- **Falloff.** A beam that spreads with range weakens only on targets narrower
  than it, so a ship or an asteroid would take the whole pulse hundreds of
  units away. The game sets each pulse's energy instead.
- **Width.** The beam is 0.01 wide. A face's safe limit grows with the beam's
  width, so a narrow beam loses little of a pulse to it. The limit is a larger
  share of a weaker pulse, so a hit on a ship from 250 units off costs about
  0.1% less than the old laser's, and one from 1,350 about 1.4% less.
- **Calibration.** From 40 units off in the match tests' fixture, the beam
  fails a fresh ship's hull in 13.87 seconds, where the old laser took 13.88.
  A pod's hull is half as thick as the beam is tall, so it takes half of each
  pulse and lasts 5.75 seconds, where it lasted 7.18. Pilot tissue has a
  toughness of 35,000, so a pilot on foot lasts 2.87 seconds, where it lasted
  3.62. Each form still lasts more than 1.8 times the next.
- **Rounds on a pilot on foot.** Pilot tissue is tough enough to outlast a
  slow round. A round kills a pilot on foot on the first hit only when it
  closes at about 236 or faster, as the match tests' rounds do at 300. One
  closing at 200 takes 71 of the pilot's 100 health, and one at 150 takes 40,
  where the old rules' round killed at every speed from 50 to 300. A softer
  pilot dies to slower rounds and sooner to the laser: at a toughness of
  8,750, a round closing at 150 kills one, and the laser takes 0.72 seconds.
  The game keeps the tougher tissue, so the laser keeps its tiers.
- **Protection.** In a match, a pod or a pilot on foot takes a beam's hit
  unless ejection protection covers it, and a protected one still ends the
  beam. Outside a match, the beam ends on pods and pilots uncharged, as it ends
  on planets, the sun, terrain and the edge of the universe.
- **Other bodies.** The rest keep their materials, and the beam wears them at
  its own rate:
  - A rover keeps the reference hull, so each pulse costs it 2.5 times the old
    damage. A rover 40 units off lasts 1.5 seconds in the beam, where it lasted
    3.7 in the old laser.
  - Rock takes 0.64 times the old damage from each pulse, so an asteroid lasts
    about 1.6 times as long in the beam.
  - A tank's armor holds 582.4 work per point of health, so a tank 40 units
    off lasts 9.33 seconds in the beam.
  - A round's 1 health stands for 375, so the beam destroys one about as fast
    as the old laser did: in 1 tick from 40 units off and 10 from 300, where
    the old laser took 2 and 12. A classic shell's stands for 4,712, so the
    beam takes 10 ticks from 40 units off and 62 from 300.
- **A muzzle inside a body.** The beam then ends at the muzzle, where the old
  ray ended where it left the body.

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
  sub-contacts, so an extra collider cannot multiply a share. A beam keys each
  pulse afresh, so one that fires all game keeps nothing between pulses.
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
take. For the payload, `budget - delivered` is what no recipient's region
took: what never reached a recipient, and what shares paid for a blast's
push.
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
- `Weapons` is not saved. It clones, and a clone of the world and its weapons
  plays on as the original would. A `Source` serializes for diagnostics, but
  nothing restores live sources, so a host that saves replays them, as
  Material Lab does.

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
- a clone of the world and its weapons, which plays on as the original, and a
  shot whose host replaces its body, which flies on as it would have;
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
- a body half as thick as a beam is tall taking half of what reaches it, and
  one thicker than the beam taking all, with the same share reported while the
  host shields it;
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
- the calibration: about a third of a tank for a direct hit, within a point at
  the cannon's lowest and highest power, so a tank falls to the third or
  fourth, and near misses costing less the farther they land;
- the same damage at scales 1 and 3, and a retired tank's share and airborne
  shell;
- a host's moss passed once by a shell that then hits as it would without it, a
  burst shared with moss and stopped by a body in its way, tissue the law
  refuses stopping energy rather than spoiling the tanks' shares, and moss
  losing the same share of its capacity at scales 1 and 3.

`scenarios/spacewars/src/integrity.rs` covers:

- the hulls' perimeters, measured on the ship's, the pod's and the rover's
  collision shapes, the area of the pilot's capsule, and each section the
  plate or tissue its volume counts;
- the capacities, work per point and sections of the table above, and each
  form of a ship with its own hull;
- each body's region at full, half, a quarter and no health, health past full
  held to full, health that is not a number refused, and no region for a full
  health that is zero, negative or not finite;
- work and health converting both ways, with full health standing for the
  whole capacity;
- a ship built with five times the health, and a rover with twice, having
  that much more hull at the same work per point;
- debris regions weighing what the debris weighed when made, rock taking the
  same work per point at any size, and chipping and shrinking leaving the
  region alone;
- debris breaking by the game's rule while its region is still nearly four
  fifths whole;
- a contact's damage, a repair and a rover's damage moving the region;
- the calibrated materials differing from the reference ones only in
  toughness.

`scenarios/spacewars/src/shots.rs` and the match and terrain tests cover
shells as shots:

- shells flying as shots from their first step;
- a shot charging the ship it stops on, which loses that and no old damage
  besides, the contact still recorded and the shell gone with the shot, and a
  hit counting for that body alone;
- an armed shot hitting its own ship;
- a shell stopping at the edge of the universe, where an old shell bounced;
- a shot retiring in the step its shell dies to a laser, and when its shell
  leaves the game;
- a round that a laser destroys in the step it strikes a ship or a pod landing
  no hit and counting none;
- a shot recording no contact with a ship or a rover it did not charge;
- a shell contact that no shot carries costing neither the body nor the shell
  and recording nothing, however hard it pushes;
- a shot that charges a ship nothing still counting its hit and throwing the
  sparks the old shell's damage did;
- two shots in one step charging a ship no more than it has;
- a round costing asteroids of radius 10, 40 and 100 what it did under the
  old rules, with the same chips and exactly what its shot charged;
- a round destroying a rover as a shell did, and a slow round costing a rover
  what its shot charged;
- a game with rounds in flight playing the same twice and from a clone made
  mid-game, with a hit landing after the clone;
- a round failing a match's ship on the 4th hit, a pod on the 2nd and a pilot
  on foot on the 1st, each step's loss what the shot charged and each hit
  counted as a cannon hit, with a hull shielding its pilot;
- ejection protection covering a pod and a pilot on foot, which still stop the
  round and are charged nothing, and ending in the tick the old rules' did;
- a shot digging the crater a shell did.

`scenarios/spacewars/src/lasers.rs` and the match tests cover lasers as beams:

- a beam charging the ship it ends on what its pulse deposits, which the ship
  loses with no old damage besides, the beam ending where the old laser's ray
  does and its sparks the old laser's size;
- a beam ending on the first body it meets;
- two beams in one step charging a body no more than it has;
- a beam wearing a rover by its hull, and a rover with less left than a pulse
  takes losing what it has and dying;
- a game with lasers playing the same twice and from a clone;
- the laser failing a match's ship in 13.87 seconds from 40 units off, a pod in
  5.75 and a pilot on foot in 2.87, each step's loss what the beam charged and
  a hull shielding its pilot;
- a round closing at 240 killing a pilot on foot on the first hit, and one
  closing at 230 not;
- ejection protection covering a pod and a pilot on foot from the laser, which
  still end the beam and are charged nothing;
- the laser ending on a pod and a pilot on foot outside a match, charging
  them nothing.
