# Pursuit disengagement: escaping is not the whole handoff

This follows the [contested-approach investigation](bot-contested-approach.md)
from `67690d7`, with `24021e6` as the runtime comparison baseline.

**A bounded escape maneuver is now available as a headless, opt-in experiment.**
It establishes physical separation in all four retained cases where it starts.
It is not enabled by any normal policy selection or by the Picade UI. One
asteroid case still loses its pilot much earlier than baseline after the next
transfer gives that separation back. We retain the maneuver and its comparison
switch for the next mission-choice experiment, not as a promoted match policy.

## What the task does

The runner accepts `--disengagement-seats none|0|1|both`, defaulting to `none`.
The comparison enables only the v11 seat. Policy IDs, landing route selection,
weapon settings and the shared live-planning allowance stay unchanged.

When a 30-second pursuit expires, the optional task starts if an unoccluded,
living full enemy ship remains within 350 units and there is still unowned
ground to pursue. It does not infer cover from lack of visibility: visibility
is a first-solid query toward the target, independent of aim, and a missing hit
does not positively identify an occluder. Already supported ships,
committed surface tasks, on-foot work and recovery retain their priority.

The task has a fixed 12-second deadline. At entry it compares **seven escape
directions** spanning the half-plane away from the opponent. Each uses twelve
half-second forecast steps, accounting approximately for turn time, inertia,
observed gravity, current thrust limits and translating planet bounds. The
opponent continues its observed velocity in this estimate. Clear forecast legs
are preferred, followed by estimated minimum range with a smaller preference
for final range. If every leg intersects a forecast margin, the least-obstructed
estimate supplies a direction; the normal navigation controller still handles
obstacles. These estimates never authorize a landing or certify a collision-free
physical path.

The direction is held during the maneuver. Existing world guidance handles
planet/sun detours and ground clearance. Open wings provide turn authority;
swept wings are allowed after alignment on an unobstructed leg. Direct flight
requests 110 units/second; ordinary transfer guidance requests at most 55.
These are desired velocities, not instantaneous changes to physical motion.

The maneuver completes after one continuous second of either:

- at least 350 units of range with nonnegative relative opening speed; or
- current body or terrain-fragment occlusion reported by the combat observation.
  This includes moving debris and does not certify durable ground cover.

A failed 12-second attempt returns to a fresh bounded fight, rather than
admitting a landing beside the pursuer. Hits do not extend either timer. Solar
avoidance can temporarily take control without changing the deadline; recovery
cancels the maneuver. Finite counters and the latest attempt preserve the
start/end ticks, reason, direction, estimates and measured separation.

The seven forecasts run once per attempt. They add **no physics queries or
graph expansions**. This is a small fixed motor calculation, not a claim that
all bot work is now covered by the graph/query scheduler or that the forecast
is a physics rollout.

## Three physical probes

The diagnostic replay uses seed `7725194555774358125`, v10 P1 / v11 P2,
mixed asteroids every three seconds, early objective routes, and the shared
16,384 graph / 1,024 query allowance. Both players are traced densely starting
at tick 6,000. Each prototype reproduces the baseline observations and actions
through tick 6,301.

| Variant | Escape behavior | Observed result |
|---|---|---|
| v1 | Directly away, ordinary 55-unit guidance | Never clears contact; ship becomes a pod at 6,662. |
| v2 | Same axis, 110-unit request and aligned swept wings | A moving planet forces a detour/slowdown; no separation, pod at 6,662. |
| v3, retained | Seven initial direction estimates, then the v2 flight controller | Clears contact at 6,802 with 57.76 hull points; later transfer loses the advantage. |

The baseline pod tick is **6,611**. v1/v2 first change an actual action at 6,309,
even though their goal changes at 6,302. v3 changes the turn direction at 6,302.
None of these task labels alone establishes an improvement.

The final direction is 60 degrees away from the raw directly-away vector. Its
coarse minimum-range estimate is 93.25 units; actual dense observations dip
lower. The estimates are useful for ranking these alternatives, not conservative
safety bounds.

## The remaining failure is after separation

| Tick | Retained v3, asteroid P2 |
|---|---|
| 6,302 | Begins escape: enemy 132.78 away; hull 59.81. |
| 6,400 | Turning/burning with swept wings; enemy 84.98 away, hull unchanged. |
| 6,600 | Enemy 290.80 away; hull 57.76. |
| 6,742 | Range first exceeds 350 while opening. |
| 6,802 | Sustained-clearance test passes: range 371.82, opening speed 21.48. Coordinator selects planet 2, opens wings and brakes for transfer. |
| 7,000 | Transfer continues; enemy has closed to 230.09, hull still 57.76. |
| 7,082 | Combat resumes at range 176.92, but hull is already down to 20.02. |
| 7,335 | Ship becomes a pod, 12.07 seconds later than baseline ship loss. |
| 7,707 | Pilot dies; baseline pilot survives until 27,575. |

The pursuit retry delay still ends at 7,022, and the opponent is visible then.
P2 owns no planet, the opponent still has 54% hull, and the last hit is too old
to qualify for pursuit. New damage at 7,080 qualifies; the first-solid query
reports the opponent visible again at 7,082 and combat resumes. The escape task
protects its own interval; it does not evaluate the braking and turning required
by the next transfer. This corrects the earlier attribution to aiming alignment:
aim is not part of the visibility sensor.

