# Airborne acquisition defense: known rescue, fresh counterexample

**Retain this experiment; keep defaults unchanged.** The bounded defensive
handoff changes the diagnosed loss into a win and preserves the other three
known results. In the eight fresh pairs, it activates once and makes that pilot
die **2,426 ticks / 40.43 seconds earlier**, without improving the match result,
ship-loss count or completed departures. The frozen screen fails
`no_useful_fresh_change`. There is no runtime retuning or default promotion.

## Implementation and scope

The [frozen plan](acquisition-defense-plan.md), runtime, runner and initial tests
were committed at `c59fac6` before games. `--acquisition-defense-seats` defaults
to `none` and enables only v13. After the current native capture update, a fresh
weapon hit can interrupt an airborne task that has never selected a landing
site or touched the surface. The exact acquisition receipt and original actions
are retained. Previously committed, landed and on-foot tasks keep priority.

The response uses existing escape guidance, arena braking and native combat
weapon gates, with an immutable 720-tick deadline and a 30-second source-planet
cooldown. Missing opponent evidence cannot establish separation. Recovery,
solar avoidance, new ground contact and disabled controls consume the original
clock; they cannot restart it. The separate stale-recovery defect is unchanged.

All 24 games finish under the planned shared execution-routes host. The eight
fresh pairs cover **two world clusters**, v10, both evaluated seats and asteroid
intervals 0/3 seconds; climb laser stays enabled. Ordinary v13 is represented
only by its retained known qualification, not by this fresh matrix. These are
not device measurements or evidence of general bot strength.

## Complete pair results

Counts refer to the evaluated bot. `A0` means no asteroids; `A3` means an asteroid
every three seconds. Off/on below refers to **acquisition defense**, not the
separately frozen climb-laser setting.

| Pair | Result, off → on | Ship losses | Completed departures | First handoff |
| --- | --- | --- | --- | ---: |
| Known diagnosed loss, integrated, climb laser on | loss → win | 1 → 1 | 4 → 4 | 31,361 |
| Known win, integrated, climb laser off | win → win | 0 → 0 | 4 → 4 | none |
| Known rescued draw, integrated | draw → draw | 1 → 1 | 4 → 4 | none |
| Known retained win, ordinary v13 | win → win | 0 → 0 | 6 → 6 | none |
| Fresh world 0, A0, P1 | win → win | 0 → 0 | 1 → 1 | none |
| Fresh world 0, A0, P2 | loss → loss | 1 → 1 | 0 → 0 | none |
| Fresh world 0, A3, P1 | win → win | 0 → 0 | 1 → 1 | none |
| Fresh world 0, A3, P2 | loss → loss | 0 → 0 | 0 → 0 | none |
| Fresh world 1, A0, P1 | loss → loss | 1 → 1 | 2 → 2 | none |
| Fresh world 1, A0, P2 | win → win | 0 → 0 | 3 → 3 | none |
| Fresh world 1, A3, P1 | loss → loss | 1 → 1 | 3 → 3 | none |
| Fresh world 1, A3, P2 | loss → loss, earlier death | 1 → 1 | 2 → 2 | 15,455 |

The ten pairs without a handoff retain complete gameplay parity after removing
only the new option telemetry, including non-timing reports and charged work.
The changed pairs have exact state prefixes and matching source observations,
native captures and native actions at their first handoffs.

Fresh totals stay at **3 wins / 5 losses, 12 completed departures, 4 ship losses
and 4 pilot deaths** per option. Neither changed game completes a new capture
or returns to objective selection after its defense. Both retain their earlier
completed visits. No earlier enabled victory censors a later baseline departure
in this matrix; common-horizon visit lists are retained for every pair.

The fresh no-progress fraction falls from 22.60% to 19.66%, but eligible ticks
also fall from 83,627 to 81,158. The changed failed continuation ends earlier,
and defensive combat is excluded by that metric. This is not evidence of a
faster return to objectives. The longest eligible no-progress interval remains
13,056 ticks. Both players' complete visit, ownership, recovery and progress
results are retained in the manifest.

## The known rescue

