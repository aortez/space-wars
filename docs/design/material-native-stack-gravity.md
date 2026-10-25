# Native supported-stack gravity extension (#327)

The public `PhysicsWorld::step_material_flat_gravity` operation now admits the
qualified three-plate stack as well as the original pair. It runs bootstrap,
coupled impact, supported flight/departure, full-stack rest and optional bounded
settlement in one transaction. Complete public calls choose their own schedules
and match the independent [three-plate fixture](material-ballistic-gravity.md#coupled-full-stack-support-and-settling).
This remains a bounded extension of the [native gravity operation](material-native-gravity.md),
not general contact loading or completion of #327.

## Scene and enrollment

Keep the existing constant-normal-gravity frame and restrictions. The three
roles, determined by geometry rather than IDs or insertion order, are listed in
local coordinates. The [translated-scene extension](material-translated-gravity.md)
now admits a bounded common world offset:

| Role | Body and initial motion | Position |
| --- | --- | --- |
| A | Dynamic, refreshed mass in [5, 80]; free normal motion | x <= 1e-6, y = 0 |
| B | Dynamic, refreshed mass in [5, 80]; exactly stationary between verified phases | (1, 0) |
| C | Fixed, exactly stationary | (2, 0) |

The larger dynamic mass must not exceed four times the smaller, using actual
refreshed backend masses. The [bounded-mass extension](material-unequal-mass-gravity.md)
retains v7 for 20/20 and uses v9 for other admitted combinations at zero offset.
A nonzero offset uses revision 3 and snapshot v11. The original
v7 storage contract described below remains frozen; numerical and work limits
can still refuse a call within the admitted mass bounds.

Each role has a distinct live region and consistent source, a certified filled
rectangle with half-extents (0.5, 1), one/two/four tangent slices and one/two
copies. The total is bounded by 24 colliders. Preserve the existing exclusions
for rotation, transverse motion, friction, other forces/torques, damping, joints,
CCD, sensors, skin, filters, sleeping and additional bodies. Gravity is (a, 0),
finite a > 0, with scale 1. Both dynamic masses are checked after refresh in the
disposable world. Failed materials refuse; damaged but intact materials remain
eligible if their actual supporting loads are safe.

Geometry observation is not authorization to adopt contact history. First
enrollment requires no existing ordinary episode or pending ball/flat departure
proof. The native transaction establishes B-C Support itself with a zero-duration
capture that admits only B-C and leaves A's physical velocity and pose unchanged.
This must also work when A starts touching and approaching B: support bootstrap
cannot consume, rebound or classify A's incoming energy. Check safe B-only support
before capture. The new B-C owner persists for the lifetime of this admitted scene.

Then process A's current state: fly if separated, resolve an approaching arrival,
or establish A-B Support if stationary and combined support is safe. Bootstrap
and any following arrival/hold commit together with the first successful positive
call. A later failure discards both. Pause cannot bootstrap or enroll. An old
ordinary B-C episode lacks this verified history and must refuse adoption.

The shared geometry observer intentionally accepts fitting geometry regardless
of its episode state; continuation/enrollment performs the separate ownership
check. Public dispatch selects the stack scheduler for a three-body world or retained
stack history. Invalidated history refuses before dispatch and cannot switch kind.

## Persistent state and compatibility

One gravity continuation now distinguishes pair and stack history. It cannot hold
both kinds of active history or switch scene kinds to renew a numerical allowance.
The existing pair payload and its snapshot v6 layout remain frozen. Reading v6
constructs legacy pair history; writing that history emits the same v6 bytes. An independent
serializer in the regression tests spells the original fields and lifecycle tags.

Snapshot v7 stores a distinct tail for the original mass-20 stack history: the existing
optional ball and flat tracker slots followed by the required stack continuation.
Unenrolled worlds retain v1-v5. Legacy stack history retains v7 after invalidation or
ordinary stepping, even after all former sources are deleted. Restore explicitly
rejects a stack payload labeled v6 and a pair payload labeled v7. Unknown
versions/tags, truncation, extra bytes and mixed live claims refuse.

Bounded-mass stacks use revision2 and v9; bounded-mass pairs use revision2 and
v8. The tail field layout is unchanged. Restore checks both kind and revision
against the outer version. These histories retain their new versions after
invalidation, including when A's reference mass is 20 and only B differed.

The shared initial reference/flight history precedes the lifecycle discriminator.
Existing pair Active/Invalidated tags retain values 0/1 and their original fields.
The new Stack tag is 2; its payload stores correction totals followed by its own
Active/Invalidated state. Invalidating a stack drops its active identity claims
without dropping the Stack tag or corrections. Public setters and alternate
stepping use the same invalidation entry point for both kinds. Rebuilding an
invalidated stack as a pair still returns stale continuation, even if a caller
selects a different settling policy.

The active stack payload contains:

- Exact A/B/C identities, registered material definitions/sections, bounded
  collider prefixes, actual endpoints and mass properties, capture configuration,
  integration settings excluding transient dt, acceleration and settling policy.
- B-C's canonical pair and episode ID, plus optional A-B ownership with retained
  mechanical energy, complete excursion upper bound, observed turn, held state
  and spent settlement.
- The original A flight reference and fixed energy/momentum/distance scales,
  signed cumulative flight defects, and separate cumulative support-recoil
  correction impulse and energy. B is stationary and contributes no initial
  incoming kinetic budget.

End removes only A-B's episode and excursion evidence. All numerical totals and
their original bounds survive End, later impacts, support, pause and restore.
Invalidation retains the scene kind, policy, reference, totals and first cause;
it drops live claims but cannot re-enroll. The existing external-mutation
invalidation boundary covers all three roles, including edit-and-restore.
Structural restore checks both owners, distinct episode IDs, zero absence timers,
fixed-size topology and finite coherent numerical state. Correction totals must
fit the original momentum/energy bounds even after invalidation; a zero initial
energy reference grants no correction allowance. Nonzero correction impulse also
requires positive correction loss. The native arrival phase separately checks
each actual correction and accumulates it against those retained fixed bounds.

Every positive stack call separately re-proves geometry and exact
continuity before using that state. Structural restore alone is not permission
to step and is not an authentication boundary against fabricated histories.

## Physical phases and accounting

While A flies, apply its verified ballistic integration and support B through
two B-C-only projections around the common force-free drift. The projections
must leave A untouched even at a touching endpoint. B-C carries only B's weight
during flight. All B/C poses and velocities remain exact; every selected positive
duration is retained by the existing exact native clock.

At arrival, one zero-duration material capture sees both contacts. Only the new
A-B Impact root supplies incoming work; B-C transmits that finite pulse under its
old owner. Its earlier reactions, weight and material strain reference supply no
new energy. Check the response budget against A's actual kinetic energy and the
material loss against both dynamic bodies' actual post-response kinetic energy.
Keep the existing rounded-motion, prescribed-work and failure checks. Resolve
observed approaching frame endpoints even if their analytic root is later.

Permit a B recoil correction only within the fixture's `1e-6 * incoming_A` speed
bound. Report its signed `mass_B * velocity_B` reaction and actual removed kinetic
energy separately under B-C. Cumulative signed impulse and nonnegative loss must
fit `1e-5` of the original A momentum and energy scales. Reject nonfinite values,
lost f64 increments and exhausted limits. This cannot authorize freely moving B
or charge material capacity.

Use the qualified actual/predicted lower separation bound to retire A-B. Preserve
pending evidence across rounded threshold crossings and split outbound flight at
the apex. A later genuine return creates a new A-B root from current motion while
B-C's identity remains unchanged.

At rest, A-B carries mA*a and B-C carries (mA+mB)*a. Evaluate both materials on
each contact with zero measured plastic displacement. Overload or failure refuses.
Project both contacts together after each half-kick, with unchanged poses, measured
reactions, explicitly recorded residual corrections and cancellation of auxiliary
kick/projection energies. Clear contact caches and temporarily disable warmstarting
inside these coupled projections; restore caller settings before commit.

Strict policy allows initial rest and an exact material stop. Optional settling
requires the retained A-B owner, observed turn, complete excursion upper bound
inside the departure band, unspent retained energy and safe combined support.
Remove the actual incoming kinetic energy once as numerical settlement. Retain
both owners and material capacity; later hold cannot repeat the settlement.
Ambiguous excursions continue to refuse.

## Reports and finite work

Keep `step_material_flat_gravity(dt, settling)` as the explicit entry point and
preserve its transaction/pause semantics. A three-body response must identify all
three roles. Extend timeline identity to a bounded Pair/Stack representation,
with fixed-size body/source arrays in each variant, rather than treating B as a
fixed body or omitting C. This is a deliberate Rust report-type change; update
existing pair consumers and retain their current pair values and replay checks.
The exported identity type is `GravityRoles`, stored in `GravityTimedRecord.roles`.
Its `Pair` and `Stack` variants contain `bodies` and `sources` arrays of length
2 and 3 respectively. Existing consumers must pattern-match that field instead
of accessing the old two-element arrays directly.

Retain the existing pair Flight/Impact/Support/Settlement data where applicable.
Add full-stack Support and Settlement payloads with two contact owners/reactions
and one complete energy account, plus a distinct support-recoil correction record.
The exported payloads are `GravityStackSupport`, `GravityStackSettlement` and
`GravitySupportCorrection`, carried by the corresponding `GravityRecord` variants.
`GravityNumericalSummary.support_corrections` is `None` for pairs and contains a
`GravityCorrectionSummary` for stacks, even after invalidation. It reports signed
`impulse`, cumulative `dissipated` energy, and the fixed `impulse_bound` and
`energy_bound`. A
correction must not be hidden inside the material response or counted twice.
Impact records retain their link to the committed response batch.

Capture transitions precede their first impact/support record. During supported
flight, report B-C Support at the first projection's time/phase, then A's Flight
at its endpoint and any proved End. Both records describe one physical interval;
list its duration only once. Coupled Impact precedes its residual correction;
settlement precedes subsequent hold. Full-stack records identify roles A/B/C,
reaction arrays use A-B/B-C order, and episode pairs remain canonical.

Retain eight positive intervals and eight material impact solves per requested
step. For the stack operation, bound each coupled projection to 32 raw solves and
all projection solves in one call to 256, including B-only support projections.
Permit at most two support-capture solves: bootstrap B-C and a fresh stationary
A-B. Existing two-plate work caps remain unchanged. Check caps before every raw
call and record one metric for each call. Bootstrap, End, Support, Settlement and
correction records all consume the bounded capture record allowance. Roll back
the entire requested step after any late refusal; expose no provisional output.

## Qualification and implementation order

The shared native observer now passes 576 configurations spanning all six source
orders, both insertion orders, unsplit/sliced plates, one/two copies, four initial
A states and 1/4/8 internal steps. Separate tests reject 31 unsupported geometry,
motion, force, mass and solver configurations plus failure of each material.
Public three-body refusal, invalid time and pause preserve the required state and
outputs, including a pre-existing ordinary B-C episode. These are admission tests,
not evidence of native three-body stepping.

Ten persistence regressions now add:

- 42 combinations of source order, policy and episode state roundtrip exactly.
  They cover B-C-only support, pending A-B, exact-stop support and settled support.
  Public and ordinary pause preserve bytes; modeled A-B End and later ownership
  retain B-C, the original flight ledger and the correction totals.
- 21 real edits spanning all three roles invalidate permanently, including
  edit-and-restore, collider changes, source retirement and body removal. Removing
  all old sources and rebuilding preserves the first cause and all totals.
  No-op/failed setters and queries preserve active state. A fixed-body velocity
  request ignored by Rapier is correctly treated as a no-op.
- Ordinary and successful alternate stepping invalidate inside their transaction;
  refusal and pause preserve history. Rebuilding a pair cannot discard a stack's
  invalidation or obtain a fresh allowance under either settling policy.
- 48 malformed owner, topology, endpoint, policy, material, solver and numerical
  histories refuse restore. Fixed correction bounds accept their endpoints but
  refuse the next representable value beyond either bound, before and after
  invalidation. Zero initial energy permits only zero correction work/impulse.
- Both optional legacy tracker slots, wrong versions/tags, truncated/trailing
  data and mixed pending departure claims are checked. An invalidated stack may
  accompany another mode's valid witness, but its numerical totals remain checked.
- 30 pair histories, including both invalidation causes and optional old tracker
  slots, match the independently spelled v6 tail byte for byte. Legacy v1-v5
  tests and the existing two-plate physical comparisons remain unchanged.

Nine native phase regressions now qualify bootstrap and instantaneous arrival:

- 576 bootstrap configurations span all source orders, both insertion orders,
  slices/copies, four A states and 1/4/8 internal steps. Capture admits only B-C,
  with unchanged endpoints, no material charge and one native Support owner.
  A touching approach remains available for its later coupled response. Raw
  contact points all belong to B-C; a patch need not contain just one point.
- 576 arrivals compare unsplit ordinary-contact references across source order,
  insertion order, slices/copies, speeds, internal steps and both policies.
  One A-B root funds both contacts. Actual motion, material loss and contact
  impulses agree; B-C's owner persists. Each accepted bootstrap/arrival records
  its backend solve and restores the caller's gravity, dt and hook settings.
- 216 configurations match the independent three-plate fixture exactly in
  outgoing motion, response, capacity, event ownership and optional recoil
  correction. They include gravity 1/10/100, three speeds, 1/4/8 internal steps,
  slices/copies and insertion order. Same-build native snapshot replay is exact.
  One speed reproduces a real negative B recoil of about 1.19e-8; its signed
  reaction and actual kinetic loss remain separate from material work.
- Real recoil preserves the original flight history and prior correction totals.
  Exhausted impulse/work bounds, lost f64 increments, and a record limit reached
  only after both provisional captures refuse the whole transaction. A separate
  guard rejects larger/nonfinite residual motion before changing the totals.
- Solve/capture/record limits, subthreshold unaccounted approach, failure after
  provisional impact, unsafe support and stale configuration/ownership refuse
  without changing the caller's snapshot or prior outputs. Explicitly lowering
  the approach threshold accounts for the formerly refused slow approach.
  Exact material stop is valid under strict policy only when both contacts can
  safely carry the combined weight. Ordinary B-C episodes still cannot enroll.

Eleven additional regressions qualify native supported flight and departure:

- 576 configurations cover all source orders, both insertion orders, slices,
  copies and 1/4/8 internal steps, with approaching, stationary-separated,
  departing and apex-crossing A motion. B-C carries only B's weight; A moves
  ballistically, B/C endpoints remain exact, and materials remain unchanged.
  Every interval records two support projections and one drift. The caller's
  gravity, dt and hook flags are restored; auxiliary kicks never enter ordinary
  gameplay collision events.
- 36 exact endpoint configurations retain A's incoming velocity through the
  final B-C-only projection, then apply the coupled material response. Outbound
  flight ends A-B and later return obtains a new A-B owner under every source
  order, without changing B-C. Budget checks account for f32 inverse-mass
  representation using the existing energy tolerance.
- 240 three-impact histories match the independent fixture exactly in motion,
  capacity, responses, support reactions, auxiliary energies, flight defects,
  departure evidence, events and correction records. They cover two speeds,
  single-call or 30/60/120/240 Hz schedules, partitions/copies, insertion order
  and 1/4/8 internal steps. Public pause exposes the same original numerical
  scales and cumulative errors. Same-build snapshot replay is exact. These tests
  initially replayed the fixture's phase durations; they now compare complete
  public calls, including independently selected schedules and report payloads.
- The actual endpoint can cross the departure threshold before the predicted
  endpoint. Ownership survives that step, pause and restore; a later proof ends
  A-B, and reimpact obtains a new owner while B-C persists. Three rounded arrival
  endpoints resolve with the unchanged incoming budget, even when the analytic
  root lies just beyond the chosen interval.
- Tiny support intervals after large reactions retain measurable weight impulse
  and exact auxiliary energy cancellation. Departure preserves real accumulated
  recoil and the original flight reference/errors. A contained return retains
  its owner, turn evidence and energy, and refuses another impact budget.
- Invalid/crossing intervals, missing free motion, stale ownership, interval
  and projection caps, exhausted numerical allowances and lost increments refuse
  atomically. A deliberately late End-record limit discards bootstrap, damaging
  impact and all three flight solves. A second-projection refusal preserves
  previously committed impact reports too.

Native capture, ballistic drift/departure calculations, fixed-support projection
and safe-region evaluation are shared with the existing pair operation. The
independent fixture retains its own physical implementation. Shared projections
filter the selected body pair, clear contact warm-start impulses, measure only
that pair's reaction and verify every other endpoint. Stack phases cap positive
intervals at eight and projection solves at 256 per transaction; limits can be
lowered for qualification but cannot be raised past those caps.

Twelve additional regressions qualify full-stack hold and bounded settling:

- 864 resting configurations cover every source order, insertion order,
  slices/copies, 1/4/8 internal steps, warm-start coefficients 0/0.5/1 and both
  policies. A fresh A-B Support capture preserves B-C's owner and material
  capacity. Both contacts carry their respective weights with zero physical
  work. All endpoints, original numerical totals and settings remain exact,
  including repeated holds and a tiny interval after much larger reactions.
- 144 independent resting cases, 72 contained hops, 36 exact material stops
  and 36 complete bounce-to-rest histories match the fixture exactly in motion,
  material state, responses, owners, support/settlement reaction arrays, actual
  removed energy, residual corrections and raw solve counts. The fixture still
  chooses phase times. Same-build snapshot replay and retained numerical
  summaries agree through every phase; longer histories include actual B recoil.
- Fresh rest and exact material stop need no numerical settlement. Optional
  settlement requires the retained A-B owner, a proved contained turn, positive
  unspent energy and safe combined support. It removes actual incoming kinetic
  energy once, preserving both owners and material capacity. A settlement at a
  frame endpoint survives pause/restore and later hold without spending again.
  Separate retained nonzero flight and correction totals survive settlement.
- Combined weight overload refuses even when B-only support was safe, including
  at a contained return. Failed regions, moving or unheld A, strict-policy
  settlement, missing turn evidence, an uncertain excursion, zero/exceeded
  energy, and already spent settlement refuse without replacing an owner.
- Capture/record/interval limits and late projection limits discard all
  provisional work. A successful provisional settlement followed by a rejected
  hold rolls back too. Cold projection restores the caller's warm-start setting,
  gravity, dt and hook flags on success and error. Nonfinite/negative input
  velocities refuse. An admitted solver with zero velocity iterations stops at
  the hard 32-solve projection cap even if given a larger limit; the shared
  256-solve transaction cap likewise cannot be raised by a caller.

The coupled native projector preserves every pose and mass property, verifies
each measured reaction against momentum change, checks that kinetic energy does
not increase beyond the established tolerance, and accepts convergence only
inside the `1e-6` residual-speed bound. Residual impulse and energy are explicit.
Cold point caches and a temporary zero warm-start coefficient are both required.
Holding uses two projections around a force-free stationary drift; its auxiliary
kick and projection energies cancel exactly. Settlement reports the one actual
kinetic loss separately from material work. Every raw solve has its own metric.

## Complete scheduler and public qualification

The stack scheduler shares the pair's exact f64 clock, bounded f32 remainder,
stable arrival root and rounded apex selection. It resolves an approaching A-B
contact using only A-B ownership: B-C is always owned and cannot select settlement.
Observed touching arrivals are accounted at the frame endpoint even when the
analytic root lies later. Positive tails are consumed separately and listed once.
Support/Flight records share the same interval; their phase indices identify the
first B projection and the A drift respectively. Bootstrap precedes all contact
work; impact precedes its optional B correction. Each coupled support/settlement
payload includes its actual raw solve count.

Complete public calls now replace the scripted phase driver in the independent
comparisons: 192 separated-arrival configurations, 240 repeated-return
configurations, 144 stationary configurations,
72 contained hops, 36 exact stops and 36 full bounce-to-rest histories, plus
rounded departure/arrival and frame-end settlement regressions. They compare all
physical/accounting payloads, event order, solve counts, capacities, endpoints,
retained numerical totals and exact snapshot replay. The fixture's physical
implementation remains separate. The earlier 216 instantaneous-arrival
comparisons continue to qualify the native impact phase independently.

Native scheduler regressions retain rounded positive remainders and tiny tails,
reject time loss, resolve exact frame-end impacts and roll back after late
interval/impact/projection/capture/record caps. Downstream tests use only public
APIs to exercise damage, repeated departures/returns, persistent B-C ownership,
settlement, later support, strict rest/exact stops, pause/restore, overload,
failed material, capture limits and invalidation even after rebuilding as a pair.
The original pair tests preserve their physical results with the new role type;
the frozen v6 serializer checks remain in place.

Geometric admission is not a promise that every step passes its numerical proof.
One retained opposite-source-order case starts A at x=-0.02, speed 4 with g=10,
uses slices [4,2,4] and duplicate colliders. A short 0.001-second flight enrolls,
but a 0.01-second call reaches a callback/backend mismatch: B's reported material
response recoil is about 3.8e-7 while its actual backend endpoint is zero.
The entire call returns `Numerical`, preserving capacity, ownership and prior
outputs. Accepting it would hide an unaccounted velocity change; no bound was
relaxed to make it succeed.

General geometry, moving B, overloaded deformation and failure consequences
remain separate work. No scenario selects this operation automatically.
