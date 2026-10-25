# Constant-gravity landing and return fixture (#327)

`response/tests/flat_arrival/gravity/ballistic.rs` qualifies a candidate
force-integration contract independently of native stepping. It uses the
existing two-plate fixture: a mass-20 dynamic plate, a fixed plate centered at
x=1, locked rotation, no friction, and constant acceleration toward +x. The
dynamic plate touches at center x=0; the physical contact plane is x=0.5.
Equivalent slices and duplicate colliders represent the same physical plates.

The base fixture supports free flight, a first landing and outbound rebound.
The optional departure tracker additionally qualifies genuine returns to the
same face. An opt-in support experiment adds safe rest and bounded numerical
settling of small unresolved hops. This does not expand native admission, add
production APIs, select gameplay damping, or change native snapshot formats.

## Separate the impact from gravity integration

For an interval of duration `h`, flight uses:

```text
v_mid = v_start + a*h/2
x_end = x_start + v_mid*h
v_end = v_mid + a*h/2
```

The two velocity kicks are explicit, rounded backend velocities. Rapier performs
the intervening force-free drift with its existing internal solver subdivision.
The final kick restores the first kick's rounding residual: it rounds
`v_start + a*h` once, rather than adding another rounded half-increment to the
rounded midpoint. The drift still uses the actual stored midpoint velocity.
Before rounding, this gives the exact constant-acceleration endpoint and closes
`m*a*(x_end-x_start) = kinetic_end-kinetic_start`. It removes the ordinary
semi-implicit free-flight defect measured in the [backend fixture](material-gravity-loading.md).

The positive root of `gap = v_start*t + a*t*t/2` splits a flight at its arrival.
An approaching body uses the cancellation-resistant form
`2*gap/(v_start+sqrt(v_start*v_start+2*a*gap))`. A departing body uses the other
form to retain the later positive return time. For positive acceleration the
position curve is convex, so its maximum over the interval occurs at an
endpoint. Checking both endpoints proves the entire interval stays outside the
fixed face, including a flight through its apex. Only with this proof does the
drift hook remove contact constraints, including speculative contacts.

At a touching approach, an internal **zero-duration Rapier solve** captures and
applies the existing material response at the physical incoming velocity. No
gravity kick occurs during this phase. Body positions must remain bit-identical,
one material response must apply, and its ledger must match the rounded kinetic
loss. The existing checked material response still validates its own budget.
An arrival exactly at the end of a requested frame resolves in that frame.

This zero-duration impact phase is distinct from a public pause: an externally
requested `dt=0` only clears ephemeral outputs and preserves physical state.
It also avoids treating a future half-kick as part of the incoming collision.

## Actual work, rounding and rollback

Each flight records gravitational work from actual backend displacement and
the difference between that work and actual kinetic change. This difference
is retained as numerical error; it never becomes a material charge. Flight
acceptance requires an error no larger than `1e-5` times the larger endpoint
mechanical energy, using potential relative to the touching-center position.
Geometry and rounded drift are checked separately, and no endpoint may exceed the touching
tolerance.

Tracked/support modes also retain `ballistic/numerics.rs`'s ledger across calls
and serialized continuation. Each flight adds three signed discrepancies:

```text
work_error     = actual_gravity_work - actual_kinetic_change
impulse_error  = mass * (a*h - (v_end-v_start))
position_error = (x_end-x_start) - (v_start*h + a*h*h/2)
```

At every accepted endpoint, each cumulative magnitude must stay within `1e-5`
times its fixed initial scale: mechanical energy, `sqrt(2*mass*energy)`, or
`max(abs(initial_x), 1)` respectively. The unit distance is the certified
plate's longitudinal extent; it is not a departure uncertainty floor. Opposite
observed discrepancies can cancel, but a new frame, impact, End, settling
projection, pause or snapshot restore cannot clear the ledger or enlarge its
scales. The ledger checks exact motion/mass/gravity continuity; an error too
small for its f64 accumulator to retain refuses instead of disappearing.