The first handoff is **31,361**, exactly the first new laser-hit observation in
the diagnosed exposed acquisition. All **62,722 preceding actor rows** match.
The four earlier completed visits retain identical milestones, including their
departures at 2,585 / 6,940 / 18,575 / 26,526.

The original branch continues surveying until ship loss at 31,605, then dies
at 32,268. The enabled branch abandons the uncommitted acquisition at 31,361
and issues escape guidance immediately. It executes **567 escape-guided and
153 boundary-guided updates**, requesting neither weapon. At the fixed deadline
**32,081**, it still has **94.82% hull** and full pilot health.

The ordinary coordinator then resumes pursuit immediately. The ship is lost
at **32,314**; the pilot survives in its pod to the 36,000-tick ownership win,
with **83.44 health**. There is no subsequent selected capture site or completed
capture. This demonstrates survival on the selected case, not reliable escape
to a new objective or preservation of the ship throughout the match.

## The fresh counterexample

Fresh world 1 / A3 / P2 has already completed two departures. Its current
uncommitted capture began at **12,563**. The first qualifying hit arrives at
**15,455**, with only **25.31% hull** remaining from prior damage. The saved
native acquisition is `scan_deferred`; no site or objective route is selected.

All **43 defensive updates are boundary-guided**. On the handoff tick, native
capture requests a small turn and thrust. Defense instead requests full turn,
braking and no thrust. It fires neither weapon. A current cannon-hit receipt at
**15,498** coincides with ship destruction, which immediately hands control to
recovery. The disabled branch also takes a cannon hit at 15,498 but retains
9.53% hull and loses its ship at **15,580**, 82 ticks later.

The enabled pilot dies at **15,971**, versus **18,397** disabled. Both lose and
retain the same two completed departures. This is a **survival-time regression**
even though the frozen screen's coarse outcome/ship/death-count regression list
is empty. The separate `earlier_pilot_deaths` review field records it explicitly.

These observations bind the actual controller handoff, hit clocks, forms and
motion. This fresh case has no native impact stream, so the review does not
claim projectile identity, contact geometry, or the cause of the final death.
The selected escape forecast already has negative minimum clearance; that is a
model diagnostic, not proof that every real escape is impossible.

The next bounded investigation should reproduce this counterexample with the
native impact observer and inspect the boundary-guided turn/braking response.
Keep this failed screen intact before choosing another policy change. Broader
strength sampling and the stale-recovery correctness fix remain separate work.

## Validation and retained evidence

- **459 Rust tests pass**, including 11 new tests for handoff identity,
  commitments, fixed deadlines, safety preemption, scheduled breaks and reset.
- **856 Python tests pass**, including 11 new audit/matrix regressions. Formatting,
  the profiled release build and package Clippy with `--no-deps` pass. The broader
  Clippy invocation encountered an existing `collapsible_else_if` warning in
  `engine-rapier/src/spaceling.rs`; no unrelated source was changed.
- All four disabled qualification replays reproduce their original streams:
  9 exact streams each for the two recent games and 10 each for the two earlier
  games, plus non-timing report/sensor/planning and physical-result agreement.
- **1,021,016 dense actor rows** pass action/observation, physical visit, route,
  budget, trigger, deadline, separation, boundary and weapon audits. The two
  earlier qualification pairs retain **183,512 impact rows**, with zero overrides.
- One auditor correction at `ae056e6` normalizes integer seat-map keys to their
  persisted JSON representation. The original failure and both cached games are
  retained; neither was rerun. All 46 frozen input hashes remain verified except
  the explicitly recorded runner/test correction. Runtime, matrix and decision
  rules are unchanged.

The [manifest](data/acquisition-defense-v1.json) binds the commands, binary,
checks, comparisons, reviews and every raw archive. The
[review bundle](data/acquisition-defense-v1.json.gz) includes exact plans,
auditors, changed runtime sources, physical audits, defense witnesses, snapshots,
logs and the preserved failed audit. All **340 archived files** are verified
losslessly before removing generated raw copies: **21,241,249,994 raw bytes**
are retained in **2,900,573,598 compressed bytes** under
`target/acquisition-defense/v1/archives`. Original evidence remains intact.
Work is committed locally; nothing is pushed or deployed.
