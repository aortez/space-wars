# Timed ball-arrival experiment (#327)

The [native response adapter](material-native-response.md) can account for an
already-touching impact, but ordinary speculative solving may spend its incoming
energy before a physical impact episode exists. Ordinary CCD can also leave a
penetrating arrival pose. Those counterexamples remain covered by regressions;
the production adapter continues to refuse these unsupported paths.

`crates/engine-rapier/src/world/loading/response/tests/arrival.rs` demonstrates
an alternative for an isolated pair: advance to the first physical contact
without speculative braking, then run the existing native material response.
It is fixture code with asserted assumptions, not a runtime step API or general
CCD replacement. No vendor code, solver settings or scenario behavior changes.

The follow-up [native isolated-ball operation](material-native-arrival.md) now
provides bounded admission and transactional stepping for this restricted case.
The fixture below remains independent evidence for the arrival sequence.

## Sequence and scope

The fixture contains exactly two nonrotating balls, represented by two to eight
colliders. Coincident duplicates on each body share the same radius and physical
patch. Collider centers coincide with body origins. There are no forces, gravity,
damping, friction, joints, sensors, contact skins or other bodies. Supports are
fixed or translate at constant velocity; the second body may also be dynamic.
Ordinary CCD is disabled in this fixture. The isolation and collision-filter
assumptions are supplied by construction, not inferred from a partial contact
graph.

For relative center offset `d`, approach velocity `v` and combined radius `R`,
the first contact solves `|d - v*t|² = R²`. The fixture uses f64 intermediates
and the stable smaller root `c / (b + sqrt(b² - a*c))`, with `a = v·v`,
`b = d·v` and `c = d·d - R²`. The square root uses `libm`. Separating pairs
and misses have no positive arrival; exact tangency has no normal impact budget.

Each test frame does at most one drift and one ordinary response step:

1. Drift for the lesser of time to contact and remaining frame time. A temporary
   local contact hook removes separated speculative points only. It asserts
   that no touching/penetrating point is suppressed. Rapier integrates the
   motion with its default solver settings; no position is snapped or teleported.
2. Restore the collider hook flags. If frame time remains, call the native
   `PhysicsWorld::step` with that remainder. The usual capture creates the
   physical episode, and transactional material response applies at the actual
   manifold before further motion.
3. Once the pair is touching within `1e-6`, use the ordinary native step. The
   checked response still enforces its existing `1e-5` contact tolerance.

When contact lands exactly at the frame endpoint, response occurs at the start
of the next step, before further integration. The fixture neither charges early
nor introduces a fake positive-duration solve. A runtime implementation must
define endpoint event publication explicitly.

Removing speculative constraints is justified here by the complete isolated
ball trajectory. It would be unsafe to copy the hook into arbitrary scenes:
an unseen third body, constraint or force could invalidate the free-flight proof.
There is no partial admission, event-loop recovery or general rollback policy
implemented by this fixture.

## Evidence

Four native tests cover:

- Initial gaps 0.001, 0.01, 0.1 and 2 at relative speed 400; 30/60/120/240 Hz;
  fixed/moving supports; reversed insertion; one/two/four coincident collider
  representations. Each run has exactly one response, exactly the same material
  outcomes and remaining capacities as a touching reference, and matching
  flight time. Later solver steps do not spend the work again.
- Oblique arrivals against fixed, translating and dynamic bodies. Actual
  kinetic loss plus signed prescribed work closes the material ledger under
  default backend settings. The dynamic pair also conserves linear momentum.
- Relative speeds 0.25, 1, 40 and 120, including an exact frame-boundary arrival.
  These cover low-energy rebound as well as damaging impacts. Flight-only frames
  preserve energy and capacity, and each contact is charged once.
- Tangential misses, near misses and snapshot continuation after a flight-only
  frame. Misses preserve capacity and energy; restored worlds reproduce response
  records and serialized state exactly through arrival.

Impact energy closure tolerance is `1e-5` of initial energy (including transport
energy for the world-frame driver check). Misses use only a 64-f64-epsilon energy
tolerance for arithmetic roundoff. These are bounded fixture results, not proof
of timestep independence for arbitrary scenes or calibration of the pulse model.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib response::tests::arrival
```

## Integration beyond the isolated pair

The native follow-up supplies admission and transaction semantics for two balls.
A broader path must validate the complete scene before suppressing any solver
contact, find the earliest arrival across all supported collider pairs, and
recompute the schedule after each response. Simultaneous arrivals must enter one
coupled batch. It also needs bounded work/subdivision counts, explicit fallback
when admission or numerical arrival checks fail, and no lost capture time,
episode transitions or output records across internal advances. The isolated
native path defines snapshot and frame-end behavior; broader scheduling must
preserve that contract across multiple physical arrivals.

The fixture resets transient capture output between its drift and response;
the drift has no material episodes by construction. That shortcut does not
establish correct lifetime handling for existing supports or repeated impacts.
Flat faces, rotating/offset shapes, friction, joints, external work and general
CCD remain unfinished, as does pulse-allocation calibration. This experiment
establishes a viable isolated arrival sequence rather than completion of #327.
