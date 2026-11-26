# Contact charges (#327)

`PhysicsWorld::charge_contact_impacts` prices each new impact that
[contact capture](material-contact-records.md) saw in a step, after the step,
against the surfaces the host keeps. `PhysicsWorld::charge_contact_supports`
then prices the load each steady support carried in the step, as a sustained
load. The host absorbs what each consumed. The physics is unchanged: Rapier's
restitution still sets the bounce, and having the work ledger set the bounce
is a later part of #327. [Ledger-set bounce](material-ledger-bounce.md)
reports how far that can go.

The [native response](material-native-response.md) prices and responds inside
the solve, so it admits only restricted worlds. Charges admit any world:
gravity, damping, joints, sleeping bodies, CCD and any number of bodies are
all allowed, though some impacts go unpriced, as the limits below say. A world
uses either charges or the native response, never both.

## Host contract

1. Enable capture, then speculative arrivals. Charges refuse a capture without
   arrivals with `LoadingError::ArrivalsDisabled`: under the touching rule, an
   impact starts on whatever approach the solver left, which is usually a small
   fraction of the real one.
2. Register regions and bind colliders, as for capture.
3. Implement `ContactSurfaces`:
   - `surface(source)` returns the `ContactSurface` behind a source. It holds
     the region with the capacity it has left, its thickness into the screen,
     how deep it deforms, and the surface's radius of curvature where it meets
     others (`None` is flat). The depth is `Depth::Fixed`, such as a plate's,
     or `Depth::Patch`: as deep as the patch is wide, up to how far the
     material reaches behind the contact. `Depth::Patch(None)` is a
     half-space, such as terrain.
   - `absorb(source, work)` charges the region behind a source.
   - `protected(source)` shields a source for now, as a host might shield a
     ship just after it spawns. A shielded side is priced as any other. Its
     work is reported rather than charged.
4. Call `charge_contact_impacts` after every step, then
   `charge_contact_supports`. The next step drops this step's records, and a
   clone or snapshot carries none, so a step left unpriced stays unpriced. A
   second call after the same step prices nothing.
   - Supports read the impulses the solve left in Rapier's contacts. A
     collider removed before the call takes its supports with it.
5. Watch `loading_diagnostics().rejected_batches`. A batch that capture refused
   is missing from the charges, and its impacts may start later, on whatever
   approach the solver left.

Each new impact is reported once, as a `ContactCharge`:

- `Charged` carries the episode, its sources, the approach that started it,
  the patch and load the law priced, the law's `Outcome` and which sides were
  shielded. The outcome's slots follow the sources. The law on the two
  surfaces as the host kept them just before the charge, across that patch
  with that load, gives the outcome, so a record of the three is enough to
  check it.
- `Unpriced` carries the reason and, once the budget is solved, the impact's
  energy:
  - `Retired`: a side's region was retired after the step.
  - `Undeclared`: the host keeps no surface behind a side.
  - `Budget`: the impact's budget failed. Its rows, the ends of its patches,
    can be more than 32, or it can fail to converge.
  - `Material`: the law refused the impact, such as at a single point
    between two flat surfaces, or a load outside the law's range.

Each support is reported once a step, as a `SupportCharge`:

- `Charged` carries the episode, its sources, the force, the patch width, the
  duration, each side's own `Outcome` and which sides were shielded.
- `Unpriced` carries the force and the reason, `Undeclared` or `Material`.

## Impacts

Each CCD substep's batch is priced in order, one new `Impact` episode at a
time:

1. **Rows.** An impact's rows are the points of its episode that the solve
   acts on, those touching or arriving, whose approach exceeds
   `min_approach_speed`. Capture uses the same acting rule to start the
   episode.
2. **Patches.** An impact's rows that push along one normal, to within about
   2.6 degrees, form a patch.
   - For rigid bodies, the points between a patch's two ends add no way to
     push, so only the ends enter the budget.
   - Ends closer than a thousandth of the world's length unit are one point.
   - A face across many colliders, such as a hull on terrain made of chunks,
     is one patch of two points, so its budget stays within 32 points.
   - Each piece of a patch, the colliders or compound children that meet
     there, is convex, so its points cover one interval of the patch. Pieces
     closer than a thousandth of the length unit join, and the gaps between the
     rest cover nothing.
3. **Motion.** Each body's motion is rebuilt from its records:
   - center of mass;
   - spin;
   - effective inverse mass and inertia;
   - center velocity, which is the point velocity less the spin's share at the
     point.
4. **Budget.** The new impacts of the substep that share a dynamic body form
   one budget.
   - `shared_impact::allocate` solves the coupled compression over their rows,
     at unit stiffness, and each impact takes its own rows' share of the energy.
   - A body that strikes two surfaces at once therefore spends its motion once.
   - Only a patch's ends are rows, so splitting a body or a surface into more
     colliders changes neither the energy a body spends nor how it splits
     between impacts.
   - Fixed and kinematic bodies give no way and couple nothing, and a moving
     one brings its own work.
5. **Law.** `impact()` prices the impact's energy across what its points
   cover, summed over its patches. Each piece's interval is at least as wide
   as `patch_width_from_radii` gives for the two radii, and a patch covers the
   union of its pieces' intervals.
   - A face landing flat is priced across the face, not at a corner.
   - A face across a gap is priced across what it meets on either side.
   - Two flat surfaces are priced across their overlap.
   - A `Depth::Patch` surface is as deep as that width, up to its extent.
   - Its fastest approach carries the energy, and the law depends only on the
     energy.
   - Each unshielded side with consumed work is charged.

Impacts are priced in turn, and each reads the surfaces after the charges of
the one before it.

## Supports

`charge_contact_supports` prices each steady support in the step, in turn,
after the impacts' charges:

