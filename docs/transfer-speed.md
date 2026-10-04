# Relative closing-speed guidance during committed transfer

## Frozen question and policy

The [clear-entry experiment](transfer-approach.md) still enters a neighboring
planet's local frame too quickly: at the transition, 104.393 units of clearance
is below the unchanged climb rule's 111.288-unit requirement for the observed
45.436 inward speed. Test a bounded velocity constraint before that transition.
Preserve the native climb, frame selection, arrival and physical capture gates.

`relative_transfer_speed_v1` is opt-in through
`--transfer-speed-seats none|0|1|both`, default `none`, and requires clear-entry
approach/travel configuration. It runs only when the existing approach decision
is fresh and actually used for **transfer** during the committed post-escape
trip. Launch/climb, solar avoidance, recovery, unavailable controls/queries,
disabled flight, supported feet, changed vehicle/selection and capture handoff
retain their existing priority. The original 60-second transfer deadline and
20-second progress limit remain unchanged.

For each non-destination planet, use the current ship/body motion to compute
outward radial normal, surface clearance and inward relative speed. Derive a
maximum desired closing speed from the existing 70-unit climb floor, time to
turn through half a revolution at the observed turn-speed limit plus 0.6 seconds,
and a braking estimate. As with the existing boundary guidance, reserve half the
nominal brake acceleration and inward gravity; clamp this estimate to 5–25.

```text
t = pi / max(turn_speed, 0.1) + 0.6
a = clamp(brake_acceleration / 2 - max(inward_gravity, 0), 5, 25)
room = max(clearance - 70, 0)
maximum_closing_speed = sqrt((a*t)^2 + 2*a*room) - a*t
```

These are guidance estimates, not certificates of future physical clearance.
Braking operates in the current local frame; later body motion, gravity,
contacts and the flight motor remain authoritative. The destination is excluded
so its ordinary arrival controller can take over. Existing obstacle routing and
solar checks remain active.

Each planet defines a half-plane of admissible world velocities. Select the
closest velocity to the ordinary waypoint request satisfying all constraints,
with a correction magnitude no larger than the existing 55-unit transfer speed.
In two dimensions, examine the original request, each single-line projection,
and each nonparallel pair intersection: at most **1 + N + N(N−1)/2** candidates
for N other planets. Use feasibility tolerance 0.001 and skip pair determinants
of magnitude at most 0.0001. There are no added planner graph operations,
physics queries, simulation forecasts or clock renewals.

If no admissible bounded correction exists, retain the ordinary desired velocity
and request braking. Also request braking when actual inward speed exceeds any
current cap by more than 0.001. Pass this request through the existing flight
motor, including its native braking compensation, thrust, turn and wing controls.
Keep defensive weapon eligibility unchanged. This does not grant interaction,
landing or capture permission, or extend the earlier escape boundary controller.

## Comparison plan fixed before candidate outcomes

Baseline: `target/transfer-approach/v1/summary.json`, SHA-256
`074b66abbaa7e280cdcd9e7438cd5a5106d565f3ea6bf0bba762a0f4a114d9b3`.
Retain all **17 cases**, seeds, full match lengths, opponents, budgets, physics
and prior options. Replay all 17 with this option disabled and require exact
prior reports, seven streams, sensors and allocation ledgers. Run 17 comparisons
with only the evaluated seat enabled in the existing 15 enabled cases; retain
the two disabled controls. Keep the three health regressions separate from the
eight primary armed cases. Use at most two concurrent simulations. Freeze code,
tests and this plan before outcomes, then copy/hash the profiled release binary.

`tools/validate-transfer-speed.py` retains all earlier native route, physical
crossing, continuation, cover, receipt, abort, escape, travel and entry audits.
Save the exact consumed observation, waypoint avoidance, desired velocities,
constraint inputs/outputs and actual actions for each eligible speed decision.
Independently reconstruct the ordinary waypoint velocity, body-relative bounds,
bounded joint projection, braking requirement and counters. Verify the actual
brake and wing bytes, immutable travel identity/deadline and native capture
handoff. A first changed control must follow a recorded speed/brake intervention.
Cases without an intervention must remain exact after removing only the new
telemetry, including every earlier experiment's evidence.

