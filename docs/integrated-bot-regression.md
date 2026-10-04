# Integrated candidate regression: combat and recovery diagnosis

This follows the [completed integrated comparison](integrated-bot-results.md).
The candidate remains experimental. Both diagnostic replays reproduce the
world-0 P1 asteroid control/candidate games exactly. The actionable next
hypothesis is **laser fire during required pursuit climbs without changing
flight guidance**. This is a proposed experiment, not a demonstrated rescue.

The active combat controller requests fire whenever its non-marginal diagnostic
gates permit it. The candidate instead suppresses nine available, already aimed
laser ticks during a clearance climb while the opposing ship has **0.174% hull**.
The subsequent scheduled weapons-off break has no such present-aim window.
This narrows [#155](https://github.com/aortez/space-wars/issues/155) to a specific
controller boundary; it does not justify removing ground avoidance or broadly
changing bot aggression.

## Capture, pursuit and loss chronology

Ticks refer to the observed/completed simulation state. An action selected at a
tick applies to the following step. The first changed action remains **4950**:
the powered ground navigator considers the flag reached while the control keeps
walking. The candidate claims and departs that visit 24 ticks earlier. It also
completes the next visit earlier, then enters its first pursuit at **7891**,
versus **7932** for the control.

| Tick | Candidate P1 observation or event |
| ---: | --- |
| 8018 | Starts engaging; own hull 98.80%, opposing ship 100%. |
| 8963–9202 | Scheduled weapons-off break; opponent is at 43.18% hull. |
| 9203 | Resumes engaging. |
| 9691 | The 1,800-tick pursuit budget expires at 21.29% versus 1.49% hull; selects a planet and stops firing. |
| 9734 → 9749 | Opponent requests the fatal cannon shot; native contact destroys P1's ship 15 ticks after projectile spawn. |
| 11678 | Completes ordinary recovery and returns to mission control. |
| 15556 / 15781 | Claims and departs its fourth completed capture. |
| 16557 | Starts pursuit of the still-vulnerable opposing ship, at 100% versus 1.49% hull. |
| 16949 | Starts clearing ground at 72.70% versus 0.174% hull. |
| 16949–16957 | Nine ready, visible, aligned laser observations; all nine actions keep the laser off. |
| 17409 | Resumes engagement after the climb. |
| 17431 | Scheduled break begins at 71.19% versus 0.174% hull. |
| 17527 → 17539 | Another opposing cannon shot destroys P1's ship, 12 ticks after spawn. |
| 17661 | A subsequent cannon contact sharply accelerates and spins the protected pod. |
| 17831 | World-boundary impact kills the pilot; P1 loses. |

Both fatal ship contacts have matching native damage/contact clocks and P2
cannon requests at their recorded spawn ticks. These are recently fired rounds,
not stale contact records or speculative attribution from enemy proximity.

The control's earlier fight instead destroys the opposing ship at **8504**.
It completes six captures, loses no ship and wins at **25483**. The candidate
completes four captures, loses two ships and dies at **17831**. The earlier
capture timing changes the subsequent interaction with the opponent; this replay
does not isolate each enabled feature or show that returning to the slower
ground approach is the right remedy.

The retained v16 pair has the same recorded visit metrics, final physical states,
combat counters, outcomes and P1 pilot-damage receipts as the respective v10
arms. Its reports were hash-verified. Only the v10 pair receives new dense
diagnostics here; these related cases are not independent world samples or a
claim of complete per-tick v10/v16 identity.

## What the firing diagnostic establishes

Across all non-marginal observations in `engage ship`, the recorded requests
agree with the existing aim, range, visibility and readiness gates. The measured
P1 totals within that mode are:

| Arm | Engaging ticks | Laser requests / available windows | Cannon requests / available windows |
| --- | ---: | ---: | ---: |
| Control | 2,309 | 1,603 / 1,603 | 21 / 21 |
| Candidate | 1,712 | 1,144 / 1,144 | 11 / 11 |

The opponent's non-marginal engaging observations also agree. One control P2
tick lies near the aim threshold and remains indeterminate. These counts cover
different match durations and are not a comparative accuracy or missed-shot
percentage. A request does not prove that the physical beam or projectile hits.

The candidate's intentional suppression separates into distinct situations:

| Interval | Mode | Laser windows with no request | Cannon windows with no request |
| --- | --- | ---: | ---: |
| 8963–9202 | First scheduled break | 9 | 0 |
| 9691–9748 | Transfer after pursuit timeout | 10 | 0 |
| 16949–17408 | Required pursuit clearance | **9** | 0 |
| 17431–17538 | Second scheduled break | **0** | 0 |

The clearance opportunity spans **16949–16957**: one tick in the combat
controller's `climb clear of ground`, followed by eight in the mission
controller's `climbing for a firing pass`. The target remains visible and
unoccluded at roughly **99.76 → 95.53 units**, with absolute bounded-lead aim
error **0.0192 → 0.0740 radians**, below the existing 0.08 limit. Laser energy
is **60.77 → 62.10%**, and the observed readiness gate stays true. Cannon
ammunition is unavailable. None of these nine observations is near the
diagnostic's indeterminate band.

The remaining climb lasts hundreds of ticks; nine moments of suitable existing
aim do not justify turning back toward the target throughout that interval.
The control also suppresses 41 such laser windows during its required climbs,
so its successful trajectory is a necessary retention case for any change.

## Why a late braking change is not selected

After the second ship loss, native center-of-mass measurements show:

| Tick | Observation | COM speed | Spin, rad/s | Radial boundary clearance |
| ---: | --- | ---: | ---: | ---: |
| 17539 | Pod appears | 22.22 | -4.62 | 307.02 |
| 17660 | Before protected cannon contact | 12.97 | -1.43 | 286.95 |
| 17661 | After that contact | 155.79 | 101.83 | 285.00 |
| 17830 | Last observation before fatal contact | 50.53 | 84.93 | 1.76 |

Pilot protection lasts until **17719**. The contact at 17661 changes physical
motion while pilot health stays 97.14. The bot holds full brake with no thrust
on all **291 ticks from 17540 through 17830**; the first pod observation is
unarmed. Subsequent laser damage leaves 96.47 health, all removed by the final
world impact. The observer uses center-of-mass velocity because the spinning
vehicle origin has a large additional rotational velocity.

This does not prove that every earlier escape is impossible. It does rule out
diagnosing the recorded final interval as a forgotten brake input. The previous
[pod-impact comparison](pod-boundary-impact.md) also retains evidence against a
late brake-only remedy in a different known loss.

## Next bounded experiment

Add an opt-in **laser-only opportunity during pursuit clearance**, using the
current combat observation and existing native firing gates. Preserve the
selected turn, thrust, brake and wing commands exactly. Do not turn to acquire
a shot or fire the cannon during the climb. Keep explicit weapons-off breaks,
on-foot tasks, capture, recovery and solar avoidance outside this option.
Preserve the existing pursuit budget and thresholds.

First compare the recorded losing candidate and winning control with this one
option enabled, checking the first changed action and complete capture/survival
outcomes. Then use a frozen broader retention/fresh-match plan before drawing
strength conclusions. Do not choose a health cutoff from this nearly destroyed
opponent or assume that firing for these nine ticks must win the match. The
separate ten-tick transfer opportunity remains recorded for a later hypothesis.

## Verification and artifacts

The diagnostic code and plan were frozen at `cf5935d`; the same frozen runtime
binary as the original integrated comparison was used. All **816 Python tests**
pass, including eight checks for changed/sparse observations, actor identity,
timing-only exclusions, command preservation, threshold ambiguity, damage
provenance and contiguous phase counts. No Rust runtime code changed.

Both existing case audits pass. The replay retains eight complete evidence
streams byte for byte per arm, exact non-timing reports and planner ledgers,
and all **12,291** previously recorded full observations. New dense traces
cover **86,628 pilot observations**; native impact traces cover **56,228**.
There are zero control overrides. Denser diagnostics add read/serialization
work outside the shared planning quota; they establish no device performance
result. Original and replay input hashes were verified after execution.

The [manifest](data/integrated-bot-regression-v1.json) records the replay parity,
mode counts, loss provenance and v16 report checks. The
[evidence archive](data/integrated-bot-regression-v1.json.gz) preserves the exact
summaries, frozen plan, audits, derived per-tick diagnostics, selected full source
rows and logs. Full raw streams remain under `target/integrated-bot/regression-v1`
and the original `comparison-v1`, with hashes retained in the summaries.
The integrated candidate's original retain decision and defaults are unchanged.

## Frozen diagnostic plan

Use the existing frozen binary and exact v10 control/candidate commands, changing
only output locations and diagnostic density. Trace full mission/combat
observations from tick 0; enable the existing native impact observer from tick
7600 through native match completion, with `--impact-pod-control bot`.
Run these two replays concurrently. No controller, physics, break setting,
pursuit limit, seed or match horizon changes. These are replays of known games,
not independent strength samples.

Before interpreting the new observations, require:

- Exact original capture/action streams, destination behavior, evaluation and
  flag-survey streams, including both players.
- Exact non-timing reports, sensor counts and charged planning ledgers. Remove
  only millisecond measurements and the declared trace-density setting.
- Every previously recorded full observation unchanged, plus complete new
  per-tick coverage for both seats and no control overrides.
- The existing configuration, physical-visit, launch/continuation, freshness,
  allocation and prediction audits; exact recorded outcomes for both players.

Retain chronology from the first movement difference through capture completions,
pursuit entries/exits, ground-clearance climbs, scheduled weapons-off breaks,
ship losses, recovery and pilot death. Join a projectile spawn to a loss only
when the native contact and damage timestamps agree; do not reuse stale contacts.
Use native center-of-mass motion when examining spinning escape pods.

Firing requests come from exact action packets. Present-aim/readiness windows
use the existing bounded-lead geometry diagnostic; values adjacent to f32 range
or alignment thresholds remain indeterminate. Count intentional suppression by
controller mode separately. These windows do not predict hits, justify removing
navigation safety, or measure a general missed-shot percentage. Establish a
specific subsequent hypothesis before changing combat behavior.

```sh
python3 tools/diagnose-integrated-regression.py \
  --prior target/integrated-bot/evaluation-v2/summary.json \
  --out target/integrated-bot/regression-v1
```
