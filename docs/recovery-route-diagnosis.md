# Recovery stalls at a ledge outside the measured flight envelope

The disconnected recovery route has no complete alternative to the flag or
assigned pod in its consumed observations. The blocking ledge, nodes
276 → 282, rises approximately **11.080 units**. Its lowest proposed cruise
requires **14.080 units** above the lower footing; the terrain-gap survey
rejects anything above **10 units** before checking physical clearance.

This completes the bounded-fallback investigation proposed in the
[arrival results](crossing-arrival-results.md). There is no evidence supporting
a fallback over an existing complete measured route. No runtime policy,
flight limit or deadline changed, and no new games ran. The arrival candidate
remains experimental after its recorded win-to-loss regression.

## Evidence and route reproduction

The [retained fixture](../crates/spacewars-ai/tests/fixtures/recovery-route-stall.json)
contains four actual P2 observations from
`new-world1-p2-asteroids3-no-stop`. Extraction verifies the entire source
trace against its archived SHA-256 and byte count. The diagnostic checks
that the fixture is exactly the selected projection of those rows and that
all **931 predecessor inputs** still match the frozen arrival experiment.

The [native regression test](../crates/spacewars-ai/tests/recovery_route_stall.rs)
reconstructs each measured ground map and adds both directions of every
consumed jetpack corridor through the production `connect_jetpack` API.
It then calls the production route solver for both the flag and the union of
the pod's two hatch envelopes. Every complete-route query fails as disconnected.

| Tick | Start node | Ground nodes reachable | With measured flights | Partial flag route | Ground budget left |
| --- | ---: | ---: | ---: | --- | ---: |
| 22,575 | 242 | 34 | 163 | 10.931 units | 18.38 s |
| 22,965 | 273 | 34 | 140 | None | 11.88 s |
| 23,205 | 272 | 33 | 139 | 1.306 units | 7.88 s |
| 23,655 | 273 | 34 | 140 | None | 0.38 s |

The test reproduces all recorded route diagnostics exactly at native `f32`
precision. When a partial route exists, the controller reports that combined
search. When it does not, the controller returns the direct ground failure;
this explains its reported 33–34 reachable nodes despite the combined graph
having 139–140. Those extra measured connections lead away from the flag and
still do not reach either hatch.

A positive control adds only a **synthetic** 276 → 282 edge to a copy of each
graph. All four then obtain a complete flag route containing that edge.
This isolates the missing graph connection. The synthetic edge has no
clearance or flight measurement and is never supplied to a game or bot.

## Why another survey does not provide this crossing

The ledge is the second-nearest eligible gap at 22,575 and the nearest at
the other three snapshots. It is already inside the eight-gap survey budget;
it is not being omitted because more distant gaps consume the budget.

For each gap, the existing survey tries eight symmetric endpoint margins,
then cruise heights of 3, 5 and 7 units above the higher endpoint. Here the
endpoint pairs are 276/282 through 269/289. All 24 candidates pass the
endpoint-distance gate but fail the rise limit. Even their lowest cruises
require approximately 14.080–14.701 units above the lower endpoint.
None reaches the physical clearance query.

Jetpack charge is 80% at the first snapshot and 100% at the other three.
Full charge does not establish that a higher flight is executable. These
records contain no collision-clearance, fuel-consumption or landing evidence
for the rejected higher climbs. They also do not rule out unmeasured routes
around the other side of the planet or different pod landing choices earlier
in recovery.

## Next experiment

Test this higher ledge as a bounded, physically measured terrain crossing.
The existing forecast implementation models parked full-ship crossings and
its public validation requires a vehicle anchor; it cannot simply authorize
this terrain gap. A terrain proposal needs its own valid measured endpoints
and must use the same flight-control, fuel-reserve and landing checks.

First measure clearance and execution feasibility for the shortest ledge
proposal while keeping the existing timer and reserve. If it fails, preserve
the rejection and investigate an earlier recovery landing choice. If it
passes, expose the measured connection early enough for normal route planning
to use it, then run a frozen full-game comparison against both the arrival
candidate and the original winning case. Require native claim, rebuild and
boarding progress, retained survival and unchanged controls in unaffected
games before treating the combined change as an improvement.

Increasing the survey count cannot remove this ledge's height rejection.
Resetting the timer or routing through a synthetic edge would not establish
physical recovery success.

## Reproduction

The [diagnostic result](data/recovery-route-diagnosis-v1.json) binds the
fixture, source trace, native output and relevant source files by SHA-256.
All observations and native route results needed to inspect the finding are
committed; the original full trace remains in the preceding lossless archive.

Validation passes: **46 Rust tests** covering the recorded stall, ground
navigation and jetpack traversal; **5 Python diagnostic tests**; formatting;
and Clippy with warnings denied for the new Rust test. The Python checks
reject changed source traces, missing or duplicate snapshots, incomplete
native evidence, and failed native test logs. Native comparisons normalize
JSON decimals to the engine's `f32` type before exact comparison.

```sh
cargo +1.89.0 test --locked -p spacewars-ai --test recovery_route_stall -- --nocapture
python3 -m unittest discover -s tools/tests -p test_recovery_route_diagnosis.py -v
```

To regenerate the result, save the native test output and pass its path along
with the extracted original trace:

```sh
python3 tools/diagnose-recovery-route.py analyze \
  --trace target/crossing-arrival/v1/review-extracts/new-world1-p2-asteroids3-no-stop/trace.jsonl \
  --native-log target/recovery-route-investigation/v1/native-test.log \
  --out target/recovery-route-investigation/v1/diagnosis.json
```

This is an observation and graph diagnosis, not a new full-game qualification.
