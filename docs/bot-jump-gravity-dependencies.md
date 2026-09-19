# Route validity when scalar jump gravity changes

This implements the next slice from the
[asteroid objective investigation](bot-asteroid-objective-diagnosis.md), against
baseline `d439074` on `jetpack-landing-planning`. The optional live adapter now
lets a survey finish after scalar jump gravity changes, then validates the
individual successful routes. It no longer cancels walking and powered-route
work solely because an unrelated ordinary-jump hypothesis changed.

The dependency fix works in the controlled tests and delivers walking routes in
one retained asteroid match. It does **not** establish a stronger bot: those
routes arrive without fresh landing candidates, and neither asteroid comparison
delivers a powered route. The next bounded problem is the handoff between an
asynchronous survey and the controller's landing-site scan.

## Publication contract

The change applies only with `--objective-dependencies routes`:

- Keep the original job, snapshot, scalar gravity and measurement tick while
  work is pending. A gravity change does not restart or reinterpret that job.
- When publishing, compare current scalar gravity with the original value using
  the existing 0.01 tolerance. If it changed, withhold every route containing an
  ordinary jump on **either** the outward or return leg.
- A successful walking route can survive if its physical path remains valid.
  A powered route also needs its independent flight-field check and a current
  launch window. A powered route containing an ordinary jump elsewhere still
  depends on the original scalar gravity.
- Whole-survey publication, including negative answers, requires unchanged
  geometry, scalar gravity and flight dependencies. Otherwise publish only
  positive, validated entries and mark missing alternatives as unknown with
  `validated_routes_only`.
- Continue checking changed geometry, actual touchdown and both entrances,
  objective identity, equipment and the original 120-tick measurement age.
  A stale actual landed route cannot be replaced by a hypothetical landing.

An explicit launch-window check also guards the full-survey shortcut. Validation
filters a copy; it never changes the cached result, extends its life or performs
another world step. Every candidate must still finish before the job publishes.
Streaming partial positive results is a separate scheduling change.

Whole-region validation retains its strict gravity rule. Default synchronous
v9/v10 observations, UI choices and deployed Picade configuration are unchanged.
The optional live **v10 route-dependency adapter does change**, just as v11 does:
runner metadata is now `live_joint_objective_v4` / `live_jetpack_objective_v4`.
The strict-region v11 profile remains `live_jetpack_objective_v3`.

The shared allowance remains 16,384 graph operations and 1,024 physics queries
per update. Snapshot construction, dependency validation and the existing
synchronous on-foot observations remain outside that operation quota. This is
not a Pi frame-time guarantee.

## Tests

All **625 scenario/AI tests** passed in release, including **24 live-planning
tests**, which also passed in debug. New coverage checks:

- A real bounded survey completes with the same request and source tick after
  the planet's scalar gravity changes; valid walking routes survive and the
  physics world remains untouched.
- An ordinary jump on either leg is withheld, while independently valid powered
  routes remain eligible. Solver-generated jump diagnostics isolate this
  dependency contract; the existing physical flight tests cover execution.
- Expired launch windows are rejected even with unchanged geometry, and
  validation leaves retained output unchanged.
- A stale actual return cannot be substituted with another landing, and a
  changed scalar cannot certify old negative answers in unchanged geometry.
- The existing unrelated-debris/obstructed-path test now runs with both unchanged
  and changed gravity. The unrelated motion permits publication; an obstacle
  moved onto the selected path revokes it in both cases.

Client compilation, formatting and diff checks passed. Clippy completed with
existing warnings in unchanged files. No allowances or field/geometry tolerances
were increased.

## Matched simulation results

Five before/after comparisons use generated duel seed `7725194555774358125`,
a 600-second match limit, ground reuse and the same shared allowance. The four
v11/v10 comparisons swap seats with either no asteroids or mixed asteroids every
three seconds. A v10/v10 asteroid comparison retains strict-region validation.
All ten reports finish the round with clean physics audits; every recorded job
allocation and per-tick sum stays within the quota. The two asteroid baselines
are the retained `d439074` runs from the previous investigation; the other three
baselines and five candidate runs were executed for this change.

| Case | Before: result / seconds | After: result / seconds | Validated surveys by P1 / P2, before → after |
|---|---|---|---|
| Quiet, v11 P1 | P1 win / 598.35 | P2 win / 310.12 | 168 / 75 → 0 / 111 |
| Quiet, v11 P2 | P1 win / 438.73 | P2 win / 310.12 | 168 / 58 → 0 / 111 |
| Asteroids, v11 P1 | P1 win / 538.87 | P2 win / 221.57 | 0 / 0 → 8 / 0 |
| Asteroids, v11 P2 | Draw / 600.00 | P1 win / 459.58 | 0 / 0 → 0 / 0 |
| Asteroids, strict-region v10/v10 | P1 win / 215.12 | P1 win / 215.12 | 0 / 0 → 0 / 0 |

