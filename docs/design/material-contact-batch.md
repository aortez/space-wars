# Material contact batch experiment (#327)

The first integration experiment establishes a global pre-solve contact phase
and tests finite impact accounting on real, partitioned Rapier contacts. It
builds on the [material law](material-damage.md). It is a foundation for #327,
not the completed engine-facing loading adapter or a gameplay migration.

The follow-up [engine contact records and episodes](material-contact-records.md)
adds opt-in canonical capture, incarnation tokens, bounded episode tracking and
serialized continuation. The [shared normal energy experiment](material-shared-energy.md)
then tests coupled compression budgets, an explicit pulse-allocation
approximation and checked paired impulses under default solver settings.

## Backend boundary

Rapier's existing `modify_solver_contacts` callback runs independently for each
manifold during narrow-phase collision detection. At that point a callback
cannot see and order the complete set of contacts sharing a material region or
an incoming motion budget. Charging damage directly inside that callback would
make capacity allocation depend on backend traversal order.

The vendored backend adds `PhysicsHooks::modify_solver_contacts_batch`:

1. Narrow-phase collision detection and existing per-manifold modifiers run.
2. Rapier chooses the CCD substep duration and interpolates kinematic velocities.
3. The batch hook sees all retained contact pairs, current bodies/colliders and
   that substep's duration. It can collect, canonicalize, evaluate and apply
   restitution before any velocity constraints are solved.
4. The ordinary island construction and velocity solve proceed.

The hook runs once per actual solver substep, even without collider hook flags.
Its default implementation does nothing. No additional collision pass, physics
step, persistent backend state or snapshot field is introduced. Existing
per-manifold tangent commands remain intact. External forces have **not** been
integrated at this boundary; the adapter must account for those separately.

The original context exposes read-only contact/body/collider data and a checked
restitution setter for an existing manifold. The follow-up also adds a checked
paired normal-impulse operation, documented in the shared-energy note. It cannot
change contact topology, normals or collision graph membership. Pair iteration order
is unspecified. Raw handles are valid for this synchronous phase; they are not
the stable event/material identities required by the eventual engine API.

Retained pairs include empty, sleeping and speculative contacts. The batch
boundary alone does not classify physical impacts, establish available energy,
or supply a sustained-load model.

## Planar experiment

`crates/engine-material/tests/contact_batch.rs` supplies a test-only consumer:

- A rotation-locked 20 mu slab meets a flat support with a 2 wu face. Both use
  ARMOR; friction and gravity are zero. The support is fixed or translates at
  a prescribed constant velocity. A shifted case gives both bodies the same
  additional velocity and compares energy in the support frame.
- Each body uses one, two or four adjacent cuboid colliders. Duplicate
  overlapping colliders and reversed insertion order exercise redundant and
  differently ordered manifolds without changing mass or physical geometry.
- Actual solver-contact endpoints define intervals along the face. Unioning
  those intervals merges overlap and ignores duplicate seam points. The
  deformation depth comes from the whole patch/body (1 wu here), not each
  collider's width.
- Equivalent contacts with one material identity produce one material call.
  Four disjoint support regions instead receive proportional shares of the
  **single** normal-mode energy, `0.5 * 20 * relative_speed²`. Calls are ordered
  by fixture region identity and each charge is applied before the next call.
- For these equal-compliance, aligned patches, allocating energy by union width
  scales work and area together. This is a restricted physical approximation;
  it is not a general rule for arbitrary multi-contact assemblies.
- The batch derives a common restitution from total recoverable energy. Excess
  is explicitly assigned to failure dissipation, as in the earlier isolated
  solver fixture. Tests compare the ledger's loss with actual pre/post normal
  kinetic energy, using a relative tolerance of 0.003 (absolute floor 0.003).

At relative speed 1 wu/s, the available energy is 10 and damage is zero. At
40 wu/s, available energy is 16000 and both materials take damage. Splitting
or duplicating colliders with the same region identities produces exactly the
same material outcome as the single-collider case. Four distinct regions retain
the same total budget and approximately the same total damage; reversing
collider insertion produces exactly the same ordered per-region outcomes.

Additional regressions check:

- An already damaged shared region with only 1000 capacity left fails once;
  later contacts in the batch consume none of its exhausted capacity.
- Contact persistence does not charge repeatedly, resting contact charges
  nothing, and a new launch after actual rebound/separation charges again.
- Cloning physics state and the fixture's episode/material state mid-contact
  gives identical subsequent motion, material balances and event records.
- A separate fast-ball case actually subdivides a CCD step, verifies one batch
  callback per substep and confirms that batch restitution reaches the solver
  without per-collider hook flags. This tests the backend phase, not the planar
  fixture's admission policy for high-speed arrivals.
- Position-based support velocity is visible after interpolation. Invalid
  restitution values, invalid handles and missing manifolds leave coefficients
  unchanged.

## What this does not establish

The planar consumer intentionally asserts its narrow geometry and small scene
bounds. Its identities are fixed fixture IDs, its episode latch is for one
planar body pair, and its contact admission requires touching faces. It is not
used by scenarios and does not expose raw Rapier types to a new engine API.

Before completing #327, implement and test:

- A backend-private conversion to canonical engine records, with stable
  source/region/episode identities across split, merge, retirement and handle
  reuse. Preserve ordering independently of backend traversal.
- Budget ownership for arbitrary normals, angular motion, unequal compliance,
  dynamic supports and constraints. Width division is not justified for these
  cases. Multiple contacts must share motion without discarding later distinct
  impacts or treating an entire actor as one permanent episode.
- Speculative-contact and CCD arrival admission, separation/reimpact hysteresis,
  and suppression of chatter and solver stabilization as new impact energy.
- Sustained/connection loads and attributable external work, including moving
  supports and nonblocking soft-footprint sweeps.
- Production traversal/cache/event limits, admission and overflow diagnostics,
  pause handling, stale-ID rejection, queued consequences and serialized replay.
  Fixture bounds and clone continuation are not substitutes for these contracts.

Gameplay failure consequences remain owned by #328. No destruction, ejection,
protection or penetration policy is changed by this experiment.

## Reproduction

```sh
cargo test -p engine-material --test contact_batch
cargo test -p engine-material
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib
cargo clippy -p engine-material --all-targets --no-deps -- -D warnings
```

Validation at this revision: 51 material tests (including eight new batch
regressions) and 208 engine-physics tests pass; one existing engine test is
ignored. Material Clippy passes. Extending the same Clippy command to
`engine-rapier` encounters six pre-existing errors in unchanged `world.rs`
(`tank_route_query`) and `plant_light.rs` (`publish_with_occluders`): unit error
type, argument count, collapsible conditions and boolean comparison.

The backend extension is listed in `vendor/rapier2d/SPACEWARS_PATCH.md` so an
upstream upgrade cannot silently discard the phase needed by the adapter.
