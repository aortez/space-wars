# Ledger-set bounce: feasibility (#327 step 3)

Step 3 of #327 has the work ledger set the bounce. When an impact starts, the
[impact law](material-damage.md) splits its energy into what the materials
consume and what they can return. Today
[contact charges](material-contact-charges.md) price the impact after the
step, and Rapier's restitution sets the bounce regardless. This note reports
the experiment that came first, because step 3 touches the vendored solver and
its size was the least certain. It ends with a recommendation for step 3's
PRs.

Nothing in the engine, the games or the AI baselines changed. The probe, its
sources and its records are on the owner's workstation in
`/home/data2/space-wars3-artifacts/bounce-probe-20261123`, with a README.

## Summary

- **The cheap route works where a contact is on its own.** A hook before the
  solve prices each new impact and sets its contacts' restitution to the
  ledger's. For a contact between two bodies with no other contact, joint or
  gap, Rapier returns the coefficient it is given to within 1% of the
  available energy, spinning or not. In Spacewars that held for 19,942 of
  19,948 such impacts, at Rapier's own coefficients. Friction on a spinning
  contact adds a few percent.
- **Few of the game impacts that matter are on their own.** In Scorched, where
  the ledger's bounce is large, 41% of impacts come back within 1% of their
  available energy, and 10% of the tanks'. Gravity within the substep,
  suspension joints, gaps that close during the solve and the other contacts
  of a pile or a tank take the rest. The proposed criterion, 95% within 1%,
  fails.
- **Most of a tank's impact energy is never priced.** Rapier bounces each new
  point. The ledger prices only the strike that opens a contact episode. In
  Scorched's duels, the strikes inside open episodes carried 2.5 times the
  energy of the tank impacts that were priced.
- **The exact route admits nothing.** With its checks limited to the bodies in
  contact, the native response would admit none of either game's impacts.
- **The ledger's own bounce would change both games.** With the materials they
  declare, Spacewars would lose almost all its bounce. Scorched's tanks would
  gain one, a restitution of 0.75 at the median for a sprung wheel on the
  ground, where today they have none.
- **It is cheap.** Pricing and setting the bounce took a median of 8 µs a step
  in Spacewars and 13 µs in Scorched's duels.
