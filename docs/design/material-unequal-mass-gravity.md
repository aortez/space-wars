# Unequal-mass gravity qualification (#327)

`step_material_flat_gravity` now admits refreshed dynamic masses in **[5, 80]**.
A stack additionally requires the larger dynamic mass to be at most four times
the smaller. Geometry, loading, finite work, numerical bounds and rollback retain
the [native pair](material-native-gravity.md) and
[native stack](material-native-stack-gravity.md) contracts. Admission does not
guarantee success: each call must still pass all physical and numerical checks.

Independent fixtures establish the physical evidence below. Native complete-call
comparisons then qualify the implementation and its saved-state continuation.

## Physical scope

Keep the existing filled rectangles, locked rotation, constant +x gravity and
fixed base. In a stack, B remains stationary between verified phases. Vary
collider mass and inertia before the initial support capture. The material
definitions, volume and section stay unchanged, so this isolates inertial mass
from material capacity. Coincident copies divide the body's mass instead of
adding mass. All force, momentum and energy assertions use the refreshed backend
mass: a nominal mass of 13 becomes approximately 12.999999 in the backend.

Pair samples use nominal dynamic masses 5, 13, 20, 31 and 80. Stack samples use
the following A/B masses:

| A | B |
| ---: | ---: |
| 5 | 5 |
| 5 | 20 |
| 20 | 5 |
| 13 | 31 |
| 31 | 13 |
| 20 | 80 |
| 80 | 20 |
| 80 | 80 |

These are sampled cases, not proof for every intermediate mass or an unrestricted
mass ratio. Layouts compare one collider per body with partitioned, duplicated
rectangles: pair slices [4,2], stack slices [4,2,4], two copies per slice. Sweeps
also vary insertion order, solver iterations, frame subdivision and gravity as
listed below. Native source-order qualification is described at the end.

## Independent evidence

Ordinary zero-gravity contacts at analytically calculated incoming speeds supply
the reference response and capacity loss. They do not use the gravity scheduler.
Repeated-return references carry material state through three separate ordinary
contacts; ballistic flight determines return speed and time. Every fixture step
also runs from a restored snapshot and compares physical state, contact records,
material responses and retained history exactly.

| Case | Configurations | Evidence |
| --- | ---: | --- |
| Pair landing | 360 | Speeds 0.5/4, gap 0.02, gravity 10; iterations 1/4/8, 30/120/240 Hz; actual work, momentum, rebound and capacity match analytic/ordinary references |
| Pair rest and contained return | 120 | Strict rest or optional settling; owner retained, no material charge, subsequent holds at 30/240 Hz and a 2^-31-second tail |
| Stack landing | 576 | Same landing speeds, gap, gravity and subdivisions; B-C keeps its owner, only A-B supplies new work, both dynamic masses included in energy and momentum |
| Stack rest | 288 | Gravity 1/10/32; reactions mA*g and (mA+mB)*g, exact pose retention, zero physical work and unchanged capacity |
| Stack contained return | 96 | One bounded numerical settlement under the incoming owner, unchanged capacity and no repeated settlement on the next hold |
| Stack three returns | 192 | One complete call or 30/120/240 Hz, iterations 1/4/8; three new A-B roots with ordered departures, unchanged B-C owner and numerical limits, total work/momentum/capacity checked |

The stack fixture retains exact requested time, including positive tails. The
older independent pair scheduler rounds its last f32 interval. Its new landing
test explicitly bounds that per-call discrepancy by `f32::EPSILON * dt` and uses
the measured duration for analytic trajectory and momentum checks. This does not
relax the native operation's exact-time contract; native promotion still needs
complete-call comparisons against that contract.

## Findings and refusals

The stack rest sweep exposed an arithmetic-order defect at masses 13 and 31.
The fixture previously calculated kick energy from total mass, but calculated
projection loss by summing each body's kinetic energy. Those expressions can
differ in f64 even when both projections exactly remove their supplied kicks.
The fixture now sums the actual per-body kick energies in both half-kicks.
The exact cancellation check remains unchanged; no energy tolerance increased.
Native support now uses the same per-body physical accounting, implemented
separately from the fixture. The mass 20 result is unchanged.