1. **Supports.** A support is an open episode whose pair the step's solve
   acted on and loaded, and whose points held steady: each point the solve
   acted on moved against the other surface no faster than a tenth of the
   length unit a second, along the normal or across it. Other steps are not
   a support's:
   - A step that starts the episode with an impact is the impact's.
   - A step in which a point moves faster along the normal is a hit's or a
     bounce's. Capture folds a hit into the open episode, and nothing prices
     it. Separating points carry an impulse only when something in the step
     drives them back together, such as gravity, a joint or another contact.
     That is an impact within the step, which capture cannot see: it records
     the motion before the solve.
   - A step in which a point slides faster is a grind's. Its load moves
     across both surfaces rather than resting on one patch.
   - A step in which the solve stops a point that was apart, such as a corner
     that gravity lands within the step, is a landing's.
   - Rapier solves a pair when it has a dynamic body and each of its dynamic
     bodies is awake. A pair it did not solve, such as one whose body sleeps,
     keeps an earlier solve's impulses.
   - A step that CCD splits, or with a batch that capture refused, prices no
     supports.
2. **Force.** The force is the solve's own: the normal impulse the step's
   solve applied to the support's points, over the step.
   - Rapier reports each point's impulse with the warm start counted in. Its
     solve begins from the previous solve's last impulse and adds that to the
     total it reports. The report less the warm start is what this solve
     applied. At four solver iterations, a resting box reports a quarter more
     than its weight, and its support carries exactly its weight.
   - Joints and other contacts reach a support through that impulse. A
     chassis welded onto a foot loads the foot's support with both bodies'
     weight.
3. **Width.** As for an impact, the support is priced across what its pieces
   cover, summed over its patches. A face resting on several points is
   indeterminate, and the solve can load any of them, so each piece it loaded
   covers all its points within a thousandth of the length unit of touching.
   Points apart, such as speculative points beside a wall, cover nothing. A
   `Depth::Patch` side is as deep as that width, up to its extent.
4. **Law.** `sustained()` prices each side over the whole step at that force
   and width, with no measured plastic displacement.
   - A load whose stress, its force over the patch's width times the side's
     thickness, is at or below the side's effective strength costs nothing
     however long it rests.
   - An overload follows the law's fallback progression. Work grows with the
     overstress and the time, at `CRUSH_TIME_CONSTANT` (0.25 s), up to the
     region's remaining capacity.
   - Each unshielded side with consumed work is charged.

## Limits

- Capture folds a distinct impact into an episode that is still open, such as
  a hull that lands flat and then rocks onto its corners. That impact is
  priced neither as an impact nor as a support.
- Only steady supports are priced. A support is not priced in a step a point
  of it moves faster than a tenth of the length unit a second, such as a hull
  rocking on a corner or bodies grinding past each other, nor in a step it
  lands in. A load within its side's strength costs nothing either way, but
  an overload is under-charged by the steps skipped.
- A sleeping support is not priced. A host that wants a resting load priced
  keeps the body awake, with `can_sleep: false`.
- A step that CCD splits prices no supports. Rapier splits the whole world's
  step at a CCD body's impact, so every support in the world misses that
  step. The last substep's impulse could carry a hit delivered in a small
  part of the step.
- A hit carried through a support is priced as a load over the whole step,
  so its force depends on the step's length. At 120 Hz the same hit loads
  the support twice as hard, for half as long. The figures below are at
  Scorched's 60 Hz.
- Supports are priced for their normal load alone, not for friction.
- Where a seam between one side's pieces lies on a seam between the other's,
  each piece there meets the piece across the seam at a corner as well as
  along its face. Once a body drifts a hair across the seam, Rapier pushes
  those corners apart sideways, and the support counts the pushes as load,
  though they cancel across the seam. A box of four pieces resting on a floor
  of two or four, their middle seams together, drifted under a millionth
  across it and was charged up to 2.3% more work over its first second than a
  box of one, and up to 0.5% more over five. A box of two on the same floors
  stayed on the seam and was charged alike, as are seams apart. #694 tracks a
  fix.
- No plastic displacement is measured, so an overload follows the law's
  fallback progression. The solve still holds the body up, so a crushed
  support does not give way.
- A budget holds only the substep's new impacts. A contact that carries on from
  an earlier substep, such as a support under a body, is not in it, so a
  supported body gives way as if it were free. This under-prices a hit that
  drives a body into its support. The support carries what the hit drives
  through it, and is priced for that as a sustained load.
- Joints are not in the budget: a jointed body gives way by its own mass and
  inertia alone.
- A piece is as wide as its points span, as if the whole face met at once.
- A polyline is one piece, so a patch on one spans the gaps between its
  segments. Neither game's colliders are polylines.
- A single point between two flat surfaces is not priced: it has neither a
  radius nor a span.
- A budget whose rows are nearly dependent can still fail to converge. For
  example, a grain wedged against terrain at five points on three normals is
  singular for the solver. The solver's restarted pass, in
  [shared energy](material-shared-energy.md), converges on some budgets that
  are only ill-conditioned, but not all of them.
- A batch that capture refuses is not priced.
- Records come before the substep's gravity, so a falling body's approach
  lacks one substep of the speed the solve stops. Without CCD, capture
  first sees a falling body in the step after the one that carries it into
  its support, or to within Rapier's prediction distance of it, at the
  speed it ended that step with. That is within a step of gravity of its
  free fall's, so the step rate moves a drop's price. A ball dropped 1.3 at
  120 Hz is priced at 5.167, against the 5.099 of its free fall.
- Rapier's restitution sets the bounce. The ledger's recoverable energy is not
  applied. [Ledger-set bounce](material-ledger-bounce.md) measures what
  setting it would change.
- Capture's bounds still refuse a whole batch on overflow. Each pair the
  narrow phase tracks counts toward its visits, touching or not and bound or
  not. Neither game comes near them, as the next section shows.
- A refused batch ends none of the episodes it would have ended. Once a scene
  fills capture's room for episodes, each batch that opens more episodes than
  it ends is refused, and pairs that have parted keep their room while the
  scene goes on meeting new ones. With room for 48, the busy scene under
  Evidence had 984 of its 1,200 steps refused.

## Game worlds

