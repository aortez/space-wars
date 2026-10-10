# The ledge has a placement preview missed by both sampling stages

The read-only probe finds **one usable standing-point preview with two ship
offsets**, while preserving the entire preceding corrected replay byte for
byte. Footing bearing 343 passes all native placement checks at offsets -11
and -12. The normal search omits that footing, and neither offset belongs to
its four placement samples.

Both dimensions matter in this scene: more footing samples with the original
four offsets yield no valid preview; more offsets at the original sparse
footings also yield none. This supports a bounded refinement of nearby
standing points and intermediate offsets.

**Recovery remains unqualified.** The diagnostic does not change controls or
authorize a build. Its actual live continuation still stops at 24548 with no
rebuild or boarding. A preview establishes a measured proposal, not successful
arrival, landing or boarding.

## Frozen probe and unchanged execution

The [plan](rebuild-coverage-plan.md) freezes source `ddfc7b2`, a copied release
executable, 958 input hashes and one diagnostic call at tick **24330**. It uses
the original tape and warmed recovery task from the
[verified ledge replay](rebuild-replay-results.md).

The same prefix and both corrected continuations run once. All **20 retained
files** match the preceding corrected replay byte for byte, including the dense
prefix, recorded-controls continuation, live continuation, reports and generated
audits. The probe's ordinary relocation survey equals the actual task's survey
at 24330. Native physics snapshots before and after the diagnostic also match.

The prefix retains 23,768 tick rows, 47,536 native pilot observations, 6,948
task/control steps and 396 one-second world audits. The original task start
16820, high crossing completed at 23226, claim and ground allowance remain
unchanged. There are no fresh seeds, simulation retries or audit repairs.

Production placement still uses `[-8, -14, +8, +14]`, in the same order. A
private shared evaluator accepts the diagnostic's additional offsets; only
the opt-in probe supplies them. The probe is compiled under `sensor-profile`
and is not part of normal bot observations.

## Coverage and results

The probe uses the existing local ground patch, measured routes and 24-unit
route limit. It checks every measured footing within 24 units, removing the
diagnostic search's two-unit thinning and minimum-distance exclusion. It does
not search the rest of the planet or interpolate new terrain.

| Outbound result | Footing points |
| --- | ---: |
| Reachable within the route limit | 57 |
| Disconnected | 34 |
| Route exceeds the limit | 8 |
| Total inspected | 99 |

Each of the 57 reachable points is checked with the four original offsets and
with 26 offsets covering both sides from 8 through 14 at half-unit spacing.
The first four detailed results match exactly. The expanded set produces
1,482 offset evaluations; the separate four-offset reference calls are not
included in that count.

| Expanded placement result | Evaluations |
| --- | ---: |
| No ground | 1192 |
| Hull obstructed | 96 |
| No hatch route | 95 |
| Landing misaligned | 63 |
| No hatch footing | 34 |
| Accepted preview | 2 |

The accepted previews share footing **343**, local position
`(29.0051708, -15.9642696)` on planet zero, revision 21:

| Measurement | Result |
| --- | --- |
| Distance from current foot | 19.935 units |
| Outbound route | 23.652 units; one flight and three jumps |
| Passing ship offsets | -11 and -12 |
| Selected offset | -11; equal route cost retains the first accepted proposal |
| Predicted resting angle, both offsets | 0.0198 degrees |
| Hatch route, both offsets | 3.490 units; no jumps or flights |
| Original four-offset result at this footing | No valid placement |

The original -8 offset has no hatch footing; -14, +8 and +14 have no suitable
ground. Intermediate offsets are not uniformly usable: -11.5 fails the hatch
route despite passing alignment. The complete report retains every rejection.

Footing 343 is **0.866 units from sampled footing 341**, so the original
two-unit thinning omits it. It is also the 81st of the 99 points in increasing
distance order. Keeping only the nearest 32 unthinned points would still miss
it. The next search needs bounded refinement and deliberate scheduling within
the existing time allowance.

## Arrival still needs verification

The measured outbound route ends at footing **342**, about **0.430 units** from
343, within the survey's 0.8-unit endpoint range. Footing 342 itself has no
passing preview among the same 26 offsets. This does not prove where the live
controller will stop, but it means a route to the neighborhood is insufficient
evidence of a usable actual build position.

The next controlled experiment should refine nearby footing and intermediate
offsets with a fixed work budget, then verify the actual stopping point and
native placement there. It must retain the original task history and deadlines
and require real movement, rebuilding, two-foot settling and boarding. A
preview alone must not reset progress or complete recovery. No landing,
support, hull-clearance or hatch-route threshold needs loosening to obtain the
two measured proposals.

## Validation and retained evidence

Passed: **3 native placement tests**, **63 replay-harness tests**, **952 Python
tests**, example-scoped Clippy with warnings denied, formatting and the locked
Rust 1.89 release build. The new native test checks repeated measurements,
unchanged physics and actor state, native four-offset retention, accepted
alignment/routes, invalid seats and dirty-query rejection.

The broader native-crate Clippy check **does not pass**: it reports 16
diagnostics across six unchanged files. Every reported file is byte-identical
to the preceding `9e7355e` baseline; no diagnostic points to the new probe or
placement refactor. The full failed log and file hashes are retained alongside
the passing checks. This is an outstanding repository lint limitation.

The [manifest](data/rebuild-coverage-v1.json) records the frozen command, source
and binary hashes, counts, candidate receipts and validation outcomes. The
[portable bundle](data/rebuild-coverage-v1.json.gz) includes the complete
coverage report and both measured ground maps, all reports/audits, the actual
scene observation, source changes, logs and reproducible exporter.

The losslessly checked archive under
`target/rebuild-coverage/v1/archives/replay.tar.gz` retains the complete prefix,
both continuations and probe output: 248,810,263 raw bytes compressed to
38,657,658 bytes. Its hashes link back to the preceding replay and source tape.
All work remains local on `bot-rebuild-coverage`; defaults and deployment are
unchanged.
