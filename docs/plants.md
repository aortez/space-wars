# Shared plant foundation

`engine_core::plants` implements P1 (#208): a headless tissue lifecycle and
observation/intent contract for the later soil/light adapters, grass policy and
Garden Lab. It creates no physics world. Lightweight tissue occupies an integer
host cell; `Support` refers to a body and generation in the host's canonical
world. `Environment` must check actual space and support liveness/attachment.
Attachment validation makes no stability guarantee under physical loads.

A host owns `Plants`. A brain receives an owned `Observation` and returns
`Intent`s; it cannot mutate authoritative state. The host submits those intents
with the observation's tick. Old ticks reject atomically. `Allocate` sets a
relative maintenance priority for subsequent ticks; `Grow` purchases one adjacent
cell from the parent's living component. The runtime checks size, living parent,
Manhattan adjacency, occupied cells, host space/support, total tissue limit and
all three costs before changing stores or biomass. Requests execute in supplied
order; one rejection does not undo earlier valid requests. Batch work-limit
rejection happens before any mutation. Initial `seed` is a trusted host operation
with an explicit bounded endowment and a fresh organism ID, never resurrection.

## Units and tunable defaults

One call with `elapsed_ticks = 1` advances one fixed physiology tick. The host
chooses the tick's real simulation duration, records it with its scenario config,
and dispatches each tick exactly once. Render frames never advance physiology.
Pause and zero ticks are exact no-ops, including intents. Values other than zero
or one reject; accelerated playback loops over the same ticks. Tick is a monotonic
sequence number, not wall time. No fractional-duration accumulation occurs here.

Resources are integer abstract nutrient, water and chemical-energy units, in that
order. Health is 0..100. Light and root nutrient/water availability are integer
fractions 0..1000. These values are tuning defaults, not measured biology.

| Quantity | Root | Runner | Shoot |
| --- | --- | --- | --- |
| Capacity per size, each resource | 20 | 10 | 20 |
| Maintenance per size per tick (N,W,E) | 1,1,1 | 1,1,1 | 1,2,2 |
| Growth cost per size (N,W,E) | 3,3,3 | 2,2,2 | 4,4,4 |

Roots contacting dirt collect at most 4 nutrients and 3 water per size per tick,
scaled independently by host availability. Shoots collect at most 8 energy per
size per tick, scaled by local light. Products round down. Runners collect none.
Roots and runners have no local light requirement and can live underground on
pooled energy. Shoots require local light >=250 regardless of pooled stores.
Shortage and local shade have separate consecutive-tick counters. The first 3
consecutive deficit ticks have grace; later ticks subtract 20 health. Both
conditions together still charge damage only once. Sufficient resources AND
local light reset counters and recover 10 health per tick, capped at 100.
Restoring supply during grace stops the deficit; surviving damaged tissue can
recover. Dead tissue has no intake, upkeep or growth and cannot recover.

`Config` names every rate, threshold and limit and carries a caller-assigned
configuration identity. Observations include the complete config, seed, policy
identity, health, size, stores, allocation, local deficit counters, connections,
and ledger. The engine makes no stochastic decisions; policies can use the
existing `seeded_rng` and recorded seed. Policy identity is fixed at seeding and
inherited by every descendant, including disconnected components. Split
components retain their original organism lineage; they are not new seeds.

## Tick order and conservation

1. Reject invalid ticks, excessive intent counts and out-of-range host samples
   before mutation. Sample each living tissue exactly once.
2. Reset the tick ledger and normalize component stores.
3. Collect passive intake into each connected living component, then clamp to its
   aggregate capacity. Record discarded overflow before paying maintenance.
4. Charge every living element's role/size upkeep, in descending allocation
   priority then ascending tissue ID. Pay available resources independently;
   unmet amounts are recorded and mark that element short of resources.
5. Update local shade, shortage, health and recovery. Partition remaining stores
   using the pre-death capacities; discard each newly dead element's share.
   Recompute living components without transferring across dead bridges.
6. Validate and commit growth/allocation intents in order. New tissue first
   collects resources and pays maintenance on the NEXT tick. Growth into shade
   is permitted and starts its own deficit counter next tick.

A component's bounded pool is represented by canonical shares on its tissues.
Each resource is partitioned proportional to living role/size capacity, rounding
down, then assigning the remainder in ascending ID order without exceeding any
capacity. This avoids duplicating stores when connections disappear. `sever`
normalizes before disconnecting and preserves those shares exactly. `remove`
discards the removed share. `resize_storage` only permits shrinkage, normalizes
before shrinking, then clamps the remaining component pool to the smaller
capacity and records overflow loss. It cannot create free larger tissue.
These host topology operations occur between ticks, and their losses appear
immediately in the current ledger; consume that observation before the next
ledger reset. Dead tissue retains its cell until explicitly removed.

For a tick without external topology operations:

`stores_before + intake = stores_after + paid_maintenance + growth + loss`.

`unmet` is diagnostic demand, not spent resources. The ledger exposes aggregate
intake, paid upkeep, unmet costs, growth, loss, per-intent outcomes and per-tissue
idle/shortage/shade/dead reasons. Costs per element are its config rate × size.

## Bounds and verification

Defaults permit 256 tissues (including dead ones), 64 organism lineages, 16
intents per tick and size <=16. Hard ceilings allow at most 1024 tissues/intents;
resource parameters are bounded at one million units and size is a `u16`.
Grace must be below `u32::MAX` so saturating counters eventually exceed it.
IDs monotonically increase and are not reused. Each growth adds one edge, so
there are fewer than N edges, no cycles or hidden unbounded graph inputs.
A tick performs at most N exposure queries and I space/support checks. Graph
walks and normalization cost O((I+1)N²), storage is O(N+I). The configured count
and batch limits bound the entire synchronous step; work cannot defer upkeep.
Adapters must themselves bound each query's work. No wall-clock deadlines or
frame-dependent decisions are used.

Non-ignored `plants::` fixtures cover:

- `maintenance_without_growth_and_sustainable_paid_growth`: idle upkeep, paid
  growth, delayed first upkeep and ledger conservation.
- `each_resource_independently_limits_and_recovers_before_death`: independent
  nutrient/water/energy shortage, progressive damage and recovery.
- `underground_roots_survive_but_attached_local_shade_dies_and_can_recover` and
  `shade_extension_is_allowed_and_whole_darkness_has_bounded_survival`: roots,
  attached shade, recovery, permitted shade extension, total death/no revival.
- `rejection_is_atomic_*`, `tick_rejections_*`: full-state rejection comparisons,
  conflicting growth, space/support/cost/size/adjacency validation.
- `severing_*`, `unequal_capacity_*`, `death_of_bridge_*`: partition, policy
  preservation, overflow/removal/death conservation and disconnected intake.
- `work_counts_*`, `role_size_*`, `brain_observation_*`: bounds, per-element bills,
  immutable brain contract and bounded host queries.
- `paused_zero_duplicate_and_seeded_replay_ignore_render_schedule`: pause,
  zero-duration, duplicate tick rejection and complete seeded lifecycle/ledger
  replay across ordinary and accelerated batches.

The foundation fixtures use controlled host inputs. Soil/raycast integration,
a concrete grass brain, visible lab, ShipBrain and tree stability remain separate
issues. Candidate acceptance also requires the registered foundation regression,
formatting, whitespace and nextest plants-contract checks and independent review.

## Renewable dirt contact adapter (#209)

`engine_rapier::root_uptake::RootUptake` is a borrowed environmental view of one
canonical `TerrainAssembly`, terrain, derived geometry and `PhysicsWorld`.
A plant instance uses that support's local integer cell frame. Absorbing roots
are lightweight points at terrain cell centers, independent of tissue size
(size scales physiological capacity and intake, not geometric radius). The
point must lie in canonical solid geometry whose source material equals the
explicit `dirt` material ID. Air, proximity, other materials and VOID do not
qualify. No neighbouring-cell search or face count multiplies intake.

Use `TerrainAssembly::plant_support()` when seeding or growing attached tissue.
It includes the canonical rigid body's slot and generation, so removing and
replacing a body under the same scenario ID invalidates old attachments. The
adapter checks that identity, body liveness, current terrain geometry, surface
mode and synchronized assembly revisions on every query. Refresh geometry and
synchronize colliders after edits before the next tick. An unsynchronized edit
fails closed. Translation and rotation preserve support-local contact; removed
support, material or tissue affects the next physiology boundary. Growth first
samples on the following tick. Hosts must not reuse an attachment across worlds.

Rates and availability are independently configurable 0..1000 fractions for
nutrients and water. `Config::root_intake_per_size` specifies the maximum units
per size per simulation second for this adapter. A fixed `tick_ms` of 1..1000
specifies physiology duration. Effective fraction is
`floor(rate * availability * tick_ms / 1_000_000)`; the runtime then floors
`maximum * size * fraction / 1000`. There is no fractional carry: choose a
fixed duration and rates large enough for the intended integer supply. The
1000 ms default gives the foundation's 4 nutrient / 3 water allowance. Retune
maintenance and grace if selecting another duration. Render schedules never
choose duration or issue extra ticks; acceleration batches the same fixed ticks.
Invalid controls reject atomically. Pause and zero elapsed ticks are no-ops.
Queries are observational and the runtime samples each living root once; stale
tick submissions reject before mutation. The substrate is renewable and never
loses material or gains unused ecology fields.

`contact()` distinguishes eligible, dead, non-root, missing/mismatched support,
stale geometry, air and ineligible material. Ledger `roots` records each living
root's eligible contact, offered intake, accepted storage and overflow in tissue
ID order. Zero accepted intake distinguishes no contact, supply/rounding and
capacity. The existing aggregate `intake` remains offered intake; discarded
amounts remain in `loss`, preserving the original conservation equation.
Acceptance is into the connected component before maintenance. No energy is
supplied by this adapter; hosts compose shoot lighting separately. Dead roots
are absent from intake ledgers and remain inspectable through `contact()`.

Non-ignored `root_uptake_contract` fixtures exercise actual terrain assemblies:
positive/air/non-dirt/dead contacts, transformed and replaced supports, terrain
removal and stale synchronization, query/tick duplication, pause, independent
shortage damage/recovery/death, growth timing, invalid controls, duration scaling
and 1000-second renewable bounded accounting across ordinary/accelerated batches.
Long runs provide a finite energy endowment sufficient for the test duration.
This adapter advances the environment milestone toward Garden Lab; plant light
integration, grass policy, lab presentation and ordinary gameplay remain later
issues.