The following refusals remain explicit regression cases, preserving snapshots,
capacity, ownership and previous outputs:

- A/B = 40/5 at rest, one solver iteration: the coupled projection exceeds its
  existing 32-solve bound. The whole call refuses.
- A/B = 80/5 at incoming speed 4: the material contact budget reports
  `NotConverged`. No material damage commits.
- A/B = 80/20 at gravity 100: B's weight alone is safe, but the combined resting
  load exceeds the wood base's strength. Static reaction cannot fund damage.
- Changing either dynamic body's mass after a verified step invalidates the
  fixture's motion evidence. Exhausting an advance limit also preserves prior
  outputs and state.

Exploratory sweeps also found other large-ratio failures. No solve cap, numerical
allowance or material strength was increased to admit them.

## Native admission and saved-state compatibility

Mass bounds are checked after refreshing collider mass properties in the
disposable world. The ratio check uses f64 conversions of those actual f32
masses, with no tolerance at the boundary. Mass or ratio outside these bounds
returns `UnsupportedScene` before enrollment. Static overload still returns
`UnsafeSupport`; smaller within-range numerical failures retain their existing
refusals. No solve cap or numerical allowance was increased.

| History | Contract revision | Snapshot |
| --- | ---: | ---: |
| Pair, dynamic mass 20 | 1 | v6 |
| Stack, both dynamic masses 20 | 1 | v7 |
| Pair, other admitted mass | 2 | v8 |
| Stack, other admitted combination | 2 | v9 |

The existing serialized fields and enum tags are unchanged. The new outer
versions distinguish the broader mass contract and must match both the history
revision and pair/stack kind. Cross-labeled payloads refuse. Revision 1 still
requires mass 20; revision 2 validates the range, ratio, finite scales and active
A mass against the original reference. An existing continuation retains its
revision instead of selecting a new contract on each call. A stack with A=20
and B=5 correctly selects revision 2 despite A's legacy-compatible reference.

Invalidation keeps its first reason, revision, initial scales, flight defects
and stack correction totals, even after all bodies/regions are removed. Pause
cannot re-enroll or change the policy. Changing and restoring physical mass or
motion cannot renew a numerical allowance. Old v1-v7 snapshots remain readable;
independent serializers freeze the v6 and v7 tails across active and invalidated
lifecycles. Older readers correctly refuse the new v8/v9 versions.

Native validation adds:

- 180 pair landing comparisons across masses, layouts, iterations and frame
  rates, including exact requested native time and restored replay. The older
  pair fixture's small time-tail discrepancy remains bounded as described above.
- 600 stack phase configurations against complete independently scheduled calls:
  separated arrival, rest, contained settling, outbound flight and later
  continuation. These include the eight original mass pairs plus fractional
  pairs 5.25/20.5 and 79.5/20.25. Every record, endpoint, capacity, numerical total
  and replay matches, including a 2^-31-second final call.
- Eight three-impact histories at 30/240 Hz retain distinct A-B roots and the
  original B-C support owner, fixed scales and actual accumulated work.
- 192 resting/impact configurations across all six source orders, reversed
  insertion and physical partitions, checked against ordinary-contact work and
  mass-weighted reactions.
- Public-only pair/stack rest, settlement, pause, replay, version checks and
  terminal invalidation, plus outside-range/ratio, combined overload and late
  subthreshold-arrival rollback.
- Corrupted reference, endpoint mass, revision, owner, scales and correction
  totals refuse restoration. Supported physical mass edits invalidate continued
  stepping rather than minting new history.

Changing geometry, freely moving B, deformation and failure consequences remain
separate work. These bounded samples and per-call guards are not a claim of
unrestricted mass ratios or universal success throughout the admitted range.
