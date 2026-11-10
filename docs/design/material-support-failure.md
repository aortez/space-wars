# A collapsing material support (#328)

`PhysicsWorld::step_material_collapsing_support(dt)` opts into a bounded,
irreversible failure response for one material layer under an ideal rigid
dynamic platen. The support may be fixed under constant normal gravity, or a
second free body with zero gravity. It uses the existing
[dynamic compression](material-dynamic-compression.md) law, geometry, mass limits
and numerical allowances.
The ordinary dynamic-compression API retains its refusal on capacity exhaustion.
An enrolled history keeps its selected policy; changing APIs cannot reset it.

## Failure boundary and work

The layer resists through the internal interval that spends its last capacity.
The sustained-load API supplies the capacity charge, capped at the precise
remaining amount. At that interval's endpoint the layer loses its load-bearing
response permanently. The boundary keeps the body's position and velocity,
emits one `CompressionFailure` and ends the contact episode.

This is an explicit dissipative-collapse policy:

- The last charge exhausts capacity exactly. Plastic work beyond that charge
  becomes `excess_work`; it is not additional damage or numerical error.
- The layer's stored elastic energy becomes `released_elastic` dissipation.
  The boundary adds no impulse and promises no elastic return afterward.
- Subsequent intervals have zero force, impulse, material work and charge.
  Permanent set and viscous loss stay fixed while the platen follows gravity
  or both free bodies coast.

The compression summary's energy balance is:

```
initial kinetic + initial elastic + gravity work
  = kinetic + elastic + damage + viscous loss
  + failure.dissipated() + charge error + energy error

yield force * permanent set = damage + failure.excess_work + charge error
failure.dissipated() = released_elastic + excess_work
```

Each `DynamicCompressionInterval` has `failure_dissipated`, zero except at the
collapse boundary. Add it to the load's elastic change, damage, viscosity and
charge error to recover signed material work. The last interval's endpoint
force and elastic storage are zero; its impulse still reports the resistance
during that interval. The rigid body's remaining motion is counted only once.

Failure time is quantized to the existing internal subdivision, bounded by
1/4096 second. This policy continues compression through the exhausting interval;
it does not locate a continuous fracture time inside it. Public-call partitions
that preserve the internal intervals preserve the failure and physical history.

## Identity, persistence and limits

`DynamicCompressionStep.failures` emits the observation once. Its source carries
the world-issued material-region incarnation and patch; its episode identifies
the contact. `elapsed` is absolute physical time, while ordinary contact events
retain their time within the current public call. The failure also retains the
boundary displacement, velocity, permanent set and energy disposition.

`material_compression_failure()` reads that retained observation without consuming
it, including after pause, restore, invalidation or body removal. The failed
region remains registered with zero capacity. Mass, body geometry and actor
ownership remain intact. Hosts retain responsibility for actor retirement and
debris; ordinary stepping or public edits invalidate this specialized continuation.

Fixed-support collapse selects snapshot v21 with its own tag and appended active
status. Its required bounded
extension contains the initial incoming motion, source and optional single failure.
Restore validates capacity, deformation and work, identifiers, contact ownership
and the post-failure ballistic trajectory against the original error allowances.
Earlier modes keep their v1–v20 formats. Missing extensions, retagged versions and
corrupt retained failures refuse, including after removal.

One material load is emitted per internal interval. Failure and contact End each
count toward the output limit. If the public call cannot accommodate them, the
whole call rolls back, including motion and capacity; a smaller call can fit the
same internal intervals. Numerical allowances never renew at failure or save/load.
The original quarter-depth relative-motion window still applies after collapse.

The explicit acceleration API permits only its current value. Two finite
materials, connection severing, region splitting and fragment motion remain
subsequent failure-lifecycle work. This path does not close #328.

## A moving material support

The same opt-in API also accepts the [moving-support](material-moving-support.md)
scene: two free, rotation-locked bodies with masses in `[5,80]`, zero gravity,
normal-only motion and material on the support alone. Both recoil before failure.
The relative material law uses reduced mass, and every reaction impulse acts
equally and oppositely. Collapse adds no impulse to either body. Each then keeps
its own velocity while moving through the failed envelope.
Coasting ends at the quarter-depth window, or earlier when a light pair exhausts
a position allowance. Any moving-collapse Lab case must fit inside that window.

The ordinary failure record reports relative velocity and indentation.
`material_moving_support_failure()` additionally retains both normal positions
and velocities at the internal collapse endpoint, in platen/support order. These
values remain fixed even when the public call continues beyond failure, or when
the bodies are later removed. They are observations, not actor-retirement commands.

The laboratory-frame energy balance is the sum of both bodies' kinetic energies,
elastic storage, damage, viscous loss, collapse dissipation and charge/numerical
errors. Total momentum has only the already measured numerical defects. Shared
translation cannot supply more damage or enlarge the existing error allowances.

Moving collapse selects snapshot v22 with a distinct tag and active status. Its
bounded extension requires the original two-body motion history, failure history
and optional boundary motion. Before failure there is no boundary motion; after
failure there must be exactly one. Restore checks both coasting trajectories and
velocities, including common motion invisible to the relative summary. Fixed
collapse keeps its frozen v21 layout; ordinary moving and paired modes keep v19
and v20 and continue refusing exhaustion.

## Demonstration and evidence

Material Lab's **Support gives way** uses an undamaged wood layer of represented
volume 2 and capacity 120, a mass-20 platen, initial gap 1/64 and load 8000.
It pauses at collapse, then Run continues the fall through a short observation
window. The display retains the failure, zero capacity and collapse dissipation.
See [Material Lab](material-lab.md#collapsing-support).

Public engine tests compare capacity crossing with the independent continuous
loading solution for masses 5, 20 and 80, both initially touching and approaching.
After failure they compare motion with the ballistic solution and close interval
and total work. All quarter turns, collider partitions/duplicates, insertion
orders and public-step partitions retain the same physical result. Unit tests
cover safe loads, mode restrictions, exact continuation, one-time emission,
output-limit rollback and corrupt/removed histories. The Lab tests and its full
UI workflow cover the pause, single-step, save/load and continued fall.

Moving-failure tests compare capacity crossing against the continuous law at the
reduced mass for equal and unequal mass pairs from 5 to 80, all quarter turns,
and shared velocities of -1, 0 and 1. They check both subsequent straight-line
trajectories, total and relative energy, momentum, one-time failure, exact replay,
safe contacts and identical histories across geometry/public-step partitions.
Native tests cover pending contact ownership, corrupt or missing boundary motion,
removal, output/motion limits, stale policy changes and frozen fixed-collapse saves.
The current Material Lab collapse demonstration remains the fixed-support case.
