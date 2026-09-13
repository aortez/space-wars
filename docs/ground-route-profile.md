# Remaining ground-route sensor stalls

The [landing survey cadence](landing-survey-cadence.md) reduced repeated full
landing searches. This follow-up measures the remaining individual sensor spikes
on merged main `ba916aa`, using `sw-picade.local` on 2026-09-12/13.

The retained changes add opt-in diagnostics and fix the kiosk's forced raster
scale. They do not optimize the routing algorithm yet.
The subsequent [indexed-route implementation](indexed-ground-routes.md) preserves
the measured behavior and reduces the routing contribution to the slowest calls.

## Findings

Both known seeds reproduce the recorded gameplay. The largest measured calls
combine three substantial jobs: constructing ground connections, searching
routes to the flag and back, and filtering the map around proposed ship poses.

| One bot observation | Seed `9216675843324634618` | Seed `7725194555774358125` |
| --- | ---: | ---: |
| Runner row / seat | 21,511 / P1 | 8,626 / P2 |
| Complete sensor observation | 51.88 ms | 53.69 ms |
| Ground nodes | 2.39 ms | 2.28 ms |
| Ground connections | 19.23 ms | 19.11 ms |
| Proposed-ship map filtering | 5.89 ms | 6.24 ms |
| Route searches, outbound and return | 14.31 ms | 16.00 ms |
| Landing candidates | 9.41 ms | 9.51 ms |

These are nested measurements. The table's component rows are separate work;
the complete observation includes them and the remaining adapter work. Do not
add `landing_objective_survey` or `survey_ground_with_gravity` from the raw
profiles to their children. Runner row numbers are one greater than the
completed physics tick used by the observation.

### Ground connections dominate the accumulated cost

Each worst-case map tests 1,024 directed neighbor pairs, using **9,216 capsule
clearance queries and 3,072 floor rays**. All 1,024 paths are walks; no jump path
is tested in these two observations. The 512-point ring still requires a large
number of queries on straightforward walking ground.

Across the complete runs, ground connections account for **60.4% and 58.0%**
of nested sensor time. The first run constructs 1,521 maps and issues 13.88
million connection capsule queries; the second constructs 424 maps and issues
3.89 million. These counts exclude capsule/ray work used to find ground nodes
and to test landing candidates.

The code evaluates both directed paths, including their endpoints and three
intermediate floor samples. This makes within-survey query reuse a useful next
experiment. Reversed interpolation and independently calculated endpoint
normals can differ in floating-point bits, so a geometric resemblance alone
does not establish identical query results.

### Route searches amplify the worst pauses

The first slow observation performs 15 searches, visits 4,626 nodes across
those searches, and scans **4,648,896 edge entries**. The second performs 16
searches, visits 5,012 nodes, and scans **5,055,952 edge entries**.

`GroundMap::route_with_height` currently searches all 512 cost slots for each
next node, finds that node by scanning the node vector, then scans the entire
edge vector to find its outgoing edges. Candidate maps have already been
measured at this point: these route searches perform no physics queries.

This work is concentrated in difficult route decisions. Across the full runs,
route searches consume only 0.71 and 0.30 seconds, compared with 28.03 and 7.84
seconds connecting ground nodes. Optimizing graph lookup should target the
largest pauses; it will not remove the dominant accumulated connection cost.

### Most proposed-ship filtering avoids the hull test

Eight shortlisted landing poses each filter the surveyed map. The slow
observations evaluate 76,672 and 76,960 clearance sample positions, but only
264 and 488 reach the actual proposed-assembly preview. The existing distance
guard rejects the need for that preview elsewhere.

The remaining 5.9–6.2 ms therefore includes substantial work calculating and
transforming repeated sample positions, plus map filtering. Detailed timing
inside every preview or sample was deliberately avoided. The counters identify
how often the preview runs; they do not time its individual collider tests.

## Next implementation

Start with **indexed route lookup within each measured candidate map**:

1. Build direct node lookup and an outgoing-edge index once for the candidate
   map, sharing them between its outbound and return search.
2. Preserve edge order, cost arithmetic, the current next-node tie behavior,
   route diagnostics and partial-route rules. Measure this before separately
   considering a priority queue for the 512-slot minimum search.
3. Compare the returned routes and diagnostics, then replay these two seeds and
   the existing damaged-ground, jetpack, objective and asteroid cases on the Pi.