Two temporary probes measured capture and charges on the games, with every
collider bound to a region for its body. Their sources and outputs are on the
owner's workstation in
`/home/data2/space-wars3-artifacts/contact-charges-probe-20261122`. Their
rerun on the final code, in
`/home/data2/space-wars3-artifacts/support-loads-review-probe-20261122`, gave
the Scorched figures with patches and the severe contacts. The probes came
before the games' hosts below, which bind fewer bodies and set their own
surfaces. Scorched's host gives a hull a radius of 0.5 where the probe gave
0.2, and binds no loose dirt.

**Scorched.**
- **Setup:** #331's contact probe from 2026-11-15 was rerun with charges beside
  its own rule. It covered the scripted contacts and the 48 CPU duels.
- **Surfaces:**
  - A tank's hull and wheels are its armor, with a fixed plate depth. Hull
    corners have radius 0.2, and wheels 0.65.
  - Terrain and loose dirt are rock as a half-space.
  - Wood is wood, as deep as the patch is wide, up to its thickness.
- **Capture:** the largest batch was 1,621 visits and 483 points, against
  bounds of 16,384 and 2,048. No batch was refused.
- **Before patches:** every impact was priced at a corner.
  - Rigid tanks paid 37.9 hp in all over 32 tank-matches, about 3.5 times the
    old rule's 10.7.
  - Settling from spawn cost 3.4 hp.
  - Drops of 10 units or more overflowed the budget's 32 points.
  - 58 tank impacts failed to converge.
- **With patches:**
  - Rigid tanks paid 0.57 hp in all, 0.33 in the worst tank-match. Sprung
    tanks paid nothing.
  - Settling and resting cost nothing. Driving the courses cost at most
    0.04 hp, on Slope.
  - No budget overflowed, and 5 tank impacts in the duels failed to converge.
  - Charges took 5 µs a step at the median in the duels, adding 1.4% to the
    scenario's step.
