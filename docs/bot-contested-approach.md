# Contested approaches: reacting after the hit is too late

This investigation starts at `24021e6`, following
[early objective candidates](bot-early-objective-candidates.md). The early
adapter delivers a powered landing route at tick 6,604 in the retained v11-P2
asteroid replay, but the ship becomes a pod at 6,611.

We measured the preceding decisions and tested a narrow defensive interruption.
It did not save that ship. It also shortened pilot survival in both replays
where it triggered, despite passing its implementation tests. **The prototype
was removed from the branch and archived for reproduction.** Runtime code and
default behavior remain at `24021e6`; this checkpoint adds investigation notes.

## What the dense replay shows

The existing runner supports a dense half-open trace window. We replayed seed
`7725194555774358125`, v10 P1 / v11 P2, with mixed asteroids every three seconds,
early route delivery and the usual 16,384 graph / 1,024 query shared allowance.
Both players are recorded on every tick from **6,000 through 6,639**. The
115-second probe reproduces all **352** retained sparse records before its end.

| Tick | P2 decision and physical evidence |
|---|---|
| 6,301 | Fighting, 59.81 hull points; enemy 133 units away. No loaded missile rounds. |
| 6,302 | The 30-second pursuit budget expires. P2 selects planet 1 and starts transferring. Enemy remains 133 units away; terrain is not reported as occluding it. |
| 6,370 | Enemy is visible at 85 units. P2 continues transferring. |
| 6,450 | Enemy is visible at only 7.8 units. P2 still follows transfer guidance. |
| 6,578 | P2 starts capture at about 105 units above the planet's nominal radius. It has 59.19 hull points and has just taken laser damage. Enemy is 29.5 units away. |
| 6,579 | A missile reduces hull to 34.96. Capture remains in its survey stage. |
| 6,604 | Early planning selects planet 1, bearing 56. Hull is 31.80; the proposed landed hull is still 150.55 units away. |
| 6,611 | The next missile destroys the ship. Pod recovery begins. |

There are two policy boundaries here:

1. Pursuit termination sets a **12-second retry delay**, through tick **7,022**.
   The ordinary opportunity check applies that delay before considering recent
   incoming fire. Travel therefore resumes while the opponent remains nearby.
2. Once capture starts, it takes priority over ordinary pursuit opportunities.
   Cover/route/landing failures can interrupt it, but recent weapon damage alone
   does not. Survey and circling guidance emit no weapon fire.

At 6,604, the existing solar corridor estimator reports roughly **4.40 seconds**
to approach the selected site, plus its separate surface allowance. That is a
coarse planning estimate, not a measured arrival promise. Only **0.117 seconds**
remain before ship loss. Finishing a search earlier cannot by itself bridge that
physical gap.

The turn is also substantial. At the first missile hit, the opponent's current
bearing is about **90.2 degrees** from the ship's forward direction. Dividing by
the observed 1.8 rad/s turn limit gives **0.87 seconds** for a fixed bearing;
the next fatal hit is only **0.53 seconds** away. This calculation ignores target
motion and projectile lead and is not a certified maneuver bound. The physical
defense trial below confirms that its controller never reaches a firing action
before losing the hull.

## The tested interruption

The archived prototype adds `--approach-defense-seats none|0|1|both` to the
headless runner, defaulting to `none`. We enabled it **only for the v11 seat** in
each paired replay. v10, the route adapter, shared allowance and sensors retain
their previous configuration.

The rule interrupts capture when an occupied full ship has at most **50 hull
points**, received laser/cannon damage within **120 ticks**, and sees an
unoccluded full enemy ship within **250 units**. These comparisons use current
observations; there is no hidden missile, enemy input or future-world access.
The hull threshold is in absolute health units, calibrated to these 100-point
matches, and would need reconsideration for custom health settings.

It preserves landed or supported ships, on-foot work, claims already completed,
and departure. It also preserves a short approach within **35 units** of a
current measured site when that site's ground is sheltered. Solar escape and
ship recovery retain priority.

An interruption defers that planet for 30 seconds and hands control to the
existing bounded combat task, including its ground-clearance behavior. New hits
do not extend the task's deadline. It reuses existing observations and performs
no extra physical queries. Telemetry records a count and the last triggering
observation, with no growing history. Clone/reset and repeat-tick behavior were
covered by tests.

### The immediate counterfactual

The recorded prefix, including **every tick 6,000–6,578 for both players**, is
identical after excluding the prototype's added telemetry. The first action
difference is at **6,579**:

- Baseline: capture/survey, full turn, brake off, thrust off, weapons off.
- Prototype: combat, full turn, brake on, thrust off, weapons off.

The ship keeps turning and emits **no firing action during the 31 updates**
before pod conversion at **6,610**, one tick earlier than baseline. Thus the
prototype changes the decision at the intended boundary; it does not establish
a useful escape or counterattack. A different goal label is not evidence of
better behavior.

