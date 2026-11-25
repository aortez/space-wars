# Engine contact records and episodes (#327)

`engine_rapier::world::loading` turns the
[pre-solve batch hook](material-contact-batch.md) into an opt-in, engine-owned
record stream. Capture alone records motion and tracks episodes without changing
physics. The separately enabled [native response adapter](material-native-response.md)
now connects restricted admitted impacts to material balances and paired impulses.

This keeps the physical observation and lifecycle contracts testable before
adding generalized shared-energy accounting. Existing scenarios continue to
use their current physics and damage paths.

## Identities and registration

The host enables capture once with `PhysicsWorld::enable_loading_capture` and
registers the regions and surfaces it wants to observe:

1. `create_loading_region(RegionId, BodyId)` creates an incarnation token for
   one logical material region on an existing body. A region's entity must
   match the body's entity. No material parameters or capacity are stored here.
2. `ContactSource { region: token, patch }` identifies one physical surface
   patch. Several equivalent collider parts can share this source. Independently
   contacting patches must have distinct source IDs, even on the same region.
3. `bind_loading_collider(ColliderId, ContactSource)` explicitly binds each
   collider. Capture requires both contact sides to be bound. A live binding
   cannot silently change source; collider replacement removes the old binding.

Region tokens contain a monotonic, world-issued incarnation. Retiring a region
removes its bindings and cached episodes; removing an entity retires its regions.
Recreating the same logical region produces a new token, and old tokens fail
`loading_region_is_live` and binding validation. Counters reject exhaustion
instead of wrapping. Tokens are scoped to a world and its snapshot lineage.

Replacing collider partitions does not retire their material region. Rebind
the replacements to the same source before stepping to preserve an ongoing
episode. A real material split or merge instead requires explicit retirement,
new region registration and rebinding; the material-capacity transfer remains
outside this observational layer.

Region allocation order is part of authoritative host input. It must replay
consistently. Collider insertion/handle traversal order does not determine the
ordering of registered sources or their event allocation.

## Canonical records

Each admitted solver substep produces a `ContactBatch`, including its substep
index and duration. `loading_batches()` exposes the last step's batches after
the solve. They retain pre-solve values rather than rereading post-solve motion.
The internal capture executes before the solve; the next adapter stage will
need to consume the batch at that boundary to apply material response.

`ContactRecord` contains:

- Ordered source/region tokens, engine collider IDs and optional compound child
  indices. Raw Rapier handles and types stay private.
- The world contact point, unit normal from source 0 toward source 1, and signed
  separation. Positive separation marks a speculative contact.
- Each body's identity, dynamic/fixed/kinematic kind, center of mass,
  contact-point velocity, angular velocity, effective inverse mass/inertia and
  sleeping state. Locked axes are reflected in the inverse properties;
  fixed/kinematic supports contribute zero inverse mass and inertia.
- Relative closing speed and tangential speed. The tangent is
  `(-normal.y, normal.x)`; positive closing speed means approach. Point velocity
  includes rotation, and common translational velocity cancels.
- An optional current episode ID. An episode ID does not assign energy to a
  contact point or grant permission to charge it independently.

Records sort by source pair, collider IDs, subshape IDs, normal, point and
separation. Every retained solver point is recorded; this layer deliberately
does not turn point count into patch width or a repeated energy budget. Actual
geometry aggregation remains a separate adapter operation.

Capture composes with moving-surface hooks and does not modify their commands.
The backend phase follows kinematic interpolation, including the existing
engine correction for position-based kinematics with CCD. Gravity and other
external forces have not yet been integrated. These records alone therefore
do not account for external work or constraint/support reactions.

## Episode lifecycle

