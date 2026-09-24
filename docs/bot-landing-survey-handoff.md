# Handing completed objective routes to a fresh landing scan

This follows the [scalar-gravity dependency fix](bot-jump-gravity-dependencies.md)
at baseline `0ab3bfe` on `jetpack-landing-planning`. That fix allowed valid routes
to finish, but the saved asteroid run delivered all 77 results between landing
scans, with no current landing candidates. The controller could not select them.

The live route-dependency adapter now delivers a revalidated completed result
alongside the next current landing scan, before starting replacement work. Both
quiet comparisons now select an enemy-flag route and physically land, capture,
reboard and depart. One asteroid comparison starts an approach; the other still
loses its ship before completing the first survey. This closes the measured
handoff gap, without establishing general v11 superiority or powered-route use
in those asteroid matches.

## Implementation

Previously, a ready job could publish on its first completed observation and
until its 30-tick refresh boundary. At that boundary, the adapter salvaged its
measurements and submitted another job. If a fresh landing scan arrived on that
same update, it received pending work instead of the completed result.

With `--objective-dependencies routes`:

- Revalidate and deliver the completed survey on the refresh update before
  replacing the job. The observation remains `Ready` even though replacement
  work has already entered the queue for the next shared dispatch.
- If prospective landing scans are deferred or not requested, retain the same
  completed job past the refresh boundary. Continue the existing dependency
  checks on every observation. Refresh when the next actual scan arrives.
- An actual landed-route survey can refresh without prospective sites, since
  the current touchdown and entrance observations supply its context.

There is no second result cache, extra scan, query or world step. A retained job
continues to occupy one slot in the same bounded queue. Refresh retires that job
before submitting its replacement. It can salvage the same ground measurements;
neither waiting nor replacement renews their original measurement tick.

All earlier checks remain: maximum 120-tick source age, objective identity,
equipment, actual touchdown and entrances, scalar-jump dependencies, directed
path geometry, flight environment and launch window. Removed landing sites are
not restored from the old survey. The controller still needs current measured
`PilotLandingSite` data, and immediate action permissions remain authoritative.

The optional v10/v11 route profiles are now `live_joint_objective_v5` and
`live_jetpack_objective_v5`. Strict-region adapters keep their previous behavior
and profile names. Default synchronous bot observations, UI choices and Picade
configuration are unchanged.

Two counters describe the handoff without storing a history:

- `publications_with_current_sites`: a delivery containing at least one
  successful route whose ID matches a current measured landing site. Repeated
  deliveries count again; this is not a selection or capture count.
- `held_for_site_refresh`: repeated successful validations retained beyond the
  refresh boundary while prospective scans are absent.

The shared allowance is unchanged at 16,384 graph operations / 1,024 physics
queries per update. Snapshot construction, dependency validation and other
synchronous sensors still sit outside that quota. Longer retention can perform
more validation checks; no Pi frame-time improvement is claimed.

## Regression coverage

All **628 scenario/AI tests** passed in release, including the existing physical
flight, capture and return fixtures. Debug runs passed the **25 live-planning
tests** and both new handoff integration tests. Client compilation, formatting
and diff checks passed. Clippy completed with existing warnings in unchanged
files.

The integration test runs a real bounded job while the world advances and the
controller receives deferred landing scans. Its opposing flag is supplied in
the observation on real measured ground; controls are withheld to isolate the
selection contract. For both joint and powered planning, the next scan at ages
45, 60 or 120 must deliver the original result and cause the real capture
controller to select a matching positive route. Replacement work starts on that
same update, with one retained request and no duplicate dispatch allowance.
This test failed on `0ab3bfe` because the fresh scan saw `Pending`.

Companion cases reject selection when the fresh scan has no sites, queries are
dirty, or the evidence has reached age 121. A physical collider test moves
debris onto a route after its completed job has been held past the refresh
boundary: the path is removed at handoff. Existing tests retain coverage of
expired flight windows, either jump leg, changed entrances, actual-return
requirements, cancellation and reset. These tests do not substitute for the
physical match milestones below.

## Matched comparisons

Five comparisons use the same generated duel seed `7725194555774358125`,
600-second match limit, ground reuse and shared allowance as the previous slice.
Both live route adapters change together. Quiet runs have no asteroids; the
asteroid cases use mixed arrivals every three seconds. Baselines are the five
retained `0ab3bfe` results. All ten reports finish the round with clean physics
audits, and every job allocation and per-tick sum respects the quota.