Every win in this table ends through pilot death. These are one seed and one
changed adapter shared by both live policies, not an estimate of v11's win rate.
The candidate loses both asteroid matches; do not promote it on these outcomes.
The strict-region control retains all **693 sparse trace records and 15 allocation
rows** exactly, excluding dispatch timing. Concurrent desktop timings are not a
hardware benchmark.

In the v11-P1 asteroid run, v11 finishes 127 candidates: 64 successful, including
16 powered, and 63 disconnected. It finishes 15 complete surveys and validates
eight for delivery. There are 77 deliveries, all containing walking routes;
none publishes a powered entry. Of 90 scalar checks, 89 find changed gravity,
and 83 validations preserve a nonempty independent result. Validation counts
include refresh checks that are not delivered, so 83 is not 83 publications.
No ordinary jumps or flight field/window entries are counted as withheld at
those respective checks. This does not bypass geometry or guarantee delivery
of every measured powered candidate.

The first delivery is tick **10,795**, from measurement **10,770**, with six
walking candidates at bearings 42–47 on planet 2. Both legs have zero ordinary
jumps. This demonstrates delivery under the changed scalar; it is not a physical
capture. The altered behavior diverges before the previous investigation's
10,890 and 11,400 requests, so those old requests are not directly comparable
later in the evolving match. Their dependency classes are covered by the
controlled tests, including the geometry rejection.

In the v11-P2 asteroid run, the request at **6,585** survives the former
gravity cancellation at 6,605. It measures four candidates, including two powered
successes, but finishes no complete survey before the ship is lost at **6,611**.
The controller switches to escape-pod recovery. The request correctly retires
with the ship unavailable. This is a remaining publication-latency case, not
evidence that a powered route was selected or flown.

## Next: make delivered routes selectable

The [landing-scan handoff follow-up](bot-landing-survey-handoff.md) implements
this next slice and records controller selection and physical captures. The
measurements here describe the earlier `0ab3bfe` checkpoint.

The trace records all 77 deliveries in the v11-P1 asteroid run, matching the
publication counter. Every one has a **deferred landing-site query, zero current
landing sites and no selected objective route**. The corresponding controller
in `tactical_sortie.rs` returns its clearance-climb command while the site query
is deferred. Its selection loop needs a current `PilotLandingSite` matching a
validated route. A path alone does not provide current touchdown clearance.

The live adapter delivers a newly finished result and can repeat it until the
request refresh interval, then starts refreshed work. Fresh landing scans and
completed objective work therefore need an explicit handoff. Simply completing
more searches does not establish that selection has both inputs together.

The next slice should retain a validated completed result until the next
appropriate landing scan, or request a bounded refresh of its candidate sites,
then join the current site data with individually revalidated routes. Preserve
the original measurement age, launch window, actual-route requirement and dirty
terrain checks. Never turn an old proposed landing into current clearance or
interpret an unmeasured alternative as unreachable.

Add a deterministic phase-offset test: the objective survey finishes between
landing scans, the next fresh scan supplies matching valid sites, and the
controller actually selects a route. Also test removed/obstructed sites and
expiration before that scan. Replay the two saved asteroid seats and the quiet
controls, recording the selected site and eventual physical progress. Earlier
publication of individually validated candidates may help the 6,585→6,611
deadline, but does not by itself solve the missing landing-site handoff.

The damaged flag-footing cases in the previous investigation remain a separate
navigation problem. This change adds neither terrain flights nor mining plans.

## Reproduction and evidence

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
  --asteroid-interval 3 --trace true --out /tmp/jump-gravity-p1
```

Swap the policies for P2, set asteroid interval to zero for quiet comparisons,
or use v10 for both seats with `--objective-dependencies region` for the retained
control. `target/jump-gravity-dependencies/` contains the before/after binaries,
reports, sparse traces, allocation tables, test logs and `summarize.py`. That
script verifies identical configurations, physics and operation totals, strict
control retention, and all recorded asteroid publication handoffs.

A verified archive is retained under
`/home/oldman/.codex/visualizations/2026/09/18/bot-jump-gravity-dependencies/`,
with a source manifest and checksums. Large sensor logs remain in the target
directory. Nothing from this slice has been deployed to a Picade.