One cached episode represents one ordered pair of host-declared surface
sources. Only a touching observation can start it: separation must be at most
`touch_tolerance` (default 0.00001 wu). Purely speculative samples remain visible
but cannot create an episode on their own. A world can let
[speculative arrivals](#speculative-arrivals) start one too.

A new episode emits one transition across all its points and collider parts:

- `Impact` when the fastest approach among the points that start it exceeds
  `min_approach_speed` (default 0.0001 wu/s). This is a candidate impact.
- `Support` otherwise, including resting contact.

Continuing contact keeps the episode ID and emits no additional start. A
complete observation with no point that could start an episode accumulates
absence time. At `separation_grace_seconds` (default 1/60 s), the episode ends
once and can be rearmed by a subsequent contact. This time-based grace
suppresses short contact gaps without tying the rule to render frames. Pause,
invalid time and rejected batches do not advance the absence timer.

The policy has explicit limits: a real departure and return inside the grace
period can be coalesced; distinct independent surfaces sharing one source ID
will also be coalesced. The host must declare separate physical patches.
Per-feature source binding inside a single compound collider, general spatial
episode matching, and refinement of fast reimpact/chatter classification remain
open. Compound child IDs are preserved as evidence, but do not automatically
become new material identities.

An `Impact` transition is not a proof of available energy. Generalized motion
sharing, driven contacts and constraint effects still need an accounting law
before any transition can cause material damage.

## Speculative arrivals

Rapier's solver acts on a speculative contact when the approach would close its
gap within the substep, and it stops the bodies before they touch. That happens
whenever a body's last substep before an impact starts within Rapier's
prediction distance of the surface, 0.002 length units by default, as every
slow approach does. In the native response's
[arrival evidence](material-native-response.md#arrival-evidence-and-next-boundary),
the solver spends over 99% of an impact's energy on a speculative contact. Under
the touching rule, that impact's episode starts a step later, on whatever
approach is left, if it starts at all.

`PhysicsWorld::enable_speculative_arrivals` changes the rule for one world. A
speculative point whose closing speed times the substep's duration exceeds its
separation counts as a touching one: it starts or continues an episode, and the
episode's `Impact` then carries the approach the bodies brought into the
substep.

Capture stays read-only: the rule changes which records start episodes, not the
physics. The material response admits only touching contacts, so a world
enables one or the other, and each refuses the other with
`LoadingError::Incompatible`. The response's own refusal,
`ResponseRejection::SpeculativeArrival`, is broader than this rule: it covers
any approaching speculative point, whether or not its gap closes within the
substep.

The rule has two known limits:

- A substep's records come before its gravity and other forces are integrated,
  and before its solver aims for a bounce. When only the substep's gravity
  carries a slow body across a gap, the rule misses the arrival, and the solver
  stops the body there. Restitution does the same: on a bouncy contact the
  solver acts on a slightly slower approach than the rule's, to start the
  rebound. The next record then shows whatever approach the solver left, often
  none, so the episode can start as a `Support` and the impact is lost. Such
  approaches are slow: their gap is within the prediction distance, so at 60 Hz
  they are under about 0.12 length units a second, plus a substep of gravity.
- A body that the solver stops just clear of its support, without touching it,
  accrues absence as a separated one does. If it stays clear past the
  separation grace, its next arrival starts a new episode.

## Bounds and admission

`CaptureConfig` bounds region and binding registries, pair/manifold/point visits,
contacts per batch, cached episodes, batches per outer step and combined output
records/transitions per step. Each configured count must be in 1..=65536.
Contact ordering costs at most a sort of the admitted contact limit; episode
preparation also visits/clones at most the episode limit. These are capture
bounds, not bounds on Rapier's own collision detection or solver work.

A whole batch is staged before episode state, sequence counters or output are
committed. On overflow or nonfinite input, it is rejected in full. There is no
backend-order-dependent prefix admission, no partial episode update, and no
discarding of earlier admitted batches. Diagnostics identify the last rejection
and count rejected/observed batches. A rejected observation cannot imply that
all contacts separated: cached episodes retain their previous state.

This is an observation admission policy. It makes missing data explicit; it
does not define what gameplay should do if a damaging event cannot be
admitted. Both games' hosts leave a refused batch's impacts unpriced and count
the refusal, and neither comes near the bounds
([game worlds](material-contact-charges.md#game-worlds)). Capture itself
creates no lethal consequences.

## Persistence

Capture-enabled worlds use physics snapshot version 2: the existing physics
state followed by capture configuration, region incarnations and counters,
bindings, episode IDs and absence timers. Restoration validates the capture
bounds, live body/collider bindings and token references. Output batches and
per-step diagnostics are transient and start empty after loading.

Worlds without capture retain the version 1 encoding. Existing version 1
snapshots remain readable. Unknown versions, truncated loading state and
trailing bytes are rejected. Old builds do not understand version 2 snapshots.

A world with speculative arrivals writes a tag after its versioned payloads,
before any other tagged section, and keeps its version. Older builds refuse that
tag as trailing data. A snapshot whose tag comes with the material response, or
without a capture, is rejected.

Same-build serialized continuation is tested, including a mid-contact snapshot,
separation, and a later new impact. This is not a cross-machine lockstep claim.

## Evidence and remaining integration

The loading tests exercise actual canonical worlds with:

- Reversed body/collider insertion, equivalent collider partitions, and distinct
  source patches on one material region.
- Angular contact velocities, locked inertia, fixed/moving support properties,
  common velocity shifts, resting and speculative contact, and repeated impact.
- Retirement and backend handle reuse, explicit repartition rebinding, pause,
  identity exhaustion, malformed snapshots and exact serialized continuation.
- Atomic rejection at visit/contact/episode/output limits, preservation of prior
  CCD batches, real CCD substeps, and moving-surface hook composition.
- Matching motion with capture enabled and disabled.
- Speculative arrivals:
  - Impacts start on the incoming approach for one, two and four collider
    partitions, where the touching rule starts them on the approach left.
  - A speculative contact starts nothing until its solve closes it.
  - A ball closing on a wall at 400 from 0.001 to 1 away starts exactly one
    impact, at 400. This holds with and without CCD, at 30 to 240 Hz and with
    one or four CCD substeps.
  - Each bounce starts one impact, and a ball resting awake or asleep keeps one
    episode.
  - Snapshots and clones keep the rule, and the material response is excluded.

Run `RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib world::loading`.

The [shared normal energy experiment](material-shared-energy.md) tests coupled
compression beyond the earlier planar fixture: differing normals, angular
motion and dynamic supports. A follow-up pulse allocation and checked direct
response now close the restricted real-solver fixtures at default settings.
The [native response adapter](material-native-response.md) supplies a restricted
first integration with explicit admission, duplicate ball contacts, unioned
rotation-locked face intervals and staged material changes. General footprints
and contact-set admission remain unfinished.
The pulse split is an explicit approximation that still needs calibration.
[Contact charges](material-contact-charges.md) price each new impact after the
step from these records, in any world whose capture admits speculative
arrivals, and leave the physics unchanged.
They price steady supports too. Welds are
[connection loads](material-connection-loads.md), nonblocking recipients such
as grass are [weapon sources'](material-sources.md#grass-and-other-nonblocking-recipients),
and each host owns its gameplay consequences, as the
[guide](material-guide.md) describes.