Consequently, a tiny frame remainder whose kicks round to unchanged velocity
may complete with its full duration and measured drift. Its missing impulse
and work discrepancy remain explicit and bounded. They are numerical errors,
not deferred forces, material work, or another collision budget. Repeated
unrepresentable steps eventually refuse if their cumulative discrepancy grows
beyond the allowance. A whole-call refusal rolls back the ledger as well as
motion, material changes and prior outputs. The bare untracked experiment still
refuses an unchanged kick because it has no persistent error ledger.

Physical-horizon tests additionally compare actual motion, work and capacity
with independent trajectories. The ledger bounds signed sums of the reported
flight defects; it is not a proof of arbitrary-horizon trajectory accuracy or
a replacement for the separate material-response/support rounding checks.
The native impact tolerance is unchanged.

A whole requested step runs on a staged world. Invalid time, unsupported
geometry, an unaccounted approach, numerical failure, unsupported support or
reimpact, or the eight-advance limit preserves the original world, capacity and
prior outputs. Each flight or held interval counts as one advance.
That includes a refusal after an earlier damaging arrival within the same call.
World gravity is restored on success; original gravity is untouched on refusal.

## Accelerated departure and reimpact

`ballistic/departure.rs` retains one witness after a successful instantaneous
impact: the ordered source pair, episode ID, actual position/velocity/mass and
acceleration. Each subsequent flight must start at that exact state under the
same owner and gravity. Collider bindings and live region ownership are checked
again. Pose/velocity/gravity edits, a changed episode or retired source refuse
without using stale evidence.

Tracked flight splits at the apex as well as the next arrival. A complete
round trip can otherwise have both endpoints at contact, and a force-free
drift at the interval's average velocity could show no actual separation.
The apex split supplies an observed backend endpoint near maximum separation.
Its duration rounds toward/just beyond the turn, matching the compensated final kick,
so a residual negative velocity does not force a spurious tiny turn step.

After each validated flight the conservative separation is
`min(actual_gap, ballistic_predicted_gap) - uncertainty`. The uncertainty is
the scale-aware bound on backend drift plus the measured midpoint-kick rounding
contribution. For this force-free, rotation-locked fixture, the drift bound
accounts conservatively for the timestep division, velocity multiplication,
position additions and COM/writeback overhead. It uses
`16*(N+4)*f32::EPSILON*(abs(start_x)+abs(midpoint_velocity*h))`, plus an absolute
subnormal allowance; `N` includes the largest additional body iteration count.
Excessively large iteration bounds refuse. Actual drift must satisfy this bound
before it can be used as evidence. There is no added one-world-unit floor.
The fixed face is checked to remain exactly at its known pose.
Only a lower bound strictly beyond twice capture's touching tolerance retires
the episode. Until then, the witness retains the exact new endpoint even if
the actual endpoint has rounded beyond the exit band. There is no new
starting-gap eligibility test that can strand this pending evidence.

The trace emits `End` after that flight and before any later impact, including
transitions inside one requested step. The timestamp records when departure
was proven, not an interpolated exact exit-crossing time. End records count
against capture limits; the staged world and tracker commit together. A later
arrival receives a fresh root only after this retirement. Without the optional
support experiment, a short excursion whose apex never proves exit still
refuses reimpact, even if time has elapsed.

The fixture's `Verified` wrapper serializes the physics snapshot together with
its witness for exact continuation. Public pause preserves both. This is a
local test snapshot wrapper, not a new native snapshot version. Without the
tracker, the original first-landing operation still refuses a later return.

## Safe support and bounded numerical settling

`ballistic/support.rs` is an explicit fixture policy. At rest it evaluates the
pure sustained law for both regions using `force = mass * acceleration`, the
certified width 2, each registered section, and measured plastic displacement
zero. Both materials must be intact and below their safe force. A yielded or
failed region refuses; a static reaction does not supply deformation work.
The law's recoverable strain energy is a constitutive reference, not energy
deposited in these undeformed rigid bodies. It is not applied to a work ledger
or persistent capacity.