Measure approach-frame transitions and speed-dependent climb margins, native
arrival, physical capture/landing/exit/claim/boarding/departure, damage, vehicle
and pilot loss, and complete match outcomes. Preserve the earlier no-escape
reference alongside the immediate baseline. Report all primary and health
cases, including failures. Do not retune the frozen policy from these outcomes.
The correlated development suite does not establish independent playing
strength or Raspberry Pi performance. Defaults and remote deployment stay
unchanged; work and commits remain local.

## Results

The limiter avoids the prior forced-climb detour **during the observed trip**, but
does not produce a capture, claim or completed departure. Both affected world 1
P1 powered runs lose their ships earlier to cannon damage. Keep the option
disabled by default. No constants or controls were retuned after the comparison.

The two cases share the same source: escape/travel handoff **8376**, destination
**2**, original deadline **11976**, and **31.174 hull**. The first velocity
intervention and changed control are both at **8555**. Each run records **516**
eligible transfer decisions, **337** desired-velocity changes, **73** forced-brake
requests and **zero** infeasible projections. Velocity changes and brake requests
are separate counters and may overlap. At most **four candidate velocities**
are checked per decision for the two non-destination planets.

The saved baseline exceeds the proposed actual closing-speed cap first at 8588.
The candidate's earlier intervention constrains the desired velocity before
that point. The prior entry screen still runs unchanged: it selects alternate
entries on 81 candidate ticks, all used for transfer. Neither candidate enters
launch/climb before ship loss, versus 202 climb ticks in each baseline trip.

### Physical approach and damage

The native local frame still changes from destination planet 2 to planet 1, now
at **8822**. At that transition, clearance above planet 1 is **193.945** and
inward relative speed is **36.957**, so the unchanged native climb threshold is
**97.317**. Transfer remains active. Across the recorded transfer decisions,
every non-destination planet remains outside its speed-dependent climb region;
the smallest measured margin is **73.185** at tick 8891. This is evidence about
the observed trajectory, not a guarantee of later clearance after the run ends.

The ship's closest approach to planet 2 before loss is at **8891**: center
distance **220.333**, destination radial clearance **176.952**, and relative
speed **34.374**. It never enters the native arrival distance of radius plus 105
(**148.381** here). No landing or capture permission is reached or bypassed.

| Trip measure, per affected case | Clear-entry baseline | Speed-limited candidate |
| --- | --- | --- |
| Travel ticks before ship loss | 809 | 516 |
| Launch/climb ticks | 202 | 0 |
| Ticks within destination arrival distance | 169 | 0 |
| Ticks within arrival distance and speed, queries ready | 52 | 0 |
| Complete native arrival gate / capture handoff | 0 | 0 |
| Closest destination center distance | 122.539 | 220.333 |
| Ship loss tick | 9185 | 8892 |
| Pilot loss / match end tick | 9407 | 10686 |

Hull remains **31.174** through 8891, then drops to zero at **8892** with a native
cannon-hit receipt on that tick. Recovery ends the commitment immediately with
`recovery required`; the deadline is neither renewed nor reached. Each pilot
later dies in an arena-boundary impact at **10686**. The longer post-loss pilot
survival does not recover the ship or produce another objective.

No new pursuit, capture task, landing, hatch exit, claim, boarding or completed
departure occurs during the trip. Neither defensive weapon fires. Each run
defers **204** ownership-based pursuit classifier decisions and no incoming-fire
responses: the first new hull-damaging tick is also the ship-loss tick. These
counts precede the optional health gate and are not distinct attacks. The
health-enabled case follows the same approach and loss; it is a correlated
diagnostic, not independent confirmation of strength.

### Complete retained outcomes

The eight primary armed cases retain **2 wins, 26 claims and 25 completed
departures**, exactly the clear-entry baseline totals. The earlier no-escape
reference had **2 wins, 28 claims and 27 departures**, including three claims and
departures in each affected case. This option does not recover that regression.
“Before” below means the immediate clear-entry baseline.