| Case | Before result / seconds | After result / seconds | After deliveries with matching current sites / all deliveries |
|---|---|---|---|
| Quiet, v11 P1 | P2 win / 310.12 | P1 win / 600.00 | 3,158 / 4,137 |
| Quiet, v11 P2 | P2 win / 310.12 | P1 win / 600.00 | 3,157 / 3,971 |
| Asteroids, v11 P1 | P2 win / 221.57 | P1 win / 209.60 | 152 / 159 |
| Asteroids, v11 P2 | P1 win / 459.58 | P1 win / 459.58 | 0 / 0 |
| Asteroids, strict-region v10/v10 | P1 win / 215.12 | P1 win / 215.12 | 0 / 0 |

The new quiet matches reach the time limit with planet ownership **2–1**. The
other results end through pilot death. These are one seed, swapped policies and
a common adapter change, not a win-rate estimate. Concurrent desktop timing is
not a device benchmark.

The unchanged v11-P2 asteroid comparison retains all **1,273 sparse trace
records / 26 allocation rows**. The strict-region control retains **693 records
/ 15 rows**. Allocation equality excludes dispatch timing. These checks concern
recorded sparse traces, not a new dense every-tick comparison.

### Physical enemy-flag captures in quiet play

In both quiet runs, P1 chooses bearing 11 on planet 2 while it belongs to P2.
The same controller then performs the capture and return through ordinary
actions:

| Milestone | v11 as P1 | v10 as P1 |
|---|---:|---:|
| Route selected with current landing site | 12,870 | 12,840 |
| Landed | 13,798 | 13,808 |
| Enemy planet captured | 14,164 | 14,174 |
| Reboarded assigned ship | 14,166 | 14,176 |
| Completed departure | 14,391 | 14,400 |

Values are simulation ticks at 60 Hz. The retained trace shows ownership
changing from P2 through neutral to P1, and the pilot's transfer count increasing
from two to four for this second exit/return. Both P1 policies finish two
sorties, compared with one in their respective baselines. This is physical
progress beyond merely generating more plans.

The v11-P1 quiet run also delivers 88 powered entries from eight measured powered
candidates; those are repeated prospective deliveries. No powered objective
route is selected in these quiet traces. The capture above uses walking access.

### Asteroid progress and remaining limits

With v11 as P1, the first handoff at **10,830** carries measurement **10,800**
and selects planet 2, bearing 42. The controller starts circling into cover.
All 152 deliveries containing current matching sites also show that selected
objective route. The eight completed-and-validated surveys are the same count
as before; the difference is that their results now reach selection.

At **11,011**, the controller receives stale work, clears the route and resumes
surveying. No landing, new capture or jetpack crossing is established during
this approach. Three powered candidates are measured over the run, but none is
published. The eventual match win does not prove that this approach succeeded.

With v11 as P2, request **6,585** still measures two powered successes among four
finished candidates but no finished survey before the ship is lost at **6,611**.
The next scheduled flag-approach scan is **6,615**. A handoff of completed work
cannot help a survey that never finishes or a scan that arrives after ship loss.
Its identical trace confirms this slice does not change that case.

## Next bounded question

Reduce the time to a **usable** first candidate under pressure. Measure when an
individual positive candidate becomes available, then consider publishing that
candidate after its own path/field checks while other candidates remain unknown.
Pair it with a bounded current landing-site check if the normal scan arrives too
late. Earlier path publication alone would still miss the 6,611 ship-loss deadline
when the next scan is at 6,615.

Retain the same total quota and original evidence age, explicit incomplete-result
semantics, actual-return guard and launch-window checks. Compare time to first
validated candidate, fresh clearance, selection and execution separately. Do not
assume earlier selection would save this ship or weaken the safety checks to make
it do so. The 11,011 stale-route case and damaged flag-footing investigation also
remain useful counterexamples: valid access can disappear, and some terrain
needs a different physical maneuver rather than more scheduling changes.

## Reproduction and retained evidence

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v11 --p2-policy material_mission_v10 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 3 --trace true --out /tmp/landing-handoff-p1
```

Swap policies for P2, set interval to zero for quiet comparisons, or use v10 for
both seats with `--objective-dependencies region` for the control.
`target/landing-survey-handoff/` retains binaries, reports, sparse traces,
allocation tables, regression logs and the expected failing baseline test.
`summarize.py` audits settings, quotas, trace retention and handoff counters;
`milestones.py` verifies the two quiet capture/boarding/departure sequences.

A verified source manifest and archive are retained under
`/home/oldman/.codex/visualizations/2026/09/18/bot-landing-survey-handoff/`.
Large sensor logs remain in the target directories. This slice is a local
checkpoint; it has not been deployed to a Picade.