- **Capture had a bug, now fixed (#707).** After a CCD substep it took each
  body's center of mass from where the body was one substep earlier. Charges
  priced Spacewars' impacts in later substeps at half their energy.

The recommendation, at the end, is not to build the cheap route yet.

## Two routes

- **The cheap route.** A batch hook runs before each CCD substep's solve, after
  capture has recorded the batch. It groups the batch's new impacts into the
  same budgets that charges use and prices each one with the law. It then sets
  the restitution of the impact's solver contacts to `e = sqrt(R / A)`. `A` is
  the impact's share of its budget, the energy a perfectly plastic stop
  removes. `R` is what the law says the two sides return. Rapier's solver then
  aims each new contact's separating speed at `e` times its approach. The
  vendored solver already lets the hook set a contact manifold's restitution.
- **The exact route.** The [native response](material-native-response.md)
  prices and responds inside the solve with its own allocation, so the
  returned energy is exact by construction. Today it admits only restricted
  worlds and checks the whole world. Step 3 would narrow its checks to the
  bodies in contact.

## How it was measured

A temporary probe added the hook, a per-region surface map the host refreshes
before each step, and a census of each budget. It ran in three modes:

- **Observe** prices each new impact and records it, while Rapier's own
  restitution sets the bounce.
- **Ledger** also sets each new impact's restitution to the ledger's `e`. Every
  other bound contact gets none. An impact the law could not price gets none.
- **Ledger with gap** adds to `e` the speed it takes to close a gap the contact
  starts with. It did worse than Ledger in both games and is not discussed
  further.

**The rebound.** For each budget, the probe measures the energy along the
normal that comes back:

- It is the energy a perfectly plastic impulse would need to stop the budget's
  rows again. The rows count only if the plastic allocation before the solve
  pushed them.
- It uses the velocities the substep's solve left, read when the next substep
  begins or the step ends, before anything else moves the bodies.
- It uses the lever arms the solve began with.
- Divided by `A`, it compares directly with `R / A`, and in Observe mode with
  the square of Rapier's own coefficient.

A closed form, the post-solve velocities projected on the plastic impulse,
needs no iteration. It moved the share of impacts within 1% by at most two
points in any run.

**Contact classes.** Each budget is put in the first of these that fits:

- **Field-bound.** The bounce asked for is no faster than twice what gravity
  and applied forces add along the normal in the substep. Scorched's gravity
  adds 0.3 u/s a step.
- **Speculative.** A row was apart when the solve began.
- **Joints.** A body of the budget has a joint.
- **Other contacts.** A dynamic body of the budget touches something else in
  the batch.
- **Several points.** More than one row carries the plastic impulse.
- **Rotating, friction**, **rotating** and **one point, central.** These are the
  contacts on their own, by how much of the row's give is rotation and whether
  the pair has friction.

**Scenes and games.**

- **Reference scenes** in the engine:
  - balls of each material thrown at rock;
  - boxes landing flat and on a corner, with and without friction;
  - balls meeting head on and glancing;
  - a CCD bullet.
- **Scorched**, with step 1's hosts: the scripted contacts and the 48 CPU duels.
  A tank's hull and wheels are its armor. Terrain and loose dirt are rock, and
  wood is wood.
- **Spacewars**, 300 seconds at the asteroid census's settings. Ships, rovers
  and debris are their integrity maps' regions at their health. The sun,
  planets and the boundary are rock.
- **Spacewars, calibrated.** The same run with every material's damping lowered
  to 0.19. A side that stays safe then returns 81% of what it stores, as a
  restitution of 0.9 returns 81% of the available energy.

## What the ledger would change (Q1)

**Scorched** has no restitution today, and 0.4% of the duels' impact energy
comes back. The ledger would return 2.3%, most of it on tanks.

- **The ledger's restitution, at the median.** A region is a body, so a
  rigid tank's wheels count as its hull, and only a sprung tank's wheels are
  wheels.
  - a wheel on the ground, 0.75;
  - a wheel or a hull on loose dirt, 0.88 to 0.90;
  - a hull on the ground, 0.35, where 68% of those impacts yield;
  - dirt on dirt or on the ground, 0.22. Rock damps 95% of what it stores.
- **By speed.** Below 0.5 u/s almost nothing yields, and the ledger returns 36%
  of the energy. Yielding rises to 16% at 1 to 2 u/s, 60% at 2 to 5 u/s and
  95% at 5 to 20 u/s. The return falls to 1.3%.

**Spacewars** today returns 49% of the energy of a strike on the sun or a
planet, at restitution 0.7, and 71 to 81% between asteroids and fragments, at
0.9. The ledger returns almost none of it:

- Rock is brittle and yields in 97 to 100% of these impacts, and a brittle
  side that yields returns nothing.
- A hull damps all it stores.

With every damping at 0.19, the ledger still returns less than 1% above
5 u/s, because rock still yields. A ship's hull returns a restitution of about
0.09.

So taking the bounce from the ledger is first a choice about the games'
materials. As declared, they would make Spacewars inelastic and Scorched's
tanks bouncy.

## How close the cheap route gets (Q2)

**Contacts on their own are exact.** In Spacewars, Rapier returned its own
coefficient to within 1% on 19,942 of 19,948 isolated impacts, 97% of the
energy, spinning bodies included. Under the ledger, whose targets there are
mostly zero, it was 99.9%. The reference scenes agree:

- a ball thrown at rock comes back with exactly the ledger's energy, for every
  material and speed, when it is touching as the solve starts;
- head-on and glancing ball pairs do too, with or without friction;
- a CCD bullet does too.

**Where it misses**, and why:

- **Field-bound.** A slow strike under gravity asks for a bounce no faster than
  twice what gravity adds in a substep. In the duels this was a quarter of
  the budgets, mostly grains settling, with 42% of the energy. Of those, 40%
  came back within 1%, and of the tanks', 3%. Nearly every miss bounced too
  little.
- **Speculative.** A contact that starts apart closes its gap during the solve.
  Rapier's restitution aims at a speed taken before the solve, and most of
  the bounce is lost. An armor ball reaching rock at 0.5 u/s at 120 Hz came
  back with 2% of its energy, where the ledger asked for 66%. In Spacewars'
  Observe run, speculative contacts were 537 of the 598 misses beyond 5%.
  Adding the gap's speed to `e` did not help. The ball came back with 1.4%
  even at `e = 1`, and in Spacewars the variant bounced too much 239 times
  and too little 10.
- **Joints.** A sprung wheel's bounce is taken up by its suspension. The
  ledger prices the wheel alone, as charges do, while the solve couples it to
  the hull. In the duels, 16% of these came back within 1%.
- **Other contacts.** A grain in a pile, or a tank that strikes while it
  touches something else, bounces against everything it touches. In the
  duels, 54% came back within 1%.
- **Friction on a spinning contact.** Friction's impulse turns a box that
  lands on a corner, and the turn changes the corner's speed along the
  normal. At 1 to 20 u/s with friction 0.8, the box's first strike came back
  with 2 to 6% of the available energy more than the ledger's. Without
  friction it was exact at 5 and 20 u/s.

**Against the criterion**, 95% of impacts within 1% of their available energy:

- **Spacewars passes, but only because the ledger asks for almost nothing.**
  98.9% of impacts were within 1%. Only 212 of 18,055 had a target above 1%,
  and 77% of those were within 1%. In the calibrated run, 997 had one, and
  60% of those were within 1%.
- **Scorched fails.** In the duels, 41% of impacts were within 1% and 64%
  within 5%. The misses beyond 5% carried 6.4% of the energy. Of the tanks'
  own impacts, 10% were within 1%, and of those at 2 u/s or more, 32%. The
  misses beyond 5% carried 19% of the tanks' impact energy. Among them, too
  little bounce outnumbered too much by 16 to 1.

**Strikes the ledger never sees.** Charges price only the strike that opens a
contact episode. Rapier treats every point that carried no impulse in the
last solve as new, and bounces it if its restitution allows. The probe counted
these strikes inside open episodes, approaching faster than the impact
threshold:

- **Scorched's duels.** They carried energy equal to 62% of all the priced
  impacts' energy. On tanks it was 2.5 times the priced tank impacts', and 93%
  of that was between a hull and the ground. A rigid tank's wheels are
  colliders of its hull, so once the first of them opens the episode, the
  others and the hull's corners strike inside it.
- **Scorched's scripted contacts.** They carried 19% of the priced energy.
- **Spacewars.** They carried 0.5%.

Under the ledger they get no bounce. This is the limit charges already list:
an impact folded into an open episode is priced neither as an impact nor as a
support.

## The native response, contact by contact (Q3)

With its checks limited to the bodies in contact and what acts on them, the
native response would admit none of either game's impacts. Each rule refuses
a share of the census:

- **Scorched's duels.**
  - Gravity, CCD and friction each refuse every impact.
  - Geometry refuses 99.7%. The response admits only two balls, or two boxes
    that meet face to face and cannot rotate.
  - Penetration refuses 95% and a starting gap 71%.
  - The size limits refuse 50%, nearly all of them in piles of grains.
- **Spacewars.**
  - Penetration refuses 98%. These contacts overlap by more than the
    response's touching tolerance of 1e-5 when the solve begins.
  - Geometry refuses 77%.
  - Planets, which Spacewars moves by position, refuse 46%, and CCD 30%.

Lifting geometry, penetration, gaps, gravity, forces, friction and CCD
together would admit 38% of Scorched's impacts and 54% of Spacewars'. That is
a general contact solver, not a step of #327.

## Cost (Q4)

The essential work is grouping the batch's new impacts, pricing them and
setting restitution. On a workstation at a load average of about 23 on 16
cores:

- **Spacewars:** 8 µs a step at the median and 20 µs at the 95th percentile, of
  a step of 1.3 ms on the CPU, or 0.6%.
- **Scorched's duels:** 13 µs at the median and 50 µs at the 95th percentile, of
  0.58 ms, or 2.2% at the median and 3.9% on average.

Capture and arrivals, which charges already need, come on top. The probe set
every bound pair's restitution in every batch. A host whose bound colliders
have no restitution of their own needs only the new impacts set.

## A stale center of mass in capture

Capture read each body's center of mass from Rapier's `RigidBody`. Rapier
refreshes it when a solve begins, after the batch hook. So in a CCD substep
after the first, the center capture recorded was where the body was when the
previous substep began, one substep behind its pose. It is fixed (#707).

- **Effect.** A body's lever arms in a later substep were off by as far as it
  moved in the substep before. Its point velocity was off too, though only by
  its spin times that offset.
- **Spacewars.** 11% of the impacts came in a later substep. Charges' budgets
  gave 92% of those an energy more than 10% off, and two thirds one more
  than twice off. In all they priced them at 51% of their energy, and all of
  Spacewars' impacts at 95.5%. One fragment striking the sun at 336 u/s was
  given an available energy of 480 instead of 218,000.
- **Scorched.** No impact came in a later substep, so nothing changed.
- **The native response** reads Rapier's center too. It refuses any world
  with a CCD body, so it never sees a later substep.
- **Tests.** Step 1's and step 2's CCD tests did not catch it. Their bullets
  strike head on without spin, and moving the center along the normal changes
  no lever arm. The CCD-split charges test now also moves its ball along the
  walls, and fails with the stale center.
- **The fix.** Capture takes the center from the body's pose,
  `local_mprops.world_com(position)`, and the point velocity about it. The
  probe made the same change, and every figure in this note uses it.

## Surfaces known before the step

The law needs both sides' surfaces when the impact starts, inside the step.
Charges ask the host after the step, through `ContactSurfaces`. The probe
handed the engine a map of every region's surface before each step. The native
response already has the right shape: `register_loading_material` keeps a
region's material and remaining capacity in the engine, and
`absorb_loading_material` and `loading_material_remaining` charge and read it.

The proposal:

- **One registration.** Move that registration from the native response to
  capture, so charges, the bounce and the response share it. Widen it from a
  `Section` to a `ContactSurface`, with the region and its capacity,
  thickness, `Depth` and radius. The host's shielding, which
  `ContactSurfaces::protected` answers today, becomes part of it too.
- **The engine owns the balance.** A new impact is priced once, in the hook,
  against the balance as it stands, and its consumed work is charged there. A
  later substep's impact sees an earlier one's charge, as charges' in-turn
  rule does now.
- **Charges report that outcome.** After the step, `charge_contact_impacts`
  reports the outcomes priced in the step instead of pricing again, so the
  bounce and the charge come from one outcome. The host applies each charge to
  its own health, as `ContactSurfaces::absorb` does today, or reads the
  balance.
- **Updates between steps.** A host that repairs a region or changes its
  material updates its registration.
- **Unpriced impacts keep the collider's restitution.** An impact the engine
  cannot price falls back to today's bounce: one whose side was retired or
  has no surface, whose budget failed, or that the law refused.

## Recommendation

The cheap route is exact where a contact is on its own, and misses where it
is not. Spacewars' impacts are nearly all on their own, but its materials
leave almost nothing to bounce. Scorched's tank impacts are almost never on
their own, and most of their energy arrives in strikes the ledger never
prices. The exact route admits no game impact. So I recommend not building
the cheap route as an opt-in yet:

1. **Fix the stale center of mass.** Done (#707): capture takes the center
   from the body's pose, and a test strikes obliquely in a later CCD substep.
2. **Decide whether the games want the ledger's bounce.** As declared,
   Spacewars' materials leave almost nothing to bounce, and Scorched's armor
   would bounce. That is a choice about the materials in #330 and #331,
   before it is an engine question.
3. **If they do, step 3's PRs, in order:**
   1. Engine-held surfaces, as above.
   2. Re-impacts. A new point in an open episode is priced as an impact of its
      own. #331 weighed this for severe flat drops and kept its rock ground
      instead. Charges would gain the tank energy that goes unpriced today.
   3. The ledger's bounce as an opt-in. The hook prices each new impact and
      sets its restitution, and each charge reports the rebound measured after
      the solve beside the ledger's, so a shortfall is visible.
   4. This probe again, against the same criterion, before #330 or #331 adopts
      it.
4. **Leave the exact route.** Admitting the games' contacts would take a
   general solver.
5. **Go ahead with step 4.** The game-like Material Lab scene and the host
   contract do not depend on step 3.