That change can address the 14–16 ms search contribution with no collision
cache. Follow it with a separate connection-query experiment: count exactly
repeated capsule poses and floor rays within one fresh survey, then test reuse
if its lookup cost is justified. Compare the complete resulting maps, including
directed jump edges, rejected nodes, moving obstacles and excavated ground.

Precomputing candidate-independent clearance sample positions is another lead
for the proposed-ship filtering stage. Keep the 512-node resolution and actual
collision rules while measuring these changes. Cross-update caching would need
additional invalidation for moving bodies and changing queries.

## Validation and measurement limits

Reference and instrumented runners use Rust 1.89.0, AArch64 release builds,
the same compatible linker, four Hz landing surveys, both rule bots, ordinary
combat breaks, and no random asteroids. Each run has a 600-second budget.

- Seed `9216675843324634618` completes 36,000 updates and reaches the time limit.
- Seed `7725194555774358125` completes 18,168 updates, ending on the same pilot
  death at 302.8 seconds in both builds.
- All **54,168 recorded per-update rows** match in every non-timing column.
  All non-timing report fields also match, including physical audits, sampled
  pilots, mission events, final state, round result and physics populations.
  Both references also reproduce the prior cadence study's recorded final
  pilots, planets, missions and outcomes.
- Both builds pass every terrain audit. The extra diagnostics do not enter
  observations or simulation state.
- Existing tests pass: two client launch-setting/CLI precedence tests, 363
  scenario tests with profiling enabled, and 57 AI ground, jetpack, objective
  and mission tests. The mission suite includes the three-minute generated
  asteroid duels. Formatting and whitespace checks pass.

Whole-run sensor means change from 0.621 to 0.647 ms and from 0.365 to 0.373 ms
with the added instrumentation (4.1% and 2.3% higher). These are diagnostics
overhead/noise measurements, not performance improvements. Temperatures range
from 69.1 to 71.6 °C. The reference has one initial 1.4 GHz sample followed by
62 at 1.5 GHz; all 64 instrumented samples are at 1.5 GHz. No normalization or
rendered-FPS estimate is made from these headless timings.

The kiosk is paused and suspended during each pair of headless runs, with a
remote cleanup trap that continues its process. Afterward its match resumes,
saved settings match byte-for-byte, and the service remains active with zero
restarts. Downloaded artifacts are checksum-verified before temporary Pi files
are removed.

## Reproduction

The existing runner writes the new stages and counters when built with
`sensor-profile`:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --seed 9216675843324634618 --seat 0 \
  --mode duel --match true --seconds 600 --asteroid-interval 0 \
  --landing-survey-hz 4 --profile-physics true --timing-csv true \
  --out /tmp/ground-route-profile
```

Repeat with `7725194555774358125`. Use the Pi for direct comparisons with the
table: desktop trajectories can differ. Preserve the ten-minute budget for the
first seed; its recorded spike occurs after three simulated minutes.

For cross-compilation, use a Pi-compatible linker. The default host GNU linker
required `GLIBC_2.43` and its first runner could not start on the Pi. A small
linker wrapper using Zig 0.16.0 resolved this:

```sh
#!/bin/sh
exec /path/to/zig cc -target aarch64-linux-gnu.2.31 "$@"
```

```sh
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=/path/to/pi-linker \
  cargo +1.89.0 build --locked --release --target aarch64-unknown-linux-gnu \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
```

Inspect required glibc versions before transferring a runner. Use separate
Cargo target directories when building different source worktrees: sharing one
reused the wrong executable during this investigation. The retained detailed
runner was explicitly rebuilt and checked for its new counter names before
execution.

`sensors.jsonl` retains `profile.stages` and adds `profile.counters`. New stages
separate `ground_route`, `ground_avoiding`, its node/edge filtering, and each
`landing_objective_candidate` and `landing_objective_routes` measurement.
Counters accumulate locally and enter the profile once when their enclosing
work completes, avoiding a clock read or map lookup on every capsule sample.
All of these additions are absent from ordinary builds.

Raw reports, complete CSVs, nested profiles, analysis and Pi collection scripts,
checksums, exact runners, source patch, compatibility failure and test logs:

`/home/oldman/.codex/visualizations/2026/09/13/ground-route-profiling/`

The companion startup fix removes the forced `--raster-scale 2.0` from both
kiosk unit files. Manual startup now uses the saved scale, with explicit CLI
overrides retaining their existing precedence. It takes effect on cabinets
after a normal image/service deployment; this investigation resumed their
existing installed build.
