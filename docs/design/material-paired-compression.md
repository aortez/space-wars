# Two finite materials in retained compression (#327, #328)

`PhysicsWorld::step_material_dynamic_compression(dt)` admits finite material on
both bodies of the [moving-support path](material-moving-support.md). Each layer
retains its own deformation, permanent set, viscous loss and remaining capacity.
The bodies receive equal and opposite impulses from one shared work calculation.
The incoming body is no longer an ideal rigid boundary in this mode.

## Admission and identity

The existing two-body, full-face rectangle, cardinal orientation, normal-motion
and mass `[5,80]` restrictions remain. Both bodies must be dynamic, nonsleeping
and rotation-locked, with no gravity, joints or external forces. Initial relative
closing speed is `(0,64]`, each body's speed is bounded by 64, and enrollment
starts touching or with a gap no greater than one quarter of the smaller depth.
No preloaded or already-owned contact can enroll.

Each body's colliders bind one finite material region. Thickness is `[0.125,8]`,
depth is `[0.125,1]`, and yield displacement must be less than depth/4 for each
layer. The body further along the shared cardinal normal is the support; source
IDs and insertion order do not choose which material receives damage. Uniform
collider partitions and duplicate geometry retain one capacity per region.
The two sections may differ; each stiffness and yield force use its own section.

## Shared reaction and separate work

For each layer, stiffness is `k_i = E_i*A_i/depth_i`, yield force is
`Y_i = strength_i*A_i`, and viscosity is `eta_i = Y_i*tau/depth_i`.
The existing unilateral elastic/plastic/viscous segment law evaluates a candidate
displacement, returning elastic storage, plastic work and viscous loss. A bounded
inner solve divides total indentation between the layers so their discrete mean
work forces agree. One layer can return elastic work while the other still flows.
There is no fixed percentage allocation and no material-pair exception.

The outer midpoint solve uses reduced mass and that common reaction. Actual
backend drift determines the committed work, followed by equal/opposite endpoint
impulses. Each material independently charges `Y_i * delta_p_i` through the
existing sustained-load API. Viscous loss and numerical error never consume
capacity. The total balance, with no external work, is:

```
K_initial = K + sum(E_i + Y_i*p_i + viscous_loss_i) + energy_error
Y_i*p_i = capacity_consumed_i + charge_error_i
```

Separation uses the sum of permanent sets as the displaced contact surface.
At a separating endpoint both springs release completely. That terminal
interval obtains the common mean reaction from their combined signed work over
actual relative displacement. Extending two separate secants into the open gap
would otherwise permit residual elastic storage after separation. The remaining
gap is assigned by elastic compliance and stores no energy. Permanent sets and
capacities survive separation, pause and restore.

The aggregate compression summary reports summed material totals and relative
motion. `material_moving_support_summary()` still reports both laboratory motion
balances. `material_paired_compression_summary()` reports the two individual
material histories in incoming-body/support order. Each interval's `load` is
the support material and `other_load` is the incoming material. Their work adds;
their shared impulse is counted once. Individual endpoint force diagnostics may
differ during transitions; the mechanical impulse comes from the mean reaction.
Impact then Support retain the same episode owner, followed by End at separation.

## Bounds and persistence

Each layer and the total indentation remain within one quarter of their
respective depth limits; total indentation uses the smaller depth. The elastic
timescale uses series stiffness, while the viscous bound conservatively uses the
larger viscosity. There are at most 1024 internal intervals per public call and
64 iterations in each scalar root. Both material load records count against the
output limit.

Fixed numerical scales use the smaller yield force and smaller depth, with the
existing `1e-5` factors and separate position allowances for both bodies. Each
layer also retains its own absolute charge error. These conservative limits can
refuse a long run before separation, particularly with unequal materials or
large common motion. They are not enlarged by stronger material, remaining
capacity, pauses or save/load. Failed material, exhausted allowances and output
limits discard the entire public call, including both bodies and both charges.
The separate opt-in [paired-collapse policy](material-support-failure.md#two-finite-material-layers)
commits exhaustion of either layer, dissipates both elastic stores and lets both
bodies coast. It preserves any surviving capacity and both permanent sets.
Ordinary paired compression retains its refusal policy; severing and fragment
motion remain later #328 work.

Snapshot v20 has a distinct tag and appended active status. Its required
fixed-size extensions carry both motion and both material histories. Restore
checks individual material totals, the aggregate work and motion balances,
live material bindings and contact phase. Invalidated histories remain checked
after either body is removed. Earlier v1-v19 modes keep their layouts; a frozen
v19 serializer supplements the existing older-version tests.

## Qualification

Public tests compare equal wood layers against the independent continuous
single-layer solution with the appropriate mass/displacement scaling. Wood and
armor are compared with a separate continuous series reference: elastic motion
is analytic and the yielding interface is integrated by a fine RK4 reference.
Both material orders, safe zero damage, elastic recoil and full separation are
covered. Additional unequal-strength fixtures yield both layers with different
damage, and unequal masses demonstrate recoil.

Tests also check individual and total work, all quarter turns, shared normal
motion, collider partitions/duplicates, insertion order, public-call partitions
and exact save/replay. Malformed extensions, per-layer corruption, stale live
materials, either layer's capacity failure, output limits and retained numerical
allowances refuse atomically. The [Material Lab](material-lab.md#free-pair-collisions)
shows a wood–wood and a wood–armor pair through this path; angular/oblique
contacts are subsequent slices.
