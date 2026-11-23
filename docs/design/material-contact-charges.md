# Contact charges (#327)

`PhysicsWorld::charge_contact_impacts` prices each new impact that
[contact capture](material-contact-records.md) saw in a step, after the step,
against the surfaces the host keeps. The host then absorbs what each impact
consumed. The physics is unchanged: Rapier's restitution still sets the
bounce, and having the work ledger set the bounce is a later part of #327.

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
4. Call `charge_contact_impacts` after every step. The next step drops this
   step's records, and a clone or snapshot carries none, so a step left
   unpriced stays unpriced. A second call after the same step prices nothing.
5. Watch `loading_diagnostics().rejected_batches`. A batch that capture refused
   is missing from the charges, and its impacts may start later, on whatever
   approach the solver left.

Each new impact is reported once, as a `ContactCharge`:

- `Charged` carries the episode, its sources, the approach that started it,
  the law's `Outcome` and which sides were shielded. The outcome's slots follow
  the sources.
- `Unpriced` carries the reason and, once the budget is solved, the impact's
  energy:
  - `Retired`: a side's region was retired after the step.
  - `Undeclared`: the host keeps no surface behind a side.
  - `Budget`: the impact's budget failed. Its rows, the ends of its patches,
    can be more than 32, or it can fail to converge.
  - `Material`: the law refused the impact, such as at a single point
    between two flat surfaces, or a load outside the law's range.

## Pricing

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

## Limits

- Only new impacts are priced. Support episodes and sustained loads are not.
  Capture also folds a distinct impact into an episode that is still open,
  such as a hull that lands flat and then rocks onto its corners.
- A budget holds only the substep's new impacts. A contact that carries on from
  an earlier substep, such as a support under a body, is not in it, so a
  supported body gives way as if it were free. This under-prices a hit that
  drives a body into its support.
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
  lacks one substep of it.
- Rapier's restitution sets the bounce. The ledger's recoverable energy is not
  applied.
- Capture's bounds still refuse a whole batch on overflow, and every
  contacting pair counts toward its visits, bound or not. Neither game comes
  near them, as the next section shows.

## Game worlds

Two temporary probes measured capture and charges on the games, with every
collider bound to a region for its body. Their sources and outputs are on the
owner's workstation in
`/home/data2/space-wars3-artifacts/contact-charges-probe-20261122`. Their
rerun on the final code, in
`/home/data2/space-wars3-artifacts/support-loads-review-probe-20261122`, gave
the Scorched figures with patches and the severe contacts.

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
  - Whether a severe flat drop should cost is a choice for #331. The options
    are a dirt material, a rigid floor and pricing re-impacts.

**Spacewars.** 300 seconds of Spacewars at the asteroid census's settings:
radius 1200, 20 asteroids a second, and each player's health at 500, so 7
games ran.
- The world held at most 406 bodies and 416 colliders.
- The largest batch was 492 visits and 20 points, and no batch was refused.
- Charges priced all 21,317 impacts. Under #683, before patches and the
  solver's restarted pass, 4 failed to converge.
- Other jobs were loading the workstation, and one repeat ran a third slow
  throughout. In the steadier repeats capture cost 1 to 4% of a 1.2 ms step,
  and charges cost 3 to 6 µs a step. Capture's cost needs measuring on the Pi
  before adoption.

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

Run `RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib world::loading::charges`.
