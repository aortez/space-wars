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
cell from the parent's living component; `Shed` drops a tissue and what grew
from it (see Wood below). The runtime checks size, living parent,
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

## Directional light adapter (#210)

`engine_rapier::plant_light::PlantLight::publish` composes measured overhead
illumination with `RootUptake`, without changing the tissue ledger or physiology.
This supplies the light input for the environment milestone toward Garden Lab;
it does not wire Garden/Clock, implement grass policy or add localized lamps.
`full_sun(direction)` supplies 1000; `dim_room(direction)` supplies 50 on the
same linear scale. Direction is a unit world-space vector toward the source,
provided independently of gravity and camera. Both presets use identical
geometry and accounting. Incident exposure is floored to the runtime's integer
0..1000 scale without multiplying by tick duration. Shade compares that measured
intensity with `local_light_threshold`, so full sun remains 1000 at subsecond
ticks and the shade threshold needs no duration adjustment.

Shoot energy retains the foundation's per-logical-tick units:
`floor(shoot_energy_per_size * size * light / 1000)` abstract chemical-energy
units offered each completed plant tick. The coefficient is not a per-second
rate and the adapter applies no additional duration multiplier. With the default
coefficient 8 and size 1, full sun offers 8 and dim room floors to 0 units per
tick. Hosts choosing a different fixed physiology duration must tune per-tick
energy, maintenance and grace coherently for their intended simulation speed;
incident intensity and shade classification remain unchanged. Root nutrient and
water supply retain their independent per-second controls and `tick_ms` scaling
from #209; this adapter does not change those inputs or ledger accounting.