All four triggered comparisons re-enter combat within about 4.7–7.5 seconds of
returning to transfer. Thus even the favorable match outcomes do **not** prove
that disengagement has produced a viable landing mission. There is no completed
new capture/boarding/departure cycle after separation in these four trials.
The initial quiet physical cycles still complete for both seats.

## Full comparisons

Eight paired conditions use 600-second match limits. Four baseline reports are
retained from the earlier checkpoint; twelve full reports are newly run. Quiet
means no random asteroids, not absence of opponent combat. Only the candidate's
v11 seat receives disengagement.

| Seed / condition | Baseline | Candidate | Disengagement |
|---|---|---|---|
| Retained seed, quiet v11 P1 | P1 time-limit win, 600.00 s; 2–0 planets | P1 pilot-death win, 201.10 s; 1–1 | Clear at 9,831, range 441 |
| Retained seed, quiet v11 P2 | P1 time-limit win, 600.00 s; 2–0 | P2 pilot-death win, 203.28 s; 1–1 | Clear at 11,160, range 369 |
| Retained seed, asteroids v11 P1 | P1 pilot-death win, 497.30 s; 2–0 | P1 pilot-death win, 313.13 s; 1–1 | Clear at 5,672, range 460 |
| Retained seed, asteroids v11 P2 | P1 pilot-death win, 459.58 s; 1–0 | P1 pilot-death win, 128.45 s; 1–0 | Clear at 6,802, range 372 |
| Seed 0, asteroids v11 P1 | P2 pilot-death win, 174.20 s | Identical | Never starts |
| Seed 0, asteroids v11 P2 | P2 pilot-death win, 172.58 s | Identical | Never starts |
| Seed 1, asteroids v11 P1 | P1 pilot-death win, 303.73 s | Identical | Never starts |
| Seed 1, asteroids v11 P2 | P1 time-limit win, 600.00 s | Identical | Never starts |

The quiet P2 outcome improves, while the asteroid P2 pilot lifetime regresses
substantially. The additional seeds check noninterference, but provide **no
additional activated examples**. This remains one world seed with four trigger
contexts, not an estimate of general strength or win rate.

## Validation and reproducibility

`target/pursuit-disengagement/compare.py` checks matched world/policy/budget
configuration, physics audits, per-tick shared allowances and allocation totals.
It verifies exact sparse observation/action and allocation equality for all
four untriggered additional-seed pairs, excluding only experiment telemetry and
dispatch timing. Both players' dense prefixes from 6,000 through 6,301 match the
baseline. The three prototype binaries and their source patches are retained.

Focused tests cover opt-in behavior, clone/reset/repeat-tick determinism, emitted
thrust, deadline handling despite repeated hits or solar preemption, sustained
separation, recovery/surface priority, and choosing a corridor independently of
the nearest-planet frame. Long scenario/AI tests continue to exercise physical
claims, boarding, departure, recovery and route planning. These verify
implementation contracts; the paired physical failures remain acceptance evidence.

The 640-test scenario/AI regression run passed, followed by six focused tests
after adding the corridor/frame test (641 distinct tests in the final tree).
Client compilation, formatting and diff checks passed. Clippy reports only
warnings in unchanged Rapier/scenario code.

The final disabled-control match reproduces all **1,280 sparse records and 26
allocation rows** of the baseline asteroid P2 match. The final enabled binary
also reproduces all **2,771 records** of the v3 dense probe after the test-only
cleanup. These two final reports pass the same physics/quota audits.

Build and reproduce the retained trial:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --require-finish true --seconds 600 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --disengagement-seats 1 --seed 7725194555774358125 \
  --asteroid-interval 3 --trace true --out /tmp/pursuit-disengagement
```

Omit `--disengagement-seats` to use unchanged normal behavior. Swap policies and
use seat `0` for the P1 experiment. Use `--seconds 180` without `--require-finish`
and add `--trace-start-tick 6000 --trace-end-tick 7250` for the dense probe.
`run_matrix.py`, `summarize_cases.py`, `compare.py`, raw reports and traces retain
the exact comparisons. Concurrent desktop timings are not performance or Pi
frame-rate measurements. A verified source/evidence archive is retained under
`/home/oldman/.codex/visualizations/2026/09/18/bot-pursuit-disengagement/`.
Large sensor logs remain in the target directory. No deployment or push is part
of this slice.

## Next decision to investigate

Follow-up: the [successor-flight investigation](bot-disengagement-handoff.md)
now tests this decision. It retains a read-only forecast probe and rejects the
automatic rule after two win-to-loss regressions and a measured false admission.

Evaluate **the escape and its successor together**. At 6,802 the local separation
condition is true, but selecting the nearest eligible planet requires a large
change in velocity that lets the pursuer close again. Raising the range threshold
or lengthening a cooldown alone does not test whether that next maneuver works.

Start with a small comparison at the same recorded decision boundaries:

1. Continue or resume the existing bounded combat action.
2. Escape along a corridor that leads into a feasible transfer.
3. Resume transfer only when its turn/deceleration and approach preserve useful
   separation or actual terrain cover.

Use observed opponent motion, own heading/velocity, weapon supply and prospective
transfer controls. Include turn and braking time in the short forecast, and
record the reason an alternative wins. Do not bypass actual landing, material
cover, route freshness or return-flight guards. The present candidate selector
ranks only escape directions; it is not yet a mission utility planner.

Acceptance should include a successful successor mission or justified combat
re-entry, pilot survival, and additional seeds where the decision actually
triggers. Retain the asteroid P2 replay as a counterexample to treating a
successful local action as a successful overall plan.
