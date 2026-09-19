# Preserve independent routes while flight evidence changes

This follows the opt-in v11 checkpoint `e58cdfc` on
`jetpack-landing-planning`. The local dependency adapter now reports
`live_jetpack_objective_v2`. The bot policy remains `material_mission_v11`;
v9/v10 and the launcher's Planner choice are unchanged. This slice has not
been deployed to the Pi. The numbers below describe checkpoint `3e03fca`;
the follow-up [moving-flight model](bot-moving-flight-planning.md) adds orbital
prediction, launch timing and physical calibration without promoting v11.

## Diagnosis

The previous flight environment compares source positions and velocities in
the measured planet frame, with 0.01-unit tolerances and a 0.0001 spin
tolerance. Sources move relative to a rotating/orbiting planet. Those changes
can matter to a real flight; simply ignoring them or widening the tolerances
would not establish that the same maneuver still works.

The immediate planning bug was broader: once any candidate attempted a flight,
an environment mismatch discarded the entire request, including independent
walking routes and candidates whose flight forecast had already failed.

A read-only instrumented replay of the old quiet P2 comparison reproduced all
269 flight-environment restarts. Every retired request had completed at least
one usable walking route; none had completed a powered route. The stages at
retirement were 242 hull overlays, 26 flight forecasts and one graph search.
Requests were 4–28 ticks old. They had spent 17,730,632 graph operations and
3,359,534 physics queries. These are charged totals in the retired requests,
not a claim that every operation was avoidable or that this is elapsed CPU time.

## Change and validity contract

With route dependencies enabled, a flight-field change no longer cancels a
pending survey. The job finishes against its original coherent snapshot and
the existing operation allowance. At publication:

- If the field and whole geometry remain compatible, the complete survey can
  still be delivered, including negative answers.
- If the field changed, only successful routes without a powered crossing can
  survive. Powered routes and negative answers are withheld. An old failure is
  not proof that a flight remains impossible in the new field.
- Surviving routes still need the existing whole-region or selected-path
  geometry certificate. If the actual landed route is unavailable, alternative
  prospective landings cannot substitute for it.
- Filtering does not mutate the retained job result. Every later delivery is
  validated again. Whole-region mode retains its previous strict field check.

The 16,384 graph / 1,024 physics-query shared allowance, 120-tick measurement
age, snapshot identity, scalar jump-gravity check, equipment, objective and
hatch checks are unchanged. There is no new physical world step or gravity
solve. The flight controller, fuel limits and field tolerances are unchanged.

The bounded telemetry adds forecast starts, approvals and rejection reasons;
field checks and mismatches; withheld powered entries; and per-actor completed
and published survey counts. Starts may include cancelled, unfinished work.
Approvals are model results, not published plans or actual flights. Validation
counts can include repeated checks and a successful check just before request
refresh, so they are not distinct-plan counts.

## Controlled checks

The new mixed-route test runs the production incremental job with the ordinary
16,384/1,024 quota, changes the observed spin once flight work starts and verifies
that the same request finishes without restarting. It retains successful ground
routes and removes powered answers. It also checks read-only physics, forecast
accounting, unchanged cached output and the strict whole-region contract.
Collider poses are deliberately held fixed in this sensor test to separate
flight validity from geometry and hatch motion.

A second test verifies that a stale powered route from the actual landed ship
cannot be replaced by an unrelated proposed landing. Existing tests cover moved
obstacles, return-path obstruction, missing equipment, changed hatches, terrain
edits, age, clone/reset and ordinary physical exit/flight/capture/boarding.

The full scenario/AI regression run passed **615 tests**. Client compilation,
formatting and diff checks passed. Clippy completed with existing warnings in
unchanged code and no warnings in this change.

## Matched replays

Seed `7725194555774358125`, generated duel, normal match rules, 600-second
limit, live planning for both seats, ground reuse and route dependencies.
Asteroid pressure uses the existing mixed severity at a three-second interval.
The baseline is the retained `e58cdfc` comparison binary. All five new runs
finished with clean physics audits; every allocation and charged total passed
the unchanged quota audit.