Each held interval uses a gravity half-kick and a zero-duration ordinary
Rapier contact projection with restitution zero, a force-free stationary drift,
then a second half-kick and projection. Internal slice faces are suppressed.
Each projection clears its contact warm-start cache: the incoming kick is
measured separately, and cancellation against a larger previous cache can
inflate or erase a tiny interval's reaction. This applies only to these
auxiliary fixture projections, as in the already-qualified native operation.
The intervening drift suppresses all impulses: the preceding projection
already established support, and warm-start or bias corrections must not move
the held body. Positions stay bit-identical; no pose is snapped to the face.
Contact impulse reports must match `mass * acceleration * duration` within
`1e-5` relative error. Actual gravitational work and final kinetic change are
both zero.

The iterative f32 projection can leave a tiny signed residual velocity. Only
a residual no larger than `1e-6` of the incoming velocity may be projected to
zero. Its signed impulse and kinetic loss are recorded separately from the
raw solver impulse and included in the total projection loss. The half-kicks'
auxiliary kinetic additions and the complete projection losses must cancel
exactly. These quantities depend on subdivision; they are not physical
gravitational work, material damping, or extra damage. Larger residuals refuse.

Resting capture creates one Support episode if none exists. A landing retains
its original Impact episode through rest. Each held interval records its
reaction under that same owner; it never creates a new impact budget. Pause
and the fixture snapshot wrapper preserve the held witness.

The last small hop needs a distinct transition. The departure witness retains
the post-impact mechanical energy and a conservative upper bound on the
entire excursion. That bound uses the continuous ballistic apex, actual and
predicted endpoints, and the flight's rounding allowance. Settling requires
an observed turn, an upper separation bound within twice capture's touching
tolerance, a touching approach, and safe sustained support. An ambiguous
excursion that proves neither exit nor containment refuses. Merely retaining
an old episode is not permission to settle it.

An eligible return uses the same zero-duration, zero-restitution projection
without material capture. Its actual kinetic loss is recorded once as
**numerical settling dissipation**, bounded by the episode's retained
post-impact energy with `1e-5` rounding allowance. It cannot reduce material
capacity or obtain another budget. Any potential energy associated with the
unchanged touching pose remains in the mechanical-energy check. This is a
candidate numerical policy, not calibrated material damping or a selected
production restitution target. Subsequent held intervals cannot spend the
settling budget again.

Settling and support records count against capture's per-step record limit.
World, witness, material changes and trace commit together, including a late
refusal after a provisional settling projection.

## Arrivals and returns against existing support

`ballistic/support/incoming.rs` adds a three-plate fixture: free plate A approaches
plate B, which already rests on fixed plate C. Their touching centers are x=0,
x=1 and x=2. Both dynamic plates have mass 20 and locked rotation. Ordinary
capture establishes B-C's Support episode before A starts its approach.

Each free-flight interval combines A's ballistic kicks/drift with B's support
half-kicks and zero-duration projections. These projections admit only B-C,
even when A reaches contact at the interval endpoint. They verify that all
other body velocities remain unchanged, so the support projection cannot
consume A's arrival energy. The common drift checks every stationary plate's
pose and velocity exactly. The support reaction balances B's weight; its
auxiliary kick/projection energies cancel as in the two-plate experiment.

At arrival the ordinary instantaneous material response sees **both A-B and
B-C together**. Capture creates one new Impact root for A-B. B-C retains its
old episode and transmits part of that new pulse, despite initially having
zero closing speed. The coupled budget must equal A's actual incoming kinetic
energy. B's weight, previous reactions and constitutive strain-energy reference
do not add to that budget. Material loss at B-C is allowed when funded by the
incoming pulse; a continuing static reaction alone still charges nothing.

Both contact impulses must match `mass_A * (incoming_A - outgoing_A)` within
the measured momentum tolerance. B's two reactions balance, and C performs zero
prescribed work. The default fixture requires exactly stationary B after the
response; the stack opt-in below additionally accounts for a bounded numerical
residual. Across the physical horizon, actual kinetic energy plus material and
explicit numerical losses balances initial kinetic energy plus A's measured
gravitational work. Total momentum separately includes gravity on both dynamic
plates, B's weight reaction, the instantaneous impact reaction at C, and any
recorded numerical correction.