- **Severe contacts:**
  - A rigid tank dropped flat from 30 units costs nothing. Across its face 3
    wide, the rock takes the work. The old rule charged 36.6 hp, from corner
    contacts as the hull rocked, which charges fold into the landing's open
    episode.
  - A sprung tank dropped from 30 units costs 2.6 hp.
  - A rigid tank thrown into the other at 8 u/s costs each 1.2 hp.
  - Whether a severe flat drop should cost was a choice for #331, between a
    dirt material, a rigid floor and pricing re-impacts. #331 kept the rock,
    as [its host](#scorched-host-331) says.

**Spacewars.** 300 seconds of Spacewars at the asteroid census's settings:
radius 1200, 20 asteroids a second, and each player's health at 500, so 7
games ran.
- The world held at most 406 bodies and 416 colliders.
- The largest batch was 492 visits and 20 points, and no batch was refused.
- Charges priced all 21,317 impacts. Under #683, before patches and the
  solver's restarted pass, 4 failed to converge.
- Other jobs were loading the workstation, and one repeat ran a third slow
  throughout. In the steadier repeats capture cost 1 to 4% of a 1.2 ms step,
  and charges cost 3 to 6 µs a step. Each host's section below gives
  capture's cost on the Pi.

**Supports.** The same probes then priced supports. The first draft's records
are in `/home/data2/space-wars3-artifacts/support-loads-probe-20261122`, and
those of the rule above in
`/home/data2/space-wars3-artifacts/support-loads-review-probe-20261122`.
- **Tanks:** no tank side yielded, in the scripted contacts or the 48 duels.
  The highest stress on a tank side was 0.85 of its armor's effective
  strength, when a sprung tank was thrown into the other at 8 u/s. In the
  duels it was 0.36.
- **Hits through a support:** a trunk of mass 3.6 dropped from 20 units onto
  a resting rigid tank loads the tank's support with 6,600. That is the
  tank's weight, about 700, and the trunk's momentum stopped within the
  step. A rigid tank thrown into the other at 8, 12 and 16 u/s loads a
  hull's support on the ground with up to 6,800, 18,600 and 23,200. A sprung
  tank's suspension spreads the trunk's blow, and its wheels' supports carry
  at most 1,200.
- **Landings:** which steps of a landing are steady varies. A sprung tank
  dropped from 5 units loads a wheel's support with 15,800 in one step, but
  dropped from 10 or 20 units its supports carry at most 370. From 30 units
  its hull reaches the ground, and that support carries 6,900.
- **Steady steps:** 93% of the tanks' loaded support steps were steady in
  the scripted contacts, and 91% in the duels. Of the duels' other steps,
  54% slid faster than 0.1 u/s, 36% moved along the normal faster than that,
  6.5% started an impact and 2.8% landed. CCD split one step.
- **Rock:** rock sides yielded 1,286 times over the duels. Each of the 60
  the probe printed was a grain on the terrain or on another grain, carrying
  29 to 2,400 across a span of 0.0015 to 0.07. Grains are declared flat, so
  a grain resting on two close points is priced across their span.
- **Flat on flat:** two flat surfaces meeting at one touching point are
  refused as `Material(InvalidGeometry)`, as impacts are. That was 4.6
  million of the duels' 6.6 million supports, none of them on a tank.
- **Spacewars:** no support in 300 seconds. With no gravity nothing rests.
  Every loaded pair in an open episode was moving, sliding, landing or
  starting an impact, or in one of the 1,866 steps of 18,000 that CCD split.
  Spacewars' steady limit is 1 u/s, a tenth of its length unit. Two drafts
  priced supports there:
  - One that also priced separating points priced 27 supports, of which 24
    yielded, at up to 4.4 million force. Each was a deep overlap being
    pushed apart on one point.
  - One that judged steadiness by motion along the normal alone priced 78
    pair-steps in which bodies slid past each other at 1.1 to 295 u/s, and 3
    of them yielded.
- **Cost:** other jobs loaded the workstation, at a load average of 8 to 16
  on 16 cores. In Scorched's duels supports took 43 µs a step at the median,
  adding 10 to 13% to the scenario's step, against 1.4% for the impacts.
  Most of those supports touch no tank, and a host prices only pairs whose
  colliders it has bound. In Spacewars supports took 3 µs of a 1.3 ms step,
  against 6 µs for the impacts. Their cost also needs measuring on the Pi
  before adoption.

## Spacewars host (#330)

`scenarios/spacewars/src/contact_charges.rs` is Spacewars' host, and every
game world has it on. It lists each step's impacts, as priced, in
`SpacewarsState::contact_hits`, and it charges asteroids for theirs. Every
other body keeps today's contact damage: what charges price for ships, pods,
pilots and rovers is listed and not charged. That is a gameplay policy, and
the probe below gives its reasons.

- **Regions.** Each body a crash can harm, or that a crash strikes, has a
  region: a ship or a pod, a pilot on foot, each of a rover's three bodies, an
  asteroid, the sun and each planet. Every collider of such a body is bound to
  its region, sensors included, which never touch. A landing foot is a patch
  of its own. Shells, chips, breakup fragments, wreckage, tanks, loose terrain
  and the universe's boundary have no region, so none of their contacts is
  priced.
- **Binding.** Before each step the host binds each unbound collider of such
  a body: a new body's, or one whose colliders were replaced. A sortie's wing
  sweep replaces a ship's colliders, and its region outlives them. Removing a
  body retires its region. The game rebuilds a body for most changes of
  shape, such as a ship's wing sweep outside a sortie, docking, a turn into a
  pod and an asteroid's chip, so the rebuilt body has a new region and its
  contacts start anew. Its impacts with a body it was already touching, as
  the contact rules recall it, go on with that contact until those rules see
  the two part, so the host lists none, as those rules count none.
  The host walks every collider each step, and asks whether charges price
  its body before it looks up its binding. In Deathmatch the walk covers
  about 1,250 colliders and takes about 50 µs a step on the workstation,
  nearly all of binding's time.
- **Surfaces.** A body with health is its health map's region, as for
  [weapon sources](material-sources.md#spacewars-bodies), at a fixed depth:
  the health it brought into the step, less what the step's earlier impacts
  consumed. A hull meets others with a radius of 1, an estimate. A pilot meets
  them with the capsule's radius, each of a rover's bodies with its wheels'
  0.75, and debris with its own radius. The sun and the planets are rock as a
  half-space, with their own radius, holding as much as a region can so that
  nothing wears them.
- **Shielding.** The sun and the planets are always shielded. So are a pod
  and a pilot outside a match, priced at full health since only a match
  harms them, and a pod or a pilot under ejection protection.
- **Supports** go unpriced. The host never calls `charge_contact_supports`,
  so the steady load of a ship, pod, pilot or rover resting on a planet costs
  nothing, and only impacts are listed.
- **Ejection.** The hit that kills a ship throws its escape pod at the hit's
  damage as a speed along it. That kick is a game rule outside the ledger: a
  weapon's source charged the hull only for what it delivered, and the pod's
  crashes keep today's rules.
- **Asteroid wear.** An asteroid loses the health its impacts consumed, and
  today's contact rules no longer wear it. It chips and breaks by its health
  as before, but what a strike costs it has changed. Today's rules took no
  more health from a big asteroid than from a small one struck as fast, so a
  big one survived faster strikes; charges break asteroids of every size
  from about the same speed. Against another of its size, one strike breaks
  an asteroid of radius 2, 5 or 10 from 40 u/s, where today's rules need 80,
  200 and 240 u/s. A radius-5 asteroid striking a ship breaks from 40 u/s,
  where it needed 200, and against a planet both rules break it from 40 u/s.
  In play, about as many asteroids break as today, since most strikes are
  fast and charges no longer wear asteroids in lasting contact.
- **When wear lands.** The wear lands just after today's rules. They size a
  ship's or a rover's damage by the asteroid's mass as the step's physics had
  it, so a laser that breaks the asteroid earlier in the step doesn't change
  what its strike costs (#714). They still charge a ship or a rover for each
  contact, but an asteroid that now breaks on its first strike can't strike
  again. Dropped on a parked rover at 80 u/s, a radius-5 asteroid cost it
  6.06 in two strikes under today's rules and 1.20 in one under charges.
- **What wears.** Only an impact wears an asteroid: one left pressing on a
  planet, which today's rules wore by each step's impulse, wears no more
  once it has struck. Its chips fly from the side away from what struck it,
  as they did from every strike but another asteroid's. A pilot on foot
  leaves it unworn, as before. What a pilot's impact prices on the asteroid
  still counts against it for the step's later impacts, but that is nothing
  up to 200 u/s and about a thousandth of a radius-2 asteroid's health at
  300 u/s. A shot still charges it, and an impact charges leave unpriced
  wears nothing.
- **Capture's bounds** are 4,096 regions, 16,384 bindings, 4,096 episodes,
  65,536 visits and 8,192 contacts a batch, and 32,768 records a step, to
  hold `MAX_ASTEROIDS` asteroids beside the rest. In 15 seconds at the
  census's settings, the host's test finds no batch or binding refused and
  every impact priced. With room for only two contacts open at once, the same
  game refuses about 250 batches and still prices over 100 impacts.
- **Diagnostics.** `contact_diagnostics()` counts the batches and bindings
  the world refused and the impacts left unpriced. Forty small asteroids that
  strike a large one in the same step have more points than a budget takes,
  so all forty go unpriced and wear nothing.
- **Cost on the Pi.** #332 timed charges on the Pi 4 and the Pi 5 beside
  their running kiosks, with seed 7, no input, and both ships at 500% healed
  before each step. The records are in
  `/home/data2/space-wars3-artifacts/material-cost-332-20261125`.
  - On the Pi 4, in Deathmatch, charges take 0.68 ms of a 22 ms step in
    release, 3%: binding 0.38, capture 0.16, pricing 0.10 and the asteroids'
    wear 0.04. In the default universe they take 0.19 ms of 9.3 ms, 2%.
  - On the Pi 5 they take 0.20 ms of a 6.0 ms step in Deathmatch, 3%:
    binding 0.13, capture 0.03, pricing 0.03 and wear 0.01. In the default
    universe they take 0.04 ms of 2.8 ms, 1.5%.
  - In the tests' `ci` build, which the 2026-11-24 baseline used, the Pi 4's
    Deathmatch charges take 0.89 ms of 28 ms, 3%. On 2026-11-24 they added
    2.7 ms to a 22.6 ms step, 12%, and 1.8 ms of it was in the physics step,
    where capture runs. Capture now passes over pairs without solver
    contacts. Every step was slower than on 2026-11-24, with charges or
    without, so compare the shares. The Pi 5 has no earlier figures.
  - Capture visits about 2,800 pairs a step in Deathmatch. About 40 touch,
    and about one has both sides bound.
  - With charges on, Deathmatch keeps 8% fewer debris. On the Pi 4 its whole
    step was 1% slower with charges in one build and 6% faster in the other,
    while repeats of one build beside the kiosk varied by 3 to 7%. On the
    Pi 5 it was 0.3 to 1.1% faster in both.

The contact-half probe of 2026-11-23 bound the same bodies, with loose terrain,
the boundary and all rock debris besides, and set charges beside today's rules
in the census, the AI games and the matches, on the workstation and the Pi.
Its records are in
`/home/data2/space-wars3-artifacts/contact-half-probe-20261123`.

- On rock planets, asteroids in play wear as they do today. Every asteroid
  that hits a planet or the sun breaks under both. One that hits a ship
  breaks 97% of the time today and 98 to 100% under charges.
- Ships take 30 to 130 times today's damage from planets and the sun, and 7
  to 40 times from asteroids. A ship that hits a rock planet at 100 u/s
  fails, where today it loses 1. A radius-2 asteroid at 100 u/s kills a
  rover, where today it costs 3.5 of 25.
- Pods and pilots take far less than today's match rule. A pod loses 18 at
  100 u/s, where today it dies from 62 u/s. A pilot at 37 u/s, where today
  it dies, would lose about 6 even if it took the crash's whole energy.
- No planet material suits both ships and asteroids. Charges grow with the
  square of the speed, where today's rules grow linearly, so one factor for
  each class would match today's rules at only one speed.
- Capture added 2 to 17% to a step on the workstation and up to 15% on the
  Pi, and charges 0.03 to 0.5 ms a step on the Pi. With everything bound,
  Deathmatch peaked at 1,577 regions.

So ships, pods, pilots and rovers keep today's contact rules, as a gameplay
policy rather than a weaker material. Their materials hold weapon fire as
#330 calibrated them, and the same materials can't also give today's crashes.
Charging those crashes would take new materials, which would move weapon
balance too, or a factor for each class.

## Scorched host (#331)

`scenarios/scorched-earth/src/armor/contacts.rs` is Scorched's host. A
standalone duel's world has it on, and a hosted duel's once its host calls
`host::capture_contacts`. It charges each tank's armor with the work its
impacts consumed, and lists them in the duel's `contacts()`.
[Scorched's own page](../scorched-earth.md#armor-and-contacts) has what
contacts cost in play.

- **Regions.** Each tank body has a region: a rigid hull, or a sprung chassis
  and each of its wheels. Each terrain body has one, attached or fallen, and
  all its chunks are bound to it as one patch. Loose dirt has none, and
  neither has anything of a host's that the host has not bound.
- **Binding.** Before each step the duel binds each unbound collider of a
  tank or of the terrain: a new body's, or the chunks a crater replaced.
  Removing a body retires its region, as retiring a tank does.
- **Surfaces.**
  - A tank's bodies are its armor region, as for
    [weapon sources](material-sources.md#scorched-tanks), at the plate's
    depth of 0.2, with what the step's earlier impacts consumed already
    absorbed.
  - A hull meets others with a radius of 0.5, an estimate, about that of the
    rigid hull's chamfered ends. A wheel meets them with its own radius.
  - Terrain is rock as a half-space, flat and always shielded, holding as
    much as a region can so that nothing wears it.
- **Scale.** A hosted duel's world is the placement's scale times Scorched's
  size. Its bodies keep their densities, so masses grow with the square of
  the scale, speeds with it and energies with its fourth power. A material
  whose stiffness, strength and toughness grow with the scale then loses the
  same share of its capacity to the same contact at every scale. The duel's
  surfaces are such materials, in volumes times the scale's cube. A tank
  thrown into the other costs each the same health at scales 1 and 3, to
  within a thousandth, and within a few percent in a world set up as Clock's
  is. A charge that takes all the scaled armor has left takes all the armor
  has left, so a tank the law fails is dead at any scale.
- **A host's surfaces.** The host binds its own colliders and declares their
  surfaces through `HostedDuel::finish_step_among`, in world units. The duel
  sees each at its own scale, as it does the host's
  [recipients](material-sources.md#scorched-tanks): its material at that
  scale, in the volume the host gave, with the same share of its capacity
  left. Work charged to it goes back by the ratio of the capacities, so it
  loses the share the same contact would cost it at scale 1. The duel makes
  the step's one call to the charges, so impacts between two of the host's
  surfaces are priced there too, at the duel's scale. The host learns of its
  charges through `absorb` alone.
- **Supports** go unpriced, as in Spacewars. In the probes above no tank side
  yielded under one. A tank resting on a host's surface is not priced either,
  which #311's plants will need.
- **Wheels on terrain.** A wheel's patch on flat rock is as wide as its
  radius gives, and a wheel of mass 2.9 lands on it for nothing up to 21 u/s.
  The terrain is many pieces, though, and each piece whose point pushes along
  its own normal counts as a patch. A faster wheel that lands near a seam, or
  fast enough for the next piece's point to arrive within the step, is priced
  across two patches or more, and the rock takes four times the energy
  before the plate yields. So a hard landing's price goes by where and how
  fast the wheel comes down: a tank dropped from 15 units lost 0, 0.2 or 0.3
  health, and from 20 and 30 units nothing. Rapier's prediction distance
  follows the world's length unit, so a hosted world can count those patches
  differently from a standalone one.
- **Bursts.** A shell's burst pushes each tank it reaches, and the push's
  energy in the tank's own frame comes out of the tank's share of the
  payload. The armor takes the rest, and what the pushed tank then strikes is
  priced from motion the payload paid for. A share too small for the whole
  push moves the tank as far as it pays for. The impact event lists each
  push as `pushed`. The burst's push on loose dirt and fallen terrain is the
  terrain's own effect, outside the ledger.
- **Clones.** With capture on, a world clones through a snapshot of itself. A
  duel's clone goes on tick for tick as the duel does.
- **Loose dirt** is left out for its cost. A grain's mass is 0.25, and as rock
  it would have to strike a hull at 56 u/s to cost it anything, where a burst
  throws dirt at 16 u/s at most. With every grain bound, capture recorded
  each grain's contacts with the terrain and its neighbors and added about
  15% to a duel's step on the workstation. With tanks and terrain alone,
  capture, binding and pricing add 1 to 5% there.
- **Cost on the Pi.** #332 timed the duels and Clock's Tanks fights on the
  Pi 4 and the Pi 5 beside their running kiosks. The records are in
  `/home/data2/space-wars3-artifacts/material-cost-332-20261125`.
  - On the Pi 4 a duel's step of 2.4 to 3.3 ms gains 0.09 to 0.10 ms, 3 to
    4%: capture 68 to 85 µs, binding 10 to 13 and pricing 4 to 8, as for
    #331. The host prices only the steps that start an impact. On the Pi 5
    a step of 0.68 to 0.97 ms gains 17 to 19 µs, 1.8 to 2.6%: capture 12 to
    16 µs, binding 2 to 3 and pricing under 2.
  - Capture visits 135 to 174 pairs a step, and passing over those without
    solver contacts saved the duels nothing measurable. On the workstation
    three quarters of capture's time is its loop over the pairs, where each
    of the 61 to 77 that touch has both colliders looked up. Only 2 or 3 of
    those are bound: the rest have a collider without a region, such as a
    loose grain.
  - In Clock's Tanks fights on the Pi 4, capture and charges add 71 to 89 µs
    to the arena's step over a whole fight, 3.9 to 4.4%: capture 52 to
    64 µs, binding 12 to 16 and pricing 3 to 9. For #331 they added 79 to
    106 µs, 4.8 to 5.5%, with capture 56 to 76. On the Pi 5 they add 12 to
    16 µs, 2.8 to 3.2%, with capture 8 to 11.
- **Capture's bounds** are the defaults with room for 8,192 bindings, 4,096
  episodes, 65,536 visits and 8,192 contacts a batch, and 32,768 records a
  step. A world that already captures keeps its own. In 96 CPU duels no batch
  or binding was refused, and 30 of the 3,524 tank impacts were unpriced, each
  a sprung tank's whose budget did not converge
  ([qualification](material-qualification.md#scorched-earth)). In a world with room for
  only two contacts open at once, a knock between the tanks loses one of its
  two blows to a refused batch, and the duel counts every batch refused.

#331's probe of 2026-11-24 priced every tank impact against soil, rock,
packed earth and a floor that never gives, on both running gears, in 96 duels
and in 48 of Clock's Tanks fights. Its records are in
`/home/data2/space-wars3-artifacts/contact-probe-331-20261124`.

- A ground strong enough to make falls cost charges routine bumps more. Packed
  earth, as strong as the armor, cost rigid tanks 16 health over 24 duels and
  a 10-unit drop 0.7. The floor cost that drop 18.5, and settling from spawn
  3.9.
- On rock, sprung tanks paid nothing in any duel or fight. Rigid tanks paid
  1.3 health over 24 duels with a hull corner of radius 0.2 and 0.1 with 0.5.
- A sprung tank meets the ground on its wheels alone, a sixth of its mass.
  Its springs take up to a 2-unit drop and bottom out from 5, so the contact
  law sees little of a fall. Pricing falls would take a rule of its own on
  the hull's whole mass. None is in place.

So the ground is rock, the sprung gear is the reference and the rigid hull's
corner is 0.5.

## Evidence

`crates/engine-rapier/src/world/loading/charges/tests.rs` covers:

- **Head-on:** a head-on hit between balls is priced at half the reduced mass
  times the approach squared, gives the law's outcome, and is charged to both
  sides.
- **Spin:** the end of a spinning bar strikes a floor at about a quarter of the
  bar's mass, as its lever arm gives.
- **Arriving points only:** a tilted box whose substep closes only its lower
  corner turns about that corner, rather than stopping flat as both corners
  would stop it.
- **Fixed and kinematic bodies:** a CCD ball at 400 against a fixed wall is
  priced on the ball's whole energy, and a kinematic slab pushing a resting
  one on the resting slab's mass at their closing speed. The slabs are priced
  across the faces they meet on.
- **CCD substeps:** a CCD ball bouncing between two walls strikes each in its
  own substep of one step. The strikes are priced in the substeps' order,
  each on half the ball's mass times its approach squared.
- **Partitions:** one, two and four collider partitions, inserted in either
  order, give the same charges.
- **Two floors:** a box landing flat across two floors spends its motion once;
  its two impacts sum to half its mass times the approach squared.
- **Flat faces:** a hull landing flat on a half-space is priced across its face
  3 wide, whether it declares a corner's radius or none.
- **Many colliders:** the same landing, spinning, on a floor of 4 pieces and
  of 80 pieces, 60 points, is priced as on one piece.
- **Gaps:** the same landing over a gap 1 wide is priced across the 2 it
  meets.
- **Split bodies:** the two-floors box, split in two, takes the same shares.
- **Patches:** a patch keeps its two ends. Normals 2 degrees apart push along
  one patch, and 3 degrees apart along two. Pieces cover their own intervals:
  abutting ones join, a gap covers nothing, and between flat surfaces a point
  covers nothing. Ends closer than a thousandth of the length unit are one
  point, at a length unit of 1 and of 100.
- **Depth:** a `Depth::Patch` surface is as deep as the patch is wide, up to
  its extent.
- **Budget limit:** a bar landing flat on 16 posts, 32 points, is priced. On
  17 posts, 34 points, each of its impacts is unpriced as `Budget(Limit)`.
- **Surface order:** two balls striking one floor in a substep are priced in
  turn, so the floor absorbs what it held and no more.
- **Host rules:** each step is priced once, through pauses and continuing
  contact. Shielded, undeclared, retired and flat-on-flat impacts are reported
  as described above.
- **Small shares:** a share too small for the law costs nothing.
- **Mixed world:** with gravity, damping, CCD and a joint elsewhere, each bounce
  is priced at half the ball's mass times its approach squared, and motion is
  identical with and without charges.
- **Arrivals:** charges need a capture that admits speculative arrivals.

`crates/engine-rapier/src/world/loading/charges/tests/supports.rs` covers
supports:

- **Resting and crushing:** a light box on rock costs nothing. A heavy one on
  soft tissue, a half-space, is charged what `sustained()` gives until the
  tissue fails, and its charges sum to the tissue's capacity. Each step's
  force is the box's weight.
- **CCD:** a step that a CCD ball's hit elsewhere splits prices no supports,
  and the steps around it price a resting box at its weight.
- **Joints:** a chassis welded onto a foot loads the foot's support with both
  bodies' weight, and motion is identical with and without supports.
- **Landings:** a box dropped from 1, or 0.0001 above the floor at 0.05, is
  priced as an impact when it lands and as a support once it rests. A tilted
  box that lands on one corner and rocks onto the other is not priced as a
  support while its corners approach. A resting box lifted 0.0019 and dropped
  at 0.2 lands within the step, and that step is not priced.
- **Steady points:** a resting box set moving along the floor's normal at
  0.05, rising or pressed in, is priced at what the step applied. At 0.5, or
  at 0.05 with a length unit of 0.1, it is not, although the floor stops it.
  A box sliding along the floor at 0.05 is priced at its weight, and one
  grinding along at 0.5 is not. A box touching the floor without gravity
  carries nothing and is not reported.
- **Width:** a box sliding slowly toward a wall is priced across its face 2
  wide. A ball rolling away from a wall at 0.15 holds its floor point still
  and is priced across its own patch, although its point by the wall moves
  at 0.21: the solve doesn't act on it. The wall's speculative points carry
  nothing, so they neither widen the patch nor unsettle the support. A box
  tilted onto one corner, its other one 0.0019 off the floor, is priced
  across the corner's patch. A box resting across a gap 1 wide, in a floor of
  two colliders, carries its whole weight across the 1 it rests on. A wheel
  is priced across the patch its radius gives.
- **Host rules:** each step is priced once, and a shielded side is priced but
  not charged. Undeclared and flat-on-flat supports are reported with their
  force. Nothing is priced for a sleeping support, a body removed after the
  step, a step with a refused batch, or a world restored since its last step.
  A box resting on a body that the host froze asleep as fixed is priced, as
  Rapier solves their pair.
- **Arrivals:** supports need a capture that admits speculative arrivals.

`crates/engine-rapier/src/world/loading/charges/tests/reference.rs` runs
#326's reference cases, as [`material-model-v2.md`](../data/material-model-v2.md)
publishes them, on real physics. Each case's bodies, from
`engine_material::fixtures`, are built with their masses, inertias and
surfaces and struck or pressed as the case does it. Each charge's energy, peak
force and stresses, consumed work, yields and failures are the case's, to
within rounding, and the host absorbs what it consumed:

- **Safe bumps:** five pairs bump a thousand times each at half their
  safe-limit speed. Each bump is its own impact, and none costs anything.
- **Single strikes:** gentle and harder landings, wood against armor, a stone
  on tissue and on wood, two hulls of one material, and a shell with more
  energy than the plate can take.
- **Progressive hits:** fresh shells wear a plate down hit by hit, and it
  fails at the 18th, as in the case. A plate of twice the volume takes 36
  hits. A strike that dents the plate dents nothing when it is twice as
  thick.
- **Glancing:** a strike at 30, 60 and 80 degrees is priced on its normal
  approach.
- **Angular:** a strike 1 to 3 off the boulder's center of mass is priced on
  the mass its lever arm leaves, and turns the boulder.
- **Partition:** a shell striking a seam, where two of the plate's colliders
  meet it at one point, is priced once, as one contact.
- **Resting support:** the tank's weight rests on a wood plank for a thousand
  ticks at no cost. The press of 1.5 times the plank's safe stress fails it on
  the case's tick, and the press of 1.05 times a tick after the case's.
- **Fixed step:** at 30, 60, 120 and 240 Hz, the press of 1.05 times consumes
  the same work by 1/30 s. It fails the plank a tick after the case's 0.2 s at
  every rate.

The press of 1.05 times is a tick late because the solver's force is one f32
step under 6,300, a part in ten million. Fallback work goes with the
overstress, 150 of the 3,150 stress, so each tick does 1.7 parts in a million
less work than the case's. The case's ticks overrun the plank by 2.3e-5, and
the solver's leave 3.9e-4 of it, which one more tick takes.

The connection cases, `beam_connections` and `fixed_step`'s moments, load
welds, not contacts, so they are not run here. Nor are `failure_allocation`'s
cases, which are a host's choices of where a failure's excess goes, or
`resting_support`'s press with a measured plastic displacement, which charges
never measure.

`crates/engine-rapier/src/world/loading/charges/tests/motion.rs` covers bodies
and supports that move, turn, split or hang together:

- **Common velocity:** a head-on strike moving with its whole scene at
  (37, -12) or (-300, 140) is priced exactly as at rest. A box carried on a
  platform at those velocities is priced at its weight to within f32 rounding
  at their speeds, 3.2e-5 of it at 39 and 7.6e-4 at 331.
- **Turning support:** a kinematic paddle turning at 3 strikes a ball 1.5 from
  its center at 4.5, priced on the ball's whole mass. A paddle turned by its
  velocity and one moved by its pose strike alike, in a whole step and in one
  that a CCD bullet elsewhere splits.
- **Moving support:** a platform that rises at 1, then speeds up and slows
  down at 5, carries the box on it at its mass times gravity and the
  platform's acceleration. The step the platform jumps to 1 isn't priced. A
  plank tilting at 0.5 carries the share of the box's weight along its
  normal, less what swings the box about the plank's center.
- **CCD into a free body:** without CCD a shell at 400 passes through a free
  box. With CCD it strikes it, priced on their reduced mass.
- **Bounces:** each bounce of a heavy ball on a wooden floor is its own
  impact, priced on the floor as the bounces before it left it. The third
  bounce takes only what the first two left, and the floor fails.
- **Fragment:** a bar that the host breaks in two in flight, each half with
  half the bar's region, costs nothing at the cut. Its front half strikes a
  wall on its own mass, as the budget's limit above says of a body pushed
  through a contact already open.
- **Stack and cart:** once settled, each support in a stack of three carries
  the weight of the boxes above it. A box riding a cart that speeds up is
  priced at its weight, and the cart's own support isn't priced once it
  slides along the floor.
- **Rover:** each wheel of the rover dropped onto rock lands on its own mass,
  since joints aren't in an impact's budget. Settled, its wheels' supports
  carry the whole rover's weight through its sprung slots.

`crates/engine-rapier/src/world/loading/charges/tests/invariance.rs` covers
what charges must not depend on, and the state they keep:

- **Partitions and order:** a box crushing soft tissue for a second, the box
  in one, two or four pieces and the tissue in one or five, is charged the
  same work to within a part in ten thousand, and its support carries its
  weight for the second. Neither which body is added first, nor the order of
  their pieces, nor which source sorts first changes it. The seams lie apart.
  Limits says what seams that meet cost.
- **Step rate:** a ball dropped 0.5 to 3.7 onto rock at 30, 60, 120 and
  240 Hz is priced at the speed it had when capture first saw it, within a
  step of gravity of its free fall's. Capture first sees those drops inside
  the rock. A drop that ends a step a thousandth above the rock is first seen
  there, within Rapier's prediction distance, and is priced as closely.
- **Pauses:** four balls bouncing on rock beside a resting box, with steps of
  zero, a negative length, NaN and infinity after every step, price nothing
  in those steps. The run prices the same charges and leaves the same
  surfaces as without them.
- **Stale IDs:** the host replaces a struck ball with a new actor under the
  same IDs, half a unit beyond it, before pricing the step. The strike is
  reported as retired, with its energy, and charges nothing, although the
  host still keeps the old surface. The new actor's region has a new token,
  and when the first ball reaches it, the strike is a new episode on that
  token.
- **Busy scene:** sixty balls are thrown about between rock walls for ten
  seconds. At the default bounds, capture refuses nothing: its episodes
  peaked at 147 and its records at 152 a step, against 1,024 and 8,192. With
  room for 48 episodes the cap holds, and capture refuses the batches that
  would exceed it, in 984 of the 1,200 steps. Each step reports each impact
  of the batches capture admitted, once, and a step with a refused batch
  prices no supports. Removing the balls retires every episode they kept.
- **Refused batch:** with room for one episode, a strike between two balls
  takes it, and capture refuses the batch in which a CCD bullet strikes a
  wall later in the same step. The bullet's strike is missing from the
  charges, and the balls' strike is priced. With room for two, both are.
- **Replay:** the bouncing scene, saved 100 steps in with the host's
  surfaces and restored in the same build, prices its last 140 steps as the
  run did and leaves the same surfaces.

Run `RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib world::loading::charges`.

## Acceptance

Each item of #327's acceptance list rests on these tests:

- **#326's reference cases on real physics:** `reference.rs`, all but the
  cases its Evidence names as not run.
- **Common velocity:** `motion.rs`'s common velocity.
- **Moving and rotating supports:** `motion.rs`'s turning, moving and
  tilting supports, and `tests.rs`'s kinematic slab.
- **Angular contacts:** `tests.rs`'s spinning bar, `reference.rs`'s angular
  strikes and `motion.rs`'s turning paddle.
- **CCD:** `tests.rs`'s CCD wall and substeps, `supports.rs`'s split step,
  and `motion.rs`'s shell into a free box.
- **Static walls:** `tests.rs`'s fixed wall and `motion.rs`'s fragment.
- **A dynamic fragment:** `motion.rs`'s fragment.
- **Repeated bounces:** `motion.rs`'s bounces, and `reference.rs`'s safe
  bumps and progressive hits.
- **Multi-collider actors:** bodies of several colliders in `tests.rs`'s
  partitions and many colliders, `reference.rs`'s seam and `invariance.rs`'s
  partitions, and `motion.rs`'s rover, an actor of three bodies with a region
  each.
- **Sustained support:** `supports.rs`, `reference.rs`'s resting support and
  fixed step, and `motion.rs`'s stack and cart.
- **Partitions and contact order:** met for impacts, by `tests.rs`'s and
  `reference.rs`'s partitions, and for supports whose seams lie apart, by
  `invariance.rs`'s, each to within a part in ten thousand of the work. It is
  open for supports whose seams meet, which Limits describes and #694 tracks.
- **Pauses and zero-time steps:** `invariance.rs`'s pauses, and `tests.rs`'s
  host rules.
- **Stale IDs:** `invariance.rs`'s stale IDs, `supports.rs`'s removed body,
  and capture's retirement and handle-reuse test.
- **Bounded state and work:** `invariance.rs`'s busy scene and refused
  batch, capture's limit and refused-batch tests, and the game worlds above.
  Limits says what a scene that fills capture's room loses.
- **Same-build replay:** `invariance.rs`'s replay, and capture's snapshot
  tests. This is not a cross-machine lockstep claim.

Capture's tests are in `crates/engine-rapier/src/world/loading/tests.rs`.