Pass the complete tissue set for one support, across all its organisms, to one
publication and one publisher per simulation boundary. Use
`publish_with_occluders` to also supply `WorldOccluders`: world-space disks from
all other relevant supports, tagged with this exact tick and geometry revision.
`publish` is the convenience form for scenes with no external lightweight
occluders. At most 256 local tissues plus external disks combined (including
local roots and dead elements) are admitted atomically, before allocation or
query work. External disks do not add receivers or root supply. Shoot, runner
and wood elements declare opaque world-space disks:
center = support pose applied to terrain cell center; radius =
`cell_size * 0.25 * sqrt(size)`. Roots have no optical footprint. These declared
disks are the initial blade/leaf/structural approximation, not colliders or a
second physics world. Dead above-ground tissue keeps shading until removed.
`publish_with_probes` also takes the share of the light each tissue's disk
lets through. A share of 0, which `publish` and `publish_with_occluders` give
every tissue, keeps a disk opaque; one below 1 lets that much through, and a
ray keeps the product of the shares it crosses
([translucent disks](lighting.md#translucent-disks)). A share of 1 or more,
below 0 or not a number is invalid input.
All attached elements must name the assembly's current generational support.
A caller-reserved surface namespace plus never-reused tissue ID identifies each
disk; reserve that namespace from physical entity IDs and other tissue sets.

Each living Shoot samples once at its disk's source-facing boundary. Only its
own local boundary contact is exempt under the shared 0.001-world-unit endpoint
policy. Neighboring tissue, even in the same organism, blocks. Coincident disks
with distinct identities block one another. A receiver embedded in an ordinary
solid remains blocked; no organism-wide exclusion exists. Sensors are ignored;
all other canonical solids (including terrain) block. This initial adapter has
no transparent physical override. Physical water represented as a collider is
therefore opaque here; hosts needing transparent water require a scoped policy
extension rather than silently excluding an entity.

Complete terrain edits, assembly synchronization and the normal canonical step
before publishing. The adapter copies the canonical query snapshot and derives
tissue transforms from that same borrowed world. Its borrowed root view prevents
world/support/terrain mutation until the view is released. Supply a monotonic
geometry revision changing with every optical movement, source/shape change or
removal, and the completed simulation tick. Publisher tick monotonicity prevents
repeated publication through the same publisher. Do not create publishers per
batch or partition a support into independently capped sets. Each receiving
support's publication must include every relevant external lightweight disk;
external supports need no tissue colliders. The external simulation owner must
transform disk centers using those supports' authoritative canonical poses,
retain the declared radii and stable surface IDs, and rebuild the disk list
after motion, tissue removal or support replacement. Supply the matching tick
and geometry revision in `WorldOccluders`; stale tags reject before queries.
Tags assert host publication ordering, not an automatic detector of stale
caller geometry. The adapter copies the supplied disks for the query and
validates their finite centers, positive finite radii and unique identities
across local and external geometry. Never resubmit the receiving support's
own disks in the external list. Root inputs and receiver transforms remain
local to the receiving support; external tissue from any organism still blocks.

Query batches are at most 64 receivers, with at most 256 rays total, 65536 disk
tests and 524288 collider-record visits per publication; shared canonical shape
complexity admission also applies. No render work, stagger or persistent exposure
cache exists. Publish every physiology boundary after growth, removal or motion;
new growth first samples on the following tick. `environment(tick)` requires
exact tick equality (maximum exposure age zero), stricter than shared lighting's
one-tick age. Pass the returned environment to that matching plant tick only.
Repeated reads perform no new rays. Diagnostics retain intensity, blocked/ray
counts, first blocker, total work, tick and geometry revision. Changed Shoot
records reject through the existing invalid-exposure sentinel, before physiology
mutation; stale access, overflow, duplicate identity and invalid sources return
explicit `LightingError`s. Failed admission leaves the publisher unchanged.

Non-ignored `plant_light_contract` fixtures cover canonical solids/terrain,
sensors, removal, own-surface contact, same-organism and overlapping shade,
transformed support/source frames, deterministic repeated publication, aggregate
batch caps, invalid/stale inputs and actual preset energy intake with unchanged
root supply. Two-support fixtures also prove collider-free external shade,
blocker diagnostics, support translation/rotation and tissue-removal refresh,
fixed-sequence replay, aggregate local/external overflow, external identity
conflicts and publication-tag validation. These are headless adapter claims,
not visible host acceptance.

## Grass policy (#211)

`engine_core::plants::grass` adds the inspectable P3 policy toward the Garden Lab
milestone. Policy identity is `0x6772617373000001` (grass version 1). `Traits`
identity 1 defaults to a four-cell Chebyshev spread radius, 32 total tissues,
size-two absorbing roots, size-one blades/runners, one growth opportunity per
fixed tick (`growth_interval_ticks = 1`), and a finite `[30,30,30]`
planting endowment. Retain the full `Grass` value, including trait identity,
origin, support and local up, with host replay inputs. Traits select geometry
and initial planting reserves; they never replace `Config` costs or survival.
`plant()` is an explicit host planting action, not part of policy updates. It
creates one root representing the planted germinating seed. Reproduction and
seed dispersal are absent. Dead patches never call planting automatically.

The policy consumes the shared owned `Observation` and a borrowed environment,
returning a `Plan` of shared `Intent`s. It prioritizes establishing blades, then
absorbing roots below runners, then lateral runner expansion, breaking ties in
ascending tissue ID and positive/negative tangent order. It purchases at most
one adjacent tissue per tick. All living components spend their own canonical
shares; no lineage-wide pool crosses a severed edge. Each candidate records
parent, target role/size/cell, available component resources, configured cost
and an occupied/size/spread/space/support/population/work/resource/proposed limit.
A slower configured growth cadence records `Plan::waiting = Cadence` without
skipping physiology or maintenance.
The authoritative ledger supplies the final post-maintenance outcome. Resource
estimates precede that tick's intake and upkeep, so a proposal can still be
rejected after upkeep. Each failed candidate remains inspectable. The policy
never screens candidates by light: blades can enter shade and acquire the
runtime's local deficit independently of their sunlit neighbors.

`engine_rapier::grass::GrassEnvironment` composes the measured physiology view
with stricter grass placement. Roots AND runners require actual solid dirt at
the cell center. Blades require air within the terrain domain. Tissue is still
lightweight; no per-cell rigid bodies are created. `local_up()` rotates actual
world gravity into the canonical support's frame, then selects its dominant
cardinal axis (ties choose Y). It rejects zero/nonfinite gravity. Construct or
refresh the policy's up from this input when host gravity relative to support
changes. The tangent is perpendicular to that local up, not screen horizontal.

Host boundary order is: finish canonical movement/terrain synchronization;
apply cuts/crushing via `Plants::damage` and explicit debris removal via
`remove`; call `detach_invalid` against current grass placement/support; observe
the remaining complete support-local tissue set; publish `PlantLight` once;
wrap its matching environment with `GrassEnvironment`; plan and submit one
fixed `Plants::tick`. For external lightweight occluders use the existing
`publish_with_occluders` combined admission, never separate capped publications.
The host must reserve the namespace and monotonically tag geometry revisions.
Pause/zero elapsed time skips publication and physiology entirely. Accelerated
viewing repeats these same boundaries. Fixtures use 1000 ms ticks, unchanged
foundation costs/intake coefficients, and actual root/light inputs: size-two
roots offer up to 8 nutrients/6 water per tick; size-one full-sun blades offer
8 energy, while dim-room blades offer zero after integer flooring. Starting
reserves are never topped up by the policy.

Damage accepts 0..100 health loss. Killed tissue loses its canonical resource
share and ceases intake, maintenance and growth. Its occupied cell and optical
footprint persist until explicit removal. Surviving health can recover through
ordinary physiology; removed blades can be repurchased from living remnants.
The [tissue integrity](design/material-tissue.md) adapter keeps mechanical
integrity under the material law apart from health.
`detach_invalid` removes cells whose support is lost/stale or whose current
substrate/space is invalid, discarding their existing shares. Surviving fragments
retain their original policy/lineage and component-local resources. A severed
shoot can use only its retained nutrients/water and actual light; disconnection
does not grant new soil supply. Hosts must consume the immediate topology-loss
ledger before the next tick resets it.

Planning visits at most four candidate sites per living tissue and proposes
at most one intent. Component pools are built once using bounded graph walks;
site scans cost O(N²), and storage is O(N) plus the at-most-4N decision trace.
Runtime's stricter configured population/intent limits still apply. Lighting's
256 combined local/external admission and publication-wide ray/geometry ceilings
remain unchanged and failures must propagate to the host. Do not use multiple
publishers to bypass those limits. The grass default of 32 tissues leaves room
for other admitted optical geometry but is not a reservation for it.

Non-ignored `grass_policy_contract` fixtures cover paid topology, policy identity,
exact fixed-seed replay, population/decision limits, pause/zero behavior,
inspectable resource/spread limits, sever conservation, invalid damage atomicity
and sustained no-revival after total death. `grass_growth_contract` fixtures use
canonical terrain and directional lighting for multi-shoot runner spread,
translated/rotated support equivalence, independent nutrient/water/light limits,
actual roof shade expansion and local death with healthy sunlit tissue,
restoration before death, underground shade exemption, paid cutting/regrowth,
query ceilings and support-loss accounting. These are deterministic headless
claims. Garden Lab visuals, device captures, reproduction and evolution remain
outside #211; no visible presentation or device performance is established here.

## Paid shoot development runtime (issue #472 checkpoint)

`Intent::Develop { tissue }` purchases at most one unit of each still-needed
resource toward the next shoot size. `Tissue::development` records the paid
N/W/E balance toward that increment, whose target is the configured shoot
`development_per_size`. `Tissue::invested` records lifetime development spending;
this is a diagnostic of resources already spent, not spendable stores. Both
start at zero on newly constructed tissue. Completing the target resets only
the current increment balance and atomically raises size by one, up to size
three and the configured size ceiling. Every payment appears once in the
existing growth ledger. Construction costs remain separate.

Development executes after upkeep and requires a living shoot with no current
resource or local light deficit, valid support, space for the next size and
sufficient funds in its own living component. Refused intents change neither
progress nor spending. Population/spread restrictions on new sites do not
restrict this operation. Identity, edges, health and deficit history survive
enlargement; expanded capacity receives only the unspent existing pool. Intake,
maintenance and light geometry use the new size starting at the next physiology
boundary. Work remains bounded by `max_intents`; one development intent pays
at most three resource units. No additional light sample is taken for admission.
Removal discards paid work with the tissue and does not refund it.

Grass profile v2 (`0x6772617373000002`, trait/config identity 2) uses
`grass::physiology()`: root uptake [8,8,0] per size per one-second tick and
incremental development target [10,10,10] per added shoot size. Construction
retains the default role costs. Roots remain size two, runners and new shoots
size one; finite seed reserves remain [30,30,30]. Generic runtime defaults
remain distinct from this selected grass profile.

The policy prioritizes eligible root sites, shoot sites, then runner sites.
When no new site can be proposed it develops the least-invested eligible shoot,
using ID order to break ties. One development proposal pays one unit of each
resource toward its target; 20 admitted proposals mature a blade. There is no
age or establishment timer. Tissue/spread caps restrict new sites while still
allowing development. Every candidate remains advisory until post-upkeep
runtime admission. A shoot with a previous deficit is reconsidered after its
physiology recovers; local shade cannot be funded by another shoot.

`remove_many` validates the entire removal set before mutation, normalizes once,
and records all pre-cut shares before redistributing survivor stores. It makes
cut losses independent of removal order. Surviving IDs, health, topology among
survivors and investment are retained; canonical store rounding can redistribute
individual survivor shares without altering their total.

## Moss organism (#213)

`Role::Moss` is a fourth runtime role: thin photosynthetic surface tissue with
capped passive supply from an eligible surface instead of roots. The role-ordered
`Config` tables now hold root, runner, shoot and moss entries. Moss stays in the
ordinary ledger and tick order: intake, upkeep, local deficit, growth. Defaults,
per size per fixed one-second tick, compared with the grass shoot baseline:

| Quantity | Moss | Grass shoot |
| --- | --- | --- |
| Capacity, each resource | 24 | 20 |
| Maintenance (N,W,E) | 1,1,1 | 1,2,2 |
| Construction (N,W,E) | 20,20,20 | 4,4,4 |
| Passive supply (N,W) at full availability | 2,2 from surface contact | none; roots supply 8,8 per size (profile v3) |
| Energy at the dim room (50) | 2 | 0 |
| Energy in full sun (1000) | 2 | 8 |
| Local light threshold | 25 | 250 |

Moss energy saturates: `floor(moss_energy_per_size × size × min(light, s) / s)`
with saturation `s = 50`, so the dim room already yields its full two units and
full sun yields no more. One dim-room moss cell therefore nets one unit of each
resource per tick, while a dim shoot earns nothing and accumulates a shade
deficit. Darkness remains fatal: light below 25 starts the same grace/damage
counter as grass, even while connected lit moss holds plenty of energy, and
restoring light before death allows recovery. These are tuning defaults, not
measured biology. The profile values come from `Config::default()`, which
`grass::physiology()` (identity 3) inherits, so one runtime can hold both.

`Exposure::surface_contact` carries moss eligibility; `dirt_contact` remains
root-only. `Ledger::surfaces` records each living moss tissue's contact supply
(eligibility, offered, accepted, overflow and zero reasons) separately from
`Ledger::roots`; the shared record type is now `ContactIntake`. Contact supply
never includes energy. Dead, detached or out-of-contact moss receives none.

### Surface contact and placement

`engine_rapier::moss::contact` treats a moss cell as a point at its
support-local cell center. It is in contact when that center lies outside
canonical solid geometry and any of the eight neighboring cell centers lies
inside it, of any material: dirt, rock or other solids. Corner contact lets a
patch wrap convex edges; nothing bridges a wider air gap. One check per tissue
supplies at most one intake, however many neighbors touch. A cell inside solid
reports `Enclosed`, a cell with no solid neighbor `Air`. `RootUptake::contact`
and `MaterialUptake::contact` are now role-aware: roots keep #209's dirt-only
rule (rock stays `Ineligible`), shoots and runners report `NotAbsorbing`, and
moss uses the surface rule. `moss::site` additionally requires the finite terrain
domain; `GrassEnvironment`, `RootUptake` and `MaterialUptake` all use it for
`Role::Moss` placement, so mixed grass/moss patches validate together.

Cells are support-local, and the runtime's occupancy and `space_available`
checks carry no support. Give each separately moving support its own `Plants`
runtime, light publisher and namespace. Translation and rotation preserve
contact; a terrain edit must be refreshed and synchronized before
`detach_invalid`, which then removes moss whose surface or support is gone and
discards its shares. Removing the support body detaches the whole patch. Dead
moss keeps its cell until removed, as other dead tissue does.

### Light

Living moss samples light at its own cell center with no surface exemption and
declares no opaque disk: a thin mat shades nothing, while shoots and runners keep
their disks. Moss under its own support, an overhang or the lab shade is
blocked like any receiver. `plant_light::footprints` returns the world-space disks
a publication declares for a support's tissues, so a host can pass them to other
supports' publications as `WorldOccluders`; roots and moss contribute none.

### Policy

`moss::POLICY` is `0x6d6f737300000001`. `Traits` identity 1 defaults to one growth
opportunity per four ticks, a 24-cell patch limit (living and dead cells of this
policy on its support) and finite placement reserves `[24,24,24]`. Every element
is size one. `Moss::place` is the deliberate host action that seeds one cell with
those reserves as a new organism; the policy never plants, and dead patches are
never revived. `plan` walks living tissue in ID order. Shaded, short or damaged
tissue records a `Shade`, `Shortage` or new `Damage` limit and waits for its own
recovery instead of spending shared reserves. Healthy tissue inspects the four
side neighbors in a fixed support-frame order (+x, -x, +y, -y) and records the
first refusal: population, size, work, occupied, support, space (no free
surface-contact cell) or resource. The first affordable site becomes the single
`Grow` intent. At most four decisions per tissue are recorded. Growth and the
runtime's Manhattan adjacency keep each addition connected and paid from that
component's own pool; component pools are shared helpers with grass.
`Plan`, `Decision` and `Limit` now live in `plants`; `grass` uses them unchanged.
The grass trait population limit counts only that grass policy's tissue on its
support, so another species in the same runtime only consumes the shared
runtime ceiling.

Fixed seeds, configurations and action sequences replay exactly. Pause and zero
elapsed ticks charge nothing, as for every role.

### Measured fixture timings

`moss_surface_contract` fixtures use canonical terrain, `PlantLight`, the
`grass::physiology()` profile and 1000 ms ticks. A dim-room patch placed on a
16×8 dirt bed beside a two-by-two rock pillar fills all 18 eligible surface cells
(dirt surface, pillar side, corner and top) by second **77**, paying 17 × 20 per
resource. On the same bed with grass planted beside it, after 200 seconds:

| Light | Living grass tissue | Living moss tissue |
| --- | --- | --- |
| Full sun | 32 | 8 |
| Dim room (50) | 0 | 10 |
| Darkness | 0 | 0 |

In full sun grass claims most surface cells first; moss is space-limited, not
light-limited.

Non-ignored fixtures cover: contact reasons for dirt, rock, pillar sides and
corners, air, enclosed and dead tissue, with rock still rejected for roots;
dim-light contiguous paid spread and replay; the grass/moss light comparison;
a round rock with a smoothed contour surface, whose patch follows the curve down
both flanks;
a free-standing rock translated and turned upside down, where moss keeps its
contact while the old top face loses light to its own support and dies back;
terrain breakage with stale-geometry refusal, detachment, conserved loss and
paid recolonization of exposed rock; removal of the support body; and a killed
bridge that splits a patch, damaged parents waiting, and paid reclamation of the
cleared cell. `moss_policy_contract` covers the headless policy and runtime
accounting. Clock moss on digits (#223), succession on the retained Clock floor
(#518), spores, seeds and fragment dispersal remain separate work.

## Wood (#743)

`Role::Wood` is a fifth runtime role, for a tree's trunk and branches, and the
role-ordered `Config` tables now hold a wood entry. Wood gathers nothing: it
takes no contact supply and no energy, and it has no local light threshold, so
it lives on its component's pool in shade or darkness. It pays upkeep as other
tissue does, its rate times its size, and its capacity stores reserves at the
same scale, so full reserves last as long at every size. Defaults, per size per
fixed one-second tick:

| Quantity | Wood | Grass shoot |
| --- | --- | --- |
| Capacity, each resource | 20 | 20 |
| Maintenance (N,W,E) | 1,1,1 | 1,2,2 |
| Construction (N,W,E) | 2,2,6 | 4,4,4 |
| Intake | none | energy from light |
| Local light threshold | none | 250 |

Building wood costs mostly energy. Wood thickens through `Intent::Develop`, as
a blade matures: each intent pays at most one unit of each resource the step
still needs, and a completed step raises its size by one, up to `max_size`. Its
target per step is its own construction rate rather than
`development_per_size`, so a segment thickened to size n has cost what building
it at size n would have. At the defaults a step takes six intents: the first
two pay all three resources and the other four energy alone.
`Config::development_target` gives a tissue's target for its next size, or
`None` when it cannot develop: shoots and cones stop at size three, and roots,
runners and moss never develop.

Wood is placed as blades are, in air and never in solid geometry, under
`GrassEnvironment`, `RootUptake` and `MaterialUptake` alike. Root and moss
contact report it `NotAbsorbing`. In `PlantLight` it declares an opaque disk by
the shoots' and runners' rule, radius `cell_size * 0.25 * sqrt(size)`, so it
shades what lies behind it and widens as it thickens. It is not a receiver, and
its exposure carries no light. `footprints` includes it for other supports'
publications. Dead wood keeps its cell and its disk until removed, as other
dead tissue does.

`Intent::Shed` lets a brain drop a tissue, living or dead, with everything that
grew from it, as a tree drops a bare branch with its dead needles. It applies
to every role. Growth always joins a new, higher ID to its parent, so the
runtime finds what grew from a tissue by walking only to higher IDs. Shedding
removes them as `remove_many` does, and their shares of the stores are lost, in
that tick's `loss`. A tissue with no parent, such as a seed or the base of a
severed fragment, cannot be shed, so a brain can never drop its whole plant.

In the plant runtime, wood stays lightweight tissue on cells, with no bodies,
colliders or loads. Garden Lab's tree bed can hang its trees' wood on hinges as
bodies while they grow, their cells the shape the wood rests in
([Physical wood](garden-lab.md#physical-wood-224)); anchorage is #225's.
`engine-material`'s `TissueTable` gains a wood entry in the same role order.
The grass and moss policies act only on their own tissue, so wood in a shared
runtime counts only toward the runtime's ceilings.

Non-ignored `plants::` fixtures cover wood's zero intake and its lack of a
shade deficit in darkness, its upkeep by size and how long full reserves last,
thickening's payments per intent, the reset at each completed step, the
investment equal to construction at that size, the capacity at each size and
the size limit, and the development targets by role and size. They also shed
a branch with its dead needle and check the lost share, the refusals for a seed
and for unknown tissue, and a dead needle shed in a balanced tick.
`plant_light_contract` covers a blade shaded by wood above it, wood's missing
sample, its disk radius by size and its placement in air.

## Cones and seeds (#760)

`Role::Cone` is a sixth runtime role, for a tree's seed-bearing tissue, and the
role-ordered `Config` tables hold a cone entry after wood's. A cone gathers
nothing and has no local light threshold, so it lives on its component's pool.
It ripens as a shoot matures, through `Intent::Develop` toward
`development_per_size` at each step, and a cone of size three is ripe.
Defaults, per size per tick:

| Quantity | Cone |
| --- | --- |
| Capacity, each resource | 10 |
| Maintenance (N,W,E) | 1,1,1 |
| Construction (N,W,E) | 2,2,4 |
| Intake | none |
| Local light threshold | none |

`Intent::Release` drops a ripe cone with its seed. The cone must be living and
ripe, with nothing grown from it. It leaves the plant, and its share of the
stores stays with the tissue it grew on. The seed starts a new organism of the
cone's policy at the cell the intent names: one tissue of the given role and
size, holding the given stores, which the cone's connected stores pay. Nothing
else is charged, since the cone paid for the seed's tissue as it ripened. The
runtime checks the seed's cell as it checks growth, and checks the organism
limit, that the stores fit the seed's capacity, and that the cone's pool can
pay them. A refused release changes nothing. The runtime does not trace the
seed's fall; the policy that releases it chooses the cell.

`Observation::parents` maps each organism grown from a released seed to the
organism whose cone dropped it. An entry lasts while its organism has tissue,
living or dead, and goes with the last of it. Planted organisms have no entry.
`Plants::seed` is a host's trusted endowment from outside the plants; a release
creates nothing, since the seed's reserves leave its parent's pool, and a tick
with releases balances as any other.

Every adapter places a cone in air, as it places wood. Root and moss contact
report it `NotAbsorbing`, and `PlantLight` gives it no disk and no sample, so
it casts no shadow. The grass and moss policies act only on their own tissue.

Non-ignored `plants::` fixtures drop a ripe cone and check the refusals: for an
unripe cone, a tissue that is not a cone, an occupied cell, stores beyond the
seed's capacity or the pool, and the organism limit. They check the seed's
organism, policy, role, cell and stores; the cone gone with its links; the
tree's stores less exactly the seed's, with nothing lost; and the lineage,
which outlives no tissue, while the seed itself cannot be shed. A cone with
tissue grown from it does not drop, and a tick with a release balances.

## Tree (#743)

`engine_core::plants::tree` grows a conifer in the shared runtime, under the
rules grass lives by. Its roots gather from dirt as grass roots do, its needles
are shoots that earn energy under the same light rule and die back in shade,
its trunk and branches are wood, and every tissue pays upkeep from its
component's pool. The policy is `0x7472656500000001` (tree version 1).
`Tree::plant` seeds one size-two root at the origin with `[40,40,40]`, as a
host action; the policy never plants. Grown, the tree hangs cones that drop
seeds, and a `Grove` takes each seed in as a tree of its own ([Seeds and the
grove](#seeds-and-the-grove-760)). Garden Lab's
[tree bed](garden-lab.md#tree-bed-743) shows the conifer growing beside grass,
with each point's bid.

The tree's wood and needles let part of the light crossing them through:
`Traits::wood_transmits` and `needle_transmits`, in thousandths, default to
750 and 600, since a branch takes a little of the light and a cluster of
needles less than half. `Tree::transmits` gives a host each tissue's share for
`PlantLight::publish_with_probes`; another plant's tissue gets 0 and stays
opaque. A needle under one tier keeps 1000 × 0.75 × 0.6 = 450 of full sun,
enough to grow there, and one under two tiers 202, which is shade.

### Growth points and bids

At each decision, every tissue of the tree is a growth point and bids, living
or dead, affordable or not. A brain sees one point's `View` and returns a
`Bid`, so the hand-written `Conifer` and a later trained model use the same
interface. The view holds:

- **The whole tree:** living stores and capacity, this tick's intake and
  upkeep, counts of short tissue, shaded needles, needles, foliage (the summed
  sizes of living needles), wood, roots and dead tissue, the trait's tissue
  limit, the highest wood and deepest root, and its living cones and
  offspring with their limits.
- **The point:** its kind (leader, trunk, branch tip, branch, root tip, root,
  needle or cone, from where it sits), whether it lives and whether it grew from
  another tissue, its height along up and offset across from the trunk's axis,
  size, health, shade and shortage ticks, a needle's light, the needles,
  foliage and dead tissue beyond it, what wood carries and the headroom on the
  way back to the seed, and its paid development and target.
- **Four candidates,** its neighbors up, down and to either side in the tree's
  frame. Each says which way the step goes, what holds the cell (nothing, the
  tree's own wood, needle, root or cone, or another plant), whether the world
  would place wood, a needle, a root or a cone there now before cost, the light
  the host measured there and the supply a root would take there.

A bid is an action, `Wait`, `Grow(role)`, `Thicken`, `Shed` or `Release`, with
a priority and a ranking of the candidates. The world takes the bids from the
highest priority down, ties to the lower tissue ID, and refuses one when:

- growth or thickening comes from a dead point;
- the point cannot grow that role, or the tree, its cones, its offspring or
  the world's organisms are at their limit;
- no ranked candidate is open, or every open one was claimed earlier in the
  decision;
- a seed would sprout into a cell lit less than `sprout_light` or not
  measured, or from dirt watered less than `sprout_water`;
- a cone is unripe, or the wind would carry its seed where it cannot lie;
- a new tissue would pass the runtime's size limit, or the point cannot
  develop further;
- a thicker segment would have no room;
- the point is shaded or short, for thickening;
- the wood behind a new or maturing needle cannot carry it;
- the bid sheds the seed, or a higher bid has already shed the point;
- the component's pool, less what higher bids spent, cannot pay.

A refused bid takes nothing, and lower bids still get their turn. Each winning
bid becomes one ordinary intent, and once `bids_per_decision` (default 4) have
won, the rest are outbid. Every view, bid and outcome is recorded in the
`Round`, for inspection and training. Costs here are advisory; the runtime's
own admission after upkeep is final.

The world's rules for what grows: needles and cones grow nothing, roots grow
only roots, only the seed starts a trunk, and wood grows wood, needles and
cones. `Thicken` pays toward a wood segment's next size, matures a needle or
ripens a cone. `Shed` drops a point with everything beyond it; it costs nothing
but takes one of the decision's bids. `Release` drops a ripe cone with its
seed.

As in the pipe model, a stem's cross-section follows the foliage it feeds: each
size of wood supplies `foliage_per_wood_size` (default 6) sizes of living
needles beyond it. A new or maturing needle needs headroom in every wood
segment back to the seed, so the tree must thicken its trunk and branches to
grow its crown, and they taper toward the tips.

### Senses

Light at an empty cell and soil supply come through `Senses`, from the host.
`Tree::probes` lists the empty air cells beside the tree's living wood and
roots, at most `MAX_PROBES` (64, one lighting query batch) and rotating with
the tick when there are more. `Grove::probes` joins every tree's in one batch:
an unsprouted seed's cells come first, so a seed always learns its light, and
the rest rotate. A candidate the host did not measure carries `None`.

In `engine-rapier`, `PlantLight::publish_with_probes` also measures those
cells. Each probe samples where a size-one blade in that cell would, adds no
disk and shades nothing, and `PlantLight::probe` reads it for that tick only.
More than 64 probes is a capacity error; a repeated cell or one off the
terrain is invalid input. `RootUptake::soil` gives a dirt cell's supply
fractions, and `None` for other material, air, cells off the terrain or invalid
controls. `TreeSenses` joins the two, and `GrassEnvironment` places the tree's
tissue and runs its physiology, as it does for grass.

### Encoder

`encode` turns a view into `INPUTS` (113) numbers in -1..=1 for a model, under
`ENCODER` version 2, which adds cones and offspring to version 1's 102. A
reading the world did not take is zero beside a zero "known" flag, never
darkness or drought. Ratios with a zero denominator are zero.

| Inputs | Meaning |
| --- | --- |
| 0–2 | Stores over capacity: nutrients, water, energy |
| 3–5 | Intake less upkeep, over upkeep |
| 6, 7 | Short over living tissue; shaded over needles |
| 8–12 | Needles, wood, roots, dead and all tissue, over the tissue limit |
| 13 | Foliage over three times the needles: how mature they are |
| 14, 15 | Height over 32; depth over 16 |
| 16, 17 | Cones over their limit; offspring over theirs |
| 18–25 | The point's kind, one-hot: leader, trunk, branch tip, branch, root tip, root, needle, cone |
| 26, 27 | Alive; attached |
| 28–31 | Height over 32; distance across over 16; size over 16; health over 100 |
| 32, 33 | Shaded; short |
| 34, 35 | Light known; light over 1000 |
| 36–38 | Needles beyond over 64; foliage beyond over three times those needles; dead beyond over 64 |
| 39, 40 | Is wood; foliage beyond over what it carries |
| 41, 42 | Headroom known; headroom over 32 |
| 43, 44 | Can develop; paid development over its target |
| 45 + 17c, for candidates c = 0–3 | Holder one-hot: empty, own wood, own needle, own root, own cone, other (6); open for wood, needle, root, cone (4); step along up and outward (2); light known and light (2); soil known, nutrients and water (3) |

### The hand-written conifer

`Conifer` bids from fixed rules, with its shape in its fields: the first tier
at height 3, a tier every 2 cells, a leader that stops at 14, roots no deeper
than 6, new needles only where the light is at least 400, and shaded needles
dropped after 5 ticks.

- **Start.** The seed starts the trunk straight up, and the trunk grows the
  first needle beside it. Hemmed in on both sides, as among grass blades, the
  leader first climbs a cell, so the first needles stand above the blades.
- **Needles** go into the brightest candidate lit enough, beside the leader
  away from tier heights, above branch wood, and on top of the leader at full
  height, while the wood can carry them. They bid higher when energy runs
  short, and they mature while lit.
- **The leader** climbs only once the needles beside it have matured and the
  energy surplus is at least 6 plus a quarter of upkeep, because climbing adds
  no light and the next leader's needles shade the ones below. At full height
  it tops itself with a needle.
- **Branches.** A tier's sides are kept for branches, which sprout once the
  leader is a tier above and energy has a surplus of 3. A branch tip extends
  outward, lower tiers first, while the surplus lasts, the branch is short of
  its reach, and the cell, if measured, has at least half the needle light. A
  tier reaches one cell, plus one for each tier spacing of trunk above it.
- **Wood** thickens when the foliage beyond it fills what it carries.
- **Roots** grow down or sideways, richest soil first, when water or nutrients
  run short and energy can keep another root. When both are plentiful and
  energy is tight, root tips are dropped.
- **Shedding.** Shaded needles, dead tissue and bare branch tips in the dark
  with nowhere to reach are shed. A bare stub beside the trunk waits instead:
  shed, it would sprout again into the same shade.
- **Cones.** At full height, while energy comes in beyond upkeep and every
  store is at least half full, a branch tip hangs a cone in the open cell
  below it, the highest tip first, while the tree has fewer cones and
  offspring than its limits. A cone ripens while that plenty lasts and drops
  once ripe. The surplus the leader waits for, 6 plus a quarter of upkeep, is
  more than a grown tree under real light keeps: at #761's tick 600 it takes
  in 521 and pays 425, a surplus of 96 against the 112 it would wait for.

In `tree_policy_contract`'s bed, light falls straight down and a cell is lit
only when nothing lies above it, so the needles that live are the ones on the
cone's outer edge, and the shaded ones inside are shed as the tiers above extend
over them. In that 25-cell bed in full sun, the seed reaches full height at tick
386. By tick 600 it has a trunk 14 cells tall, thickened from size 7 at its base
to 2 at the top, branch tiers reaching 6, 5, 4, 3 and 2 cells to each side, 13
mature needles and 16 roots down to depth 5, after 36 sheds. It takes in 312
energy a tick and pays 213.

With `PlantLight` and the tree's shares, the crown lets part of the light
through, and the tree grows fuller. Under a sun 15 degrees off vertical from the
left, as in Garden Lab's tree bed, it reaches full height at tick 570. At tick
600 it has the same tiers and 36 needles: along both sides of the top tiers and
the whole sunny side of the lower ones, and on the shaded side of the lowest two
only at their ends. It takes in 521 energy a tick and pays 425. Under an
overhead sun the conifer stops at 11 cells. There each needle under a tier keeps
450, which pays little more than its own upkeep and that of the wood carrying
it, and the leader never sees the surplus of a quarter of upkeep it waits for. A
brain that weighs a needle's cost in wood could do better.

### Seeds and the grove (#760)

A ripe cone's seed holds the tree's `starting_reserves`, `[40,40,40]`, paid
from the tree's stores. The wind carries it up to `seed_drift` (8) cells to
either side, by a fixed hash of the runtime's seed, the tick and the cone, so a
run replays exactly. It falls straight down through air and plants alike to the
first cell that is not air, and lands only on free dirt; `Tree::landing` gives
the cell. A seed that would land on another plant, a root or anything but dirt
stays in its cone, refused for space, and the next gust tries again. A tree
carries at most `max_cones` (2) living cones, and at most `max_offspring` (3)
organisms from its seeds hold tissue at once; one cleared away no longer counts.

A dropped seed sprouts as a planted one does, by starting its trunk straight
up, but only into a cell lit at least `sprout_light` (400) and from dirt watered
at least `sprout_water` (250). A cell the host did not measure counts as shade.
A seed that cannot sprout lives on its reserves. Its root takes in water and
nutrients but no energy, so its energy runs out in 20 ticks; after the grace of
60 it loses a point of health a tick, and it dies 180 ticks after landing.

A `Grove` holds the trees of this policy on one support: those a host planted,
and those grown from their seeds. `Grove::tend`, after each tick, takes in each
seed a grove tree's cone dropped as a tree with its parent's traits, and clears
away each tree whose tissue has all died, with its tissue. It reports the
organisms it took in and cleared. `Grove::plan` asks every tree in turn, with
one brain, and shares the host's intent budget: each tree gets at most what the
trees before it left. `Grove::owners` maps organisms to their trees, so a host
can give each tissue its tree's share of the light.

In `tree_policy_contract`'s bed, the first cone grows at tick 389. The first
seed lands at tick 436 in column 19, beyond the crown, and sprouts the next
tick. Those that land under the crown are cleared 180 ticks after landing. From
tick 780 the tree grown from that first seed drops seeds of its own, and by
tick 1400 three trees stand.

### Verification

`tree_policy_contract`, in `engine-core`, uses a bed with straight-down light,
in which a cell is dark when wood, a blade or a runner lies above it. Its
tests:

- **The conifer:** the trunk, its taper, the tiers on both sides, the shaded
  needles inside the crown shed (34 of its 36 sheds), every needle left lit and
  nothing dead, the budget per decision, foliage within what wood carries, and
  an exact replay.
- **Limits and recovery:** light at 400, water at 300 or nutrients at 300 for
  300 ticks each hold the seedling to at most a quarter of the wood a tree in
  full supply has by then, and in as many ticks after full supply returns it
  builds at least three quarters of that. Each limit holds it to one segment
  against 40, and it reaches 40 by tick 600.
- **Grass:** a grass patch beside the tree grows in light under the coming
  crown, then dies back in its shade, while grass beyond the crown lives.
- **Cuts:** a top branch cut away in the light grows back to its full reach
  within 200 ticks. The lowest, cut under the crown, comes back as a one-cell
  stub, the only wood the tree grows or sheds in the next 300 ticks.
- **Seeds:** over 1000 ticks the bed's ledger balances every tick while cones
  drop seeds; cones grow only below the branch tips of a grown tree; no tree
  passes its cone or offspring limit; and each seed is a root in the top row
  of dirt, holding the seed's reserves, within the wind's reach of its cone.
  Every seed that sprouts had light enough, and every one refused had too
  little. In dry dirt or dim light the planted seed never sprouts and is
  cleared by tick 300. A grove grown from one seed replays exactly.
- **The interface:** every point bids even when broke, the cadence, priority
  order with claims and the budget, each refusal, the role and foliage rules,
  the views' frame and counts, the probes, and the encoder's layout, ranges
  and unknowns.

`tree_growth_contract`, in `engine-rapier`, grows a grove beside grass on a
terrain support through `PlantLight` with probes, each tree's shares and
`RootUptake`, under a sun 15 degrees from the left. It checks the full cone;
probes read in full sun and through the crown; every kept needle is lit, at
least ten of them through tissue above, and more stand on the sunny side; a
blade dies in the crown's shadow; and the run replays. Its second test follows
the seeds over 1000 ticks: they land only in the top row of free dirt, and the
wind sometimes leaves one in its cone. Those beyond the crown and the grass
sprout at their first decision and grow needles, and one under the crown is
refused for shade at every decision, dies and is cleared. `plant_light_contract`
and `root_uptake_contract` cover probes, translucent tissue and soil readings.