The fixture witness retains all three sources and exact motion/mass, gravity,
the old support episode and the optional new arrival episode. Serialized replay
preserves it. A requested step stages world and witness together; record limits,
stale evidence and numerical failures preserve earlier state and outputs.
If the impact exhausts either supporting region, the entire call refuses,
including an arrival exactly at a frame endpoint: this fixture cannot model
the resulting loss of support. General failure consequences remain outside it.

The extension now admits repeated A-B departures and returns while B stays
supported. After each flight, A-B ends only when both the actual and predicted
gap, less the drift uncertainty, exceed twice capture's touching tolerance.
The exact motion witness advances even when rounding crosses that threshold
before the prediction does. Splitting outbound flight at the apex ensures a
long call observes a genuine separation before scheduling the return. An End
record consumes the same bounded record allowance as other transitions, and
always precedes a later Impact for that pair. B-C's support owner is untouched.

Every return must obtain its own finite budget from A's current incoming
motion. The persistent numerical ledger and its initial bounds survive End and
reimpact. A contained hop cannot retire its owner or obtain another budget.
The default fixture still refuses three-body settling and stationary full-stack
support. The explicit stack opt-in below supplies a separate coupled projection:
B-C must then support both dynamic bodies. The B-C-only projection used during
A's free flight cannot represent that load.

The selected f32 flight durations must sum exactly to the requested time in
f64. The scheduler rounds a duration down if it would exceed the remaining
time, carries any residual into a further bounded interval, and refuses a sum
whose exact residual cannot be retained. A rounded remainder previously lost
about 0.47 ns in a 60 Hz frame. Executing that tail exposed an inflated support
reaction from cancellation against the previous warm-start cache; clearing
projection caches fixes the reaction without changing the physical accounting
or its tolerances. Small flight rounding errors remain charged to the same
persistent numerical allowance, never to material capacity.

## Coupled full-stack support and settling

`incoming/stack.rs` adds explicit fixture-only support with optional bounded
settling. Strict support admits an initially stationary stack or an exact stop
from the ordinary material response. Optional settling also admits the final
contained return. Neither mode expands the public native operation's two-plate
admission rules.

At rest, A-B carries `mass_A * gravity`, while B-C carries
`(mass_A + mass_B) * gravity`. Each contact checks both of its material regions
against that force with zero measured plastic displacement. Any yielded or
failed region refuses, including a B-C contact that safely held B alone but
cannot hold the combined weight. These checks do not model failure consequences.

Each held interval applies a gravity half-kick to both dynamic plates, projects
both contacts together, drifts the stationary bodies without contact or gravity
work, then repeats the kick and projection. Both contact impulses are measured
and checked against the bodies' actual momentum changes. Accepted poses remain
exact, both velocities end at zero, and both original episode owners survive.
Physical work and material capacity loss remain zero; the auxiliary kick energy
equals the complete projection loss. Each record retains both reactions and the
signed residual impulses and residual kinetic loss separately.

The coupled projection clears contact caches and temporarily sets the backend's
warm-start coefficient to zero. Clearing the initial cache alone was insufficient:
with four internal steps, reusing an impulse within a zero-duration solve produced
a separating rebound equal to one eighth of the incoming speed. Subsequent
unilateral solves cannot remove that rebound. The selected coefficient is local
to this auxiliary operation and is restored before an accepted step commits.
Each projection permits at most 32 raw solves, and a requested step permits at
most 256 coupled projection solves. Only residual speeds within `1e-6` of the
incoming speed scale can be explicitly removed; momentum checks retain their
`1e-5` tolerance. Failed convergence or an exhausted limit refuses the whole call.

Settling requires the retained A-B owner, an observed turn, and a conservative
upper bound on the entire excursion no greater than twice capture's touching
tolerance. Its approaching kinetic energy must fit the episode's retained
post-impact mechanical energy with the existing `1e-5` allowance. Combined
support must be safe before projection. One coupled projection records the
actual kinetic loss as numerical settlement, leaves capacity unchanged, and
marks that excursion held. Later support cannot spend that budget again. An
excursion that proves neither departure nor containment still refuses.

