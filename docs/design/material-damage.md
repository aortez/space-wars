# Material damage: strength, deformation and integrity

First delivery for the shared material damage epic #325: the bounded reference
model that #326 asked for, as the pure crate `engine-material`, with its
calibration fixtures and evidence. Nothing is integrated into a host yet. The
load adapter (#327), persistent region storage and failure lifecycle (#328),
weapon sources (#329) and the Spacewars and Scorched migrations (#330, #331)
build on the contract below.

The model replaces speed-times-scalar damage with one law: a material has a
stiffness, a safe strength and a toughness; loads below the safe limit are
fully recoverable; work beyond it is charged against a finite capacity that is
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

## The law

A `Material` has `density`, `stiffness` (E), `strength`, `toughness`,
`damping`, a `FailureMode` and `residual_strength`. All are validated: finite,
within documented ranges, `strength <= stiffness`, and a `residual_strength` of
at least 0.01, so exhausted material still carries load. Its `identity()` is an
FNV-1a hash of the parameters, excluding the name, and `LAW_VERSION` names the
law itself; records carry both.

- **Safe limit.** Stress at or below the effective strength is fully
  recoverable. The charged work is exactly `0.0`, not merely small, so any
  number of safe knocks or any length of safe rest leaves integrity untouched.
  The stored elastic energy of an impact returns, less a `damping` fraction
  that is lost harmlessly and reported separately.
- **Capacity and integrity.** A `Region` is a quantity of one material with
  `capacity = toughness * volume` of irreversible work. `integrity` is the
  remaining fraction, `normalized_health` is a hundred times that, and
  `failed` means nothing remains. Geometry and represented quantity set the
  capacity; there is no per-target multiplier and no larger HP pool standing in
  for armor.
- **Softening.** Remaining integrity softens the effective strength linearly
  down to `residual_strength * strength` at zero integrity. Stiffness does not
  change. A dented plate therefore yields a little earlier on the next hit.
- **Brittle and ductile.** `resilience()` is the elastic energy per volume at
  the safe limit, `strength^2 / (2 E)`. Past the safe limit a brittle material
  does not spring back: in an impact its stored elastic energy is charged as
  fracture work too. A ductile material plateaus at yield, springs back and
  accumulates plastic work. The mode on its own changes little, because the
  extra brittle charge is only the energy stored at yield, a small fraction of
  a region's capacity, so it matters mostly just past the safe limit. What makes
  wood and rock behave brittle is the toughness that goes with the mode,
  within a few times the resilience, so their capacity runs out fast; ductile
  materials sit far above it.

## Loading modes

Every mode takes a physically attributable load: a closing speed and effective
mass, a support force over a patch, or a moment and axial force on a
connection. Nothing in the API accepts render overlap, so a soft-tissue query,
a spaceling under a falling branch for example, has to come from a sweep, a
contact force or measured work rather than from sprites touching.

### Transient impact

`impact(a, b, contact, Impact { closing_speed, effective_mass })` evaluates one
contact episode between two regions.

1. The available energy is `E = 0.5 * m_eff * v^2`. It is energy, not health
   loss. `effective_mass` comes from `effective_mass(ContactBody, ContactBody)`:
   `1 / m_eff = sum(1 / m_i + lever_i^2 / I_i)` over the movable bodies, where
   `lever` is the 2D cross product of the contact offset and the normal. An
   immovable body contributes nothing; a locked rotation contributes no lever
   term. `closing_speed(normal, v_a, v_b)` is the normal approach speed at the
   contact point and is zero when separating, so a common velocity cancels.
2. The two sides are springs in series, `k_i = E_i * area / depth_i`. The
   fully elastic peak force is `F_el = sqrt(2 E k_series)`.
3. Each side's safe force is `F_y,i = effective_strength_i * area`. If `F_el`
   does not exceed the smaller `F_y`, the event is elastic: each side stores
   `0.5 F_el^2 / k_i`, returns it less damping, and the charge is zero.
4. Otherwise the smaller `F_y` caps the force. Force is continuous across the
   patch, so the stronger side never carries more than the weaker side's safe
   force and stays elastic however much energy arrives: a weaker material
   cannot damage a stronger one in a single contact, and a penetrator damages
   plate only because its strength is higher. The plateau work
   `E - 0.5 F_y^2 / k_a - 0.5 F_y^2 / k_b` is charged to the weaker side, plus
   its own stored energy when brittle, up to its remaining capacity. Equal safe
   forces share the plateau work equally. The rule is exact, not graded: in the
   strength sweep a striker at 0.999 times the plate's strength takes all the
   plateau work, an equal one half, and one at 1.001 times none. That follows
   from perfect plasticity. Once softening separates two same-material actors
   after a shared hit, the weaker takes every later plateau alone, which
   ship-on-ship rams in #330 should expect.
5. Work the weaker side cannot absorb is reported as `excess`. It stays with
   the motion: the snapped branch flies on, the exhausted plate is broken
   through. Nothing is forwarded to the other side; the earlier idea of a
   second pass was dropped because broken wood still cannot push harder than
   wood.

`yield_speed` gives the closing speed at which a given pair first reaches the
safe limit, `min(F_y) / sqrt(m_eff k_series)`; slower impacts are recoverable.

### Sustained load

`sustained(region, SustainedLoad { force, width, section, duration,
measured_plastic_displacement })` evaluates a normal load that acts for
`duration`. Stress is `force / (width * thickness)`. At or below the safe
limit the call reports the strain energy and the recoverable deflection and
charges nothing, however many ticks the load rests. Above it the region
yields. With a host measurement the charge is `F_y * plastic` for the plastic
displacement measured during the call; without one it is the fallback below.
The stress is the applied stress; the material carries `F_y`.

### Connections

`connection(joint, ConnectionLoad { moment, axial, duration,
measured_plastic_rotation, measured_plastic_extension })` evaluates bending and
axial load on a `Connection`, a `Beam` of one material with its own region.
Stress is the conservative superposition `|F| / A + |M| / S`; elastic rotation
is `M L / (E I)` and extension `F L / (E A)`. Above the safe limit the applied
loads are scaled back to it along the same load path, and the charge is the
scaled moment times the measured plastic rotation plus the scaled axial force
times the measured plastic extension; an unmeasured component uses the
fallback. A finite root attachment is the pure tension case. Shear is not
modeled.

### Fallback progression

When the host supplies no measurement, plastic strain advances at
`(r - 1) / CRUSH_TIME_CONSTANT` per second for an overload ratio
`r = stress / strength`, with the constant at a quarter second, so intact
material under twice its safe stress starts to flow at one unit of strain per
quarter second. The strain is multiplied by the geometric scale of the mode,
the section depth for contacts, the connection length for extension and
`2 L / h` for rotation. In work terms, plastic work accrues at
`volume * (stress - strength) / CRUSH_TIME_CONSTANT` per second, where the
flowing volume is the loaded `area * depth` of a contact, a third of a beam in
pure bending and all of it in tension.

Softening lowers the strength by `strength * (1 - residual_strength) / capacity`
per unit of work, so the overstress grows exponentially and damaged material
crushes faster, never slower. Once the capacity is exhausted the material
flows at its residual strength and the further work is excess. Each call
integrates this exactly over its duration instead of taking one step, so a
constant load reaches the same state at the same time whatever the fixed step
or the substep count. One step per call is not enough: it would make the 60 Hz
plank below fail at 0.067 s instead of the continuous 0.035 s. This is a
bounded approximation with a named constant, not a raw impulse times frame
time and not an open-ended timer.

Sustained and connection ledgers are per call. A sustained call is not a
loading cycle, so its strain energy is reported as recoverable with nothing
damped, and again on every call while the load persists; only the
irreversible work accumulates. Their `available` is that stored energy plus
the charged work, so these ledgers close by construction, and closure is a
real check only for impacts. The brittle loss of stored energy also applies to
impacts only, where the event is evaluated once.

## The ledger

Every call returns an `Outcome { ledger, consumed, yielded, failed }`. The
`WorkLedger` holds `available`, per-side `recoverable`, `dissipated` and
`irreversible`, the `excess`, the `peak_force`, per-side `peak_stress` and
`deformation`. `closes(tolerance)` checks
`available = recoverable + dissipated + irreversible + excess` within a
relative tolerance; every fixture row closes at `1e-4`. `restitution()` turns
an impact's ledger into the rebound coefficient a host gives the physics
engine, as the next section describes. `consumed` is what the host passes to
`Region::absorb`; the model never mutates a region itself. `failed` is set
only by the event that takes a region's last capacity, so a host starts its
failure handling once; a region that had already failed reports `false`.
`safe()` means neither region reached its safe limit. Every `Ok` outcome is
finite: inputs inside their ranges whose magnitudes still overflow `f32`
together return `MaterialError::Overflow`.

## Energy ownership

The material model owns each contact's energy budget, and the physics engine
gets no second say. Today Rapier's restitution decides the rebound on its own,
so a host that charged the model's damage and also let Rapier bounce would
spend the bounce twice. Instead, the load adapter (#327) evaluates an impact
when the contact begins, before the solver resolves it, and sets that
contact's restitution to `WorkLedger::restitution()`,
`sqrt(recoverable / available)`; Rapier's `PhysicsHooks::modify_solver_contacts`
can set the `restitution` of each solver contact. The rebound then returns
exactly the recoverable energy: an elastic knock bounces back, a dent hardly
does, and a collider's tuned restitution adds nothing. Excess is not rebound
either. It is the motion that carries on past a failed region, a projectile
through a broken plate or a snapped branch falling on, and the failure
lifecycle (#328) returns it to the motion.

## Regions, connections and state

`Region::new(material, volume)` validates both and starts at full capacity.
`Region::with_remaining(material, volume, remaining)` restores a saved region
for hosts that persist integrity and rejects a remainder outside the capacity.
`absorb(work)` is the only path that lowers integrity and returns whatever
could not be absorbed. `repair(fraction)` restores a fraction of the full
capacity and exists only for explicit host authorization. `split(fraction)`
partitions the material into two regions at the same integrity so a cut or a
transfer creates no capacity. `mass()` is `density * volume` for hosts that
derive inertia from represented quantity. `Connection::new(material, beam)`
wraps a region sized by the beam's volume and exposes the same integrity,
`absorb` and `repair`.

Softening makes events on the same region order dependent, since each charge
lowers the strength the next one meets. Hosts therefore evaluate one step's
events in a deterministic order, for example sorted by collider handle pair
and then contact index, and absorb each outcome before evaluating the next
event on the same region. A region that fails mid-step meets later events at
its residual strength, and only the event that exhausted it reports `failed`.

## Partition, step and frame invariance

`ContactGeometry::merge` sums the widths of equivalent sub-contacts whose
sections agree, so an adapter that sees one episode through several colliders
evaluates one contact. Because the default depth follows the patch width, the
adapter derives both sections from the merged width of the patch, not from
each part's. Charging each part with the whole event is the mistake the
fixtures demonstrate: four parts overstate the plate's damage four times.
Impacts are evaluated once per episode from the closing speed, so the fixed
step and any substeps cannot change them. Sustained and connection loads
integrate the fallback exactly over each call, so a constant load leaves the
same state after the same time at any step rate. A failure is seen at the end
of the tick that crosses it, within one tick of the continuous failure time.
Common velocity shifts cancel in `closing_speed`.

## API contract

```rust
pub const LAW_VERSION: &str;
pub enum MaterialError { InvalidMaterial, InvalidGeometry, InvalidLoad, InvalidRegion, Overflow }
pub enum FailureMode { Brittle = 0, Ductile = 1 }
pub struct Material { name, density, stiffness, strength, toughness, damping, mode, residual_strength }
impl Material { validate, yield_strain, resilience, effective_strength(integrity), identity, identity_hex }
pub struct Section { thickness, depth }           // contact_stiffness(E, area)
pub struct Beam { thickness, height, length }     // area, section_modulus, second_moment, volume
pub struct ContactGeometry { width, a, b }        // area, merge(&[..])
pub struct ContactBody { mass: Option<f32>, inertia: Option<f32>, lever }
pub fn effective_mass(a, b) -> Result<f32, _>;
pub fn lever(offset, normal) -> f32;
pub fn point_velocity(linear, angular, offset) -> Vec2;
pub fn closing_speed(normal, v_a, v_b) -> f32;
pub fn patch_width_from_radii(r_a, Option<r_b>) -> Result<f32, _>;
pub fn deformation_depth(width, Option<extent>) -> Result<f32, _>;
pub struct Region;      // new, with_remaining, material, volume, mass, capacity, remaining,
                        // integrity, normalized_health, failed, effective_strength, absorb,
                        // repair, split
pub struct Connection;  // new(material, beam), beam, region, integrity, failed, absorb, repair
pub struct Impact { closing_speed, effective_mass }
pub fn impact(a: &Region, b: &Region, ContactGeometry, Impact) -> Result<Outcome, _>;
pub fn yield_speed(a, b, ContactGeometry, effective_mass) -> Result<f32, _>;
pub struct SustainedLoad { force, width, section, duration, measured_plastic_displacement }
pub fn sustained(&Region, SustainedLoad) -> Result<Outcome, _>;
pub struct ConnectionLoad { moment, axial, duration, measured_plastic_rotation, measured_plastic_extension }
pub fn connection(&Connection, ConnectionLoad) -> Result<Outcome, _>;
pub struct WorkLedger { available, recoverable, dissipated, irreversible, excess, peak_force, peak_stress, deformation }
                        // accounted, closes, restitution, finite
pub struct Outcome { ledger, consumed: [f32; 2], yielded: [bool; 2], failed: [bool; 2] }  // safe()
pub const CRUSH_TIME_CONSTANT: f32 = 0.25;
pub const DEPTH_WIDTH_FRACTION: f32 = 1.0;
pub const PATCH_WIDTH_FRACTION: f32 = 0.5;
pub const LEDGER_TOLERANCE: f32 = 1.0e-4;
```

Everything is `f32`, allocation free, deterministic for the same inputs,
validates its inputs and returns only finite results. Transcendental
functions come from the `libm` crate rather than the platform's math
library, as with Rapier's `enhanced-determinism`, so every platform gets the
same bits and the generated evidence matches everywhere. Single-region loads
fill slot 0 of the per-side arrays.

## Reference materials

Six reference materials live in `engine_material::ALL_REFERENCE`. Their values
are calibration defaults in world units, not measured properties.

| material | mode | stiffness | strength | toughness | toughness / resilience | role |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| armor | ductile | 2.0e6 | 2.0e4 | 1.0e4 | 100 | thick vehicle plate |
| hull | ductile | 7.0e5 | 6.0e3 | 2.0e3 | 78 | ship skin, weaker than rock so crashes hurt |
| shell | ductile | 2.0e6 | 3.0e4 | 3.0e3 | 13 | hardened projectile, the one thing that dents plate |
| wood | brittle | 2.0e5 | 3.0e3 | 60 | 2.7 | branches and trunks |
| soft_tissue | ductile | 2.0e3 | 2.0e2 | 60 | 6 | spacelings |
| rock | brittle | 5.0e6 | 1.0e4 | 15 | 1.5 | planets, asteroids, stones |

The ordering of strengths decides who yields: tissue below wood below hull
below rock below armor below shell. A ship crashing into a planet dents its
hull; a tank driving into the same rock chips the rock; a branch falling on a
tank snaps; a shell dents the plate.

These values prove the law; they are not tuned to game feel, and against
today's Spacewars they are harsh. A ship's hull starts to dent on a planet near
0.9 wu/s and a 20 wu/s crash costs about half of it, while the game does no
planet damage below 30 wu/s; a spaceling bumping a branch takes damage above
about 0.4 wu/s. Calibrating against gameplay is #330's job, with today's
30 wu/s gate and the game's landing and walking speeds as targets.

## Evidence

`cargo run -p engine-material --example reference_cases` prints every table and
writes [`docs/data/material-model-v1.md`](../data/material-model-v1.md) and
[`docs/data/material-model-v1.json`](../data/material-model-v1.json). A test
regenerates both and fails if the committed files differ, so the published
numbers cannot drift from the code. Reference bodies: a 200 mu tank whose 20
wu^3 plate region is 160 mu of armor, a 31.25 mu ship (the game's ship mass)
with a 6 wu^3 hull, a 2 mu spaceling, a 5 mu branch, a 70 mu boulder of solid
rock, a 2 mu stone, a 0.5 mu shell that is a solid steel slug of radius 0.1 wu,
and an immovable planet. Size, mass and material quantity are separate inputs,
but no body holds more material than it weighs, and a test checks that. Each
body also says how far its material reaches behind a contact: the tank's
0.2 wu armor plate, the ship's 0.1 wu hull, the diameter of each round body,
and a half-space for the planet.

What the acceptance cases show:

- **Safe bumps.** Five pairs at half their safe-limit speed, a thousand bumps
  each: the summed irreversible work is exactly zero and integrity stays at one.
  The safe-limit speeds range from 0.36 wu/s for a spaceling against a branch to
  1.38 wu/s for a shell against a plate.
- **Progressive hits.** A shell at 300 wu/s charges 22,443 of the plate's
  200,000 capacity per hit; the plateau force falls from 976 to 363 as the
  plate softens, and it fails on the ninth hit while the harder shell is never
  marked. A rigid-strength variant fails after exactly the analytically
  predicted count.
- **Quantity and thickness.** Doubling the plate volume takes 18 hits instead
  of 9. A hit at 1.2 times the thin plate's safe-limit speed charges 0.21 on
  thickness 1 and exactly zero on thickness 2.
- **Wood against plate.** A branch at 15, 40 or 120 wu/s snaps every time and
  the plate's irreversible work is exactly zero; the broken wood carries the
  excess away.
- **Tissue against wood.** The same stone at 10 wu/s exhausts the spaceling's
  10.8 capacity and takes 71 of the branch's 240.
- **Equal strength.** Two hulls at 20 wu/s each take 1,561.
- **Severe failure.** A shell at 1,200 wu/s brings 359,000, exhausts the plate's
  200,000 and reports 159,000 excess; the ledger closes and nothing is infinite.
- **Glancing.** A ship into a planet at 20 wu/s charges 6,238, 4,676, 1,551 and
  177 at 0, 30, 60 and 80 degrees of incidence.
- **Angular.** At 3 wu/s, a boulder's lever arm of 0, 1, 2 and 3 wu lowers the
  effective mass from 51.9 to 44.5, 31.3 and 20.9 mu, and the boulder's damage
  from 229 to 196, 136 and 90 of its 420 capacity; the plate takes nothing.
- **Resting support.** The 200 mu tank resting on a wood plank for a thousand
  ticks charges nothing. A press at 1.05 times the safe stress fails the plank
  after three ticks and one at 1.5 times within one tick; a measured 0.02 wu
  plastic displacement charges 120, half the plank, in one tick.
- **Connections.** Of three wood connections under one tip load, only the root
  exceeds its bending strength. The root fails after two ticks while the
  others keep full integrity and the assembly keeps all three.
- **Partition.** Four merged sub-contacts reproduce the single contact exactly;
  charging each part with the whole event reports 89,775 instead of 22,443.
- **Fixed step.** The 1.05 times press and a 1.08 times connection moment,
  stepped at 30, 60, 120 and 240 Hz, consume the same work by 1/30 s: 200.9
  of the plank's 240 and 59.0 of the connection's 120. Each fails within one
  tick of its continuous failure time, 0.035 s and 0.045 s.

The sweeps cover closing speed, striker mass, patch width, plate toughness and
the striker-to-plate strength ratio. The strength ratio sweep is the clearest
statement of the force-continuity rule: below one the striker alone yields,
above one the plate alone yields, at one they share, and the rows at 0.999 and
1.001 show the switch is immediate.

## Limitations

- No tangential or shear loading, and no friction work. Glancing hits use only
  the normal component.
- No penetration or hydrodynamic regime: a weaker material never damages a
  stronger one in a single contact, however fast.
- No fatigue below the safe limit, no heat, no shock waves, no anatomical
  injury, no soil continuum.
- The patch width and the deformation depth have documented defaults, not
  solved contact mechanics; hosts can override both.
- A coarse region credits its whole capacity to every contact. Splitting
  actors into regions per area is #328's job.
- The fallback progression is a strain-rate convention. Hosts that measure
  plastic displacement should pass it.
- Constrained supports and multi-contact assemblies are evaluated one contact
  or connection at a time; load redistribution after a failure is the host's
  structural model.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy -p engine-material --all-targets -- -D warnings
cargo test -p engine-material
cargo run -p engine-material --example reference_cases
```

Twenty-nine tests cover the acceptance list above, the analytic forms for
available energy, the series-spring peak force and the effective mass, the
deformation-depth rule, restitution that returns exactly the recoverable
energy, the exact fallback solution at four step rates, ledger closure on
every fixture row, reference bodies that hold no more material than they
weigh, input validation, finite results for exhausted regions and overflowing
magnitudes, failure reported once, repair, restore and split bounds, and the
drift guard on both generated files. Clippy reports nothing and the workspace
formatting passes.
