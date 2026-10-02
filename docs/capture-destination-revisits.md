# Destination reconsideration after capture failure

The [qualified-cover experiment](capture-cover-response.md) can end a blocked
approach earlier, but its directed failure returns to the same planet. This
investigation asks which part of destination selection creates those returns,
what failure information survives, and whether current route evidence resolves
the recorded obstruction. It does not change the controller or replay physics.

## Native control paths

`MaterialMissionPilot::reconsider` retains a deferred planet ID and an expiry
30 seconds after failures that request deferral. A terminal capture failure uses
that path. Recovery, leaving the destination's approach frame, and an opportunistic
pursuit use non-deferring paths. The capture task is dropped on reconsideration;
its reason stays in bounded mission event history. Deferral itself retains no
failure category, cover state, material revision or route result.

Both initial selection and the value-switch gate reject a still-deferred planet.
After expiry, initial selection chooses the nearest eligible planet, preferring
another planet over the current local frame when an alternative exists. The
value evaluator needs an existing current target to propose a switch; it does
not make this first selection. An accepted switch also passes the controller's
altitude, descent commitment, recovery and once-per-trip gates.

Pursuit runs before destination selection and has its own 30-second budget.
Those clocks run concurrently. A pursuit starting just after capture failure can
therefore consume almost the entire deferral interval before the next capture
destination is selected.

The v13 value model compares conditional completion time per ownership unit.
Taking an enemy flag contributes two units once the bot has its own foothold;
a neutral capture contributes one. Published route certificates validate their
material/flag dependencies, but their timing scope explicitly excludes native
arrival, acquisition and exposure. A freshly revalidated certificate does not
establish that the exposure which blocked a previous attempt has cleared.

## Frozen audit plan

Use every original run in the completed `target/cover-response/v2/summary.json`:
32 directed runs, 32 fresh-world runs and ten recorded regression replays. Audit
only the tested v13 seat, retaining arms and case groups separately. The same
worlds and control trajectories recur across this source study; event counts are
not independent strength samples. Do not run additional matches or change a
coefficient, timer, policy, default or frozen outcome.

For every abandoned visit:

- Bind its selection and first terminal event to the existing visit audit.
  Missing history remains unverified.
- Classify deferral only from an explicit coordinator call path or a witnessed
  terminal capture failure from that exact visit. Missing failure snapshots
  remain unknown rather than acquiring a guessed cooldown.
- Find the next recorded selection of the same planet. No observed return is
  horizon censoring, not proof of permanent avoidance or success.
- Preserve intervening visits and paired pursuit starts/ends. An unfinished
  pursuit has unknown duration. Check every verified deferred return occurs at
  or after its expiry.
- Distinguish initial-picker selections from actually accepted value/time
  switches. Reconstruct accepted switches from their exact raw source forecasts,
  not from a hypothetical preferred candidate or a later report.
- Retain conditional costs, ownership values, material revisions, source and
  validation clocks, and the return visit's actual outcome. Fresh route evidence
  and matching material revision do not prove unchanged or resolved exposure.

Verify report/evaluation hashes against the frozen study. Pin the analyzed native
source files to the original runtime commit and freeze this auditor, tests and
plan before aggregating results. Retain any failed analysis attempts. No new
hardware-performance or gameplay-strength conclusion follows from this audit.

```sh
python3 tools/analyze-destination-revisits.py \
  --study target/cover-response/v2 \
  --out target/destination-revisits/v1
```

## Results

The audit at `526385e` completed all 74 recordings from runtime `757efb5`.
The [results archive](data/capture-destination-revisits-v1.json) retains each
abandoned visit, its witnessed failure, pursuit episodes, next same-planet
selection and accepted switch forecast where applicable. It also binds the
original report/evaluation hashes and the analyzed native source files.

**The cooldown is enforced, but expiration does not establish that the previous
failure has been resolved.** All 417 visits have verified selection/terminal
history. Of 255 abandoned visits, 41 use a deferring path: 37 witnessed capture
failures and four transfer-budget failures. The other 214 use explicit
non-deferring paths; none has unknown deferral classification in this dataset.

The table counts the next recorded same-planet selection after each deferring
failure. Initial selection and accepted value switching are separate sources.
Completed means that the return visit completed its capture sortie; unfinished
means it reached the recording horizon without a terminal event.

