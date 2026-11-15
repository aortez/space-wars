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
in the pure crate. Real projectiles in the engine are the next piece of #329.

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
take. For the payload and the beam, `budget - delivered` is what never reached
a recipient, and it is never negative: a share rounds down rather than past
what is left.

The kinetic budget bounds the passes only. A hard hit is the ordinary impact
the host measured from actual motion, and it can make more available than the
launch energy: a target closing on the shot adds its own motion, and a shot
that fell gained speed.

## Limits

- No momentum. Passes do not deflect a shot, and deposits do not push.
- The kinetic budget is the launch energy, not the energy where a pass
  happens.
- A beam's damage depends on its pulses' size. Each pulse charges only the
  energy past a face's safe limit, so the same power in shorter pulses does
  less, and none once a pulse falls within the limit. A continuous beam keeps
  its pulse to the game's tick, not the physics substeps.
- A `Source` serializes for diagnostics, but it cannot be restored from a save
  yet.

## Qualification

`crates/engine-material/tests/source.rs` covers:

- a shell cutting 40 blades, each blade receiving its own collision's energy
  and keeping only its capacity and damping, with this note's figures, and
  the shell's hard hit equal to the plain impact without the grass, its own
  excess in the ledger;
- a crowd of 10,000 blades, which never takes more than the budget has left;
- brittle twigs, which keep all their work, taking the budget and never more,
  at a speed where the last share would round up in `f32`;
- duplicate pass reports and a second hard-hit callback, both refused;
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
