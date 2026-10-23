# Material damage: strength, deformation and integrity

Revision of the first delivery for the shared material damage epic #325: the bounded reference
model that #326 asked for, as the pure crate `engine-material`, with its
calibration fixtures and evidence. Nothing is integrated into a host yet. The
load adapter (#327), persistent region storage and failure lifecycle (#328),
weapon sources (#329) and the Spacewars and Scorched migrations (#330, #331)
build on the contract below.

The model replaces speed-times-scalar damage with one law: a material has a
stiffness, a safe strength and a toughness; loads below the safe limit cause
zero permanent damage; work beyond it is charged against a finite capacity that is
the material's toughness times its represented volume. Every event has one
energy ledger, so bounce, harmless dissipation, permanent damage and motion
cannot spend the same energy twice.

## What it replaces

Inventory at main `fb310ac`, with each path's physical response, damage
calculation and death or protection policy kept apart. Every current damage
calculation multiplies a speed or an impulse by a tuned scalar, or subtracts a
fixed amount, after the solver has already resolved the motion.

| Path | Physical response | Damage today | Death or protection | Migrates | Where |
| --- | --- | --- | --- | --- | --- |
| Ship against planet body or terrain fragment | Rapier contact at `PLANET_ELASTICITY` | `closing_speed * PLANET_DAMAGE_SCALAR` on a new contact at or above `MIN_DAMAGING_BODY_IMPACT_SPEED`, 30 wu/s | dies at zero life with a death impulse; escape pods ignore it | #330 | `scenarios/spacewars/src/lib.rs` |
| Ship against debris, asteroid or shell | Rapier contact at `DEFAULT_ELASTICITY` | impulse over reduced mass times `debris.damage_scalar` | as above | #330 | `scenarios/spacewars/src/lib.rs` |
| Rover against debris | Rapier contact | impulse over reduced mass times `debris.damage_scalar`, charged in full to both the rover and the debris | dies at zero life | #330 | `scenarios/spacewars/src/lib.rs` |
| Laser, cannon, asteroid chips, breakup | lasers have no contact; shells and chips are Rapier contacts | `LASER_BASE_DAMAGE / beam length`, `CANNON_SHELL_DAMAGE_SCALAR`, a minimum chip damage, fragments at zero | as for ships; a beam's kill keeps a death impulse along it | #329, #330 | `scenarios/spacewars/src/lib.rs` |
| Solar heat | none | percent of `life_max` per second | as for ships | stays: heat | `scenarios/spacewars/src/surface_sortie/solar.rs` |
| Material-match pilot and pod | the vehicle's rigid body | vehicle life is hull health; pilot health with an exposure factor | ejection grants a protection window; a pod's hull shields its occupant through the whole damage step | #330 | `scenarios/spacewars/src/surface_sortie/match_rules.rs`, `recovery.rs` |
| Pizza balls | Rapier contact; clashes rewrite velocities | `hp -= impulse / mass * DAMAGE_SCALAR`, and clash velocity change times `DAMAGE_SCALAR` | invincible balls take none | stays, #59 | `scenarios/pizza/src/lib.rs` |
| Scorched tanks | radial impulse within 6 wu and a `BLAST_RADIUS` crater | `42 * (1 - distance / 6)` within 6 wu | health zero below the floor | #331 | `scenarios/scorched-earth/src/lib.rs`, `material.rs` |
| Terrain cells | chipping and collapse release loose fragments | `durability: u8` against hardness, saturating; chipping gated by `yield_speed`; cantilever resistance `strength * h^2 / 6` | the cell turns void at zero durability | stays, #333 and #16 | `crates/engine-terrain`, `crates/engine-rapier/src/terrain` |
| Plant tissue | none | `health: u16` on 0 to 100 with a per-tick rate and an external `damage` call | dies at zero health | biological upkeep stays | `crates/engine-core/src/plants` |

Rover against debris charges the same full damage to both bodies, the double
allocation this model's ledger rules out. First-wave migrations are Spacewars
actors and weapons (#330), with weapon sources under #329, and Scorched tanks
and artillery (#331). Terrain chipping and collapse stay on their own contract
under #333 and #16, plant biological upkeep stays in the plant lifecycle, solar
heat stays a heat source outside this model, and Pizza keeps its decision
under #59. Consumers that do not exist yet, wood and roots, spacelings' tissue
and bases, start on this law rather than on new heuristics. Death and
protection stay host policy in every path: the model reports `consumed`,
`failed` and integrity, and the host decides what they mean. The terrain
cantilever already uses the rectangular section modulus `strength * h^2 / 6`;
the beam law below uses the same form, so shipped terrain behavior and the new
model agree where they meet.

## Units and geometry

Length is the world unit `wu`, mass is `mu`, time is seconds. Energy is
`mu wu^2 / s^2`. Stress and energy per volume share `mu / (wu s^2)`.

The world is 2D with a represented thickness, the same convention as
`engine-water` and `engine-soil`. A `Section` describes one side of a contact:
its `thickness` into the screen and its `depth`, the effective depth of
material that deforms along the contact normal. A `ContactGeometry` is the
in-plane patch `width` plus the two sections. The load-bearing area is the
width times the thinner thickness, since only overlapping material carries the
contact. A `Beam` is a rectangular connection section with `thickness`,
`height` and `length`: area `t h`, section modulus `t h^2 / 6`, second moment
`t h^3 / 12`, volume `t h L`.

The patch width is an input. `patch_width_from_radii` offers a bounded
approximation, half the effective radius `r_a r_b / (r_a + r_b)` of two curved
surfaces or half the radius against a flat one; it is not a load-dependent
Hertz contact. The deformation depth is an input too, and
`deformation_depth(width, extent)` gives its default: the patch width times
`DEPTH_WIDTH_FRACTION`, which is one, capped by `extent`, how far the material
reaches behind the contact along the normal, such as a plate's thickness, and
uncapped for a half-space such as a planet. Stress spreads below a contact
about as deep as the patch is wide; elastic line-contact solutions put the
effective depth at one to a few patch widths, growing slowly with body size,
so one width is the stiff end of that range. The contact stiffness of a side
is `E * area / depth`, so a pair's safe-limit speed scales with the square root
of its depths.

## Revision decisions and identity

The original unmerged `material-law-v1` used an unlimited yield plateau: the
weaker material could never damage the stronger one, at any speed. Review
selected different behavior:

- Sufficiently energetic weaker objects can damage stronger material.
- Gentle landings should settle with little bounce and no permanent damage.
- Accumulated damage reduces capacity; strength stays constant. Progressive
  fragility is deferred.
- Hosts preserve existing gameplay destruction rules and explicitly account
  for energy carried by surviving bodies/fragments or dissipated by destruction.
  General penetration is outside this slice.

This implementation is `material-law-v2`; its report is
[`material-model-v2.md`](../data/material-model-v2.md) and
[`material-model-v2.json`](../data/material-model-v2.json). The superseded report
is available in Git history. `Material::identity()` hashes the material
parameters; records also carry `LAW_VERSION` and the fixed post-yield fraction.
No production scenario depends on this crate yet. Rapier is a test-only
dependency used for the isolated response experiment.

## The material law

A `Material` has density, elastic stiffness, yield strength, toughness,
damping and brittle/ductile mode. A region holds represented volume and a
remaining work capacity, initially `toughness * volume`. Strength is independent
of integrity, including while a failed region awaits host removal. `residual_strength`
and integrity-dependent strength softening have been removed.

Damping accepts the full range 0 through 1. It removes elastic energy harmlessly;
it does not count as irreversible damage. Reference hull damping is 1 and rock
damping is 0.95, so gentle hull/ground contact has little rebound. Reference
material values are simulation calibration, not measured engineering or
biological properties.

### Transient compression

An impact uses the actual relative normal approach velocity at the contact and
contact-effective mass, including rotational inertia:

```
1 / m_eff = sum(1 / mass_i + lever_i^2 / inertia_i)
energy = 0.5 * m_eff * closing_speed^2
k_i = stiffness_i * area / depth_i
Y_i = strength_i * area
h = POST_YIELD_STIFFNESS_FRACTION = 0.01
```

Static and prescribed kinematic bodies contribute no inverse mass. Moving or
rotating surfaces still contribute contact-point velocity. A common velocity
shift cancels. Kinematic drivers require separate external-work accounting in
the host; a relative-mode energy budget is not a complete world-frame balance.

Each side uses an event-local bilinear compression curve. Up to its yield
force `Y`, displacement is `F / k`. Beyond yield, additional displacement has
slope `h * k`:

```
x(F) = F / k + max(F - Y, 0) * (1/h - 1) / k
W(F) = F^2 / (2*k) + max(F^2 - Y^2, 0) * (1/h - 1) / (2*k)
```

Both sides carry the same force. Solve `W_a(F) + W_b(F) = energy`. Each curve
is linear in force squared between its yield boundaries, so inversion crosses
at most two boundaries. There is no iterative convergence loop or event-sized
allocation. Intermediates use `f64` to avoid overflow and cancellation; impact
inputs and outputs remain `f32` and nonfinite outputs are rejected.

Bilinear post-yield stiffness is a standard approximation, with the second
slope expressed relative to the elastic slope; see the
[OpenSees Steel01 parameter definition](https://opensees.berkeley.edu/OpenSees/manuals/comparisonManual/180.htm).
Our fixed 0.01 fraction, extension to wood/rock/tissue, event-local reset and
post-failure compression are game approximations, not that constitutive model
or measured material data. There is no persistent hardening or softening state.

The positive second slope matters: low-energy wood contact can remain below
armor's yield force, but increasing energy eventually raises the common force
above it. Work sharing follows both compression curves continuously, rather
than switching all damage from one side to the other when strengths cross.

### Elastic return and damage demand

For each side at the solved peak force:

```
elastic = F^2 / (2*k)
plastic = max(F^2 - Y^2, 0) * (1/h - 1) / (2*k)
```

A ductile side returns `(1 - damping) * elastic`, dissipates
`damping * elastic`, and requests `plastic` irreversible work. A yielded brittle
side requests `plastic + elastic` as fracture work and returns no elastic
energy. An unyielded brittle side behaves elastically with damping. Brittle
fracture therefore still has an onset discontinuity in recoverable energy;
continuity of the force solution does not eliminate that documented choice.

Each side's charge is capped by its own remaining capacity. Unabsorbed demand
becomes `excess`; it is not automatically transferred to the other region.
Yield and peak force are independent of remaining capacity. An exhausted region
cannot absorb more work or report a new failure, but its mass may remain in
contact until the host processes the consequences.

This is a **compression-through-the-event approximation**: fractured/crushed
material continues resisting compression for this event, allowing force to
rise even after its damage capacity is exhausted. It does not model the timing
of fragments escaping a contact, unloading after a cut, or penetration. The
reported deformation is an effective loading displacement, not a promise to
reshape or interpenetrate the rigid bodies. Cases where material escapes early
need a later, different contact response; they must not be claimed as validated
by this law.

### Sustained and connection loads

Sustained normal loads and connections remain distinct, force-driven modes.
Safe loads charge exactly zero. With host-measured plastic displacement, a
sustained load charges `yield_force * plastic_displacement`. Otherwise the
constant-strength fallback is:

```
work = loaded_volume * (applied_stress - strength) * duration / CRUSH_TIME_CONSTANT
CRUSH_TIME_CONSTANT = 0.25 seconds
```

The rate is constant for constant input, so one call and many substeps produce
the same work within floating-point tolerance. Failure occurs when capacity
runs out, within one tick plus rounding of `remaining_capacity / work_rate`.
There is no exponential softening progression. Duration zero produces a zero
outcome even when a measured displacement was supplied.

Connections use the conservative combined stress `abs(axial)/area +
abs(moment)/section_modulus`. Measured plastic extension/rotation is charged
against the corresponding load scaled back to the safe limit. Missing
components use the fallback over their contributing volume: all the beam in
pure axial loading, one third in pure bending. Bending and axial work are
calculated separately; only the bending contribution supplies inferred plastic
rotation. Pure axial loading therefore reports zero rotation. Connections have
their own capacity and can fail without deleting the assembly.

These slow-load modes retain a yield plateau; the transient compression curve
is not applied as a history-dependent constitutive model to them. The fallback
is a named strain-rate convention, not a measured plastic displacement. The
adapter must attribute external/support work, rather than treating an algebraic
ledger closure as proof that the rigid-body solver supplied that work.

## Energy ownership and failure disposition

Every outcome carries a `WorkLedger`:

```
available = sum(recoverable + dissipated + irreversible)
          + excess + failure_motion + failure_dissipated
```

`consumed` is what the host applies with `Region::absorb`. `failed` is true only
for the event exhausting previously positive capacity. Both materials use the
same event budget; neither receives the entire incoming energy independently.

`excess` means **unallocated**, not automatically preserved motion. Before
requesting restitution, the host calls `resolve_excess(motion)`:

- `motion` must be finite and between zero and the outstanding excess.
- It moves that amount into `failure_motion` and the rest into
  `failure_dissipated`, clearing `excess` without changing the total.
- Invalid requests leave the ledger unchanged. The same work cannot be
  allocated twice.
- Zero selects a blocking, dissipative failure response. A positive allocation
  requires a separate, momentum-consistent motion/fragment response from the
  host; this method does not apply velocities or create fragments.

`restitution()` returns `Result<f32, MaterialError>` and rejects unresolved
excess. Its coefficient is `sqrt(sum(recoverable) / available)`. The coefficient
targets the elastic return of an isolated normal impact. It does not deliver
`failure_motion`, prove a multi-contact assembly's balance, or override friction
and solver stabilization. Hosts must avoid applying the same energy in both
restitution and a later failure impulse.

The initial real-solver fixture explicitly selects `resolve_excess(0)`, with
no penetration or new fragment behavior. Production policy remains owned by
#328 and the scenario migrations. Existing destruction/ejection/protection rules
are unchanged by this reference crate.

Sustained/connection ledgers describe a call, including persistent stored strain
energy. Do not sum their recoverable fields as newly supplied energy every tick;
only work increments accumulate. Restitution has no physical interpretation for
these non-impact records.

## Persistence and adapter contract

`Region::new` starts at full capacity; `with_remaining` restores bounded saved
state. `absorb` is the only lowering operation. `repair` requires explicit host
authorization. `split` partitions volume, capacity and existing damage at the
same integrity. Normalized health remains `100 * remaining / capacity`.

Remaining capacity is accumulated in `f64`, including repair and split
calculations, so small per-tick charges survive subtraction from large balances.
Save `remaining_precise()` and restore that value with `with_remaining`;
serialization and cloning also preserve this authoritative balance. The `f32`
`remaining()` accessor supplies an event budget rounded upward by less than one
`f32` ulp. This ensures that a final charge actually exhausts the region when
the outcome reports failure. `absorb` returns any unspent rounding remainder;
accounting comparisons must allow that bounded difference from `consumed`.

Removing softening removes strength-driven event order dependence, but finite
capacity and failure attribution still require a deterministic ordering. Hosts
must apply each outcome before evaluating the next event sharing its region.
Stable material/source/episode IDs, rather than backend handle traversal order,
are the #327 contract. Later events cannot recharge an exhausted region.

`ContactGeometry::merge` is only an arithmetic helper for disjoint, equivalent
patch parts whose sections agree. The adapter must first recognize overlap and
actual physical episodes. Derive deformation depths from the whole patch before
splitting its width into parts; merging cannot infer missing physical geometry.
The unit fixture proves arithmetic equivalence, not real collider-partition
invariance. Multiple different contacts sharing one body's incoming motion
also need a shared finite budget; merging duplicate points alone is insufficient.

The existing post-solver `ContactEvent` stream cannot provide the initial
response by itself. Rapier's `modify_solver_contacts` hook supplies a per-manifold
modification point, but production aggregation, stable ordering, pre-solver
state updates and CCD episode handling still need design in #327. A per-manifold
hook is not by itself a globally ordered event phase. Pause must add no work,
and resting/speculative/chattering contacts must not mint fresh impact budgets.
Failure consequences must be queued outside contact iteration.

## Evidence

The reference generator and drift test publish the exact configuration and
results. JSON includes full elastic, dissipated and failure allocations; table
rows keep per-event consumed work separate from cumulative work across calls.
A closure test recomputes each displayed event's balance from those values.

Representative v2 results:

- A 5 mu branch against the 200 mu tank at 15 or 40 wu/s exhausts its own
  240 capacity and charges exactly zero to armor. At 80, 120 and 240 wu/s the
  armor consumes about 881, 2,476 and 11,089 of its 200,000 capacity.
- Fresh shells at 300 wu/s charge about 11,124 to a fresh plate and exhaust it
  on hit 18. Projectiles have their own finite capacity and are fresh for each
  shot; the plate's strength does not decay.
- A severe shell hit at 1,800 wu/s exhausts both regions and reports finite
  excess requiring explicit disposition.
- The strength sweep near equal strength no longer switches all work between
  sides. Symmetric materials/geometry split work equally; swapping sides swaps
  their results.
- Repeated safe contacts remain safe on fresh, half-damaged and nearly exhausted
  hulls. Zero-time sustained or connection updates add no work.
- The fallback reaches matching capacity at 30, 60, 120 and 240 Hz, with
  failure time checked against a constant-rate analytical solution.
- Small-charge regressions apply force 20000.25 to ARMOR volume 20 for 600
  seconds at 1, 30, 120 and 240 Hz: all lose 600 capacity at displayed precision.
  Tests also cover precise saved balances, repair, splitting and final failure.
- Pure axial overload reports zero rotation while retaining its damage work;
  pure bending and combined loads cover measured and inferred movement.
- An independent displacement-space numerical integration checks the solved
  force below yield, with one side yielded, and with both yielded. A parameter
  grid checks closure and capacity limits over all six reference materials.

### Real-solver experiment

`tests/rapier_response.rs` uses the workspace's vendored Rapier with a test-only
contact hook. It measures actual contact-point velocity/mass/inertia before the
solve, evaluates the law, resolves excess as dissipation, and sets restitution.
The test compares actual kinetic-energy loss with irreversible work plus both
forms of dissipation; dynamic pairs also conserve linear momentum.

It covers safe, damaging and capacity-exhausting contacts against static and
dynamic supports, CCD configurations with one and four maximum substeps, an
off-center contact that rotates the support, and a common velocity shift.
Relative energy tolerance is 0.2% of initial energy with a small absolute floor;
it is distinct from the pure ledger's `1e-4` closure tolerance. Collider
restitution is deliberately 1 so the hook must replace it.

This is an isolated-contact experiment without gravity, friction, initial
penetration or simultaneous independent contacts. It is not the production
adapter and does not prove partition invariance, fragment allocation, constrained
assemblies, driven supports, or replay of authoritative episode state. Those
remain #327/#328 acceptance work.

### Landing calibration

The old readout only predicted damage alongside existing game physics. Its
values are not evidence that v2 is playable. Keep its hull stiffness 35,000,
strength 6,000 and toughness 96,000 as a candidate, now with hull damping 1 and
reference rock damping 0.95:

- At 1, 1.6 and 2.2 wu/s: zero damage, rebound coefficient about 0.069.
- At 13.3 wu/s: about 0.46% hull loss.
- At 43.2 wu/s: about 4.74% hull loss.
- At 100 wu/s: about 24.5% hull loss.

These are fixture predictions from fixed reference geometry. V2 can also damage
the contacted rock at higher speeds; terrain-cell damage remains the separate
#333 migration. The production adapter must make that compatibility boundary
explicit and must not claim energy closure across unchanged legacy terrain
chipping. Actual material lookup, contact geometry and rebound feel still need
scenario testing under #330.

## Limits and verification

The pure evaluator has constant work and no per-event allocation. Fixtures and
report generation allocate normally. The law does not provide tangential/shear
loading, friction work, fatigue, shock waves, penetration, early fragment escape,
heat, anatomical injury or a soil continuum. Capacity can represent a coarse
whole region; local region resolution and failure geometry belong to #328.

The default 1% post-yield slope is a calibration assumption and should be revised
if future cases expose bad behavior. In particular it does not distinguish
light/fast from heavy/slow impacts having identical normal energy, geometry and
materials. No rate-sensitive continuum response is claimed.

Arithmetic uses `libm` where nontrivial math functions are needed. Tests establish
local deterministic results; neither this experiment nor the generated tables
claim cross-machine lockstep. Geometry measurement, event admission, external
work, clone/replay and bounded episode caches remain adapter responsibilities.

```
cargo fmt --all -- --check
cargo clippy -p engine-material --all-targets --no-deps -- -D warnings
cargo test -p engine-material
cargo run -p engine-material --example reference_cases
RUSTDOCFLAGS="-D warnings" cargo doc -p engine-material --no-deps
```