Longer material-impact histories exposed a separate tiny recoil of B: one
reproduction left velocity `-1.1920929e-8` after an otherwise accepted coupled
response. The stack opt-in permits removing this residual only when its magnitude
is at most `1e-6` times A's incoming speed. It records the signed reaction
`mass_B * velocity_B` and actual kinetic loss `0.5 * mass_B * velocity_B^2`
under B-C's existing owner. Material-loss validation first includes B's actual
post-response kinetic energy; the correction is not attributed to the material.
Cumulative signed correction impulse and dissipated energy must remain within
`1e-5` of the initial flight momentum and energy scales. These fixed allowances
survive frames, End records, new impacts and snapshots; they are not replenished.
A larger recoil, an unrepresentable accumulated charge or exhausted allowance
refuses. Freely moving B remains outside the fixture.

The snapshot wrapper retains the selected policy, excursion evidence, settlement
state and cumulative corrections alongside the existing flight ledger. Support,
settlement and correction records consume the same per-step record allowance as
other transitions. Exact duration accounting includes flights and held intervals.
World, witness and outputs commit together, including when a late interval,
record or projection limit is reached after a provisional settlement.

## Evidence and remaining scope

The first four regressions cover:

- 192 first-landing configurations: safe and damaging incoming motion,
  30/60/120/240 Hz, 1/4/8 internal solver steps, two plate partitions, one/two
  copies and both insertion orders. Arrival energy comes from the analytic
  trajectory; material loss and rebound are compared with an independent
  ordinary zero-gravity contact at that incoming velocity. Actual fall/rebound
  energy and persistent capacity agree across the common physical horizon.
- 24 free-flight configurations through an apex, comparing actual endpoints,
  gravitational work and energy with the analytic trajectory.
- A damaging impact exactly at a frame endpoint, followed by pause and
  continuation without an additional charge.
- Atomic invalid-time, drift-limit, support, reimpact, tiny-kick and
  below-threshold touching-approach refusals, including a late refusal after
  a staged damage charge.

Five departure regressions add:

- 240 configurations containing three impacts: safe/damaging initial motion,
  one whole call or 30/60/120/240 Hz, 1/4/8 internal steps, two partitions,
  one/two copies and both insertion orders. An independent sequence carries
  material capacity through ordinary zero-gravity contacts at analytically
  predicted return speeds. Actual work closes over the common horizon, spent
  work stays within initial energy, and each old root ends before the next.
- Pending-witness pause/replay and rejection of changed pose, velocity,
  gravity, episode identity and source retirement.
- Atomic drift and record limits, including refusal after staged damage,
  an End record, or before a subsequent impact can commit.
- A rounded boundary reproduction: mass 20, initial speed 0.5, gravity 10,
  four internal steps, then durations `0.000002` and
  `f32::from_bits(0x383b2000)` (about 44.614077 microseconds). Actual separation
  exceeded the old `exit + uncertainty` while predicted separation remained
  below it. The scale-aware bound now proves departure at this earlier endpoint.
  Pause/restore and a coarse trajectory retain the same impact count and bounded
  motion differences, with no stranded episode.
- A short excursion under gravity 10,000 that cannot prove departure and must
  refuse another budget.

Successful positive steps replay from serialized snapshots with exact motion,
trace, loading and response comparisons. Impact capture has zero duration;
free drift still does not run material capture or advance its absence timer.
The fixture's ordered trace therefore establishes these episode transitions,
but is not a general native capture timeline or support API.

Six support regressions add:

- 120 stationary configurations over one second: 1/30/60/120/240 Hz, 1/4/8
  internal steps, two partitions, one/two copies and both insertion orders.
  Reaction balances weight, physical work and capacity loss stay zero,
  positions/velocities remain exact, and there is only one Support episode.
- 72 small-hop configurations over 0.01 seconds: three frame durations, all
  three internal-step counts and the same collider variations. Each has one
  material impact, one separately accounted settlement, and unchanged capacity.
  Material loss plus settling plus remaining mechanical energy closes against
  the initial energy. Auxiliary support energies are excluded from that sum.
- Eighteen longer trajectories with initial speeds 0.5, 3 and 4, 120/240 Hz and all
  three internal-step counts. An independent sequence of ordinary zero-gravity
  contacts predicts the genuine impacts, their losses and final capacity.
  Each ends with one settlement and sustained rest. Final contact timing uses
  a tolerance derived from the geometric touching allowance and return speed.
