# Material damage guide

Spacewars, Scorched Earth and Clock's Tanks event share one model of material
damage. Their weapons charge it, and so do the contacts each game has moved
onto it. This guide is its entry point: what the model is and isn't, the
values each game uses, how a host adopts it, what has moved onto it and what
it leaves out. The documents it links hold the contracts and the evidence. The
model is the shared material damage epic's, #325, and #332 qualifies it
across the games.

## What it is

- **The law.** A material has a stiffness, a safe strength and a toughness. A
  load within the safe strength costs nothing. Work beyond it is charged
  against a finite capacity, the material's toughness times its represented
  volume. Each event keeps one energy ledger, so bounce, harmless dissipation,
  permanent damage and motion can't spend the same energy twice.
  [Material damage](material-damage.md#the-material-law) has the law, its units
  and its evidence.
- **Regions.** A region, `engine_material::Region`, holds a body's material,
  its represented volume and the work it can still take. The host keeps it,
  reads its integrity as its own health, and owns death, ejection, protection
  and cleanup ([persistence](material-damage.md#persistence-and-adapter-contract)).
- **Weapon sources.** A source gives one shot a finite budget: a projectile's
  kinetic energy, an explosive payload or a beam's pulses. The payload is
  released once, a beam fires at most its pulses, and a release or a pulse
  reaches each recipient once. The kinetic budget bounds only what a shot
  leaves in what it passes through. Its hard hit is priced from the motion
  the host measured, which can make more available than the launch energy
  ([weapon sources](material-sources.md)).
- **Contact charges.** After each physics step, each new impact between two
  bound bodies is priced from the motion they brought into it, and steady
  supports can be priced too. Pricing leaves the physics unchanged, so Rapier
  still sets the bounce ([contact charges](material-contact-charges.md)).

It is a bounded approximation for games, not a stress simulation:

- The law has no shear, friction work, fatigue, shock waves, penetration,
  early fragment escape, heat or anatomical injury
  ([the law's limits](material-damage.md#limits-and-verification)).
- Lengths are world units, masses mass units and times seconds. The materials'
  values are calibration for play, not measured properties.
- Results repeat within one build. Nothing claims cross-machine lockstep.
- Energy is accounted for each event, not across the world. Rapier's bounce,
  scripted pushes, the terrain's own effects and each host's own rules sit
  outside the ledger.

## Materials and values

The six reference materials are armor, hull, rock, shell, soft tissue and wood,
with their values in
[the model's report](../data/material-model-v2.md#reference-materials). Each
game builds its regions from them:

| Game | Body | Material | Amount | Details |
| --- | --- | --- | --- | --- |
| Spacewars | ship, escape pod and rover hulls | hull, with toughness 7,800 for a ship and 12,000 for a pod | plate 0.2 deep around the collision shape's perimeter | [Spacewars bodies](material-sources.md#spacewars-bodies) |
| Spacewars | pilot on foot | soft tissue, with toughness 35,000 | the pilot's capsule | [Spacewars bodies](material-sources.md#spacewars-bodies) |
| Spacewars | asteroid, chip and breakup fragment | rock, with toughness 1,750 | what the body weighed when made | [Spacewars bodies](material-sources.md#spacewars-bodies) |
| Spacewars | cannon shell | shell | what the shell weighed when fired | [Spacewars bodies](material-sources.md#spacewars-bodies) |
| Spacewars | sun and planets | rock that nothing wears | a half-space | [Spacewars host](material-contact-charges.md#spacewars-host-330) |
| Scorched and Clock | tank | armor | 3.744: plate 0.2 deep around the rigid hull's 9.36 perimeter, 2 into the screen, on both running gears | [Scorched tanks](material-sources.md#scorched-tanks) |
| Scorched and Clock | terrain | rock that nothing wears | a half-space | [Scorched host](material-contact-charges.md#scorched-host-331) |

Contact charges also take the radius each surface meets others with. A
Spacewars hull's is 1 and a Scorched hull's 0.5, both estimates. A pilot meets
others with its capsule's radius, a rover's bodies with their wheels' 0.75, a
Scorched wheel with its own 0.65, and debris with its own radius.

The weapons:

| Game | Weapon | How it delivers | Details |
| --- | --- | --- | --- |
| Spacewars | cannon shell | a `Weapons` shot that stops on the body its strongest contact pushed and charges that body's region | [cannon shells as shots](material-sources.md#cannon-shells-as-shots) |
| Spacewars | laser | a `Weapons` beam. Each tick's pulse carries 8,950 divided by how far the beam has grown, and a body takes the share its thickness spans of the beam's height of 2 | [lasers as beams](material-sources.md#lasers-as-beams) |
| Scorched and Clock | shell | swept by ray each step. Its hit is a mass of 7 at its speed, and its payload of 28,000 bursts over the tanks within 5 units of their surface | [Scorched tanks](material-sources.md#scorched-tanks), [armor and shells](../scorched-earth.md#armor-and-shells) |

## Adopting it in a host

A host keeps its own bodies, health and rules, and gives the model what it
measures. Spacewars and Scorched are the two worked hosts:
[Spacewars host](material-contact-charges.md#spacewars-host-330) and
[Scorched host](material-contact-charges.md#scorched-host-331).

1. **Regions.** Give each body that can be damaged a region of its material and
   represented volume, kept with the body. A host that stores health can keep
   the region as a view of it, as Spacewars' `HealthMap` does. When a body
   splits, the host divides its region itself:
   [region split](material-region-split.md) divides only regions the world's
   native response keeps, which contact charges can't run beside.
2. **Weapons.** Fly shots and beams through `Weapons` in a physics world, or
   measure your own hits with `projectile_impact` and deliver through the pure
   API, as Scorched does. `Recipients` tells a delivery what it can reach, the
   region behind each body and which recipients the host shields for now. See
   [in the physics world](material-sources.md#in-the-physics-world) and
   [once only](material-sources.md#once-only).
3. **Contacts.**
   - Turn on capture and speculative arrivals, with bounds sized to the scene.
     Once on, capture stays on for the world's life. Each step it visits
     every pair of colliders the narrow phase tracks, touching or not and
     bound or not. A pair without solver contacts costs a visit and no more.
   - Create a capture region with `create_loading_region` for each body whose
     contacts should be priced, and bind its colliders, new and replaced ones
     before each step. Capture records a pair only when both sides are bound,
     so bind both what a contact can harm and what can harm it.
   - Implement `ContactSurfaces`: the surface behind each side, with the
     capacity its region has left; `absorb`, which charges that region; and
     `protected`, which shields a side for now, so that its work is reported
     rather than charged ([host contract](material-contact-charges.md#host-contract)).
   - Call `charge_contact_impacts` once after every step. Only a step's first
     call prices anything. A host that runs a Scorched duel passes its
     surfaces to `HostedDuel::finish_step_among`, which makes that call.
   - `charge_contact_supports` prices steady loads. No game calls it yet.
4. **Scale.** In a world at another scale, bodies that keep their densities
   carry energies that grow with the scale's fourth power. A material whose
   stiffness, strength and toughness grow with the scale and whose density
   shrinks with it, in a volume that grows with the scale's cube, then loses
   the same share of its capacity to the same contact at every scale, as far
   as the law goes. Capture counts patches by the world's length unit, so a
   world set up as Clock's agrees to within a few percent. Scorched's
   hosted duel scales its surfaces this way, and Clock runs it
   ([Scorched host](material-contact-charges.md#scorched-host-331)).
5. **Lifecycle.**
   - Removing an entity, or moving it to another world, retires its capture
     regions, so a body rebuilt in the same entity takes new ones. Price a
     step before removing an entity: an impact whose region was retired after
     the step comes back `Unpriced`, as `Retired`.
   - Pauses and steps of no length price nothing.
   - A snapshot or a clone carries capture's region tokens, bindings and open
     episodes, but not a step's records or the host's regions, which the host
     saves itself. Price each step before saving or cloning.
   - Replay repeats within one build.
6. **Diagnostics.** `loading_diagnostics()` covers the last step, so add up
   its `rejected_batches` after each one, and count the bindings the world
   refuses and the `Unpriced` charges, as both games' hosts do. A rejected
   batch's impacts are missing from the charges, and may start later on
   whatever approach the solver left.
7. **Cost.** `PhysicsStepMetrics::capture_time` is capture's share of a step.
   Time the host's binding and pricing beside it, as both games' hosts do,
   and measure on the Pi. The headless benchmark reports all three as
   `avg_capture_ms`, `avg_contact_bind_ms` and `avg_contact_price_ms`, for
   Spacewars and for Clock's Tanks fights. In Spacewars' Deathmatch, a
   binding walk over every collider each step is half or more of what
   charges cost.

## What is on the model

| Bodies | Weapons | Impacts | Steady loads | Issue |
| --- | --- | --- | --- | --- |
| Spacewars ships | shells and lasers charge the hull | priced and listed; the crash rules set the damage, as a gameplay policy | not priced | #330 |
| Spacewars escape pods and pilots on foot | in a match, shells and lasers charge the hull or the tissue unless ejection protection covers it; outside a match, outside the model | as for ships | not priced | #330 |
| Spacewars rovers | shells and lasers charge the hull | as for ships | not priced | #330 |
| Spacewars asteroids | shells and lasers charge the rock | charged: an asteroid wears by its contacts | not priced | #330 |
| Spacewars chips, breakup fragments and cannon shells | shells and lasers charge their rock or shell | not bound, so not priced | not priced | #330 |
| Spacewars sun and planets | outside the model | rock that nothing wears | not priced | #330 |
| Spacewars wreckage and tanks | outside the model | not bound, so not priced | not priced | tanks: #238 |
| Scorched's tanks | shells charge the armor | charged | not priced | #331 |
| Clock's tanks | as Scorched's, at the arena's scale | charged | not priced | #331 |
| Clock's face, wreck pieces and spacelings | outside the model | not bound, so not priced | not priced | none |
| Terrain | outside the model; craters and chips stay the terrain's own | Scorched's hills are rock that nothing wears; loose terrain and dirt aren't bound | the terrain's own cantilever | stays, #333 and #16 |
| Plant tissue | the tissue adapter, in Material Lab's shell through moss | not in a game yet | not in a game yet | #297, #302 and #311 |
| Wood | not in a game yet | Material Lab's cases | Material Lab's branch weld | #224, #322 and #339 |

A body outside the model takes no weapon damage. A shot or a beam that stops
on it reports its energy as unmodeled. Scorched's bursts reach past its
terrain, which neither takes a share nor shields what lies behind it. No body
has fatigue, penetration, ricochet, heat damage or fracture on failure in the
model. Solar heat stays a heat source outside it, Pizza keeps its own rule
under #59, and plants keep their biological upkeep.
[What it replaces](material-damage.md#what-it-replaces) lists the damage rules
the model replaced or left in place.

## Limits

- **The bounce.** Rapier's restitution sets the bounce, and the ledger's
  recoverable energy isn't applied.
  [Ledger-set bounce](material-ledger-bounce.md) measured what setting it would
  change, and recommends not building it yet.
- **Capture's bounds.** Capture refuses a whole batch on overflow, and the
  batch's impacts are missing from the charges. Each pair the narrow phase
  tracks counts a visit, touching or not and bound or not, and so do each
  manifold and point of a bound pair with solver contacts. Once a scene fills
  capture's room for episodes, each batch that opens more than it ends is
  refused, and the room stays full (#704). Neither game comes near the bounds
  ([game worlds](material-contact-charges.md#game-worlds)).
- **Budgets.** An impact whose budget has more than 32 rows, the ends of its
  patches, or doesn't converge goes unpriced. In 96 of Scorched's duels, 30 of
  the tanks' 3,511 impacts went unpriced, each a sprung tank's whose budget
  didn't converge.
- **Folded impacts.** Capture folds a distinct impact into its pair's episode
  while that episode is still open, such as a hull that lands flat and then
  rocks onto its corners. That impact isn't priced. In Scorched's duels such
  strikes carried 2.5 times the energy of the tank impacts priced
  ([ledger-set bounce](material-ledger-bounce.md)).
- **Single points.** A single point between two flat surfaces isn't priced: it
  has neither a radius nor a span.
- **Joints.** Budgets leave joints out, so a jointed body gives way by its own
  mass and inertia. A sprung tank's wheel lands on its own mass of 2.9, so the
  law sees little of the tank's fall.
- **Seams.** Where the seams of two partitioned bodies meet, supports are
  charged up to 2.3% more (#694). A wheel landing faster than 21 u/s on
  Scorched's terrain, which is many pieces, is priced by where their seams
  fall.
- **Step rate.** A falling body is priced at the speed it ended the step
  before with, so the step rate moves a drop's price a little: a ball dropped
  1.3 at 120 Hz is priced at 5.167, against its free fall's 5.099. A hit
  carried through a support is priced as a load over the whole step, so its
  force depends on the step's length. A beam's damage depends on the size of
  its pulses.
- **Steady loads.** No game prices supports yet. #243, #302 and #311 add them
  where bodies crush. Supports are priced only while steady, awake and outside
  a step that CCD splits, and for their normal load alone.
- **Pushes.** A Scorched burst's push on a tank comes out of the tank's share
  of the payload. Two pushes stay outside the ledger: the burst's push on
  loose dirt and fallen terrain, which is the terrain's own effect, and a
  Spacewars ship's death impulse, a game rule that throws its escape pod
  clear.
- **Weapons.** Deposits carry no momentum, a shot stops at its first hard
  contact without ricochet or penetration, and `Weapons` isn't saved
  ([sources' limits](material-sources.md#limits)).
- **Regions.** Nothing fractures on failure by itself, and the native
  response's regions can't split or move while they are in contact
  ([region split](material-region-split.md#limits)). Welds have no shear or sag
  ([connection loads](material-connection-loads.md#limits)).

## The documents

| Document | What it covers |
| --- | --- |
| [Material damage](material-damage.md) | the law, its units, the ledger and the rules it replaced |
| [The model's report](../data/material-model-v2.md) | the reference materials and acceptance cases, which `reference_cases` regenerates |
| [Shared energy](material-shared-energy.md) | the coupled budget that charges solve |
| [Contact records](material-contact-records.md) | capture, episodes, speculative arrivals and capture's bounds |
| [Contact charges](material-contact-charges.md) | pricing after each step, the host contract and both games' hosts |
| [Weapon sources](material-sources.md) | sources, deliveries, the deposit law and both games' weapons |
| [Region split](material-region-split.md) | dividing a region the native response keeps between bodies |
| [Connection loads](material-connection-loads.md) | welds between bodies, and severing them |
| [Plant tissue](material-tissue.md) | the tissue adapter for plants |
| [Material Lab](material-lab.md) | the lab's cases, in both renderers |
| [Ledger-set bounce](material-ledger-bounce.md) | what setting the bounce from the ledger would change |
| [Contact batch](material-contact-batch.md) | the first integration experiment |

The native material response and its documents, for arrivals, departures,
gravity, compression and supports, describe an opt-in response inside the
solve for bounded scenes. No game uses it: an ordinary step with gravity
refuses it, and contact charges price the games' contacts instead. Each of
those documents covers one bounded slice, and the limits it lists are the
native response's.
