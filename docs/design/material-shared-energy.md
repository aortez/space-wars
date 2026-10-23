# Shared normal energy experiment (#327)

`engine_material::shared_impact` computes a bounded, frictionless compression
projection for a complete set of touching contacts. It extends the earlier
[planar experiment](material-contact-batch.md) to differing normals, rotation
and dynamic supports. It is experimental arithmetic, with no scenario consumer
and no connection to the [engine contact stream](material-contact-records.md).

The initial-work split and restitution-only response remain as comparison
models with explicit counterexamples. The follow-up adds a versioned elastic
pulse allocation and a checked direct-impulse response. That combination reaches
resting contacts and closes the tested ledgers at ordinary solver settings.
The allocation remains a modeling hypothesis, and the experiment is still
restricted to complete, touching, frictionless contact sets without joints or
external forces. No scenario uses it yet.

## Coupled compression budget

Each contact has a stable physical patch ID, two body indices, a world point,
a unit normal and positive series stiffness. The host must first aggregate
equivalent collider contacts and supply the whole participating contact set.
Body inputs contain center of mass, linear/angular velocity and effective
inverse mass/inertia, including locked axes. Zero inverse properties represent
prescribed degrees of freedom.

Let `J` map body velocities to positive normal approach speeds, `u = J v`.
With effective inverse mass matrix `D`, define `A = J D Jᵀ`. The nonnegative
compression impulse solves:

```text
lambda >= 0
A lambda - u >= 0
lambda_i * (A lambda - u)_i = 0
```

The work extracted by this projection is
`E = lambda · u - 0.5 lambda · A lambda`. At exact complementarity this is
`0.5 lambda · u`; the implementation retains the quadratic expression when
normalizing shares. A single contact reduces to `0.5 m_eff u²`.

Summing isolated contact energies is insufficient. Two redundant contacts on
a 2 mu translating body at 4 wu/s would each claim 16 units in isolation; the
coupled budget is 16 total. Conversely, three 2 mu bodies with velocities
4, 0 and -2 wu/s have 18.6667 units of relative energy, while the two isolated
pair budgets sum to only 10. The projection preserves center-of-mass motion.

Rows connected through nonzero entries of `A` form one component. Independent
components retain separate budgets and recoverable-energy ratios. Combining
their restitution ratios would transfer rebound energy between unrelated modes.

## Work allocation is still an approximation

The original `allocate` split assigns each row a nonnegative share proportional to
`lambda_i * u_i`, normalized to the component's extracted energy. Stiffness
scales the iterative solve; for identical parallel rows it produces a split
proportional to stiffness. Subdividing an equivalent parallel patch while
scaling its area and stiffness preserves the tested material work. For a unique
coupled solution, stiffness does not change the compression impulse.

This is an initial-work allocation convention, **not a coupled elastic/plastic
compression history**. In particular, three 2 mu bodies with initial velocities
4, 0 and 0 wu/s receive compression impulses 16/3 and 8/3. The second contact
transmits impulse but receives zero allocated work because its initial approach
speed is zero. It could still deform during impact transmission. A test retains
this limitation explicitly; zero allocated work must not become evidence that
the downstream material takes no damage.

`ContactBudget::material_input` converts positive allocated energy to an
equivalent scalar mass for the existing material law. It rejects conversion
outside that law's range, including energy lost through `f32` underflow. This
conversion proves an accounting identity, not physical sufficiency of the split.

### Elastic pulse allocation

`allocate_pulse` retains the same compression impulses and total component
budget, but uses weights `lambda_i² / k_i`, where `k_i` is the patch's elastic
series stiffness. Its version is `shared-elastic-pulse-v1`, separate from the
existing material law. These weights follow the elastic energy `F² / (2 k)`
under an assumed common force-pulse duration and shape. They are normalized
within each component, so they cannot mint a second motion budget.

For the equal-stiffness resting chain above, shares have ratio 4:1. Increasing
the downstream stiffness by four changes that ratio to 16:1. Equivalent parallel
partitions still share in proportion to stiffness/area. Tests check ordering,
common translation, partition invariance and positive downstream material work.

The scalar material input uses loading speed `2 E_i / lambda_i` and effective
mass `lambda_i² / (2 E_i)`. It preserves both the allocated energy and compression
impulse, including when initial approach is zero. `ContactBudget::closing_speed`
still reports measured approach; `loading_speed` names the equivalent input.
Zero impulse/work yields no material input.

This chooses an allocation; endpoint impulses alone do **not** determine the
actual transmitted deformation history. The tests prove budget closure and
qualitative transmission, not agreement with a compliant wave simulation.
Weights use elastic stiffness before applying the nonlinear isolated law, so
they do not redistribute loading after one patch yields. This approximation
needs calibration before a general production damage migration.

## Prescribed motion and ideal rebound

Work from prescribed degrees of freedom is explicit and signed:

```text
W_driver = lambda · J_prescribed v_prescribed
E = K_dynamic_removed + W_driver
```

A support can supply or absorb energy. Under a common velocity shift the
relative budget stays the same while world-frame kinetic loss and driver work
change together. Tests cover translating supports and fully dynamic chains.

For an ideal response using impulse `(1 + e) lambda` throughout one component,
the loss is `(1 - e²) E` and driver work is `(1 + e) W_driver`. Thus
`e = sqrt(recoverable / E)` closes the ideal ledger. The caller must resolve
excess explicitly before deriving that coefficient.

