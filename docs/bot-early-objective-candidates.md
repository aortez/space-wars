# Using a finished route before its survey completes

This follows the [completed-survey handoff](bot-landing-survey-handoff.md) at
`136d2e7` on `jetpack-landing-planning`. That handoff helped quiet matches, but
the retained v11-P2 asteroid case lost its ship before a whole survey finished.

The optional live adapter can now publish individually finished positive routes
and pair them with a bounded current landing check. In that difficult replay,
v11 selects a powered route at tick **6,604**, before the scheduled scan at
**6,615**. It still loses its ship at **6,611**. This closes the measured delay
between a useful candidate and controller selection; it does not establish a
successful powered sortie or solve survival during a contested approach.

## Timing evidence

A read-only probe of the previous implementation found that request **6,585**
finishes its first successful powered candidate at the end of dispatch **6,603**.
Another finishes at **6,610**. Neither finishes the entire survey. The short
probe retains all **345** sparse trace records in its baseline prefix and the
same **26** allocation rows, excluding dispatch timing.

The new observation at **6,604** validates that first candidate, spends **57**
query units checking its current landing site, and delivers planet 1, bearing
56. The controller selects it on that update. Its source remains **6,585**;
publication does not give old evidence a new measurement date. The seven
deliveries through **6,610** represent one selected powered candidate. There is
no physical jetpack crossing during this approach; **6,611** starts pod recovery.

## Implementation and budget

Enable `--early-objective-routes true` with live planning, ground reuse and
`--objective-dependencies routes`. The route profiles become
`live_joint_objective_v6` and `live_jetpack_objective_v6`. With the option omitted
or false, the previous v5 behavior remains available. UI policies, default
synchronous observations and Picade settings are unchanged.

- A job exposes a candidate only after both directed legs and their path
  dependencies finish. Early output contains positive routes only, with
  `validated_routes_only = true`. Omitted alternatives remain unknown; this is
  neither a completed search nor an optimality claim.
- Publication runs the existing objective, source-age, equipment, path geometry,
  scalar-jump, flight-environment and launch-window checks. A landed request
  cannot use prospective routes in place of an unfinished actual return route,
  including through the unchanged-region shortcut.
- A valid partial result leaves the same job running. Losing its last published
  route sends `Stale` once, then `Pending` while other candidates continue. The
  120-tick source-age bound still applies. Completion counters count completed
  surveys; a separate counter records partial publication.
- When a prospective scan is `Deferred`, choose the cheapest unprobed positive
  route and run the ordinary landing-site check in the current world. It checks
  the feet, hull, belly, exit, settling margin and both boarding entrances. It
  supplies a real `PilotLandingSite`, not clearance copied from the snapshot.
  No extra cover survey runs; the controller retains its conservative treatment
  of absent cover information. Selected sites get normal observations afterward.

Each extra check has a hard cap of **192 query units**, at most one per actor
per physics tick and one attempt per bearing in a request. Each ray, hull test,
world capsule test and proposed-vehicle capsule test charges before it runs;
identical consecutive capsule queries retain the existing local cache. If any
charge fails, discard the whole check as unknown. Blocked or exhausted sites
can be revisited by ordinary scans or later requests. `NotRequested` does not
authorize an extra scan.

The cap must fit within an equal share of the configured queue capacity. With
two slots and the usual **1,024-query** allowance, early checks can spend at most
384 units together; remaining fuel goes to the existing shared scheduler.
Small allowances that cannot fund a complete cap keep the normal scan cadence.
Graph work retains its **16,384-operation** allowance.

The adapter records spent queries before dispatch and subtracts them from that
same tick's allowance. Repeated observation or dispatch cannot spend it twice.
Retiring a request between observation and dispatch retains an accounting row
for its old token. Partial publication also prevents delivered work being
counted as never published. The ledger is bounded by capacity and resets with
the episode.

These are operation limits, not frame-time guarantees. Snapshot construction,
dependency validation and ordinary synchronous sensors remain outside the quota.
Extra landing checks run during observation, so their wall time belongs to the
sensor timing columns, while their fuel appears in `live-planning.csv`.

## Verification

All **635 scenario/AI tests** pass in release. Debug runs pass all **31**
live-planning tests and **three** controller-handoff integration tests. Client
compilation, formatting and diff checks pass. Clippy completes with existing
warnings in unchanged code, including the pre-existing capsule-cache conditional
inside the modified landing-site function.

New cases cover finished positive legs and dependencies, exact equivalence with
the synchronous completed survey, the actual-return guard, current geometry and
query exhaustion, two-actor accounting, cancellation after sensing, repeat calls,
reset, low budgets, absent demand, path obstruction and original-age expiry.
The controller test advances the real world and a bounded job under deferred
scans for both planning modes. It verifies selection before survey completion;
controls are withheld to isolate this delivery contract. Physical outcomes are
measured separately below.

