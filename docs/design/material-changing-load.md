# Changing normal loads in retained compression histories (#327)

`PhysicsWorld::set_material_dynamic_compression_acceleration(a)` changes normal
gravity at a public-call boundary of an active dynamic-compression history.
The next `step_material_dynamic_compression(dt)` uses that acceleration. The
same body, material, permanent indentation, contact owner and numerical allowances
continue through the change. No pose, velocity or material capacity is reset.

This extends [dynamic support](material-dynamic-compression.md),
[rebound](material-rebound.md) and [falling impact/support](material-impact-support.md).
It retains their two-body, fixed-support, full-face rectangle, refreshed-mass and
quarter-turn restrictions. Only the support has finite material. The new command
changes the gravity force on the existing mass; it does not add mass, apply an
instantaneous impulse, or represent a moving driver or contact-network load.

## Command and work contract

Enrollment still requires an ordinary positive dynamic-compression step.
Acceleration is along the admitted contact normal, including in turned frames.
Each new value must satisfy `0 <= a <= 1e6` and `mass*a <= 2Y`, where `Y` is the
initial layer's yield force. Negative/tangential forces remain unsupported.
An actual change switches to the signed unilateral solve, including for an older
monotone-overload or exact-equilibrium history. Release, separation and renewed
loading therefore retain plastic set and remaining capacity.

Each accepted change records elapsed time, measured indentation and the new
acceleration. There are at most 32 changes per physical history. Repeating the
current value is a no-op, including at the limit; distinct changes while paused
consume entries but add no time, work or impulse. Invalid commands, stale histories
and exhausted change capacity refuse without changing state or ephemeral output.
Ordinary gravity edits and other stepping retain their existing invalidation
behavior. The explicit API cannot revive an invalidated history.

For each constant-force segment, accumulate `W * displacement` and `W * duration`
with that segment's weight, rather than multiplying total displacement or elapsed
time by the newest weight. Force changes themselves perform no work. The balances
are:

```
K0 + E0 + gravity_work = kinetic_energy + elastic_energy + damage_work
                       + charge_error + viscous_loss + energy_error

gravity_impulse - reaction_impulse = mass*(velocity-v0) + impulse_error

indentation-x0 = integrated_velocity + position_error
```

`material_dynamic_compression_gravity_impulse()` exposes the integrated source
impulse, also after invalidation. Existing summary and interval fields retain
their meanings and layouts. Initial material and motion, total damage, permanent
indentation and absolute numerical defects all survive force changes. The fixed
`1e-5` allowances use the original `[Y*depth, sqrt(2*mass*Y*depth), depth]` scales.
Small force changes cannot renew an exhausted allowance. Failed-region continuation
still refuses atomically; failure consequences belong to #328.

An existing contact owner is retained. A falling history also retains its pending
same-owner Support observation across a force change or save. Later recontact
uses the original enrollment convention: Impact/Support for falling histories,
Support for resting histories. A force command alone emits no contact event.

## Saved-state compatibility

An actual force change selects snapshot v18 and a distinct changing-load tag.
Revision 4 uses an appended active status variant; the shared history serializer
still skips incoming-motion and changing-load fields. Its required extension
contains optional initial incoming motion, a bounded count and the force-change
records. Restore checks the count before allocating records, then validates
ordered times, equal-time positions, acceleration bounds, source work/impulse,
live gravity, material, motion, owner phase and cumulative error allowances.

Unchanged histories retain v15/v16/v17 selection and bytes, including no-op force
commands. Frozen serializers cover the old field layouts and continued replay.
Missing, malformed, truncated or trailing extensions refuse. Rewriting the
header/tag and stripping the extension cannot downgrade the history. Invalidated
v18 histories retain their source integrals after bodies and regions are removed.

## Evidence and remaining path

Public-API tests compare piecewise elastic loading to a continuous oscillator
computed from its original state, across masses `[5,80]`, all quarter turns and
translated frames. A yielded layer unloads, separates and returns to its permanent
surface without recovering capacity. Split/duplicate contacts, insertion order
and public-call partitions preserve identical physical totals. Restore reproduces
events and physics bytes, including a pending Impact/Support handoff.

Limit and corruption tests cover zero-time commands, all 32 change entries,
invalid acceleration, terminal histories, source-integral tampering, incompatible
save discriminators and malformed counts. A longer translated oscillator reaches
the existing cumulative position-error allowance; changing force, pausing and
restoring cannot reset it. This is a bounded solver, not indefinite simulation.

The #327 audit still leaves the following work before general tank/wood contacts:

| Capability | Current boundary / next extension |
| --- | --- |
| Canonical sources, episodes and pre-solve capture | Existing [record stream](material-contact-records.md); broader response remains restricted. |
| Impact, support, unloading and changing force | One retained history for the admitted two-body layer; other loading-mode transfers remain. |
| Real tank/wood contacts | Multiple finite material bodies, moving supports, oblique/angular geometry and connection loads remain. |
| Soft vegetation | Attributable swept/nonblocking loading remains. |
| Failure and gameplay | Persistent region/failure lifecycle (#328), weapon deposition (#329), then Scorched integration (#331). |

The [Material Lab](material-lab.md) exercises constant-load cases and a guided
load/release/return case, with visible force, work and retained damage. Its load
controls and saved command replay use this API without broadening its physical
limits. This does not complete #327 or enable material damage in Clock or Scorched
gameplay.
