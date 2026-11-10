# Moving finite-mass support (#327)

`PhysicsWorld::step_material_dynamic_compression(dt)` also admits two free
translating bodies: an ideal rigid platen and a support carrying the existing
finite material layer. Contact slows the platen and accelerates the support.
Material work follows their relative compression, so shared translation does not
supply damage energy.

This extends the [falling contact history](material-impact-support.md). It reuses
the same unilateral elastic/plastic/viscous law, capacity charge, subdivision,
Impact/Support ownership, separation and atomic step transaction. It introduces
no second impact charge and does not change fixed-support stepping or save bytes.

## Admission

The existing full-face rectangle, source binding, section, quarter-turn and
bounded translated-frame checks still apply. Both bodies are dynamic,
rotation-locked, nonsleeping, and have refreshed masses in `[5,80]`. Their motion
is along the contact normal; individual normal velocities are in `[-64,64]`.
Only the support has finite material. Joints, external forces, tangential motion,
rotation and gravity are excluded.

Enrollment starts at a gap no larger than one quarter of the material depth,
or just touching, with positive relative closing speed at most 64. Preloaded or
already-owned contact cannot enroll. The same history persists through yielding,
unloading and separation. There is no external force to bring a separated pair
back together. A change through the fixed-support acceleration API refuses;
repeating zero is a no-op. Ordinary body/gravity edits invalidate the history.

## Motion and work

For platen/support masses `m0,m1`, define `mu = m0*m1/(m0+m1)` and relative
closing velocity `v = v0-v1`. Reduced mass is computed in f64 from the actual
refreshed masses. The scalar midpoint solve uses `mu` and the existing material
force. The midpoint velocities distribute relative drift around the current
center-of-mass velocity. After measuring both actual backend positions, the
constitutive reaction supplies equal and opposite impulses to the two bodies.

The auxiliary Rapier step has no forces, joints or solver contacts. This mode
uses one backend solver subdivision for that drift, avoiding repeated f32
rounding of both moving centers; it restores the configured iteration count
before checking the scene. The material solver's timescale subdivisions remain
in force. Fixed-support modes retain their original backend subdivisions.

The ordinary compression summary reports relative motion and `mu*v*v/2`.
`material_moving_support_summary()` additionally reports the masses, normal
positions and velocities, individual momenta and kinetic energies, and retained
numerical defects. With reaction impulse `J`, total laboratory kinetic energy
`K`, elastic energy `E`, permanent displacement `p`, and viscous loss `L`:

```
K_initial = K + E + Y*p + L + energy_error
-J = m0*(v0-v0_initial) + platen_impulse_error
 J = m1*(v1-v1_initial) + support_impulse_error
position_i-position_i_initial = integral(v_i dt) + position_error_i
Y*p = capacity_consumed + charge_error
```

Each velocity integral uses endpoint averages. The relative-motion balances are
also checked, including their consistency with the two individual integrals.
Errors are numerical defects, never extra damage. Each body has a fixed position
allowance of `1e-5*depth`; relative position has their combined `2e-5*depth`
allowance. Energy and impulse retain the `1e-5` factors on `Y*depth` and
`sqrt(2*mass*Y*depth)`. Common speed does not enlarge these scales. Absolute
defects accumulate across every interval and survive pauses, saves and separation.
Small bodies, large translations or long runs can exhaust an allowance. The
entire public call then refuses without changing either body, capacity or output.
Finite-region failure also still refuses; its lifecycle remains #328.

## Save and qualification

Moving histories select snapshot v19, a distinct tag and appended active status.
A required fixed-size extension stores both masses, initial/current motion,
individual velocity integrals and cumulative signed/absolute defects. Restore
checks relative and laboratory balances, live motion/masses, material and contact
phase. Even a common velocity edit that preserves relative motion is stale.
Invalidated histories retain their totals after both bodies are removed.
Missing, truncated, trailing and incompatible-version saves refuse. Existing
v15-v18 histories retain their layouts, discriminators and frozen-byte tests.

Public tests compare the relative trajectory against the continuous elastic and
plastic solution at the actual reduced mass, across unequal masses, all quarter
turns and common normal velocities. They check individual recoil, total momentum,
work, permanent set and unchanged damage under shared motion. Split/duplicate
geometry, insertion order and public-call partitions retain identical physical
totals. Restore repeats native bytes and events, including the pending same-owner
Impact-to-Support observation. Corruption and boundary tests cover independent
body balances, external edits, capacity failure and exhausted numerical budgets.

The [paired-material extension](material-paired-compression.md) admits finite
layers on both free bodies with separate deformation and damage histories.
This remains a bounded #327 path. Externally driven moving supports,
angular/oblique contacts, connection loads and transfers between older
loading modes remain. Material Lab demonstrates the fixed-support cases and two
paired-material collisions; gameplay integration and persistent failure
consequences are later slices.