| Conditions / v11 seat | Completed surveys before → after | Published deliveries before → after | Charged physics queries before → after |
|---|---:|---:|---:|
| Quiet / P1 | 243 → 243 | 4,428 → 4,428 | 932,384 → 932,384 |
| Quiet / P2 | 221 → 356 | 3,804 → 6,629 | 5,001,242 → 1,906,736 |
| Asteroids / P1 | 0 → 0 | 0 → 0 | 2,941,698 → 3,643,681 |
| Asteroids / P2 | 0 → 0 | 0 → 0 | 13,409 → 1,956,217 |

These counts combine both seats. In the updated quiet P2 run, v11 completed
149 surveys and received 2,817 deliveries; v10 completed 207 and received
3,812. Total graph work fell from 48,378,677 to 42,995,150. Match trajectories
diverged after the fix, so the query reduction is replay evidence, not an
isolated hardware speedup. Concurrent run timings are not Pi benchmarks.

Quiet P1 retained the same outcome and charged totals. Quiet P2 changed from a
2–1 v10 ownership win to a 2–1 v11 ownership win at the time limit. Asteroid P1
changed from a v10 win at 482.67 seconds to a v11 win at 538.87 seconds;
asteroid P2 changed from a v10 win at 298.67 seconds to a 600-second draw.
One seed with different subsequent trajectories is not a strength ranking.

A separate retained-v10 replay has **1,531 identical sparse trace records** and
**5,522 identical allocation rows**, excluding dispatch time, compared with the
previous checkpoint. This is not a dense every-tick equivalence test.

## What remains

The quiet P2 run started 195 flight forecasts: 120 were model-approved and 75
rejected for fuel reserve. All 1,544 publication-time field checks failed the
unchanged compatibility check. Independent ground routes survived those checks,
but this run does not establish usable prospective flight plans on moving worlds.

The flight model still approximates source motion at constant velocity and
does not model the curved orbital trajectory of its translating frame. A next
flight slice should predict the known frame trajectory and certify a forecast
for its intended launch time, with physical calibration across orbit, spin,
planet radius and both crossing directions. Avoid treating source identity or
predictable motion alone as proof that a forecast made for an earlier launch is
still valid. Final on-foot observation and real world permissions remain
authoritative.

Asteroid runs still delivered no completed objective surveys. Their 280 / 159
`routes_changed` rejections each withheld eight entries, with **zero selected-path
checks**. That means those completed surveys contained no successful routes to
certify; it does not show 439 otherwise useful paths being hit by asteroids.
The next terrain investigation should retain the raw negative survey at its
original snapshot and distinguish missing exit/flag/return nodes, disconnected
walk/jump topology and failed flight proposals. Scalar gravity changes also
still restart work. Keep the conservative negative-answer and age rules while
investigating these separately.

## Reproduce and retained evidence

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --seed 7725194555774358125 \
  --asteroid-interval 0 --trace true --out /tmp/moving-objective-quiet-p2
```

Swap seats and use interval 3 for the other matched runs; use v10 in both seats
for retention. `target/moving-objective-planning/` retains the binaries, source
patches, five final reports/traces/allocation tables, test logs and quota/equality
audit script `summarize.py`. The instrumented old replay is `diagnostic-quiet-p2`;
its raw restart log and `diagnostic.patch` reproduce the diagnosis from `e58cdfc`.
`quiet-p2` is an intermediate run before the telemetry label/profile revision;
the final comparisons are `final-*` plus `retained-v10`.

A compressed copy is retained at
`/home/oldman/.codex/visualizations/2026/09/14/bot-moving-objective-work/evidence.tar.gz`,
with a manifest and comparison summary beside it. The original checkpoint's
archive is documented in [bot-jetpack-landing.md](bot-jetpack-landing.md).