| Source group / arm | Runs | Deferring failures | Returns: initial / value | Return outcomes: completed / abandoned / unfinished |
| --- | ---: | ---: | ---: | ---: |
| Recorded / predecessor | 5 | 3 | 1 / 1 | 0 / 2 / 0 |
| Recorded / cover response | 5 | 6 | 1 / 2 | 1 / 2 / 0 |
| Directed / predecessor | 16 | 7 | 4 / 1 | 0 / 3 / 2 |
| Directed / cover response | 16 | 11 | 6 / 2 | 0 / 7 / 1 |
| Fresh worlds / predecessor | 16 | 7 | 4 / 0 | 0 / 4 / 0 |
| Fresh worlds / cover response | 16 | 7 | 4 / 0 | 1 / 3 / 0 |

There are 26 recorded returns after deferring failures; none occurs before its
cooldown expires. Twenty use the initial picker and six use accepted value
switches. Twenty-four follow an intervening pursuit that ends at or after
expiry. That count does not mean pursuit consumed the entire cooldown in every
case: the archive retains the actual overlapping ticks. Fifteen deferring
failures have no recorded return, which remains a finite observation horizon.

These are event counts, not independent trials: the recorded directed failure
also appears in the directed group, and unchanged trajectories recur across
arms. Across both fresh-world arms, all eight returns after deferring failure
use the initial picker. Six follow capture failure and two follow transfer
failure. More broadly, all 71 fresh-world returns after any abandoned visit use
the initial picker. A change confined to the value-switch gate would not address
these returns.

### Directed value-switch cycle

The recorded `directed-failure` trajectory is also
`value-destination-p1-bearing-0.8` in the directed set. Its first failed attempt
at planet 1 produces this sequence, with ticks measured at 60 Hz:

| Event | Predecessor | Cover response |
| --- | ---: | ---: |
| Capture failure consumed by coordinator | 4,953 | 4,171 |
| Planet 1 cooldown expires | 6,753 | 5,971 |
| Pursuit runs | 4,954–6,754 | 4,172–5,972 |
| Initial picker selects planet 0 | 6,754 | 5,972 |
| Accepted value switch returns to planet 1 | 7,264 | 6,482 |
| Returned visit fails | 9,003 | 7,351 |

Both returns occur 511 ticks after expiry. The cover response repeats the cycle
once more, returning at 9,668 and failing at 10,321. Neither arm completes a
capture in this trajectory. Ending a blocked attempt earlier permits another
attempt here without making it successful.

The accepted source forecast at tick 6,480 assigns the neutral current target
33.80 conditional seconds and the enemy destination 37.63. The enemy's two
ownership units give it 18.82 seconds per unit, versus the neutral's 33.80.
The switch therefore satisfies the value margin despite taking longer in the
nominal timing model. Its enemy certificate was measured at 5,974 and its route
validated at 6,479, both after the failure. All six deferred value returns in
the table have newly measured evidence and a nominally slower destination;
three are the repeated recording of the same directed events. The forecasts
explicitly leave combat risk and exposure unmodelled. Merely requiring a newer
certificate would not remove this cycle.

### Successful retries limit the conclusion

In `world2-asteroids0-p1-candidate`, planet 0's attempt ends at 19,381 after the
cover search uses its probe budget with candidates still unmeasured. Pursuit
runs from 19,382 to 21,182; the initial picker returns at 21,182, one tick after
expiry. This visit claims the flag at 24,162 and departs at 24,383. There are no
intervening destination visits. A permanent exclusion on that earlier failure
would reject this recorded successful retry.

The other completed deferred return is the recorded asteroid candidate. Its
first planet 1 visit ends at 4,187 because fresh ground surveys cannot provide a
complete flag round trip. After completing sorties on planets 0 and 2, it
selects planet 1 again at 12,155, claims at 14,631 and departs at 14,884. The
audit does not establish what changed in exposure or route feasibility between
attempts. Neither example proves that an unconditional retry is the best policy;
both show why a previous failure cannot be treated as permanent unreachability.

## Next experiment and limits

Carry structured failure context into a shared destination-admission decision
used by both the initial picker and accepted-switch gate. Distinguish a measured
route obstruction from an incomplete search, and preserve the relevant planet,
site, material/flag identity and failure clocks. A time-based expiry should not
silently become positive evidence that a route or exposure problem has cleared.
Fresh route timing alone cannot certify exposure either.

Before enabling such a policy, define what new evidence permits each kind of
retry and a bounded exploration path for incomplete evidence. Keep the two
successful returns above as regression cases alongside the directed loop. The
initial picker currently does not retain its complete candidate geometry in
these reports, so this audit identifies its control path without claiming that
a particular alternative was available or would have succeeded.

This investigation changes no controller, default, timer or score. It makes no
new strength or hardware-performance claim. The read-only auditor has ten
focused tests covering failure/visit identity, cooldown boundaries, censored
history, pursuit pairing and accepted-forecast matching. All 609 Python tests
pass; the archive records the test log hash. No physics reruns were needed.
