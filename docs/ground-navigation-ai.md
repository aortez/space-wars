# Enemy flags and shared spaceling navigation

The material Capture mission and ship recovery now share `GroundNavigationTask`.
After landing and exiting, a bot can approach an enemy flag, lower it for three
seconds, and raise its own for another three. It then returns to its ship, or
waits through the normal eight-second rebuild and boards the replacement.
Ownership, contact, balance, contested claims and transfers remain scenario rules.

This is a controlled outer-surface slice. It does not complete generated
multi-planet Spacewars, cave navigation, mining escapes, pod self-righting or
sustained random asteroid pressure. Those remain separate work.

## Observation and control boundary

`RecoveryTaskObservationV1` adds an optional `GroundMap`. Historical pilot and
flight V1/V2 observations retain their meanings. The survey is read-only and
runs only on foot, with clean completed material queries, every 30 ticks.
Player seats are staggered by 15 ticks. A missing survey between refreshes is
not evidence that the route has disappeared.

The map identifies its actor, planet, material revision and observation tick.
It samples at most 512 bearings in the retained planet body's local frame.
Each node comes from the first surviving material surface and requires an
acceptable support normal and spaceling capsule clearance. Detached fragments
do not count as planet ground. Clearance excludes only the querying spaceling;
other actors, vehicles and solid debris remain obstacles.

Directed edges inspect at most six bearings each way, bounded to 6,144 edges.
Walking requires continuous floor and capsule clearance. Jump candidates use
the spaceling's actual walk/jump limits and last shared gravity acceleration,
with nine clearance samples along a conservative arc. The graph is a planning
approximation; successful movement still requires real contacts and controls.
The denser survey matters near staircase corners and small craters, where
coarser samples can miss a reachable foothold.
If the adjacent footing is walkable, the survey omits longer jump shortcuts
in that direction. This bounds Pi cost while preserving obstacle checks where
walking stops being possible.

The task chooses a shortest measured route, with a cost for jumping, and emits
ordinary left/right and fresh jump/get-up presses. It does not move a body,
edit terrain, write health or ownership, or run another physics/gravity step.
Targets and waypoints remain in the moving planet's local frame. Completed
terrain edits invalidate cached routes; newly obstructed edges also replan.
Destroying or relocating the flag changes the destination. With no enemy flag
remaining, neutral input lets the ordinary claim rules act at the new footing.

## Composition and limits

`TacticalCapturePilot`, policy `tactical_sortie_v2`, composes the retained V1
flight/landing mission with this ground task. V1 continues receiving real
observations for claim and boarding milestones. The interactive Capture preset
and combat acceptance runner use V2; historical `TacticalSortiePilot` remains
available. A failed ground task records one failed capture attempt and hands
back to the existing combat/recovery fallback.

`RecoverShipTask` reports `recover_ship_v2` and uses the same task for hostile
flags and replacement hatches. Its HUD can distinguish surveying, walking,
jumping and getting up. Historical ordinary match policies are unchanged.

Each ground traversal has a fixed 90-second deadline, an eight-second movement
stall limit, and a five-second window to find a measured route. Missing hatch
footing is bounded to 15 seconds. An arrival does not later become a timeout
merely because its caller keeps observing it. Blocked tasks require caller reset.
Repeated observation ticks return the same intent; clone/reset, version and
actor checks are explicit.

Recovery retains its existing two-minute budget for pod stabilization, landing
and local rebuilding. If a distant hostile flag requires traversal, it receives
one fixed extra 90 seconds: at most 210 seconds from the original start. Edits,
new routes and further hits do not restart that clock. A three-minute acceptance
run therefore need not exhaust every possible recovery budget.

Large gaps can have no measured route. A parked pod or rebuilt hull can obstruct
the short path to a hatch and require a long trip around the planet. Those are
visible failures, not permission to walk through a vehicle or stand on debris.
The bot does not yet choose a better rebuild location based on the return route.

## Reproduce the controlled trial

```sh
cargo build --locked --release -p spacewars-ai --example surface_flag_soak
cargo run --locked --release -p spacewars-ai --example surface_flag_soak -- \
  --seed 42 --seat 0 --mode capture --edit none --out /tmp/flag-capture
```

The runner always advances 180 simulated seconds. A defender physically lands,
exits and claims first, then departs using the retained V1 pilot. The subject
also lands and exits using ordinary controls before its ground task takes over.
This isolates countercapture; it is not a claim under armed opposition.

Modes are `navigation` (stop after countercapture), `capture` (return and depart),
`recovery` (a physical heavy asteroid destroys the empty ship), and `pod` (the
same loss while aboard). `--seat 0|1`, `--seed` and `--offset` select the start.
The default angular offset is 0.6 radians. Hazard spawning belongs to the host;
damage, ejection and rebuilding use the shared gameplay pipeline.

