# Powered capture in the v13 mission controller

Powered capture is implemented as an opt-in v13 configuration. It completes
the directed blocked captures with native synchronous surveys. The shared
4/384 planner cannot deliver either walking or powered landing routes before
expiry, and the armed results do not support changing defaults.

The [local execution study](capture-jetpack-approaches.md) completed all six
blocked approaches using the existing jetpack model. This experiment connects
that model to the actual mission controller, preserving its destination
selection, capture handoffs, recovery and retry memory.

`--powered-capture-seats none|0|1|both` enables the headless v13 experiment.
The instance selects `JetpackRoundTrip` for both sensor requests and new local
capture tasks. Configuration is allowed before the first intent and survives
clone/reset. Other policy versions reject enablement. Mission telemetry and
the instance sensor descriptor identify the option; disabled serialization
and policy defaults retain their prior behavior. Historical transfer work is
invalidated when its source's route model changes.

## Budget distinction

The preceding v13 experiments used `--live-objective-seats none`. Their shared
4 graph / 384 query allowance covered queued mission, neutral and flag work;
native landing-objective surveys remained synchronous. The previous success
does not show that a whole landing survey fits that dispatch allowance.

Measure both configurations explicitly. The native control retains synchronous
landing surveys. The shared configuration routes landing work through the
same 4/384 allowance, with the existing ground reuse, route dependency checks
and early positive publications enabled. Mission and flag jobs use the
remaining allowance. Keep the 120-tick measurement lifetime and all geometric,
equipment, launch-window and fuel checks. On-foot sensors, snapshot creation
and publication validation retain their existing synchronous work in both
configurations. Operation counts are not a total frame-time limit.

Do not raise allowances or extend lifetimes in response to these outcomes.
Failed delivery is distinct from a negative route measurement, controller
rejection, physical failure and a lost match. A walking control that also
cannot finish its survey identifies a broader delivery problem.

## Frozen trial plan

Freeze implementation, analysis and this plan before measuring outcomes:

1. Replay all three `target/capture-jetpack/v1` source commands with the option
   disabled. Require the original reports, all 18 controller/planner streams
   and 32,400 ordinary sensor records to match. Preserve diagnostic source
   files and hashes.
2. Run six paired 180-second directed missions: native/shared delivery crossed
   with flag -0.8 and cover response on/off, plus flag +0.8 with cover response
   on. All use seed 42, seat 0 and the same value-destination fixture. Each
   pair differs only in the powered-capture option. Keep destination retry,
   published flag costs and current-neutral sensing enabled in both arms.
3. Run eight paired armed matches: native/shared delivery, two generated worlds,
   and v13 in both seats against v10. Seeds are the first eight SHA-256 bytes,
   little endian, of `powered-mission-integration-v1:0` and `:1`. Both pilots
   use the live adapter in shared mode. Use normal 600-second match rules,
   no asteroids and the same options for v13 as the directed cover-on case.
   Alternate arm order by world/seat. Do not omit armed cases if bounded
   delivery fails; report that limitation separately.

This is 3 retention replays plus 28 new missions/matches. Keep every loss,
unfinished visit and unavailable route. Two independent runs may execute
concurrently; their desktop timings are not Pi benchmarks or speed comparisons.

`--trace-capture-evidence true` records each consumed observation's physical
pilot/planet state, mission/capture state, actions, route evidence and jetpack
equipment without extra sensors or controls. Audit contiguous clocks, model
identity, original measurement age, validation date and powered launch windows.
Check claim ownership and counters, exit/boarding transfers and departure in
the original target's frame. Retain milestone and flight witness rows. Record
the first control difference in each pair and distinguish physical claims
from completed sorties and final ownership.

Require every tick's combined dispatch charge to remain at or below 4/384 and
preserve physics conservation checks. Report completed measurements,
publications, expiry/restart reasons, powered selections/launches, physical
claims/returns/departures, combat activity and match results. These correlated
directed cases and two generated worlds cannot establish a broad win rate or
justify changing bot defaults.

```sh
python3 tools/validate-powered-mission.py \
  --source target/capture-jetpack/v1 \
  --binary target/powered-mission/surface_mission_soak-COMMIT \
  --out target/powered-mission/v1
```

## Audit correction