## Matched matches

Six new matches use seed **7725194555774358125**, a **600-second** time limit and
the same shared allowance as the five retained baseline matches. Quiet cases
have no asteroids; pressure cases have mixed arrivals every three seconds.
Both live adapters change together, so swapping seats is not an isolated
v10-versus-v11 policy experiment. One seed does not establish a win rate.

| Case | Baseline result / seconds / ownership | Early result / seconds / ownership | Extra landing checks / queries |
|---|---|---|---|
| Quiet, v11 P1 | P1 / 600.00 / 2–1 | P1 / 600.00 / 2–0 | 3 / 171 |
| Quiet, v11 P2 | P1 / 600.00 / 2–1 | P1 / 600.00 / 2–0 | 3 / 171 |
| Asteroids, v11 P1 | P1 / 209.60 / 1–1 | P1 / 497.30 / 2–0 | 1 / 57 |
| Asteroids, v11 P2 | P1 / 459.58 / 1–0 | P1 / 459.58 / 1–0 | 1 / 57 |

Quiet rounds end at the time limit; asteroid rounds end through pilot death.
All reports have clean physics audits. Every allocation, per-tick sum and total
matches its shared quota and telemetry. None of the eight extra site checks
exhausts its cap. Concurrent desktop timings are not Pi benchmarks.

The two additional controls keep early publication disabled. The strict-region
v10/v10 control retains **693** sparse trace records and **15** allocation rows;
the route-profile v10/v11 control retains **1,273** records and **26** rows.
Both equal the retained baseline, excluding allocation dispatch times. Sparse
trace equality is not a dense every-tick comparison.

Both quiet early runs physically capture P2's planet 2 with P1, then board and
depart. They select bearing 11 at **12,793**, land at **13,781**, capture at
**14,147**, record boarding at **14,149**, and complete departure at **14,334**.
The baseline P1 v11/v10 departures were **14,391 / 14,400**. These are walking
sorties; no powered route is selected in the quiet runs. P1 still completes two
sorties and P2 one in each quiet match. The final ownership change alone should
not be interpreted as another completed capture.

With v11 as P1 under asteroids, the first approach starts at **10,783**, compared
with **10,830** before. That early path becomes stale at **10,797**; the bot
selects again at **10,830** and replans at **11,011**. No enemy-flag capture or
powered crossing is established during this approach. The longer round later
includes pod recovery, and its different outcome details do not establish that
earlier planning is generally stronger. It also performs substantially more
total search work: 24,080,273 graph units / 2,661,735 queries versus
2,768,098 / 257,162 before, across different match trajectories and durations.

With v11 as P2, the first powered selection now succeeds as described above,
but the round's finish and pod transition remain unchanged. Work is the same
**93,396** graph units plus **23,773** queries, exactly **57** queries above the
baseline. This is the clearest evidence that the delivery gap is fixed while
the physical survival problem remains.

## Next investigation

The next useful question is how much approach time a candidate needs, and when
the bot should abandon a contested landing. The P2 replay offers only seven
updates between selection and pod conversion. Inspect its observed health,
incoming threat, circling/landing controls and route distance around
**6,580–6,611** before expanding search or changing maneuver controls. Keep
planning delay, clearance availability, selection, and physical execution as
separate milestones. The P1 **10,797 / 11,011** revocations remain counterexamples
where more eager delivery does not keep ground access valid.

Do not promote v6 from these results alone. A bounded follow-up should compare
an explicit approach/retreat decision against retained v5/v6 runs and preserve
the quiet capture/return fixtures. Broader mission utility and general sensor
budgeting remain later parts of the [framework plan](design/budgeted-bot-planning.md).

## Reproduction and retained evidence

```sh
RUST_MIN_STACK=16777216 cargo +1.89.0 test --locked --release \
  -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --seed 7725194555774358125 --asteroid-interval 3 --trace true \
  --out /tmp/early-objective-p2
```

Swap policies for P1 or set interval zero for quiet runs. Disable the new option
for the retained route control; use both v10 and region dependencies for the
strict control. `target/early-objective-candidates/` retains binaries, the
read-only timing probe, regression logs, raw traces, reports and quota audits.
`summarize.py` verifies matched settings, current-site publications, control
equivalence and physical capture milestones.

A source manifest and verified archive are retained under
`/home/oldman/.codex/visualizations/2026/09/18/bot-early-objective-candidates/`.
Large sensor logs remain in the target directories. This is a local checkpoint;
it has not been deployed to a Picade.