- Pause/replay, stale velocity/gravity/source edits, failed and overloaded
  regions, and atomic advance/record limits, including a record refusal after
  provisional settling but before support can commit.
- The previously unresolved-band regression: initial speed 4, gravity 10, four internal
  steps and 240 Hz eventually produce an apex about `1.90925e-5` from the face.
  The old allowance of about `9.54e-7` straddled the `2e-5` exit band. The tighter
  bound proves containment, allowing one bounded settlement and stable support
  without another collision budget. Support continuation preserves the ledger.
- A deliberately ambiguous apex: set the exit threshold to the rounded analytic
  apex of the speed-0.01 rebound. Even the tighter lower/upper bounds straddle
  that threshold. The return still refuses atomically rather than guessing an
  owner or silently settling.

Eleven supported-arrival and return regressions add:

- 192 configurations: safe/damaging approach speeds, 30/60/120/240 Hz, 1/4/8
  internal steps, unsplit/sliced plates, one/two copies and both insertion
  orders. Analytic arrival speed and independent ordinary zero-gravity contact
  responses predict rebound, patch work and capacity. Actual energy and total
  momentum close with separate weight and impact reactions. There is exactly
  one new root, and the old support owner survives.
- Eighteen approaches at accelerations 1/10/100 and initial speeds 0/1/1.9, with
  gaps selected for the same incoming speed 2. Different support durations and
  reactions produce the same 40-unit impact budget and material response,
  both in one call and at 120 Hz, including the full two-second fall under `a=1`.
- Exactly representable frame-end arrival (`gap=0.015625`, `a=8`, initially
  stationary, `dt=0.0625`), pause and outbound continuation without a new charge.
- Atomic drift/record limits after staged damage, support failure, initial
  overload, invalid time, and stale motion/gravity/
  support identity/source retirement. Successful steps replay exactly.
- An explicit numerical boundary reproduced from a 120 Hz fall under `a=1`:
  `gap=0.016631931_f32`, `speed=1.9916679_f32`, `dt=1/120`. About 12 ns remain
  after arrival, and the rebound's gravity half-kicks cannot change its f32
  velocity. The full call now completes and records the work and impulse
  discrepancies. Flight durations account for the entire request. Splitting
  at the rounded arrival time gives identical motion and numerical ledger;
  pause/restore and continuation preserve the discrepancies without rebudgeting.
- 240 three-impact histories: two initial speeds, one-call or 30/60/120/240 Hz
  execution, 1/4/8 internal steps, unsplit/sliced plates, one/two copies and both
  insertion orders. An independent sequence of fresh zero-gravity contacts
  carries material capacity forward and predicts each return's speed, time,
  response and remaining capacity. Each history has three Impact/End pairs,
  one unchanged B-C support owner, exact requested duration sums and unchanged
  numerical bounds. Energy and momentum close with separate gravity, weight
  and impact reactions; every frame replays exactly from its snapshot.
- A rounded departure crossing at speed `0.6`, acceleration `10`, one internal
  step: durations `2e-6` and `f32::from_bits(0x3811c540)` put the actual gap beyond
  exit plus uncertainty while the predicted gap remains below it. Pause/replay
  retain ownership; a subsequent `0.0001` proves End, and the later return
  agrees with a single `0.12`-second call.
- Late interval/record refusals roll back a provisional End and a new impact,
  preserving prior nonempty outputs, capacity and continuation. Stale incoming
  owners, edited motion and retired sources refuse. A contained small hop
  cannot obtain another root, and the default mode refuses stationary
  full-stack support.
- Three rounded frame-end arrivals with speed `0.25`, acceleration `1`, and
  one internal step resolve in the current frame even when analytic TOI is
  slightly later. Continuation cannot duplicate their response.
- A `2^-31`-second support interval after a `0.01`-second interval reports its
  own positive weight reaction, with zero physical work or capacity loss.

Nine full-stack regressions add:

- 144 stationary configurations: both policies, 1/4/8 internal steps,
  warm-start coefficients 0/0.5/1, two partitions, one/two copies and both
  insertion orders. Both reactions balance the appropriate weight, poses and
  capacity remain exact, and auxiliary energies cancel. Continuations at
  30/120 Hz and `2^-31` seconds retain both owners and restore the coefficient.
- 72 contained-return configurations: 30/120/240 Hz, all three internal-step
  counts and the same collider variations. Each records one impact and one
  numerical settlement with unchanged capacity, exact duration sums, and no
  renewed charge after pause, snapshot restore or a further second at rest.
- 36 complete bounce histories: initial speeds 0.5/3/4, 120/240 Hz, all three
  internal-step counts and both insertion orders with sliced, duplicated
  colliders. Independent fresh zero-gravity contacts carry only material state
  forward and predict the impacts, losses and final capacity. Both owners
  persist through final settling, with ordered earlier End/Impact transitions.
  Actual work and momentum include the measured support and correction records;
  snapshot replay is exact and the original numerical limits remain fixed.
- Strict support after an exact material stop, and refusal of an unsettled
  contained return under that policy.
- Combined-weight overload despite safe support of B alone, overloaded landing,
  and failure of each of the three regions.
- Atomic projection, interval and record limits, including failures after
  provisional capture or settlement while preserving earlier nonempty outputs;
  stale B motion, incoming identity and gravity also refuse.
- An ambiguous excursion that cannot authorize either a new root or settlement.
- Frame-end settlement, pause with zero work limits, and tiny held continuation
  without changing either owner or charging material again.
- Exhaustion of each fixed recoil-correction allowance at a reproduced impact,
  plus refusal to treat materially moving support as a numerical residual.

Two numerical regressions add:

- Repeated `5e-8`-second steps at speed 1 under acceleration 1, with one/four
  internal steps. Velocity cannot represent the increments; depending on
  subdivision, position may also remain unchanged. The persistent work,
  impulse and position ledger forces an atomic refusal in fewer than 500
  accepted steps, preserving capacity and surviving pause/replay. The body
  cannot remain frozen through arbitrarily many successful calls.
- 36 backend drift configurations over three gap scales, 1/3/4/7/8/16 internal
  steps and zero/three additional body steps. Actual drift satisfies the
  scale-aware bound, including non-power-of-two subdivision near the exit band.

The implemented [native gravity API](material-native-gravity.md) exposes
`step_material_flat_gravity(dt, settling)` with explicit policy, persistent
numerical/episode state, terminal invalidation and snapshot v6 enrollment.
Its 144 isolated native impacts additionally match this fixture exactly, including
the physical/capture snapshot after removing the native history envelope. The
native flight/departure scheduler additionally agrees with 18 free-flight,
72 landing and 240 three-bounce configurations. It enforces exact split-duration
accounting and explicitly refuses time that f64 cannot retain. The independent
fixture retains its own duration selection, so comparisons allow bounded physical
differences where native duration lists differ. Native safe support and explicit
settling now additionally match 240 stationary, 72 small-hop and 18 longer
bounce-to-rest cases. The fixture distinguishes strict safe support from optional
settling without adopting native phase logic. Native auxiliary projections clear
warmstarts to measure tiny held remainders; the fixture's projection remains an
independent check at its original durations. All 660 complete native comparison
configurations now call the public entry point and verify its response-batch
links. Separate downstream tests use only public setup, reports and snapshots.

Checkpoint review found that both schedulers could defer a rounded arrival:
the analytic root lay just beyond the frame, but actual drift ended at x=0 with
positive approach and no response. A separate fixture regression reproduces
this for three gap/duration pairs at speed 0.25, gravity 1 and one solver
iteration. Both schedulers now resolve the observed approaching contact before
returning, including a contained return requiring settlement. The fixture's
physical phases, duration selection and numerical tolerances remain independent
of the native implementation.

Overloaded support still needs physical deformation work. These fixtures do not
establish arbitrary long-horizon
accuracy, changing forces, mixed shapes, freely moving stacks, material failure
consequences or a general sustained-load API. Three-plate support and settling
are qualified only in the independent fixture; native promotion remains separate.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib ballistic_
```