`--edit crater` removes a radius-one circle from the selected route during the
walk. `flag` removes the flag footing; `blocked` cuts both directions with
radius-four circles. Edits use the ordinary queued material boundary. The runner
requires completion except for the deliberate `blocked` fixture. An explicit
`--expect blocked` can verify other known failure cases while retaining
`complete: false` and the actual reason in the report.
`--expect bounded` permits completion or a terminal ground failure after
countercapture, for pod returns where a parked vehicle can block the hatch.

Run the repeatable sixteen-case matrix with:

```sh
python3 tools/run-ground-navigation-trials.py \
  --binary target/release/examples/surface_flag_soak --out /tmp/flag-matrix
```

Use the equivalent `$CARGO_TARGET_DIR` path when set. The script supports an
installed remote runner through `--ssh`, repeatable `--ssh-option`, and
`--remote-out`. It records incomplete returns separately from accepted outcomes.

Reports retain goals, paths, invalidations, claim/loss/rebuild milestones,
per-second observations and audits. Observation and following-step audit ticks
are separate. Audits require finite bounded motion, no material issues, and
occupied plus removed cells equal to the starting count. The speed alarm stays
at 500 units/s. Timings separate observation/policy, survey-refresh frames and
the shared simulation step; a low overall P95 must not hide survey spikes.

## Validation

Final sixteen-case matrices ran on desktop and Pi, each for 180 simulated
seconds per case. Both platforms produced the same outcomes:

| Cases per platform | Completed | Bounded failure | Physics/material audits |
| --- | ---: | ---: | ---: |
| Capture, both seats, seeds 42/7 | 4 | 0 | 4/4 |
| Empty-ship loss, countercapture, rebuild and departure, both seats, seeds 42/7 | 4 | 0 | 4/4 |
| Small crater inserted during traversal, both seats | 2 | 0 | 2/2 |
| Flag destroyed during traversal, both seats | 2 | 0 | 2/2 |
| Both directions cut, both seats | 0 | 2 | 2/2 |
| Occupied-ship loss and pod recovery, both seats | 1 | 1 | 2/2 |
| **Total** | **13** | **3** | **16/16** |

Across both platforms: 32 accepted trials, 26 completions, four deliberate
unreachable-route failures, and two incomplete pod returns. P1 countercaptures
and rebuilds in that pod case, but the vehicle obstructs the short return path;
the long route reaches its fixed 90-second deadline. P2 completes its pod
recovery and departure. These are not 32 successful sorties.

The final cache optimization preserves the preceding desktop matrix's exact
claim and departure ticks. The runner now reuses its tactical observation's
recovery component, so Capture timing includes one survey, as in the client.
These serial runs measured:

| Metric: range across cases | Desktop | Raspberry Pi |
| --- | ---: | ---: |
| Observation/policy P95 | 0.003–0.101 ms | 0.010–0.339 ms |
| Survey-refresh observation/policy P95 | 5.35–6.06 ms | 13.69–15.41 ms |
| Largest observed survey-refresh frame | 6.17 ms | 15.64 ms |
| Shared simulation-step P95 | 0.049–0.055 ms | 0.144–0.156 ms |
| Largest observed shared step | 0.45 ms | 0.71 ms |

The original dense Pi survey took about 26 ms. Omitting redundant jump
shortcuts and reusing the capsule query shape reduced that cost. Survey frames
still use much of a 16.67 ms frame budget; these headless timings exclude GUI
rendering and do not establish generated-world performance.

The previous Pi combat outlier was also replayed separately with continuation
after its alarm. It retains the same 77-second speed spike (567.14 units/s,
113.26 rad/s), so the runner correctly exits nonzero. Pod stabilization still
settles at tick 5814, lands at 9251 and exits at 9253. It now walks/jumps toward
the enemy flag instead of immediately reporting `enemy flag route required`.
At 180 seconds it is still making progress, at waypoint 20 of 61; full recovery
from this particular impact is not established by the three-minute run.

Reports, preliminary failures, frozen desktop/ARM runner hashes and timing logs:
`/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/flag-navigation-20260908/`.
The final matrices are `desktop-matrix` and `pi-matrix`; older dense/sparse
experiments remain separately named. `pi-combat-repro.json` is the retained
alarm reproduction, not part of the clean 32-case ground matrix.

Final source checks: all 1,037 workspace/all-target tests pass (21 explicit UI
tests remain ignored in that command). The three material recovery, combat and
duel UI workflows were then run explicitly under Xvfb and pass launch, pause,
restart and both renderers. Navigation V1's six frozen episodes and Strategy
V1's twelve episodes match. Clippy completes with inherited repository warnings;
the new ground-navigation files have no remaining warnings. Formatting and
diff whitespace checks pass. No merge or push was performed.