The frozen `3742b98` implementation completed all 31 planned replays/trials.
The first audit rejected one armed run because it required a bidirectional
vehicle forecast for every jetpack launch. The recorded launch was the existing
v10 opponent's return trip across a measured terrain gap (seat 0, tick 22,871,
`native-armed-world1-p2-powered`). That path uses the older corridor sensor,
not the new vehicle forecast. No gameplay failure is inferred from this audit
assumption.

Preserve all original files and the rejected summary. Before re-auditing, pin
their hashes and freeze a correction that distinguishes `ground_navigation_v12`
forecasted flights from existing v10/v11 corridor flights. Keep forecast age,
identity and charge checks for the former; require a matching native corridor
from the latest 30-tick ground survey and launch charge for the latter. Keep
both categories in the results, identified by actor and controller. Re-audit
the exact saved games into a separate directory without rerunning or changing
any gameplay configuration.

The immutable input manifest pins all 369 original files (including the failed
summary and its unclassified run) at
`target/powered-mission/frozen-v1-files.json`, SHA-256
`b5b87eb5c02c18f3b7db0ec3eab33ea8f6c789ff3bb06b40ce7ae48ba38e762e`.
The re-auditor verifies that complete file set before and after analysis, retains
the original three retention results, and requires all 14 paired comparisons
to remain identical. It writes new witnesses only under its new output path.

```sh
python3 tools/audit-powered-mission.py \
  --manifest target/powered-mission/frozen-v1-files.json \
  --out target/powered-mission/v2
```

That correction (`ff4abf7`) audits both actual legacy launches, then encounters
a second logging assumption at tick 23,790 in the same run. The ground task
pauses for a new terrain survey at tick 23,772, while its crossing remains in
`Lift`. It resumes that same crossing with 84.44% charge; its actual launch
was tick 23,681 with 100% charge. A ground-goal transition alone was incorrectly
counting another takeoff. Preserve the v2 rejected summary, SHA-256
`56fb70746352fa1b79acd95d19b62d4d68a17bba155c5cff5b98d2931cfec9f3`.

Freeze a second audit correction: identify each takeoff by actor, ground-task
start and crossing-task start; retain subsequent Lift resumptions separately.
Require the original charge threshold at every takeoff and matching, fresh
corridor evidence at resumptions. Reuse the same pinned games and comparisons.

```sh
python3 tools/audit-powered-mission.py \
  --manifest target/powered-mission/frozen-v1-files.json \
  --previous-audit target/powered-mission/v2/summary.json \
  --out target/powered-mission/v3
```

## Mission results

The `3742b98` gameplay binary completed all 31 planned replays/trials. The
`c208352` auditor passes all 28 new runs in `target/powered-mission/v3`, with
all original files and all 14 paired comparisons unchanged. All three disabled
retention replays match their original physical/mission report fields,
18 controller/planner streams
and 32,400 ordinary sensor records.

In both blocked native cases, walking completes only the neutral capture.
Powered v13 completes the enemy capture and then the neutral capture, retaining
its real mission/retry state throughout. With cover response either on or off,
the enemy visit lands at tick 5,149, exits at 5,161, launches at 5,192 with 100%
charge and a fresh forecast, completes the crossing at 5,670, claims at 6,590,
boards its original ship at 7,095 and departs at 7,320. Both powered runs finish
two physical sorties by the 180-second horizon; walking finishes one.

The walkable control completes two sorties in both arms, with no jetpack
launch. The powered arm departs its enemy visit at tick 6,357 versus 7,387 for
walking. Powered mode reuses the existing v12 capture controller, including
continuous walking; the comparison therefore includes those existing control
differences, even when no flight is used.

All six shared directed runs finish with zero physical claims or departures,
zero completed landing candidates and zero published landing surveys. Each
blocked run records 29 expired requests; each walkable control records 31.
No flight forecast starts. This is unavailable evidence, rather than a measured
negative route or a rejected powered flight.

## Shared delivery diagnosis

The first shared blocked request starts at tick 3,270 and remains pending
through 3,390. It receives 121 dispatches, spending 476 graph steps and 6,724
physics queries, and expires on observation 3,391. The last dispatch spends
4 graph steps and only 48 queries. Increasing query throughput alone cannot
address this graph-work limit.

