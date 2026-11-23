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
     the region with the capacity it has left, its `Section`, and the
     surface's radius of curvature where it meets others (`None` is flat).
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
  - `Budget`: the impact's budget failed. It can have more than 32 points, or
    not converge.
  - `Material`: the law refused the impact, for example between two flat
    surfaces or for a load outside the law's range.

## Pricing

Each CCD substep's batch is priced in order, one new `Impact` episode at a
time:

1. **Rows.** An impact's rows are the points of its episode that the solve
   acts on, those touching or arriving, whose approach exceeds
   `min_approach_speed`. Capture uses the same acting rule to start the
   episode.
2. **Motion.** Each body's motion is rebuilt from its records:
   - center of mass;
   - spin;
   - effective inverse mass and inertia;
   - center velocity, which is the point velocity less the spin's share at the
     point.
3. **Budget.** The new impacts of the substep that share a dynamic body form
   one budget.
   - `shared_impact::allocate` solves the coupled compression over their rows,
     at unit stiffness, and each impact takes its own rows' share of the energy.
   - A body that strikes two surfaces at once therefore spends its motion once.
   - Points split across equivalent colliders share motion, so a partition
     doesn't change the energy a body spends. Each point is its own row,
     though, so it can change how that energy splits between impacts.
   - Fixed and kinematic bodies give no way and couple nothing, and a moving
     one brings its own work.
4. **Law.** `impact()` prices the impact's energy across the patch that
   `patch_width_from_radii` gives for the two radii. Its fastest approach
   carries that energy, and the law depends only on the energy. Each unshielded
   side with consumed work is charged.

Impacts are priced in turn, and each reads the surfaces after the charges of
the one before it.

## Limits

- Only new impacts are priced. Support episodes and sustained loads are not.
  Capture also folds a distinct impact into an episode that is still open.
- A budget holds only the substep's new impacts. A contact that carries on from
  an earlier substep, such as a support under a body, is not in it, so a
  supported body gives way as if it were free. This under-prices a hit that
  drives a body into its support.
- Joints are not in the budget: a jointed body gives way by its own mass and
  inertia alone.
- The patch width comes from the radii the host declares. A hull that lands
  flat is priced across a corner's patch, and so over-charged, and two flat
  surfaces are not priced.
- Each point is a row, so an impact with more points takes more of a shared
  budget, and a budget over 32 points is refused. A hull landing flat across
  many terrain pieces comes back as `Unpriced(Budget(Limit))`.
- A batch that capture refuses is not priced.
- Records come before the substep's gravity, so a falling body's approach
  lacks one substep of it.
- Rapier's restitution sets the bounce. The ledger's recoverable energy is not
  applied.
- Capture's bounds still refuse a whole batch on overflow, and every
  contacting pair counts toward its visits, bound or not.

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
  one on the resting slab's mass at their closing speed.
- **CCD substeps:** a CCD ball bouncing between two walls strikes each in its
  own substep of one step. The strikes are priced in the substeps' order,
  each on half the ball's mass times its approach squared.
- **Partitions:** one, two and four collider partitions, inserted in either
  order, give the same charges.
- **Two floors:** a box landing flat across two floors spends its motion once;
  its two impacts sum to half its mass times the approach squared.
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
