# Original ledge replay verified; rebuild search still blocks

The isolated replay preserves the original high crossing, planet-zero claim
and recovery-task history. The old native runtime reproduces both bad builds
and the same boarding failure. The corrected runtime rejects the unsuitable
placement, then exhausts its existing search without building a ship.

**The original ledge-to-recovery chain remains unqualified.** The placement
correction prevents the observed invalid builds, but does not supply a usable
alternative. The next investigation is the coverage of the standing-point and
placement search in this retained scene. Default/frontier selection is unchanged.

This resolves the ambiguity in the [preceding full-game result](rebuild-settling-results.md):
that game recovered on a different route after an earlier P1 decision changed.
Here P1 follows recorded controls, while P2 resumes its original recovery task.
This is a controlled recovery probe, not a scored match or evidence of a win.

## Exact history and reference reproduction

The [frozen plan](rebuild-replay-plan.md) uses the original World 1/P2 no-stop
trace from `e46f5a5`. The tape contains 29,422 tick rows through 29421. Both
executables use replay source `9d94d48` and the same controller source. The
reference substitutes only the old `landing.rs` and `rebuild_placement.rs`, and
was built in a separate Cargo target directory. The plan records 954 input
hashes, both copied executable hashes and the complete commands.

Each executable verifies the following before either continuation:

| Retention check | Exact matches per executable |
| --- | ---: |
| Tick rows, 0 through 23767 inclusive | 23,768 |
| Native pilot observations, both seats | 47,536 |
| Original task telemetry and generated control sets, 16820 through 23767 | 6,948 |
| One-second world physics audits | 396 |

Native comparisons remove only placement diagnostics. Status, counters,
motion, support, ownership, query cadence and controls remain checked. Floating
point values use the retained trace's JSON representation, without a tolerance
or narrowing conversion.

The task is advanced from its original start at 16820. It retains the ground
start at 18278, high crossing completed at 23226, new ownership at 23767 and
5400-tick ground allowance. The handoff clones the real world and this warmed
task; it does not reset the task or its clocks.

The reference recorded-controls continuation matches the original native
observations, full task telemetry and proposed controls through 29421. Its live
continuation also matches all three through its original terminal failure at
27962. This confirms that removing the surrounding mission controller did not
change this task's original behavior before failure.

## Continuations

| Runtime and continuation | Last tick | Builds | Boardings | P2 ship losses | Result |
| --- | ---: | --- | ---: | ---: | --- |
| Reference, recorded controls | 29421 | 25498, 27060 | 0 | 2 | Fixed end; task already blocked |
| Reference, live recovery | 27962 | 25498, 27060 | 0 | 2 | Replacement ship also has no accessible return |
| Corrected, recorded controls | 29421 | None | 0 | 1 | Fixed end; task already blocked |
| Corrected, live recovery | 24548 | None | 0 | 1 | No reachable standing site with hatch access |

P2 remains alive in all four continuations. Health is 100 in both reference
continuations and the corrected live continuation. The corrected
recorded-controls continuation ends at 91.27 after a native impact at 26773.
All sampled native physics audits pass, including material conservation and
speed bounds.

The corrected task first differs at **24330**, when the reference accepts a
relocation site and the corrected preview does not. The first changed proposed
control is at **24347**. In the live continuation this changes the actual native
observation at **24348**. In the recorded-controls continuation, those proposals
are not applied: native observations remain identical until **25498**, when the
reference creates its first unsuitable ship and the corrected runtime refuses
to build it.

The reference's final ship at 27962 has one supported foot, a 23.375-degree
landing angle and no accumulated settling time. The corrected live continuation
never creates a full ship, so the retained escape pod's landed state is not a
boarding or successful recovery receipt.

## Why the corrected search stops

The missing-site timer starts at 24247 and expires at 24548, the first tick
past its unchanged 300-tick allowance. Ten native relocation surveys inspect
19 distinct footing bearings. Their repeated route checks produce:

| Route result | Checks |
| --- | ---: |
| Reachable within the 24-unit route limit | 33 |
| Disconnected | 19 |
| Reachable but beyond the route limit | 8 |

The 33 checks cover 11 distinct reachable footing bearings. Each tries the four
existing offsets, for 132 repeated placement proposals:

| Placement rejection | Proposals |
| --- | ---: |
| No ground | 96 |
| Landing misaligned | 15 |
| No hatch route | 9 |
| Hull obstructed | 6 |
| No hatch footing | 6 |

These counts describe repeated previews, not distinct physical sites or actual
build attempts. The live task completes zero relocations and zero rebuilds,
without an additional ship loss.

At the decisive survey at 24330, footing bearing 330 is at local position
`(24.0357475, -18.7576408)`. Its outbound route is valid and 17.981 units long,
including one jetpack flight. The reference accepts placement offset -8. The
corrected preview measures a **26.069-degree** resting angle there, exceeding
the native limit. Its remaining offsets fail as follows:

| Offset | Corrected result |
| ---: | --- |
| -8 | Landing misaligned, 26.069 degrees |
| -14 | No ground |
| +8 | Aligned at 0.028 degrees, but disconnected hatch route |
| +14 | No hatch footing |

Across the live search, rejected alignment angles range from 21.514 to 28.076
degrees. The search still uses its existing local patch, at least two units
between sampled standing points, at most 32 candidates, and eight previews per
survey. It does not test every standing point or ship offset. Therefore this
result establishes failure of the bounded sampled search, not physical
impossibility everywhere on the ledge.

The next bounded probe should test coverage between the sampled standing
points and offsets on this same scene, retaining native alignment, hull,
footing and hatch-route checks. It should first establish an actual usable
alternative before changing playing behavior or extending the search.

## Validation and execution corrections

Validation passed: **63 Rust harness tests**, **949 Python tests**,
example-scoped Clippy with warnings denied, formatting, and both locked Rust
1.89 release builds. This turn adds diagnostic replay access and instrumentation;
it does not change the production controller or native placement rules. The
preceding native and ground/recovery test results are documented in the prior
report and were not rerun as part of this probe.

Two execution corrections are retained with the result:

1. Both v1 attempts, source `04c1df9`, stopped at tick zero before any native
   physics step or continuation. Typed float serialization differed from the
   original trace's JSON `Value` serialization. Source `9d94d48` matches the
   original representation and adds a regression test. New executables were
   frozen for v2; comparison tolerance and acceptance criteria did not change.
2. After the v2 reference completed, the Python audit passed a string path to a
   reader requiring `Path`. Audit source `e31eb01` fixes that error and adds a
   hash-checked resume. The completed reference output was reused, and the
   previously unstarted candidate ran once with its frozen executable. Only
   the auditor and its tests changed; no native simulation was retried.

There are two successful prefixes and four continuations in v2, plus the two
zero-step v1 preflight failures. No fresh random games or tuning were performed.

## Retained evidence

The [manifest](data/rebuild-replay-v2.json) records the commands, hashes,
validation results, continuation outcomes, survey counts and archive members.
The [portable review bundle](data/rebuild-replay-v2.json.gz) contains both plans,
reports and audits, the complete 782-row corrected live continuation, selected
dense reference and recorded-control windows, the decisive paired survey,
frozen source changes, audit repair, build/test logs and reproducible exporter.

The four hash-verified local archives retain both v1 failures, both full v2
prefixes and all four complete continuation traces under
`target/rebuild-replay/{v1,v2}/archives`. The source tape, original trace,
read-only executables and separate reference checkout remain under v2. The
portable bundle includes the source archive and tape hashes so these larger
local artifacts can be checked against the frozen experiment.