The earlier topology probe at the same clock contains 503 retained ground
nodes. Every recorded pilot field except the omitted site list, and the full
planet observations, match this request's physical input. In
`ground_navigation/survey_job.rs`, each retained node needs at least one
`Candidate` graph operation in each direction. Successful adjacent walks can
skip longer spans, so the conservative lower bound is **1,006 graph steps**,
before phase transitions, hull overlays or route solving. The 120-tick evidence
lifetime permits at most **484 graph steps** across the inclusive dispatches.

`live_planning/objective_job.rs` must complete that base ground survey before
checking candidate hulls, round trips or flight forecasts. Thus this first
request cannot leave its ground phase within the allowance. Across all 14
shared runs there are zero completed landing candidates, zero publications and
zero started flight forecasts. Ground reuse and early publication do not
produce a usable result in these trials; the directed blocked request records
no reused ground work. The manifest retains its complete dispatch ledger,
source topology, matching observations and expiration evidence.

The next implementation step is to reduce the work required before the first
usable landing route, beginning with the base ground survey. Any focused survey
or reuse change must retain physical route checks and the original evidence
age, and demonstrate timely delivery under the same 4/384 allowance. These
results do not justify increasing quotas or extending expiry.

## Armed matches

All 16 armed matches reach a normal match ending and pass physics checks. The
table counts the experimental v13 seat's physical claims and completed
departures; ownership can subsequently change. Results are from v13's side.

| Delivery | World | v13 seat | Walking claims / departures | Powered claims / departures | Walking result | Powered result |
| --- | --- | --- | ---: | ---: | --- | --- |
| Native | 0 | P1 | 1 / 1 | 3 / 1 | Loss | Loss |
| Native | 0 | P2 | 3 / 3 | 3 / 3 | Win | Win |
| Native | 1 | P1 | 3 / 3 | 3 / 3 | Win | Loss |
| Native | 1 | P2 | 5 / 5 | 4 / 4 | Win | Win |
| Shared | 0 | P1 | 1 / 1 | 1 / 1 | Loss | Loss |
| Shared | 0 | P2 | 2 / 2 | 2 / 2 | Win | Win |
| Shared | 1 | P1 | 1 / 1 | 1 / 1 | Loss | Loss |
| Shared | 1 | P2 | 1 / 1 | 1 / 1 | Win | Win |

The native world-0 P1 powered bot launches one forecasted flight at tick 12,405,
then records a flight interruption at 12,810 and abandons that visit at 13,123.
It earns three claims but only one completed departure in that match. This
flight has no recorded crossing completion. The other native powered matches
contain no powered-v13 takeoff. The world-1 P2 run contains two measured terrain
hops by its v10 opponent, with one survey pause/resumption; these are retained
separately from the experimental vehicle crossing.

The shared armed runs retain ordinary captures and combat activity, with no
observed powered launch or delivered landing route. In world 1, all shared
arms end at tick 7,385 when P1 hits the sun. Those wins/losses cannot establish
powered-route effectiveness. The native sample has two powered wins versus
three walking wins, including one changed winner; two worlds and seat swaps
cannot estimate a general win rate. Keep powered capture opt-in.

## Evidence and checks

The [result manifest](data/powered-mission-integration-v1.json) contains every
run's physical visit results, combat counters, delivery counts, allocation
totals and paired differences. The [compressed evidence](data/powered-mission-integration-v1.json.gz)
preserves the exact original rejected summary, both re-audit summaries, the
369-file raw manifest, milestone/flight witnesses and the budget diagnosis.
The full per-tick streams remain in their hashed local paths.

The successful audit checks 849,938 pilot observation rows and all 489,769
new-trial dispatch ticks. Combined charged work stays within 4 graph / 384
queries per tick, including retired work. This excludes native synchronous
sensors, snapshot creation and publication validation, as declared above.

Validation: 478 Rust tests, 644 Python tests, strict AI library/harness Clippy,
formatting and both profiled and ordinary release builds pass. No Pi performance
measurement or default change is claimed. Binary SHA-256:
`9fd02615d61b6554722783429254a3a82872a02280395519a4f5e98a4ac95207`.
Final audit summary SHA-256:
`e537ffbb45058d71c3711f4e31378eb7d2bcf6f59fa97bee78a4c8d71e4a2200`.