| Primary case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 4 → 4 | 4 → 4 | 21907 → 21907 |
| World 0 P1 powered | win → win | 5 → 5 | 5 → 5 | 36000 → 36000 |
| World 0 P2 walking | loss → loss | 4 → 4 | 4 → 4 | 26591 → 26591 |
| World 0 P2 powered | loss → loss | 4 → 4 | 3 → 3 | 30536 → 30536 |
| World 1 P1 walking | loss → loss | 1 → 1 | 1 → 1 | 36000 → 36000 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 9407 → 10686 |
| World 1 P2 walking | win → win | 4 → 4 | 4 → 4 | 36000 → 36000 |
| World 1 P2 powered | loss → loss | 3 → 3 | 3 → 3 | 36000 → 36000 |

The health regressions remain separate:

| Health case | Result before → after | Claims before → after | Departures before → after | End tick before → after |
| --- | --- | --- | --- | --- |
| World 0 P1 walking | loss → loss | 3 → 3 | 3 → 3 | 16190 → 16190 |
| World 0 P1 powered | loss → loss | 3 → 3 | 3 → 3 | 15014 → 15014 |
| World 1 P1 powered | loss → loss | 1 → 1 | 1 → 1 | 9407 → 10686 |

### Verification and evidence

Implementation, tests and the comparison plan were frozen in **`a9ab053`**.
All **1,079 Rust tests** and **731 Python tests** pass. Coverage includes joint
velocity constraints, infeasible/bounded fallback, moving-body input, brake-byte
validation, unchanged unconstrained controls, clone/reset/duplicate ticks, native
climb priority and neutral capture handoff. Formatting, strict AI Clippy with
`--no-deps`, and profiled/ordinary release builds pass. Scenario Clippy retains
its same seven pre-existing findings.

All **34 saved-run audits** pass: 17 disabled replays retain exact prior state,
and 15 candidate runs remain exact after removing only the new telemetry. The
two changed control sequences first differ at 8555, on a recorded intervention.
The prior route, physical crossing, continuation, cover, receipt, abort, escape,
travel and entry audits remain active. Native dispatch stays capped at **4 graph
operations / 384 queries**, with maximum publication age **120 ticks**. No
checker correction or simulation rerun was needed for this comparison.

The frozen profiled binary is
`target/transfer-speed/surface_mission_soak-a9ab053`, SHA-256
`f4cc149d6591e58a192aa3570bbc2b1f759277acfa14b5fbe549cedb7df1fa2a`.
The completed summary is `target/transfer-speed/v1/summary.json`, SHA-256
`0f6d1217d577f14ba590781ac00987a4ee595db99bab04c8264b9d1f742d865b`.
[The manifest](data/transfer-speed-v1.json) records complete outcomes, checks and
summary/archive hashes. [The archive](data/transfer-speed-v1.json.gz) preserves
the frozen plan, exact consumed observations and velocity decisions, native
route/physical witnesses, first changed controls, mission/vehicle/damage history,
approach geometry, baseline diagnosis, validation logs, reproduction scripts and
raw-file hashes. All **203 embedded documents**, **721 raw files**, retained
inputs and both frozen binaries were hash-verified. Full streams remain at their
hashed local paths. Defaults and
deployment remain unchanged; work stays local.

### Next investigation

Investigate the threat along the post-escape trip and the viability of committing
the remaining hull to it. The measured clearance improves, but a fatal hit ends
the trip before arrival. Inspect the opponent's range, visibility and relative
motion before that hit, using the current observations, to distinguish a safer
route or an earlier retreat decision from a reaction after damage. The current
combat observation exposes opponent motion and visibility plus last-hit
telemetry, not an incoming-projectile track. Preserve the native safety and
capture gates, and freeze any new threat-response hypothesis before comparing
complete outcomes. Do not infer useful escape/travel from a longer match or a
cleaner approach alone.

The [completed threat investigation](transfer-threat.md) identifies the fatal
contact as a missile launched at 7977, before this transfer; neither ship fires
during the candidate trip. That finding shifts the next experiment toward
bounded projectile observation before choosing an avoidance response.