## Full comparisons

Four new 600-second-limit matches are compared with four retained `24021e6`
matches. The seed, policy placement, early planning profile, weapon breaks,
asteroid settings and work allowance match. Quiet means no random asteroids;
opponents still fight. This is a diagnostic sample, not a win-rate estimate.

| Case; only v11 receives the prototype | Interruptions | Baseline round / seconds | Prototype round / seconds | P2's first pod tick, before → after |
|---|---:|---|---|---|
| Quiet, v11 P1 | 0 | P1 time-limit win / 600.00 | Same / 600.00 | unchanged |
| Quiet, v11 P2 | 1 at 8,761 | P1 time-limit win / 600.00 | P1 pilot-death win / 232.32 | 11,826 → 13,108 |
| Asteroids, v11 P1 | 0 | P1 pilot-death win / 497.30 | Same / 497.30 | unchanged |
| Asteroids, v11 P2 | 1 at 6,579 | P1 pilot-death win / 459.58 | P1 pilot-death win / 131.53 | 6,611 → 6,610 |

The quiet interruption has a mixed result: the full ship lasts **21.37 seconds
longer**, but P2's pilot dies at **13,939**, whereas both baseline pilots remain
alive through the time limit. Baseline quiet ownership ends 2–0; the prototype
ends 1–1. The two quiet baseline sorties by P1 become one before the earlier
round ends. We cannot treat extra hull lifetime as successful mission completion.

In the asteroid-P2 trial, pilot death moves from **27,575** to **7,892**. The
later trajectories differ after the intervention; this does not prove the rule
is universally harmful. It supplies no evidence to promote it. Both untriggered
cases retain all recorded observations/actions and all allocation rows, after
excluding only the new experiment telemetry and dispatch timing respectively.

All **eight full reports and two short probes** pass physics and shared-quota
audits. The prototype passes **639 scenario/AI tests**, including four new
interruption tests, plus client compilation, formatting and diff checks. Clippy
reports existing warnings. Those tests establish the implementation's stated
contract; they do not establish that the contract makes a stronger bot. The four
new tests are archived with the rejected prototype, not retained as tests for
behavior that no longer exists on the branch.

## What to try next

Investigate **the transition out of a timed-out pursuit**, before another
landing is admitted. Do not start by shortening another route budget or tuning
the late damage threshold to this seed.

A bounded next experiment can introduce an explicit disengagement phase when a
pursuit ends with an armed opponent still nearby. It should establish a useful
separation or covered route before transferring into landing, then yield to a
fresh mission decision. Retain a fixed deadline so repeated damage cannot
extend combat forever. Recovery, solar escape, supported landings and on-foot
objectives must keep their existing priority.

Start investigation at **6,302–6,578**, tracking enemy range, visibility, observed
heading and motion, our weapon supply, action outputs and the selected transfer
corridor. Compare continued fighting, breaking contact and resuming the transfer
as explicit alternatives. The ordinary combat task is a fight controller; this
experiment shows it cannot be assumed to implement a timely retreat.

Acceptance needs physical evidence of separation and a subsequent viable
mission, plus quiet capture/boarding/departure and swapped-seat pressure cases.
Preserve this failed late-interruption case as a counterexample. Add more seeds
before interpreting survival differences as general policy strength. This fits
the [framework's mission-choice work](design/budgeted-bot-planning.md) while
keeping the current route-planning primitive intact.

## Reproduction and retained evidence

The current branch can reproduce the dense baseline without the prototype:

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
target/release/examples/surface_mission_soak \
  --world generated --mode duel --match true --seconds 115 \
  --p1-policy material_mission_v10 --p2-policy material_mission_v11 \
  --live-objective-planning true --reuse-objective-ground true \
  --objective-dependencies routes --early-objective-routes true \
  --seed 7725194555774358125 --asteroid-interval 3 --trace true \
  --trace-start-tick 6000 --trace-end-tick 6640 --out /tmp/contested-approach
```

For a full baseline, use `--seconds 600 --require-finish true`. Swap policies for
P1 or set interval zero for quiet matches.

`target/contested-approach/` retains both binaries, reports, raw traces, allocation
tables, prototype tests and `summarize.py`. The latter checks matched settings,
quotas, the dense action boundary and unchanged untriggered cases.
`rejected-approach-defense.patch` applies to `24021e6` in an isolated checkout;
after building it, add `--approach-defense-seats 1` for P2 or `0` for P1. It is
evidence for a rejected experiment, not a pending production fix.

A verified source manifest and archive are retained under
`/home/oldman/.codex/visualizations/2026/09/18/bot-contested-approach/`.
Large sensor logs remain in the target directories. No deployment or push was
performed for this investigation.