These are predictions for that impulse response. They do not measure the work
of an arbitrary backend response. Gravity, forces, friction, joints, incomplete
contact sets, speculative arrival and changing geometry require more accounting.

## Actual Rapier response

`tests/shared_response.rs` applies the existing material law to the experimental
shares, resolves excess as failure dissipation and sets one restitution per
component through the batch hook. It tests an oblique two-support impact,
angular motion with two offset contacts and a three-body dynamic chain. It also
reverses insertion order and applies common velocity shifts. These fixtures
have no gravity, friction, joints or initial penetration.

With effectively rigid contact softness and 16 internal PGS iterations, the
actual solver losses match the ideal ledger within the existing 0.3% tolerance;
the dynamic chain also preserves linear and angular momentum. This is a
reference setting in the test only. Engine defaults are unchanged.

The oblique fixture also preserves a counterexample at `dt = 1/240 s`, measured
after one actual step with the current vendored backend:

| Solver setting | Ledger loss | Measured kinetic loss | Relative discrepancy |
| --- | ---: | ---: | ---: |
| Default | 24723.8291 | 24447.0320 | 1.1196% |
| 16 internal PGS iterations, default softness | 24723.8291 | 24558.1388 | 0.6702% |
| 16 internal PGS iterations, effectively rigid contacts | 24723.8291 | 24723.8300 | 0.0000036% |

The reference uses contact natural frequency `1e6` and damping ratio `1`.
Vendored `SpringCoefficients::cfm_factor` controls soft-contact impulse scaling;
the velocity solver interleaves biased solving, position integration and
stabilization. Iteration count and softness therefore matter to the observed
response. The experiment demonstrates that restitution alone is insufficient
for exact coupled material accounting under defaults. It does not establish a
universal error bound, or justify tuning global settings to hide the discrepancy.

### Checked direct response

The 2D batch context now offers `apply_contact_impulses`. The host supplies
nonnegative paired normal impulses at existing solver points; the experiment
uses `(1 + e) lambda` from each component's material ledger. It keeps contact
constraints active, clears targeted restitution and normal/tangent warmstarts,
and applies the impulses before the ordinary velocity solve. No default solver
settings change. The body view is now the read-only `bodies()` accessor.

Application is atomic: it stages only body velocities, validates the complete
request, then commits velocities and clears the targeted cached response. The
operation rejects duplicate/stale targets, invalid numbers, more than 256
requests, friction, tangent drives, separation/penetration beyond `1e-5`, and
any targeted point still approaching faster than `1e-4` after the combined
response. Staging stores at most 512 small velocity records, with quadratic
bounded searches. It does not clone body collider lists or alter topology.

This validates application, not ownership of the entire coupled contact set.
Unlisted contacts, forces and joints may still change motion during the solve.
Pre-applied impulses are not included in Rapier's later force-event totals;
the future adapter must retain its own impulse/work records. Material capacity
changes must be staged until response admission succeeds.

Direct-response regressions cover oblique and angular impacts, dynamic and
initially resting chains, and elastic hits with zero damage. They reverse
insertion order and shift the scene by 17 wu/s. Across 30, 60, 120 and 240 Hz,
eight subsequent steps do not recharge these fixture impacts or change the
predicted loss. Seeded solver warmstarts are cleared. Atomic-rejection tests
compare serialized body/contact state before and after invalid requests.

The direct tests use tolerance `1e-5 * max(ledger_loss, 1)` in the translating
support frame. World-frame loss plus prescribed work is also checked, with an
additional `8 * f32::EPSILON * (K_before + K_after)` roundoff allowance. A tiny
impact riding a fast support exposes velocity quantization when large transport
energies are subtracted; that error is not evidence of new material work. The
old default-solver counterexample stays unchanged and still fails the tighter
response contract. Linear and angular momentum remain checked for dynamic chains.

## Bounds and failure

The kernel accepts at most 64 bodies and 32 contacts. It sorts by stable patch ID
and uses double-precision intermediates with `libm` for square roots. Each
component gets at most 2048 accelerated projected-gradient iterations; each
iteration costs at most quadratic work in its row count. The matrix occupies
at most 32 by 32 doubles. `Budget::iterations` sums all component iterations.
The speed residual tolerance is `1e-9` times the largest approach in the component.

Invalid/nonfinite inputs, duplicate IDs, separating rows, immovable contacts,
range failures and nonconvergence return an error with no partial budget.
Opposing incompatible driven contacts exercise bounded nonconvergence. An
empty batch or stationary normal mode returns zero work. Completeness of the
input set is a host responsibility and cannot be inferred by this kernel.

The production admission policy must decide what to do when a batch is refused;
silently skipping its damage is not an implemented policy. The current
experiment has no persistent region mutations outside its test fixtures.

## Next integration boundary

The next step is a bounded opt-in adapter that joins canonical records, physical
patch aggregation, episode ownership, staged material changes and the checked
response. It needs explicit admission for complete contact sets and a defined
fallback when a set is unsupported or refused. The fixture's one-shot latch is
not a production episode policy. Pulse-allocation calibration and admission of
friction, joints, penetration correction and external work remain open.

Patch geometry/admission, sustained and connection loads, other external work,
soft queries and failure consequences remain unfinished #327/#328 work. This
experiment is not completion of either issue.

```sh
cargo test -p engine-material --lib shared_impact
cargo test -p engine-material --test shared_response -- --nocapture
cargo test -p engine-material
cargo clippy -p engine-material --all-targets --no-deps -- -D warnings
```
