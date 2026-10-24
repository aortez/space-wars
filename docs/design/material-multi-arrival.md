# Three-body arrival experiment (#327)

The original native arrival operation admits two balls. This experiment in
`world/loading/response/tests/multi_arrival.rs` establishes scheduling evidence
for exactly three collinear, centered, rotation-locked balls of radius 0.5.
Fixture construction supplies full registration, collider bindings, equal
radius duplicates, zero friction, no gravity, forces, joints, damping or CCD,
and disabled sleeping. It is not a runtime admission checker or public API.
The separate [native three-ball operation](material-native-line-arrival.md) now
implements this restricted scheduler; these fixtures remain an independent
reference, including the timer-only refusal below.

## Response before choosing the next duration

An impact changes the velocities that determine the next arrival. Choosing all
arrival times from the incoming velocities misses a collision caused by the
first response. Advancing the whole remaining frame after that response can
also cross another physical contact before its work has been accounted for.

The fixture stages the complete frame in a snapshot clone. For each advance:

1. Make a disposable preview clone and invoke the existing native capture and
   response through a test hook. Read the rounded velocities immediately after
   response, before ordinary solving/integration. Preview capture observes zero
   elapsed time, so preview alone cannot age absence or spend authoritative work.
2. Recompute all three pair arrival times from those velocities and the current
   positions. For these ordered collinear balls, an approaching separated pair
   arrives after gap divided by relative speed. Take the earliest arrival or
   the frame remainder. Do not advance through an unresolved touching approach.
3. Run the actual duration on the staged world. Require its material response
   and immediate post-response velocities to match the preview. Require ordinary
   solving to leave those velocities unchanged and final geometry to stay finite
   and outside the existing penetration limit.
4. Repeat with the new positions and velocities. Publish only a completely
   accepted frame; otherwise discard all staged state and retain prior outputs.

The preview's integrated state is never authoritative: a preview using the full
remainder may cross later contacts, which is precisely why the scheduler uses
the velocities observed at the pre-solve boundary instead of its final motion.

Test hooks suppress separated speculative points before they can brake the
incoming motion. They also suppress separating contacts: in these straight,
force-free collinear trajectories such pairs diverge. Resting touching contacts
remain available to the coupled response. All nonseparating contacts within the same
`1e-6` touching tolerance are processed together, in existing canonical order;
nearby future arrivals outside that tolerance are not merged or charged early.

The fixture bounds each frame to eight accepted advances and at most sixteen
backend calls, counting previews. Existing capture/output limits also apply to
the accepted advances. Snapshot cost still depends on backend storage/history.
The final f32 duration is the rounded frame remainder; no extra sub-ULP solve
is added to consume a f64 subtraction residue. Captured durations match every
accepted advance and sum to the outer duration within f32 rounding tolerance.
An arrival at the frame endpoint responds on the next positive-duration call.

## Positive evidence

All three dynamic cases use armor, armor and wood, respectively, with one stable
region per body and the existing reference sections/volumes:

| Case | Incoming configuration | Required accounting |
| --- | --- | --- |
| Sequential | Mass-1 A approaches mass-20 B at 40 wu/s across gap 0.01; B and mass-20 C initially share velocity, with gap 0.002 | A-B response first; recomputed motion produces B-C response in the same frame, each with one distinct root |
| Simultaneous | Three mass-20 balls; A and C approach B at relative speeds +40 and -40, each across gap 0.01 | One response batch, two roots, two patches and one shared normal component |
| Supported | Three mass-20 balls; B-C already has a resting episode before A approaches across gap 0.01 | One incoming root; both patches receive allocated work; B-C retains its support episode without inventing incoming energy |

Sequential first arrival is approximately 0.00025 s and second arrival is
approximately 0.0011283 s. The second is independently checked against the
two-body momentum/restitution solution for A-B. Simultaneous/supported cases
respond at approximately 0.00025 s.

Tests sweep 30/60/120/240 Hz, common velocities 0/17, one/two/four coincident
collider representations and both insertion orders. They check actual kinetic
loss against summed material ledgers, dynamic momentum, persistent capacities,
canonical patch ownership, capture time/substep indices, absence of extra
responses, and exact same-build snapshot/output replay on every frame. Timing
comparisons use spatial contact tolerance divided by approach speed: f32 world
positions on a translating frame can move the slow second arrival by about
0.21 microseconds without changing its material accounting. Energy tolerance
remains `1e-5` of initial relative kinetic energy.

Additional cases verify fixed and translating prescribed C supports with signed
driver work; distinct arrivals separated by a larger gap are not merged;
frame-end arrival, pause and invalid durations; and atomic rejection after an
already accepted response when the advance or capture budget is exhausted.

## Retained counterexample: rapid return to a spent episode

Changing the sequential case to three mass-20 balls and a B-C gap of 0.02
produces A-B, then B-C, then A-B again. The third hit occurs before the existing
`1/60`-second separation grace has rearmed A-B. Its episode still exists, so
capture does not issue another impact root. The scheduler detects the unresolved
touching approach and refuses the whole frame instead of silently spending
ordinary-solver work or automatically charging the old episode again.

A separate test stops after the first two accepted impacts at 0.0015 s, then
attempts the return. That failure preserves the prior snapshot and output
records exactly. The lighter projectile in the successful sequential fixture
avoids this rapid return; it does not establish general repeated-collision
support for equal masses.

This is the next semantic boundary before native multi-body integration:
distinguish verified physical separation/reimpact from contact chatter without
letting arbitrary scheduler subdivisions mint impact roots. Simply lowering
the grace time or clearing episodes after each response would not establish
that contract.

The optional [verified-departure follow-up](material-verified-departure.md)
now admits this sequence in the fixture by recording physical departure during
accepted drift, then letting the return acquire a fresh episode. The timer-only
counterexample above remains unchanged. The native three-ball follow-up uses
verified departures, while the original two-ball operation keeps its contract.

## Remaining work

These fixtures do not introduce a scenario, establish general 2D/rotating/flat
geometry scheduling, or complete #327. The native follow-up supplies whole-scene
admission, persistent reimpact ownership and output/metric semantics only for
three horizontal radius-0.5 balls. The preview/capture architecture remains
experimental. Pulse-allocation calibration remains unresolved; conservation
and replay evidence do not establish physical accuracy.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib three_body_
```
